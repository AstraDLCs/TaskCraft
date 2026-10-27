//! `Transport` del navegador: `fetch` con ehttp y el token en cada petición.

use serde_json::{Value, json};

use crate::api::{Inbox, Purpose, Reply, Transport, parse, push};

pub struct Fetch {
    pub base: String,
    pub token: String,
    pub lang: String,
    pub inbox: Inbox,
    pub repaint: egui::Context,
}

impl Fetch {
    fn send(&self, purpose: Purpose, mut request: ehttp::Request) {
        request
            .headers
            .insert("Authorization", format!("Bearer {}", self.token));
        request.headers.insert("Accept-Language", &self.lang);
        let inbox = self.inbox.clone();
        let ctx = self.repaint.clone();
        ehttp::fetch(request, move |response| {
            let result = match response {
                Ok(r) => match purpose {
                    Purpose::Changes | Purpose::WhoAmI if r.ok => {
                        serde_json::from_slice::<Value>(&r.bytes).map_err(|e| e.to_string())
                    }
                    _ => parse(r.status, r.text().unwrap_or_default()),
                },
                Err(e) => Err(e),
            };
            push(&inbox, Reply { purpose, result });
            ctx.request_repaint();
        });
    }
}

impl Transport for Fetch {
    fn command(&self, purpose: Purpose, command: &str, args: Value) {
        let body = json!({ "command": command, "args": args }).to_string();
        let mut request =
            ehttp::Request::post(format!("{}/api/command", self.base), body.into_bytes());
        request.headers.insert("Content-Type", "application/json");
        self.send(purpose, request);
    }

    fn changes(&self, since: Option<u64>) {
        let since = since.map(|s| s.to_string()).unwrap_or_default();
        let mut request = ehttp::Request::get(format!("{}/api/changes?since={since}", self.base));
        request.timeout = Some(std::time::Duration::from_secs(40));
        self.send(Purpose::Changes, request);
    }

    fn whoami(&self) {
        self.send(
            Purpose::WhoAmI,
            ehttp::Request::get(format!("{}/api/whoami", self.base)),
        );
    }
}
