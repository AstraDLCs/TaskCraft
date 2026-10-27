//! Shell web (D14): wasm32 + eframe `WebRunner` sobre un `<canvas>`, contra
//! la API JSON del daemon (`serve --http … --web dist/web`). Se construye con
//! `cargo xtask web`.

// Fuera del navegador solo se compila para clippy y los tests de la UI.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod api;
mod app;
#[cfg(target_arch = "wasm32")]
mod net;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "taskcraft-web es la app del navegador: cargo xtask web y taskcraft-cli serve --http … --web dist/web"
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    const TOKEN: &str = "taskcraft.web_token";
    const LANG: &str = "taskcraft.lang";

    wasm_bindgen_futures::spawn_local(async {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let canvas = document
            .get_element_by_id("taskcraft")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else { return };
        // Mismo origen que el daemon que sirve la página.
        let base = window.location().origin().unwrap_or_default();
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let inbox = api::Inbox::default();
                    let ctx = cc.egui_ctx.clone();
                    let saved = |key: &str| cc.storage.and_then(|s| s.get_string(key));
                    let lang = saved(LANG)
                        .as_deref()
                        .and_then(i18n::Lang::from_tag)
                        .unwrap_or(i18n::Lang::Es);
                    let lang_code = lang.code().to_owned();
                    let net_inbox = inbox.clone();
                    let connect: app::Connect = Box::new(move |token| {
                        Box::new(net::Fetch {
                            base: base.clone(),
                            token: token.to_owned(),
                            lang: lang_code.clone(),
                            inbox: net_inbox.clone(),
                            repaint: ctx.clone(),
                        })
                    });
                    let mut web = app::WebApp::new(connect, inbox, lang);
                    if let Some(token) = saved(TOKEN) {
                        web.token_input = token;
                        web.remember = true;
                        web.login();
                    }
                    Ok(Box::new(Shell(web)))
                }),
            )
            .await;
        if let Err(e) = started {
            web_sys::console::error_1(&e);
        }
    });

    /// `eframe::App` sobre la app; guarda el token solo si se pidió.
    struct Shell(app::WebApp);

    impl eframe::App for Shell {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.0.ui(ui);
        }

        fn save(&mut self, storage: &mut dyn eframe::Storage) {
            storage.set_string(LANG, self.0.lang.code().to_owned());
            let token = if self.0.remember && self.0.logged_in() {
                self.0.token_input.clone()
            } else {
                String::new()
            };
            storage.set_string(TOKEN, token);
        }
    }
}
