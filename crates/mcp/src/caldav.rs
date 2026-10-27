//! CalDAV de solo lectura y suscripciones `.ics` (RF-INT-07, D18): un
//! calendario por proyecto con sus tareas con vencimiento. Cada lectura se
//! hace como el actor del token, así que su alcance limita lo que ve.

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use model::{Actor, Project, ProjectId, Timestamp};
use store::{Reader, TaskFilter};

use crate::ical::{self, Item};
use crate::web::Web;

const ROOT: &str = "/caldav/";
const DAV_HEADER: &str = "1, 3, calendar-access";
const ALLOW: &str = "OPTIONS, GET, HEAD, PROPFIND, REPORT";

pub(crate) fn router(web: Arc<Web>) -> axum::Router {
    axum::Router::new()
        .route("/.well-known/caldav", axum::routing::any(well_known))
        .route("/caldav", axum::routing::any(dav))
        .route("/caldav/", axum::routing::any(dav))
        .route("/caldav/{*rest}", axum::routing::any(dav))
        .route("/cal/{file}", axum::routing::get(subscription))
        .with_state(web)
}

async fn well_known() -> Response {
    (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, ROOT)]).into_response()
}

/// El token: contraseña de `Basic` (el usuario no cuenta), `Bearer` o `?token=`.
fn token(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    if let Some(t) = crate::http::bearer(headers) {
        return Some(t);
    }
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Basic ")
                .or_else(|| v.strip_prefix("basic "))
        })
        .and_then(|b| STANDARD.decode(b.trim()).ok())
        .and_then(|raw| String::from_utf8(raw).ok())
        .and_then(|pair| pair.split_once(':').map(|(_, p)| p.to_owned()));
    basic.or_else(|| {
        uri.query()?
            .split('&')
            .find_map(|kv| kv.strip_prefix("token="))
            .map(str::to_owned)
    })
}

fn challenge() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"TaskCraft\"")],
        "TaskCraft token required (as password)",
    )
        .into_response()
}

/// Quién llama: el supervisor (token web) o una interfaz o su agente.
async fn who(web: &Web, headers: &HeaderMap, uri: &Uri) -> Option<Actor> {
    let token = token(headers, uri)?;
    if crate::web::supervisor_token(web.home.as_deref(), &token) {
        return Some(Actor::Supervisor);
    }
    let engine = web.engine.clone();
    tokio::task::spawn_blocking(move || engine.read(|r| engine::identity::from_token(r, &token)))
        .await
        .ok()?
        .ok()
}

/// Proyectos que puede ver el actor (`None` = todos).
fn visible(r: &dyn Reader, actor: &Actor) -> Result<Option<Vec<ProjectId>>, engine::EngineError> {
    Ok(match actor {
        Actor::Agent(id) => match r.agent(*id)? {
            Some(agent) => engine::identity::scope(r, &agent)?,
            None => Some(Vec::new()),
        },
        Actor::Interface(id) => r.interface(*id)?.map_or(Some(Vec::new()), |i| {
            (!i.scope.is_empty()).then_some(i.scope)
        }),
        _ => None,
    })
}

/// Calendarios visibles con sus tareas con vencimiento.
type Calendars = Vec<(Project, Vec<Item>)>;

async fn load(web: &Web, actor: Actor) -> Option<Calendars> {
    let engine = web.engine.clone();
    let read = tokio::task::spawn_blocking(move || {
        engine.read(|r| {
            let allowed = visible(r, &actor)?;
            let keys = engine::lookup::Keys::load(r)?;
            let mut out = Vec::new();
            for p in r.projects(false)? {
                if allowed.as_ref().is_some_and(|ids| !ids.contains(&p.id)) {
                    continue;
                }
                let items = r
                    .tasks(&TaskFilter::project(p.id))?
                    .into_iter()
                    .filter(|t| t.due.is_some())
                    .map(|t| Item {
                        key: keys.of(&t),
                        task: t,
                    })
                    .collect();
                out.push((p, items));
            }
            Ok(out)
        })
    })
    .await;
    read.ok()?.ok()
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn multistatus(responses: &[String]) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\" \
         xmlns:CS=\"http://calendarserver.org/ns/\">\n{}</D:multistatus>\n",
        responses.concat()
    );
    (
        StatusCode::MULTI_STATUS,
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::HeaderName::from_static("dav"), DAV_HEADER),
        ],
        body,
    )
        .into_response()
}

fn response(href: &str, props: &str) -> String {
    format!(
        "<D:response><D:href>{}</D:href><D:propstat><D:prop>{props}</D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\n",
        xml(href)
    )
}

fn root_props() -> String {
    format!(
        "<D:resourcetype><D:collection/></D:resourcetype>\
         <D:displayname>TaskCraft</D:displayname>\
         <D:current-user-principal><D:href>{ROOT}</D:href></D:current-user-principal>\
         <D:principal-URL><D:href>{ROOT}</D:href></D:principal-URL>\
         <C:calendar-home-set><D:href>{ROOT}</D:href></C:calendar-home-set>"
    )
}

fn calendar_href(p: &Project) -> String {
    format!("{ROOT}{}/", p.key)
}

fn item_href(p: &Project, i: &Item) -> String {
    format!("{ROOT}{}/{}.ics", p.key, i.key)
}

fn calendar_props(p: &Project, items: &[Item]) -> String {
    format!(
        "<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>\
         <D:displayname>{}</D:displayname>\
         <C:supported-calendar-component-set><C:comp name=\"VTODO\"/></C:supported-calendar-component-set>\
         <CS:getctag>{}</CS:getctag><D:getetag>{}</D:getetag>",
        xml(&format!("{} · {}", p.key, p.name)),
        xml(&ical::ctag(items)),
        xml(&ical::ctag(items)),
    )
}

fn item_props(i: &Item, data: bool, now: Timestamp) -> String {
    let mut props = format!(
        "<D:getetag>{}</D:getetag>\
         <D:getcontenttype>text/calendar; charset=utf-8; component=vtodo</D:getcontenttype>",
        xml(&i.etag())
    );
    if data {
        let body = ical::calendar(&i.key, &[ical::todo(i, now)]);
        props.push_str(&format!(
            "<C:calendar-data>{}</C:calendar-data>",
            xml(&body)
        ));
    }
    props
}

/// `href`s de un `calendar-multiget`, sin depender del prefijo del espacio de nombres.
fn hrefs(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("href>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find('<') else { break };
        let href = after[..end].trim();
        if !href.is_empty() {
            out.push(href.replace("&amp;", "&"));
        }
        rest = &after[end..];
    }
    out
}

fn ical_response(body: String, etag: Option<String>) -> Response {
    let mut resp = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
        body,
    )
        .into_response();
    if let Some(tag) = etag.and_then(|t| HeaderValue::from_str(&t).ok()) {
        resp.headers_mut().insert(header::ETAG, tag);
    }
    resp
}

async fn dav(State(web): State<Arc<Web>>, request: Request) -> Response {
    let method = request.method().clone();
    if method == Method::OPTIONS {
        return (
            StatusCode::OK,
            [
                (header::HeaderName::from_static("dav"), DAV_HEADER),
                (header::ALLOW, ALLOW),
            ],
        )
            .into_response();
    }
    let Some(actor) = who(&web, request.headers(), request.uri()).await else {
        return challenge();
    };
    let depth = request
        .headers()
        .get("depth")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("0")
        .to_owned();
    let path = request
        .uri()
        .path()
        .trim_start_matches("/caldav")
        .to_owned();
    let Ok(body) = axum::body::to_bytes(request.into_body(), 1 << 20).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let body = String::from_utf8_lossy(&body).into_owned();
    let Some(calendars) = load(&web, actor).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let now = Timestamp::now();
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let find = |key: &str| {
        calendars
            .iter()
            .find(|(p, _)| p.key.eq_ignore_ascii_case(key))
    };
    match (method.as_str(), parts.as_slice()) {
        ("PROPFIND", []) => {
            let mut out = vec![response(ROOT, &root_props())];
            if depth != "0" {
                for (p, items) in &calendars {
                    out.push(response(&calendar_href(p), &calendar_props(p, items)));
                }
            }
            multistatus(&out)
        }
        ("PROPFIND", [key]) => match find(key) {
            Some((p, items)) => {
                let mut out = vec![response(&calendar_href(p), &calendar_props(p, items))];
                if depth != "0" {
                    for i in items {
                        out.push(response(&item_href(p, i), &item_props(i, false, now)));
                    }
                }
                multistatus(&out)
            }
            None => StatusCode::NOT_FOUND.into_response(),
        },
        ("PROPFIND", [key, file]) => match find(key).and_then(|(p, items)| {
            items
                .iter()
                .find(|i| format!("{}.ics", i.key) == *file)
                .map(|i| (p, i))
        }) {
            Some((p, i)) => multistatus(&[response(&item_href(p, i), &item_props(i, false, now))]),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        ("REPORT", [key]) => match find(key) {
            Some((p, items)) => {
                let wanted = body.contains("calendar-multiget").then(|| hrefs(&body));
                let out: Vec<String> = items
                    .iter()
                    .filter(|i| {
                        wanted
                            .as_ref()
                            .is_none_or(|hs| hs.iter().any(|h| *h == item_href(p, i)))
                    })
                    .map(|i| response(&item_href(p, i), &item_props(i, true, now)))
                    .collect();
                multistatus(&out)
            }
            None => StatusCode::NOT_FOUND.into_response(),
        },
        ("GET" | "HEAD", [key]) => match find(key) {
            Some((p, items)) => {
                let todos: Vec<String> = items.iter().map(|i| ical::todo(i, now)).collect();
                ical_response(ical::calendar(&p.name, &todos), Some(ical::ctag(items)))
            }
            None => StatusCode::NOT_FOUND.into_response(),
        },
        ("GET" | "HEAD", [key, file]) => {
            let found = find(key)
                .and_then(|(_, items)| items.iter().find(|i| format!("{}.ics", i.key) == *file));
            match found {
                Some(i) => ical_response(
                    ical::calendar(&i.key, &[ical::todo(i, now)]),
                    Some(i.etag()),
                ),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
        ("PROPFIND" | "REPORT" | "GET" | "HEAD", _) => StatusCode::NOT_FOUND.into_response(),
        // Solo lectura (D18).
        _ => (StatusCode::FORBIDDEN, [(header::ALLOW, ALLOW)]).into_response(),
    }
}

/// `/cal/API.ics`: calendario de eventos para suscribirse (Google Calendar…).
async fn subscription(
    State(web): State<Arc<Web>>,
    Path(file): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let Some(actor) = who(&web, &headers, &uri).await else {
        return challenge();
    };
    let Some(key) = file.strip_suffix(".ics") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(calendars) = load(&web, actor).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let now = Timestamp::now();
    match calendars
        .iter()
        .find(|(p, _)| p.key.eq_ignore_ascii_case(key))
    {
        Some((p, items)) => {
            let events: Vec<String> = items.iter().filter_map(|i| ical::event(i, now)).collect();
            ical_response(ical::calendar(&p.name, &events), Some(ical::ctag(items)))
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::hrefs;

    #[test]
    fn multiget_hrefs_with_any_prefix() {
        let body = "<C:calendar-multiget xmlns:D=\"DAV:\"><D:prop/>\
                    <D:href>/caldav/API/API-1.ics</D:href><href>/caldav/API/API-2.ics</href>\
                    </C:calendar-multiget>";
        assert_eq!(
            hrefs(body),
            ["/caldav/API/API-1.ics", "/caldav/API/API-2.ics"]
        );
    }
}
