//! Atajos de teclado configurables (RF-VIS-14): acción → combinación,
//! guardado en las preferencias de la app como `palette=Ctrl+K;…`.

use std::collections::BTreeMap;

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// Lo que se puede lanzar con un atajo. Esc, Supr y Tab dependen del
/// contexto y no se configuran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Palette,
    NewTask,
    Search,
    SaveView,
    MarkToday,
    Activity,
    ViewList,
    ViewBoard,
    ViewCalendar,
    ViewTable,
    ViewGantt,
    ViewToday,
    ViewMatrix,
    ViewGraph,
}

impl Action {
    pub const ALL: [Action; 14] = [
        Action::Palette,
        Action::NewTask,
        Action::Search,
        Action::SaveView,
        Action::MarkToday,
        Action::Activity,
        Action::ViewList,
        Action::ViewBoard,
        Action::ViewCalendar,
        Action::ViewTable,
        Action::ViewGantt,
        Action::ViewToday,
        Action::ViewMatrix,
        Action::ViewGraph,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Palette => "palette",
            Self::NewTask => "new_task",
            Self::Search => "search",
            Self::SaveView => "save_view",
            Self::MarkToday => "mark_today",
            Self::Activity => "activity",
            Self::ViewList => "view_list",
            Self::ViewBoard => "view_board",
            Self::ViewCalendar => "view_calendar",
            Self::ViewTable => "view_table",
            Self::ViewGantt => "view_gantt",
            Self::ViewToday => "view_today",
            Self::ViewMatrix => "view_matrix",
            Self::ViewGraph => "view_graph",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Palette => "ui-shortcut-palette",
            Self::NewTask => "ui-shortcut-new-task",
            Self::Search => "ui-shortcut-search",
            Self::SaveView => "ui-shortcut-save-view",
            Self::MarkToday => "ui-shortcut-mark-today",
            Self::Activity => "ui-shortcut-activity",
            Self::ViewList => "ui-view-list",
            Self::ViewBoard => "ui-view-board",
            Self::ViewCalendar => "ui-view-calendar",
            Self::ViewTable => "ui-view-table",
            Self::ViewGantt => "ui-view-gantt",
            Self::ViewToday => "ui-view-today",
            Self::ViewMatrix => "ui-view-matrix",
            Self::ViewGraph => "ui-view-graph",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.code() == code)
    }
}

/// Una combinación: Ctrl (⌘ en Mac), Mayús y Alt más una tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Shortcut {
    pub const fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            shift: false,
            alt: false,
            key,
        }
    }

    /// Desde lo que pulsó la persona.
    pub fn pressed(modifiers: Modifiers, key: Key) -> Self {
        Self {
            ctrl: modifiers.command || modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            key,
        }
    }

    pub fn text(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut out = Self::ctrl(Key::Escape);
        out.ctrl = false;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            match part.to_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => out.ctrl = true,
                "shift" | "mayús" => out.shift = true,
                "alt" => out.alt = true,
                _ => key = Key::from_name(part),
            }
        }
        out.key = key?;
        Some(out)
    }

    fn modifiers(self) -> Modifiers {
        Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.ctrl,
            ..Modifiers::NONE
        }
    }

    pub fn egui(self) -> KeyboardShortcut {
        KeyboardShortcut::new(self.modifiers(), self.key)
    }

    /// Cuántos modificadores lleva: los más específicos se prueban antes.
    pub fn weight(self) -> usize {
        usize::from(self.ctrl) + usize::from(self.shift) + usize::from(self.alt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap(BTreeMap<Action, Shortcut>);

impl Default for Keymap {
    fn default() -> Self {
        use Action as A;
        Self(BTreeMap::from([
            (A::Palette, Shortcut::ctrl(Key::K)),
            (A::NewTask, Shortcut::ctrl(Key::N)),
            (A::Search, Shortcut::ctrl(Key::F)),
            (A::SaveView, Shortcut::ctrl(Key::S)),
            (A::MarkToday, Shortcut::ctrl(Key::T)),
            (
                A::Activity,
                Shortcut {
                    shift: true,
                    ..Shortcut::ctrl(Key::A)
                },
            ),
            (A::ViewList, Shortcut::ctrl(Key::Num1)),
            (A::ViewBoard, Shortcut::ctrl(Key::Num2)),
            (A::ViewCalendar, Shortcut::ctrl(Key::Num3)),
            (A::ViewTable, Shortcut::ctrl(Key::Num4)),
            (A::ViewGantt, Shortcut::ctrl(Key::Num5)),
            (A::ViewToday, Shortcut::ctrl(Key::Num6)),
            (A::ViewMatrix, Shortcut::ctrl(Key::Num7)),
            (A::ViewGraph, Shortcut::ctrl(Key::Num8)),
        ]))
    }
}

impl Keymap {
    pub fn get(&self, action: Action) -> Option<Shortcut> {
        self.0.get(&action).copied()
    }

    /// `None` deja la acción sin atajo.
    pub fn set(&mut self, action: Action, shortcut: Option<Shortcut>) {
        match shortcut {
            Some(s) => self.0.insert(action, s),
            None => self.0.remove(&action),
        };
    }

    /// Para comprobar en orden: primero los que llevan más modificadores.
    pub fn ordered(&self) -> Vec<(Action, Shortcut)> {
        let mut all: Vec<_> = self.0.iter().map(|(a, s)| (*a, *s)).collect();
        all.sort_by_key(|(_, s)| std::cmp::Reverse(s.weight()));
        all
    }

    /// Acciones que comparten combinación con otra.
    pub fn conflicts(&self) -> Vec<Action> {
        self.0
            .iter()
            .filter(|(a, s)| self.0.iter().any(|(b, t)| a != &b && s == &t))
            .map(|(a, _)| *a)
            .collect()
    }

    /// Solo lo que difiere de los predeterminados; `-` = sin atajo.
    pub fn to_pref(&self) -> String {
        let default = Self::default();
        Action::ALL
            .into_iter()
            .filter(|a| self.get(*a) != default.get(*a))
            .map(|a| {
                let value = self.get(a).map_or_else(|| "-".to_owned(), Shortcut::text);
                format!("{}={value}", a.code())
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    pub fn from_pref(text: &str) -> Self {
        let mut map = Self::default();
        for (code, value) in text.split(';').filter_map(|p| p.split_once('=')) {
            let Some(action) = Action::from_code(code.trim()) else {
                continue;
            };
            match value.trim() {
                "-" => map.set(action, None),
                v => {
                    if let Some(s) = Shortcut::parse(v) {
                        map.set(action, Some(s));
                    }
                }
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_preferences_and_finds_conflicts() {
        let mut map = Keymap::default();
        assert_eq!(map.to_pref(), "");
        assert!(map.conflicts().is_empty());
        map.set(Action::Search, Shortcut::parse("Ctrl+Shift+P"));
        map.set(Action::Activity, None);
        let text = map.to_pref();
        assert_eq!(text, "search=Ctrl+Shift+P;activity=-");
        assert_eq!(Keymap::from_pref(&text), map);

        map.set(Action::ViewToday, Some(Shortcut::ctrl(Key::K)));
        let mut conflicts = map.conflicts();
        conflicts.sort();
        assert_eq!(conflicts, [Action::Palette, Action::ViewToday]);
        assert_eq!(
            map.ordered()[0].0,
            Action::Search,
            "más modificadores primero"
        );
        assert_eq!(Shortcut::parse("nada"), None);
    }
}
