//! L3 — Formato nativo de intercambio `.taskcraft` (RF-EXP-01…05).
//!
//! Un ZIP con `manifest.json` (formato, versión, fecha) y `workspace.json`
//! (todas las entidades, con sus IDs e historial). Este crate sabe recoger
//! los datos de un `Reader`, escribir y leer el archivo (migrando versiones
//! anteriores), validarlo, planificar la importación en un workspace y
//! aplicarla con un `Writer`. El engine decide quién puede hacerlo y lo
//! ejecuta en una sola transacción.

mod merge;
mod plan;
mod text;

use std::collections::HashSet;
use std::io::{Read, Seek, Write};

use i18n::{Localize, Msg};
use model::{
    ActivityEntry, Agent, Comment, Dependency, InboxItem, Label, Project, ProjectId, Task,
    Timestamp, WorkspaceInfo,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use store::{ActivityFilter, Reader, StoreError, TaskFilter};

pub use plan::{Collision, Counts, Mode, Plan, apply, plan};
pub use text::{MarkdownFile, csv, markdown};

/// Identificador del formato en el manifiesto.
pub const FORMAT: &str = "taskcraft";
/// Versión del formato que escribe esta versión de la app (RF-EXP-04).
/// v2 (M5): épicas e hitos, plantillas, adjuntos y relaciones.
/// v3 (M11, D15): interfaces y asignados con rol.
/// v7 (RF-MOD-19): diagramas Mermaid.
pub const VERSION: u32 = 8;

const MANIFEST: &str = "manifest.json";
const DATA: &str = "workspace.json";
/// Carpeta del ZIP con el contenido de los adjuntos, un archivo por hash.
const BLOBS: &str = "attachments/";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub exported_at: Timestamp,
    pub app_version: String,
    pub workspace: String,
    /// Claves de los proyectos exportados; vacío = el workspace entero.
    #[serde(default)]
    pub projects: Vec<String>,
}

/// Todo lo que se exporta, con los IDs originales.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dump {
    pub workspace: WorkspaceInfo,
    pub projects: Vec<Project>,
    pub labels: Vec<Label>,
    /// Sin hashes de token: un token no viaja nunca (D09, D15).
    #[serde(default)]
    pub interfaces: Vec<model::Interface>,
    pub agents: Vec<Agent>,
    /// Padres antes que hijas.
    pub tasks: Vec<Task>,
    pub dependencies: Vec<Dependency>,
    pub comments: Vec<Comment>,
    /// De la más antigua a la más reciente.
    pub activity: Vec<ActivityEntry>,
    pub inbox: Vec<InboxItem>,
    #[serde(default)]
    pub epics: Vec<model::Epic>,
    #[serde(default)]
    pub templates: Vec<model::Template>,
    #[serde(default)]
    pub attachments: Vec<model::Attachment>,
    #[serde(default)]
    pub relations: Vec<model::Relation>,
    /// Vistas guardadas (RF-VIS-12); solo en la exportación completa.
    #[serde(default)]
    pub views: Vec<model::SavedView>,
    /// Coste reportado por los agentes (RF-HIS-06), desde la v4.
    #[serde(default)]
    pub costs: Vec<model::CostEntry>,
    /// Vínculos con ramas, commits y PRs (RF-INT-04), desde la v4.
    #[serde(default)]
    pub git_links: Vec<model::GitLink>,
    /// Ciclos de los proyectos (RF-INT-05), desde la v4.
    #[serde(default)]
    pub cycles: Vec<model::Cycle>,
    /// Diagramas Mermaid de los proyectos (RF-MOD-19), desde la v7.
    #[serde(default)]
    pub diagrams: Vec<model::Diagram>,
    /// Recorrido de las tareas (D24), desde la v8; del paso más antiguo al
    /// más reciente.
    #[serde(default)]
    pub transitions: Vec<model::Transition>,
    /// Contenido de los adjuntos por `sha256`; viaja aparte en el ZIP.
    #[serde(skip)]
    pub blobs: std::collections::BTreeMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub enum ExchangeError {
    /// No es un archivo `.taskcraft`.
    NotTaskcraft(String),
    /// Lo escribió una versión más nueva de la app (RF-EXP-04).
    NewerFormat {
        found: u32,
        supported: u32,
    },
    /// Datos inconsistentes: referencias rotas, duplicados…
    Invalid(Vec<Problem>),
    /// La importación como workspace nuevo exige un workspace vacío.
    TargetNotEmpty,
    Io(String),
    Store(StoreError),
}

impl Localize for ExchangeError {
    fn msg(&self) -> Msg {
        match self {
            Self::NotTaskcraft(detail) => Msg::new("exchange-not-taskcraft").arg("detail", detail),
            Self::NewerFormat { found, supported } => Msg::new("exchange-newer-format")
                .arg("found", *found)
                .arg("supported", *supported),
            Self::Invalid(problems) => Msg::new("exchange-invalid")
                .arg("count", problems.len())
                .arg(
                    "first",
                    problems
                        .first()
                        .map(|p| p.detail.clone())
                        .unwrap_or_default(),
                ),
            Self::TargetNotEmpty => Msg::new("exchange-target-not-empty"),
            Self::Io(detail) => Msg::new("exchange-io").arg("detail", detail),
            Self::Store(e) => e.msg(),
        }
    }
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.msg().fmt(f)
    }
}

impl std::error::Error for ExchangeError {}

impl From<StoreError> for ExchangeError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

fn io(e: impl std::fmt::Display) -> ExchangeError {
    ExchangeError::Io(e.to_string())
}

/// Un problema de consistencia encontrado al validar (RF-EXP-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// Código estable: `dangling-project`, `duplicate-id`…
    pub code: &'static str,
    pub detail: String,
}

/// Recoge el workspace entero o solo algunos proyectos (y lo que cuelga de ellos).
pub fn collect(r: &dyn Reader, only: Option<&[ProjectId]>) -> Result<Dump, StoreError> {
    let wanted = |p: &ProjectId| only.is_none_or(|ids| ids.contains(p));
    let projects: Vec<Project> = r
        .projects(true)?
        .into_iter()
        .filter(|p| wanted(&p.id))
        .collect();
    let mut tasks = Vec::new();
    for deleted in [false, true] {
        let filter = TaskFilter {
            include_archived: true,
            deleted,
            ..TaskFilter::default()
        };
        tasks.extend(
            r.tasks(&filter)?
                .into_iter()
                .filter(|t| wanted(&t.project_id)),
        );
    }
    let tasks = parents_first(tasks);
    let ids: HashSet<_> = tasks.iter().map(|t| t.id).collect();
    let dependencies = r
        .dependencies()?
        .into_iter()
        .filter(|d| ids.contains(&d.blocker) && ids.contains(&d.blocked))
        .collect();
    let mut comments = Vec::new();
    for t in &tasks {
        comments.extend(r.comments(t.id)?);
    }
    let all_history = ActivityFilter {
        limit: usize::MAX >> 1,
        ..ActivityFilter::default()
    };
    let mut activity: Vec<ActivityEntry> = r
        .activity(&all_history)?
        .into_iter()
        .filter(|e| match (only, e.project_id) {
            (None, _) => true,
            (Some(ids), Some(p)) => ids.contains(&p),
            (Some(_), None) => false,
        })
        .collect();
    activity.reverse();
    let agents = r.agents()?;
    let interfaces = r
        .interfaces()?
        .into_iter()
        .map(|i| model::Interface {
            token_hash: None,
            token: None,
            ..i
        })
        .collect();
    let inbox = match only {
        None => r.inbox_items(None)?,
        Some(_) => Vec::new(),
    };
    let epics = r
        .epics(None)?
        .into_iter()
        .filter(|e| wanted(&e.project_id))
        .collect();
    let relations = r
        .relations(None)?
        .into_iter()
        .filter(|rel| ids.contains(&rel.a) && ids.contains(&rel.b))
        .collect();
    let mut attachments = Vec::new();
    let mut blobs = std::collections::BTreeMap::new();
    for t in &tasks {
        for a in r.attachments(t.id)? {
            if !blobs.contains_key(&a.sha256)
                && let Some(data) = r.attachment_data(&a.sha256)?
            {
                blobs.insert(a.sha256.clone(), data);
            }
            attachments.push(a);
        }
    }
    let costs = r
        .cost_entries(&store::CostFilter::default())?
        .into_iter()
        .filter(|c| ids.contains(&c.task_id))
        .collect();
    let git_links = r
        .git_links(None)?
        .into_iter()
        .filter(|l| ids.contains(&l.task_id))
        .collect();
    let cycles = r
        .cycles(None)?
        .into_iter()
        .filter(|c| wanted(&c.project_id))
        .collect();
    let diagrams = r
        .diagrams(None)?
        .into_iter()
        .filter(|d| wanted(&d.project_id))
        .collect();
    let transitions = r
        .transitions(&store::TransitionFilter::default())?
        .into_iter()
        .filter(|t| ids.contains(&t.task_id))
        .collect();
    let (templates, views) = match only {
        None => (r.templates()?, r.views()?),
        Some(_) => (Vec::new(), Vec::new()),
    };
    Ok(Dump {
        workspace: r.workspace()?,
        projects,
        labels: r.labels()?,
        interfaces,
        agents,
        tasks,
        dependencies,
        comments,
        activity,
        inbox,
        epics,
        templates,
        attachments,
        relations,
        views,
        costs,
        git_links,
        cycles,
        diagrams,
        transitions,
        blobs,
    })
}

/// Ordena las tareas para que cada padre vaya antes que sus hijas.
pub(crate) fn parents_first(mut tasks: Vec<Task>) -> Vec<Task> {
    let mut placed: HashSet<model::TaskId> = HashSet::new();
    let mut out = Vec::with_capacity(tasks.len());
    while !tasks.is_empty() {
        let before = out.len();
        let (ready, rest): (Vec<Task>, Vec<Task>) = tasks
            .into_iter()
            .partition(|t| t.parent_id.is_none_or(|p| placed.contains(&p)));
        for t in ready {
            placed.insert(t.id);
            out.push(t);
        }
        tasks = rest;
        if out.len() == before {
            // Padres que no están en el lote: la validación lo explicará.
            out.extend(tasks);
            break;
        }
    }
    out
}

/// Escribe el archivo `.taskcraft`.
pub fn write(w: impl Write + Seek, manifest: &Manifest, dump: &Dump) -> Result<(), ExchangeError> {
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    let mut zip = zip::ZipWriter::new(w);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(MANIFEST, options).map_err(io)?;
    serde_json::to_writer_pretty(&mut zip, manifest).map_err(io)?;
    zip.start_file(DATA, options).map_err(io)?;
    serde_json::to_writer(&mut zip, dump).map_err(io)?;
    for (sha, data) in &dump.blobs {
        zip.start_file(format!("{BLOBS}{sha}"), options)
            .map_err(io)?;
        zip.write_all(data).map_err(io)?;
    }
    zip.finish().map_err(io)?;
    Ok(())
}

/// Lee un archivo `.taskcraft`, migrando su contenido si es de una versión
/// anterior del formato (RF-EXP-04).
pub fn read(r: impl Read + Seek) -> Result<(Manifest, Dump), ExchangeError> {
    let mut zip =
        zip::ZipArchive::new(r).map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?;
    let manifest: Manifest = {
        let file = zip
            .by_name(MANIFEST)
            .map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?;
        serde_json::from_reader(file).map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?
    };
    if manifest.format != FORMAT {
        return Err(ExchangeError::NotTaskcraft(manifest.format));
    }
    if manifest.version > VERSION {
        return Err(ExchangeError::NewerFormat {
            found: manifest.version,
            supported: VERSION,
        });
    }
    let raw: Value = {
        let file = zip
            .by_name(DATA)
            .map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?;
        serde_json::from_reader(file).map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?
    };
    let migrated = migrate(manifest.version, raw)?;
    let mut dump: Dump =
        serde_json::from_value(migrated).map_err(|e| ExchangeError::NotTaskcraft(e.to_string()))?;
    for a in &dump.attachments {
        if dump.blobs.contains_key(&a.sha256) {
            continue;
        }
        let Ok(mut file) = zip.by_name(&format!("{BLOBS}{}", a.sha256)) else {
            continue;
        };
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(io)?;
        dump.blobs.insert(a.sha256.clone(), data);
    }
    Ok((
        Manifest {
            version: VERSION,
            ..manifest
        },
        dump,
    ))
}

/// Cada migración lleva el JSON de la versión `n` a la `n + 1`. Nunca se
/// edita una publicada: se añade otra.
type FormatMigration = fn(Value) -> Result<Value, ExchangeError>;
const MIGRATIONS: &[FormatMigration] = &[
    v1_to_v2, v2_to_v3, v3_to_v4, v4_to_v5, v5_to_v6, v6_to_v7, v7_to_v8,
];

/// v7 → v8 (D24): lista de transiciones vacía; al importar, el recorrido se
/// reconstruye del historial.
fn v7_to_v8(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    root.entry("transitions")
        .or_insert_with(|| Value::Array(Vec::new()));
    Ok(data)
}

/// v6 → v7 (RF-MOD-19): lista de diagramas vacía; los proyectos empiezan
/// a numerarlos en 1.
fn v6_to_v7(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    root.entry("diagrams")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(Value::Array(projects)) = root.get_mut("projects") {
        for p in projects.iter_mut().filter_map(Value::as_object_mut) {
            p.entry("next_diagram").or_insert_with(|| Value::from(1));
        }
    }
    Ok(data)
}

/// v5 → v6 (RF-INT-05): el ciclo único de cada tarea pasa a ser una lista.
fn v5_to_v6(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    if let Some(Value::Array(tasks)) = root.get_mut("tasks") {
        for t in tasks.iter_mut().filter_map(Value::as_object_mut) {
            let cycles = match t.remove("cycle_id") {
                Some(id @ Value::String(_)) => vec![id],
                _ => Vec::new(),
            };
            t.insert("cycles".into(), Value::Array(cycles));
        }
    }
    Ok(data)
}

/// v4 → v5 (D20): asignados sin rol y sin votos; los proyectos sin política
/// de cierre.
fn v4_to_v5(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    if let Some(Value::Array(projects)) = root.get_mut("projects") {
        for p in projects.iter_mut().filter_map(Value::as_object_mut) {
            p.remove("close_policy");
        }
    }
    if let Some(Value::Array(tasks)) = root.get_mut("tasks") {
        for t in tasks.iter_mut().filter_map(Value::as_object_mut) {
            t.remove("reviews");
            let mut actors: Vec<Value> = Vec::new();
            if let Some(Value::Array(list)) = t.remove("assignees") {
                for a in list {
                    let actor = match a {
                        Value::Object(mut o) => o.remove("actor").unwrap_or(Value::Null),
                        other => other,
                    };
                    if actor.is_string() && !actors.contains(&actor) {
                        actors.push(actor);
                    }
                }
            }
            t.insert("assignees".into(), Value::Array(actors));
        }
    }
    Ok(data)
}

/// v3 → v4 (RF-HIS-06, RF-INT-04): listas de costes y de vínculos con git vacías.
fn v3_to_v4(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    for list in ["costs", "git_links", "cycles"] {
        root.entry(list).or_insert_with(|| Value::Array(Vec::new()));
    }
    Ok(data)
}

/// v2 → v3 (D15): el asignado único pasa a ser el ejecutor; sin interfaces.
fn v2_to_v3(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    root.entry("interfaces")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(Value::Array(tasks)) = root.get_mut("tasks") {
        for t in tasks.iter_mut().filter_map(Value::as_object_mut) {
            let assignees = match t.remove("assignee") {
                Some(Value::String(actor)) => {
                    serde_json::json!([{ "actor": actor, "role": "executor" }])
                }
                _ => Value::Array(Vec::new()),
            };
            t.entry("assignees").or_insert(assignees);
            t.entry("reviews")
                .or_insert_with(|| Value::Array(Vec::new()));
        }
    }
    Ok(data)
}

/// v1 → v2: listas nuevas vacías; los proyectos y tareas sin flujo, etapa,
/// estimación ni campos (D10, M5).
fn v1_to_v2(mut data: Value) -> Result<Value, ExchangeError> {
    let Some(root) = data.as_object_mut() else {
        return Err(ExchangeError::NotTaskcraft(
            "workspace.json is not an object".into(),
        ));
    };
    for list in ["epics", "templates", "attachments", "relations"] {
        root.entry(list).or_insert_with(|| Value::Array(Vec::new()));
    }
    if let Some(Value::Array(projects)) = root.get_mut("projects") {
        for p in projects.iter_mut().filter_map(Value::as_object_mut) {
            p.entry("workflow")
                .or_insert_with(|| Value::Array(Vec::new()));
            p.entry("estimate_unit")
                .or_insert_with(|| Value::String("hours".into()));
            p.entry("fields")
                .or_insert_with(|| Value::Array(Vec::new()));
        }
    }
    if let Some(Value::Array(tasks)) = root.get_mut("tasks") {
        for t in tasks.iter_mut().filter_map(Value::as_object_mut) {
            for key in ["stage", "estimate", "epic_id", "milestone_id"] {
                t.entry(key).or_insert(Value::Null);
            }
            t.entry("fields")
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
        }
    }
    Ok(data)
}

fn migrate(from: u32, mut data: Value) -> Result<Value, ExchangeError> {
    if from == 0 {
        return Err(ExchangeError::NotTaskcraft("version 0".into()));
    }
    let first = from as usize - 1;
    for step in MIGRATIONS.iter().skip(first) {
        data = step(data)?;
    }
    Ok(data)
}

/// Comprueba que el contenido sea consistente: sin IDs repetidos ni
/// referencias a entidades que no vienen en el archivo (RF-EXP-05).
pub fn validate(d: &Dump) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    for id in d
        .projects
        .iter()
        .map(|p| p.id.to_string())
        .chain(d.tasks.iter().map(|t| t.id.to_string()))
        .chain(d.labels.iter().map(|l| l.id.to_string()))
        .chain(d.agents.iter().map(|a| a.id.to_string()))
        .chain(d.comments.iter().map(|c| c.id.to_string()))
    {
        if !seen.insert(id.clone()) {
            problems.push(Problem {
                code: "duplicate-id",
                detail: format!("ID {id} appears twice"),
            });
        }
    }
    let projects: HashSet<_> = d.projects.iter().map(|p| p.id).collect();
    let tasks: HashSet<_> = d.tasks.iter().map(|t| t.id).collect();
    let labels: HashSet<_> = d.labels.iter().map(|l| l.id).collect();
    let agents: HashSet<_> = d.agents.iter().map(|a| a.id).collect();
    let interfaces: HashSet<_> = d.interfaces.iter().map(|i| i.id).collect();
    for a in &d.agents {
        if a.interface_id.is_some_and(|i| !interfaces.contains(&i)) {
            problems.push(Problem {
                code: "dangling-interface",
                detail: format!(
                    "agent {} belongs to an interface that is not in the file",
                    a.name
                ),
            });
        }
    }
    let mut keys = HashSet::new();
    for p in &d.projects {
        if !keys.insert(p.key.clone()) {
            problems.push(Problem {
                code: "duplicate-key",
                detail: format!("project key {} appears twice", p.key),
            });
        }
    }
    let mut numbers = HashSet::new();
    let actor_ok = |a: &model::Actor| match a {
        model::Actor::Agent(id) => agents.contains(id),
        _ => true,
    };
    for t in &d.tasks {
        if !projects.contains(&t.project_id) {
            problems.push(Problem {
                code: "dangling-project",
                detail: format!("task {} belongs to a project that is not in the file", t.id),
            });
        }
        if !numbers.insert((t.project_id, t.number)) {
            problems.push(Problem {
                code: "duplicate-key",
                detail: format!("task number {} appears twice in its project", t.number),
            });
        }
        if let Some(parent) = t.parent_id
            && !tasks.contains(&parent)
        {
            problems.push(Problem {
                code: "dangling-parent",
                detail: format!("task {} has a parent that is not in the file", t.id),
            });
        }
        if let Some(l) = t.labels.iter().find(|l| !labels.contains(l)) {
            problems.push(Problem {
                code: "dangling-label",
                detail: format!("task {} uses label {l}, which is not in the file", t.id),
            });
        }
        let claim_ok = t.claim.is_none_or(|c| agents.contains(&c.agent));
        let people_ok = t.assignees.iter().all(actor_ok);
        if !actor_ok(&t.creator) || !people_ok || !claim_ok {
            problems.push(Problem {
                code: "dangling-agent",
                detail: format!("task {} refers to an agent that is not in the file", t.id),
            });
        }
    }
    for dep in &d.dependencies {
        if !tasks.contains(&dep.blocker) || !tasks.contains(&dep.blocked) {
            problems.push(Problem {
                code: "dangling-dependency",
                detail: format!(
                    "dependency {} -> {} refers to missing tasks",
                    dep.blocker, dep.blocked
                ),
            });
        }
    }
    for c in &d.comments {
        if !tasks.contains(&c.task_id) || !actor_ok(&c.author) {
            problems.push(Problem {
                code: "dangling-comment",
                detail: format!("comment {} refers to a missing task or agent", c.id),
            });
        }
    }
    let epics: HashSet<_> = d.epics.iter().map(|e| e.id).collect();
    for e in &d.epics {
        if !projects.contains(&e.project_id) {
            problems.push(Problem {
                code: "dangling-project",
                detail: format!("epic {} belongs to a project that is not in the file", e.id),
            });
        }
    }
    for t in &d.tasks {
        if [t.epic_id, t.milestone_id]
            .into_iter()
            .flatten()
            .any(|g| !epics.contains(&g))
        {
            problems.push(Problem {
                code: "dangling-epic",
                detail: format!("task {} refers to an epic that is not in the file", t.id),
            });
        }
    }
    for rel in &d.relations {
        if !tasks.contains(&rel.a) || !tasks.contains(&rel.b) {
            problems.push(Problem {
                code: "dangling-relation",
                detail: format!("relation {} ~ {} refers to missing tasks", rel.a, rel.b),
            });
        }
    }
    for c in &d.costs {
        if !tasks.contains(&c.task_id) || !actor_ok(&c.actor) {
            problems.push(Problem {
                code: "dangling-cost",
                detail: format!("cost {} refers to a missing task or agent", c.id),
            });
        }
    }
    for t in &d.transitions {
        if !tasks.contains(&t.task_id) || !actor_ok(&t.actor) {
            problems.push(Problem {
                code: "dangling-transition",
                detail: format!("transition {} refers to a missing task or agent", t.id),
            });
        }
    }
    let cycles: HashSet<_> = d.cycles.iter().map(|c| c.id).collect();
    for c in &d.cycles {
        if !projects.contains(&c.project_id) {
            problems.push(Problem {
                code: "dangling-project",
                detail: format!(
                    "cycle {} belongs to a project that is not in the file",
                    c.id
                ),
            });
        }
    }
    let mut numbers = HashSet::new();
    for g in &d.diagrams {
        if !projects.contains(&g.project_id) {
            problems.push(Problem {
                code: "dangling-project",
                detail: format!(
                    "diagram {} belongs to a project that is not in the file",
                    g.id
                ),
            });
        }
        if !numbers.insert((g.project_id, g.number)) {
            problems.push(Problem {
                code: "duplicate-diagram-number",
                detail: format!(
                    "diagram {} repeats number D{} in its project",
                    g.id, g.number
                ),
            });
        }
    }
    for t in &d.tasks {
        if t.cycles.iter().any(|c| !cycles.contains(c)) {
            problems.push(Problem {
                code: "dangling-cycle",
                detail: format!("task {} refers to a cycle that is not in the file", t.id),
            });
        }
    }
    for l in &d.git_links {
        if !tasks.contains(&l.task_id) || !actor_ok(&l.actor) {
            problems.push(Problem {
                code: "dangling-git-link",
                detail: format!("git link {} refers to a missing task or agent", l.id),
            });
        }
    }
    for a in &d.attachments {
        if !tasks.contains(&a.task_id) || !d.blobs.contains_key(&a.sha256) {
            problems.push(Problem {
                code: "dangling-attachment",
                detail: format!("attachment {} lacks its task or its content", a.name),
            });
        }
    }
    problems
}

#[cfg(test)]
mod tests;
