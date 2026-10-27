//! L6 — Interfaz egui: lee el estado de una réplica y lo dibuja (D08).
//!
//! Nunca modifica los datos directamente: solo envía comandos, que el daemon
//! (o el engine en proceso) ejecuta con `Engine::dispatch` (RNF-02).

mod app;
mod conn;
mod data;
mod detail;
mod detail_extra;
mod dialogs;
mod dialogs_project;
mod flow;
mod hierarchy;
mod keymap;
mod layout;
mod palette;
mod state;
mod theme;
mod views;
mod widgets;

pub use app::{TaskCraftApp, VIEW_ENV, Workspaces, prefs};
pub use conn::Conn;
