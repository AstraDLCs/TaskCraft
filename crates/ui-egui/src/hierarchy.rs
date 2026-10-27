//! Subtareas: una subtarea es una tarea con padre, así que convertir una en
//! otra es solo cambiar el padre (`task.set_parent`). Aquí se calcula a qué
//! padre iría cada movimiento y se dibujan los menús que lo ofrecen; el engine
//! sigue validando ciclos, proyecto y profundidad máxima.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, Msg, t, tr};
use model::TaskId;

use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState};

/// Payload al arrastrar una fila de la lista para cambiarle el padre.
#[derive(Clone, Debug)]
pub struct Reparent {
    pub id: TaskId,
    pub key: String,
}

/// `parent = None` la convierte en tarea principal.
pub fn set_parent(key: &str, parent: Option<&str>) -> Intent {
    Intent::run(
        "task.set_parent",
        &[("task", key), ("parent", parent.unwrap_or("none"))],
    )
}

fn position(snap: &Snapshot, id: TaskId) -> Option<usize> {
    snap.rows.iter().position(|r| r.id() == id)
}

pub fn row(snap: &Snapshot, id: TaskId) -> Option<&TaskRow> {
    snap.rows.iter().find(|r| r.id() == id)
}

/// La tarea y todos sus descendientes (las filas van en orden de árbol).
pub fn subtree(snap: &Snapshot, id: TaskId) -> HashSet<TaskId> {
    let Some(i) = position(snap, id) else {
        return HashSet::new();
    };
    let rows = snap.rows.get(i..).unwrap_or_default();
    let depth = rows.first().map_or(0, |r| r.depth);
    let mut out: HashSet<TaskId> = HashSet::from([id]);
    out.extend(
        rows.iter()
            .skip(1)
            .take_while(|r| r.depth > depth)
            .map(TaskRow::id),
    );
    out
}

fn ancestors(snap: &Snapshot, id: TaskId) -> HashSet<TaskId> {
    let by_id: HashMap<TaskId, &TaskRow> = snap.rows.iter().map(|r| (r.id(), r)).collect();
    let mut out = HashSet::new();
    let mut at = by_id.get(&id).and_then(|r| r.task.parent_id);
    while let Some(p) = at {
        if !out.insert(p) {
            break;
        }
        at = by_id.get(&p).and_then(|r| r.task.parent_id);
    }
    out
}

/// Sangrar (Tab): pasa a colgar de la hermana inmediatamente anterior.
pub fn indent_target(snap: &Snapshot, id: TaskId) -> Option<&TaskRow> {
    let i = position(snap, id)?;
    let me = snap.rows.get(i)?;
    snap.rows
        .get(..i)?
        .iter()
        .rev()
        .take_while(|r| r.depth >= me.depth)
        .find(|r| {
            r.depth == me.depth
                && r.task.parent_id == me.task.parent_id
                && r.task.project_id == me.task.project_id
        })
}

/// Subir un nivel (Mayús+Tab): pasa a colgar del abuelo. `Some(None)` = queda
/// como tarea principal; `None` = ya lo es.
pub fn outdent_target(snap: &Snapshot, id: TaskId) -> Option<Option<String>> {
    let parent = row(snap, id)?.task.parent_id?;
    Some(row(snap, parent).and_then(|p| p.parent_key.clone()))
}

fn matches(row: &TaskRow, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || row.key.to_lowercase().contains(&q)
        || row.task.title.to_lowercase().contains(&q)
}

/// Posibles padres: del mismo proyecto, sin ser ella, sus descendientes ni su padre actual.
pub fn parent_candidates<'a>(snap: &'a Snapshot, id: TaskId, query: &str) -> Vec<&'a TaskRow> {
    let Some(me) = row(snap, id) else {
        return Vec::new();
    };
    let banned = subtree(snap, id);
    snap.rows
        .iter()
        .filter(|r| {
            r.task.project_id == me.task.project_id
                && !banned.contains(&r.id())
                && Some(r.id()) != me.task.parent_id
                && matches(r, query)
        })
        .collect()
}

/// Tareas que se pueden adoptar como subtareas: del mismo proyecto, sin ser
/// ella, sus ancestros ni sus hijas actuales.
pub fn child_candidates<'a>(snap: &'a Snapshot, id: TaskId, query: &str) -> Vec<&'a TaskRow> {
    let Some(me) = row(snap, id) else {
        return Vec::new();
    };
    let banned = ancestors(snap, id);
    snap.rows
        .iter()
        .filter(|r| {
            r.task.project_id == me.task.project_id
                && r.id() != id
                && !banned.contains(&r.id())
                && r.task.parent_id != Some(id)
                && matches(r, query)
        })
        .collect()
}

/// Buscador y lista de tareas; devuelve la clave de la elegida.
pub fn task_picker(
    ui: &mut Ui,
    candidates: &[&TaskRow],
    query: &mut String,
    lang: Lang,
) -> Option<String> {
    ui.add(
        egui::TextEdit::singleline(query)
            .hint_text(t(lang, "ui-search-task"))
            .desired_width(300.0),
    );
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            if candidates.is_empty() {
                ui.weak(t(lang, "ui-no-matches"));
            }
            for r in candidates {
                let indent = "   ".repeat(r.depth);
                let mut text = RichText::new(format!("{indent}{}  {}", r.key, r.task.title));
                if r.is_closed() {
                    text = text.strikethrough().weak();
                }
                if ui
                    .add(egui::Button::selectable(false, text).truncate())
                    .clicked()
                {
                    chosen = Some(r.key.clone());
                }
            }
        });
    chosen
}

/// Opciones de jerarquía para el menú contextual de una tarea.
pub fn menu_items(
    ui: &mut Ui,
    snap: &Snapshot,
    row: &TaskRow,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let id = row.id();
    if let Some(prev) = indent_target(snap, id)
        && ui
            .button(tr(
                lang,
                &Msg::new("ui-make-subtask-of").arg("parent", &prev.key),
            ))
            .clicked()
    {
        out.push(set_parent(&row.key, Some(&prev.key)));
        ui.close();
    }
    if let Some(up) = outdent_target(snap, id) {
        if let Some(grand) = &up
            && ui.button(t(lang, "ui-outdent")).clicked()
        {
            out.push(set_parent(&row.key, Some(grand)));
            ui.close();
        }
        if ui.button(t(lang, "ui-make-root")).clicked() {
            out.push(set_parent(&row.key, None));
            ui.close();
        }
    }
    ui.menu_button(t(lang, "ui-move-under"), |ui| {
        let candidates = parent_candidates(snap, id, &st.parent_query);
        if let Some(parent) = task_picker(ui, &candidates, &mut st.parent_query, lang) {
            out.push(set_parent(&row.key, Some(&parent)));
            st.parent_query.clear();
            ui.close();
        }
    });
}

/// Menú contextual completo de una tarea (lista, tablero y calendario).
pub fn context_menu(
    ui: &mut Ui,
    snap: &Snapshot,
    row: &TaskRow,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.label(RichText::new(&row.key).monospace().weak());
    if ui.button(t(lang, "ui-open")).clicked() {
        out.push(Intent::Select(Some(row.id())));
        ui.close();
    }
    // En la lista la subtarea se escribe bajo la fila; en las demás vistas, en el detalle.
    if ui.button(t(lang, "ui-add-subtask")).clicked() {
        out.push(Intent::Select(Some(row.id())));
        if st.view == crate::state::ViewKind::List {
            st.add_child = Some((row.id(), String::new()));
            st.collapsed.remove(&row.id());
        } else {
            st.focus_subtask = true;
        }
        ui.close();
    }
    if !row.is_closed() {
        let text = if snap.today.contains(&row.id()) {
            "ui-today-unmark"
        } else {
            "ui-today-mark"
        };
        if ui.button(t(lang, text)).clicked() {
            out.push(crate::views::today::toggle(snap, row));
            ui.close();
        }
    }
    if !crate::views::common::cycles_for(snap, &row.task).is_empty() {
        ui.menu_button(t(lang, "ui-cycles-plan"), |ui| {
            crate::views::common::cycle_checks(ui, &row.task, &row.key, snap, out);
        });
    }
    if row.stale && ui.button(t(lang, "ui-someday-move")).clicked() {
        out.push(Intent::run("task.someday", &[("task", &row.key)]));
        ui.close();
    }
    ui.separator();
    menu_items(ui, snap, row, st, out);
    ui.separator();
    if ui.button(t(lang, "ui-delete-task")).clicked() {
        out.push(crate::views::list::delete_intent(&row.key));
        ui.close();
    }
}

/// ¿Se está soltando una fila arrastrada sobre `rect`? Devuelve si pasa por
/// encima y, al soltar, lo soltado. Se mira el puntero y no la respuesta del
/// widget porque los hijos de la fila (botones, enlaces) taparían la zona.
pub fn drop_on(ui: &Ui, rect: egui::Rect) -> (bool, Option<std::sync::Arc<Reparent>>) {
    let ctx = ui.ctx();
    if !egui::DragAndDrop::has_payload_of_type::<Reparent>(ctx) {
        return (false, None);
    }
    let over = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| rect.contains(p) && ui.clip_rect().contains(p));
    if !over {
        return (false, None);
    }
    let released = ui
        .input(|i| i.pointer.any_released())
        .then(|| egui::DragAndDrop::take_payload::<Reparent>(ctx))
        .flatten();
    (true, released)
}

#[cfg(test)]
mod tests {
    use engine::{Args, Context, Engine};
    use store_sqlite::SqliteStore;

    use super::*;
    use crate::data::{Load, Scope, load};

    /// API-1 > API-2 > API-3, API-4 y API-5 sueltas.
    fn snap() -> Snapshot {
        let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
        let sup = Context::supervisor();
        engine
            .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
            .unwrap();
        for parent in [None, Some("API-1"), Some("API-2"), None, None] {
            let mut args = Args::none().with("project", "API").with("title", "t");
            if let Some(p) = parent {
                args = args.with("parent", p);
            }
            engine.dispatch(&sup, "task.create", &args).unwrap();
        }
        load(
            &mut crate::Conn::local(engine),
            &Load {
                scope: Scope::All,
                include_archived: false,
                activity_limit: 0,
                activity_filter: Default::default(),
            },
        )
        .unwrap()
    }

    fn id(snap: &Snapshot, key: &str) -> TaskId {
        snap.rows
            .iter()
            .find(|r| r.key == key)
            .map(TaskRow::id)
            .unwrap()
    }

    fn keys(rows: &[&TaskRow]) -> Vec<String> {
        rows.iter().map(|r| r.key.clone()).collect()
    }

    #[test]
    fn indent_hangs_from_the_previous_sibling() {
        let s = snap();
        assert_eq!(indent_target(&s, id(&s, "API-5")).unwrap().key, "API-4");
        assert_eq!(indent_target(&s, id(&s, "API-4")).unwrap().key, "API-1");
        // La primera de su nivel no tiene hermana anterior.
        assert!(indent_target(&s, id(&s, "API-1")).is_none());
        assert!(indent_target(&s, id(&s, "API-2")).is_none());
    }

    #[test]
    fn outdent_goes_to_the_grandparent_or_root() {
        let s = snap();
        assert_eq!(
            outdent_target(&s, id(&s, "API-3")),
            Some(Some("API-1".into()))
        );
        assert_eq!(outdent_target(&s, id(&s, "API-2")), Some(None));
        assert_eq!(outdent_target(&s, id(&s, "API-1")), None);
    }

    #[test]
    fn a_task_cannot_move_under_itself_or_its_descendants() {
        let s = snap();
        assert_eq!(
            keys(&parent_candidates(&s, id(&s, "API-1"), "")),
            ["API-4", "API-5"]
        );
        // API-2 ya cuelga de API-1: no se ofrece de nuevo.
        assert_eq!(
            keys(&parent_candidates(&s, id(&s, "API-2"), "")),
            ["API-4", "API-5"]
        );
        assert_eq!(
            keys(&parent_candidates(&s, id(&s, "API-4"), "5")),
            ["API-5"]
        );
    }

    #[test]
    fn ancestors_and_current_children_cannot_be_adopted() {
        let s = snap();
        assert_eq!(
            keys(&child_candidates(&s, id(&s, "API-3"), "")),
            ["API-4", "API-5"]
        );
        assert_eq!(
            keys(&child_candidates(&s, id(&s, "API-1"), "")),
            ["API-3", "API-4", "API-5"]
        );
    }
}
