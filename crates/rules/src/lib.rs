//! L3 — Reglas puras del dominio: transiciones de estado, tareas *listas*,
//! detección de ciclos, leases y selección de `task.next`.
//!
//! Sin IO ni estado propio: funciones sobre el modelo, fáciles de testear.

mod aging;
pub mod cost;
mod deps;
pub mod filter;
pub mod git;
pub mod graph;
pub mod matrix;
pub mod metrics;
mod project;
pub mod recurrence;
mod transitions;
mod validate;

pub use aging::{SOMEDAY, idle_weeks, is_stale};
pub use deps::{closed_statuses, find_cycle, is_ready};
pub use project::{
    column_owner, diagram_mentions, field_def, field_options, field_value, find_field,
    format_estimate, mentions, parse_estimate, resolve_status, slug, stage_key, workflow,
};
pub use transitions::{allowed_targets, check_transition};
pub use validate::{
    agent_name, comment_body, default_label_color, interface_name, label_color, label_name,
    project_key, task_title,
};

use i18n::{Localize, Msg};
use model::Status;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    ProjectKey(String),
    Title,
    Color(String),
    LabelName,
    AgentName(String),
    InterfaceName(String),
    CommentBody,
    Transition {
        from: Status,
        to: Status,
        allowed: String,
    },
    UnknownStatus {
        key: String,
        valid: String,
    },
    StageKey(String),
    StageExists(String),
    Estimate {
        value: String,
        unit: String,
    },
    FieldKey(String),
    FieldExists(String),
    FieldOptions,
    UnknownField {
        key: String,
        valid: String,
    },
    FieldValue {
        field: String,
        kind: String,
        value: String,
    },
    Filter {
        position: usize,
        expected: String,
    },
    Amount(String),
    Currency(String),
    Recurrence(String),
}

impl Localize for RuleError {
    fn msg(&self) -> Msg {
        match self {
            Self::ProjectKey(key) => Msg::new("rule-project-key").arg("key", key),
            Self::Title => Msg::new("rule-title"),
            Self::Color(color) => Msg::new("rule-color").arg("color", color),
            Self::LabelName => Msg::new("rule-label-name"),
            Self::AgentName(name) => Msg::new("rule-agent-name").arg("name", name),
            Self::InterfaceName(name) => Msg::new("rule-interface-name").arg("name", name),
            Self::CommentBody => Msg::new("rule-comment-body"),
            Self::Transition { from, to, allowed } => Msg::new("rule-transition")
                .arg("from", from.as_str())
                .arg("to", to.as_str())
                .arg("allowed", allowed),
            Self::UnknownStatus { key, valid } => Msg::new("rule-unknown-status")
                .arg("key", key)
                .arg("valid", valid),
            Self::StageKey(name) => Msg::new("rule-stage-key").arg("name", name),
            Self::StageExists(key) => Msg::new("rule-stage-exists").arg("key", key),
            Self::Estimate { value, unit } => Msg::new("rule-estimate")
                .arg("value", value)
                .arg("unit", unit),
            Self::FieldKey(name) => Msg::new("rule-field-key").arg("name", name),
            Self::FieldExists(key) => Msg::new("rule-field-exists").arg("key", key),
            Self::FieldOptions => Msg::new("rule-field-options"),
            Self::UnknownField { key, valid } => Msg::new("rule-unknown-field")
                .arg("key", key)
                .arg("valid", valid),
            Self::Amount(value) => Msg::new("rule-amount").arg("value", value),
            Self::Currency(value) => Msg::new("rule-currency").arg("value", value),
            Self::Recurrence(value) => Msg::new("rule-recurrence").arg("value", value),
            Self::Filter { position, expected } => Msg::new("rule-filter")
                .arg("position", *position)
                .arg("expected", expected),
            Self::FieldValue { field, kind, value } => Msg::new("rule-field-value")
                .arg("field", field)
                .arg("kind", kind)
                .arg("value", value),
        }
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.msg().fmt(f)
    }
}

impl std::error::Error for RuleError {}
