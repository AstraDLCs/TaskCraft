//! Estado de la interfaz (qué vista, qué filtros, qué está seleccionado) e
//! intents: lo que el usuario pidió durante el frame. Los intents se ejecutan
//! al final, siempre a través del engine (RNF-02).

use std::collections::{HashMap, HashSet};

use engine::{Args, Param};
use i18n::{Lang, Msg};
use jiff::civil::Date;
use model::{Actor, InboxId, Priority, ProjectId, Status, TaskId};

use crate::data::{Scope, Snapshot, TaskRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    List,
    Board,
    Calendar,
    Table,
    Gantt,
    Today,
    Matrix,
    Graph,
    Review,
    Inbox,
    Agents,
    Trash,
    Cycles,
    Epics,
    Diagrams,
    Metrics,
}

impl ViewKind {
    pub const TASK_VIEWS: [ViewKind; 8] = [
        ViewKind::List,
        ViewKind::Board,
        ViewKind::Calendar,
        ViewKind::Table,
        ViewKind::Gantt,
        ViewKind::Today,
        ViewKind::Matrix,
        ViewKind::Graph,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Board => "board",
            Self::Calendar => "calendar",
            Self::Table => "table",
            Self::Gantt => "gantt",
            Self::Today => "today",
            Self::Matrix => "matrix",
            Self::Graph => "graph",
            Self::Review => "review",
            Self::Inbox => "inbox",
            Self::Agents => "agents",
            Self::Trash => "trash",
            Self::Cycles => "cycles",
            Self::Epics => "epics",
            Self::Diagrams => "diagrams",
            Self::Metrics => "metrics",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        [
            Self::List,
            Self::Board,
            Self::Calendar,
            Self::Table,
            Self::Gantt,
            Self::Today,
            Self::Matrix,
            Self::Graph,
            Self::Review,
            Self::Inbox,
            Self::Agents,
            Self::Trash,
            Self::Cycles,
            Self::Epics,
            Self::Diagrams,
            Self::Metrics,
        ]
        .into_iter()
        .find(|v| v.code() == s.trim().to_lowercase())
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::List => "ui-view-list",
            Self::Board => "ui-view-board",
            Self::Calendar => "ui-view-calendar",
            Self::Table => "ui-view-table",
            Self::Gantt => "ui-view-gantt",
            Self::Today => "ui-view-today",
            Self::Matrix => "ui-view-matrix",
            Self::Graph => "ui-view-graph",
            Self::Review => "ui-view-review",
            Self::Inbox => "ui-view-inbox",
            Self::Agents => "ui-view-agents",
            Self::Trash => "ui-view-trash",
            Self::Cycles => "ui-view-cycles",
            Self::Epics => "ui-view-epics",
            Self::Diagrams => "ui-view-diagrams",
            Self::Metrics => "ui-view-metrics",
        }
    }
}

/// Carriles del tablero (RF-VIS-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lanes {
    None,
    Assignee,
    Priority,
    Project,
    Epic,
}

impl Lanes {
    pub const ALL: [Lanes; 5] = [
        Lanes::None,
        Lanes::Assignee,
        Lanes::Priority,
        Lanes::Project,
        Lanes::Epic,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Assignee => "assignee",
            Self::Priority => "priority",
            Self::Project => "project",
            Self::Epic => "epic",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code() == s)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::None => "ui-lanes-none",
            Self::Assignee => "ui-lanes-assignee",
            Self::Priority => "ui-lanes-priority",
            Self::Project => "ui-lanes-project",
            Self::Epic => "ui-lanes-epic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssigneeFilter {
    Unassigned,
    Actor(Actor),
}

/// El texto del buscador ya interpretado (D12), para no leerlo en cada fila.
#[derive(Debug, Clone)]
pub(crate) struct Parsed {
    text: String,
    today: Date,
    query: Result<Option<rules::filter::Expr>, rules::filter::FilterError>,
}

/// ¿Usa el lenguaje de filtros (campos, OR, NOT) o es texto suelto?
fn structured(e: &rules::filter::Expr) -> bool {
    use rules::filter::Expr;
    match e {
        Expr::Text(_) => false,
        Expr::And(parts) => parts.iter().any(structured),
        _ => true,
    }
}

/// Filtros comunes a lista, tablero y calendario (RF-VIS-11). El buscador
/// acepta también el lenguaje de filtros (RF-VIS-12, D12).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub text: String,
    pub(crate) parsed: std::cell::RefCell<Option<Parsed>>,
    pub statuses: HashSet<Status>,
    pub priority: Option<Priority>,
    pub label: Option<String>,
    pub assignee: Option<AssigneeFilter>,
    /// Solo las de esta épica o hito (RF-MOD-13).
    pub epic: Option<model::EpicId>,
    /// Solo las de este ciclo o sprint (RF-INT-05).
    pub cycle: Option<model::CycleId>,
    /// Las tareas terminadas o canceladas se ven (tachadas) salvo que se pida ocultarlas.
    pub hide_closed: bool,
}

impl Filter {
    /// Filtros de los desplegables en uso (sin contar el texto).
    pub fn picked(&self) -> usize {
        [
            !self.statuses.is_empty(),
            self.priority.is_some(),
            self.label.is_some(),
            self.assignee.is_some(),
            self.epic.is_some(),
            self.cycle.is_some(),
        ]
        .into_iter()
        .filter(|on| *on)
        .count()
    }

    pub fn is_active(&self) -> bool {
        !self.text.trim().is_empty()
            || !self.statuses.is_empty()
            || self.priority.is_some()
            || self.label.is_some()
            || self.assignee.is_some()
            || self.epic.is_some()
            || self.cycle.is_some()
    }

    /// El filtro D12 del buscador, si lo es; `Err` si está mal escrito.
    pub fn query(&self) -> Result<Option<rules::filter::Expr>, rules::filter::FilterError> {
        let text = self.text.trim();
        let mut cache = self.parsed.borrow_mut();
        let today = rules::filter::today();
        if let Some(p) = cache.as_ref()
            && p.text == text
            && p.today == today
        {
            return p.query.clone();
        }
        let query = rules::filter::parse(text).map(|e| structured(&e).then_some(e));
        *cache = Some(Parsed {
            text: text.to_owned(),
            today,
            query: query.clone(),
        });
        query
    }

    /// Si el buscador lleva un filtro D12, la búsqueda de texto completo sobra.
    pub fn structured(&self) -> bool {
        matches!(self.query(), Ok(Some(_)))
    }

    /// `snap.search`: tareas que la búsqueda de texto completo encontró
    /// (también en comentarios, RF-VIS-11) para el texto actual del filtro.
    pub fn matches(&self, row: &TaskRow, snap: &Snapshot) -> bool {
        let hits = &snap.search.1;
        let t = &row.task;
        if self.hide_closed && row.is_closed() && self.statuses.is_empty() {
            return false;
        }
        if !self.statuses.is_empty() && !self.statuses.contains(&t.status) {
            return false;
        }
        if self.priority.is_some_and(|p| p != t.priority) {
            return false;
        }
        if let Some(label) = &self.label
            && !row.labels.iter().any(|(n, _)| n == label)
        {
            return false;
        }
        if let Some(e) = self.epic
            && t.epic_id != Some(e)
            && t.milestone_id != Some(e)
        {
            return false;
        }
        if self.cycle.is_some_and(|c| !t.in_cycle(c)) {
            return false;
        }
        match &self.assignee {
            Some(AssigneeFilter::Unassigned) if !t.assignees.is_empty() => return false,
            Some(AssigneeFilter::Actor(a)) if !t.is_assigned(a) => return false,
            _ => {}
        }
        if let Ok(Some(expr)) = self.query() {
            let today = self
                .parsed
                .borrow()
                .as_ref()
                .map_or_else(rules::filter::today, |p| p.today);
            let extra = rules::filter::Extra {
                key: &row.key,
                parent: row.parent_key.as_deref(),
                ready: row.ready,
                locked: snap.locked.contains(&row.id()),
                today: snap.today.contains(&row.id()),
                stale: row.stale,
            };
            return rules::filter::matches(&expr, &snap.names.facts(t, extra), today);
        }
        let text = self.text.trim().to_lowercase();
        text.is_empty()
            || hits.contains(&row.id())
            || row.key.to_lowercase().contains(&text)
            || t.title.to_lowercase().contains(&text)
            || t.description.to_lowercase().contains(&text)
    }
}

/// Tipo de evento del feed de actividad (RF-SUP-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Created,
    Updated,
    Deleted,
    Links,
    Comments,
}

impl EventKind {
    pub const ALL: [EventKind; 5] = [
        EventKind::Created,
        EventKind::Updated,
        EventKind::Deleted,
        EventKind::Links,
        EventKind::Comments,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Created => "ui-event-created",
            Self::Updated => "ui-event-updated",
            Self::Deleted => "ui-event-deleted",
            Self::Links => "ui-event-links",
            Self::Comments => "ui-event-comments",
        }
    }

    /// Restringe un filtro del historial a este tipo de evento.
    pub fn apply(self, filter: &mut store::ActivityFilter) {
        use model::{Action, EntityKind};
        match self {
            Self::Created => filter.actions = vec![Action::Created],
            Self::Updated => filter.actions = vec![Action::Updated],
            Self::Deleted => filter.actions = vec![Action::Deleted],
            Self::Links => filter.actions = vec![Action::Linked, Action::Unlinked],
            Self::Comments => filter.entity = Some(EntityKind::Comment),
        }
    }
}

/// Filtros del feed de actividad (RF-SUP-02): se aplican al leer el historial.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityFilter {
    pub actor: Option<Actor>,
    pub project: Option<ProjectId>,
    pub kind: Option<EventKind>,
}

impl ActivityFilter {
    pub fn is_active(&self) -> bool {
        self.actor.is_some() || self.project.is_some() || self.kind.is_some()
    }

    pub fn to_store(&self, limit: usize) -> store::ActivityFilter {
        let mut f = store::ActivityFilter {
            actor: self.actor.clone(),
            project: self.project,
            limit,
            ..store::ActivityFilter::default()
        };
        if let Some(kind) = self.kind {
            kind.apply(&mut f);
        }
        f
    }
}

/// Filas visibles en la lista: las que cumplen el filtro y sus ancestros (para
/// no perder el contexto del árbol), sin los descendientes de las plegadas.
pub fn list_rows<'a>(snap: &'a Snapshot, st: &UiState) -> Vec<(&'a TaskRow, bool)> {
    let by_id: HashMap<TaskId, &TaskRow> = snap.rows.iter().map(|r| (r.id(), r)).collect();
    let mut keep: HashSet<TaskId> = HashSet::new();
    let mut direct: HashSet<TaskId> = HashSet::new();
    for row in snap.rows.iter().filter(|r| st.filter.matches(r, snap)) {
        direct.insert(row.id());
        let mut at = Some(row);
        while let Some(r) = at {
            if !keep.insert(r.id()) {
                break;
            }
            at = r.task.parent_id.and_then(|p| by_id.get(&p).copied());
        }
    }
    let mut out = Vec::new();
    let mut hidden_below: Option<usize> = None;
    for row in &snap.rows {
        if let Some(depth) = hidden_below {
            if row.depth > depth {
                continue;
            }
            hidden_below = None;
        }
        if !keep.contains(&row.id()) {
            continue;
        }
        if st.collapsed.contains(&row.id()) {
            hidden_below = Some(row.depth);
        }
        out.push((row, direct.contains(&row.id())));
    }
    out
}

/// Filas para tablero y calendario: las que cumplen el filtro, sin árbol.
pub fn flat_rows<'a>(snap: &'a Snapshot, st: &UiState, roots_only: bool) -> Vec<&'a TaskRow> {
    snap.rows
        .iter()
        .filter(|r| st.filter.matches(r, snap) && (!roots_only || r.depth == 0))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Checklist,
    Criteria,
}

/// Textos que se están editando en el panel de detalle.
#[derive(Debug)]
pub struct DetailEdit {
    pub task: Option<TaskId>,
    pub title: String,
    pub description: String,
    pub editing_description: bool,
    pub comment: String,
    pub new_check: String,
    pub new_criterion: String,
    pub new_subtask: String,
    pub new_label: String,
    pub editing_item: Option<(ItemKind, usize, String)>,
    pub release_reason: String,
    pub answer: String,
    /// Estimación tal como se escribe (`None` = aún no se ha leído de la tarea).
    pub estimate: Option<String>,
    /// Campos de texto propios mientras se editan.
    pub field_values: HashMap<String, String>,
    pub relate_kind: model::RelationKind,
    pub attach_path: String,
    pub log_minutes: u32,
    pub log_note: String,
    /// Regla de repetición mientras se edita (`None` = aún no se leyó).
    pub recurrence: Option<RecurrenceEdit>,
}

/// Editor de la regla de repetición (RF-MOD-18, D17).
#[derive(Debug, Clone, PartialEq)]
pub struct RecurrenceEdit {
    /// 0 = no se repite, 1 = por calendario, 2 = tras terminarla.
    pub mode: u8,
    pub unit: model::RepeatUnit,
    pub interval: u16,
    /// Lunes … domingo.
    pub weekdays: [bool; 7],
    /// 0 = el mismo día del mes que el vencimiento.
    pub month_day: u8,
    pub after_days: u16,
}

impl RecurrenceEdit {
    pub fn of(rule: Option<&model::Recurrence>) -> Self {
        let mut edit = Self {
            mode: 0,
            unit: model::RepeatUnit::Week,
            interval: 1,
            weekdays: [false; 7],
            month_day: 0,
            after_days: 7,
        };
        match rule {
            Some(model::Recurrence::Every {
                unit,
                interval,
                weekdays,
                month_day,
            }) => {
                edit.mode = 1;
                edit.unit = *unit;
                edit.interval = *interval;
                for d in weekdays {
                    if let Some(slot) = edit.weekdays.get_mut(usize::from(*d).saturating_sub(1)) {
                        *slot = true;
                    }
                }
                edit.month_day = month_day.unwrap_or(0);
            }
            Some(model::Recurrence::After { days }) => {
                edit.mode = 2;
                edit.after_days = *days;
            }
            None => {}
        }
        edit
    }

    /// Argumentos de `task.set_recurrence`, o `None` para quitarla.
    pub fn args(&self) -> Option<Vec<(&'static str, String)>> {
        match self.mode {
            1 => {
                let mut out = vec![
                    ("every", self.unit.as_str().to_owned()),
                    ("interval", self.interval.to_string()),
                ];
                let on: Vec<&str> = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
                    .iter()
                    .zip(self.weekdays)
                    .filter(|(_, on)| *on)
                    .map(|(d, _)| *d)
                    .collect();
                if self.unit == model::RepeatUnit::Week && !on.is_empty() {
                    out.push(("on", on.join(",")));
                }
                if self.unit == model::RepeatUnit::Month && self.month_day > 0 {
                    out.push(("day", self.month_day.to_string()));
                }
                Some(out)
            }
            2 => Some(vec![("after", format!("{}d", self.after_days))]),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SettingsForm {
    pub max_depth: u32,
    pub trash_days: u32,
    pub lease_minutes: u32,
    pub agent_wip_limit: u32,
    pub auto_archive_days: u32,
    pub backup_hours: u32,
    pub backup_keep: u32,
}

/// Cambiar de workspace o crear uno (RF-ALM-08).
#[derive(Debug, Clone, Default)]
pub struct WorkspaceForm {
    pub current: String,
    pub names: Vec<String>,
    pub new_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectForm {
    /// `None` = proyecto nuevo.
    pub editing: Option<ProjectId>,
    pub key: String,
    pub name: String,
    pub description: String,
    pub estimate_unit: Option<model::EstimateUnit>,
}

impl ProjectForm {
    pub fn edit(p: &model::Project) -> Self {
        Self {
            editing: Some(p.id),
            key: p.key.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            estimate_unit: Some(p.estimate_unit),
        }
    }
}

/// Flujo (D10) y campos propios (RF-MOD-15) de un proyecto. Cada cambio es
/// un comando inmediato; el diálogo se dibuja con lo que hay en la réplica.
#[derive(Debug, Clone)]
pub struct WorkflowForm {
    pub project: ProjectId,
    pub new_stage: String,
    pub new_base: Status,
    /// Agente dueño de la columna nueva (D20).
    pub new_owner: Option<model::AgentId>,
    pub renames: HashMap<String, String>,
    pub new_field: String,
    pub new_kind: model::FieldKind,
    pub new_options: String,
}

impl WorkflowForm {
    pub fn new(project: ProjectId) -> Self {
        Self {
            project,
            new_stage: String::new(),
            new_base: Status::InProgress,
            new_owner: None,
            renames: HashMap::new(),
            new_field: String::new(),
            new_kind: model::FieldKind::Text,
            new_options: String::new(),
        }
    }
}

/// Instanciar una plantilla (RF-MOD-16).
#[derive(Debug, Clone, Default)]
pub struct TemplateForm {
    pub template: Option<String>,
    pub project_key: String,
    pub parent: String,
    /// Clave del proyecto nuevo (plantillas de proyecto).
    pub new_key: String,
    /// Guardar la tarea seleccionada como plantilla con este nombre.
    pub save_name: String,
}

/// Alta o edición de un agente. Un agente vive siempre en una interfaz, que
/// es la que tiene el token (D15, D22).
#[derive(Debug, Clone, Default)]
pub struct AgentForm {
    /// `Some(nombre)` = editar ese agente; `None` = crear uno nuevo.
    pub editing: Option<String>,
    pub name: String,
    pub model: String,
    pub description: String,
    pub color: [u8; 3],
    /// Permisos (D11); `None` = el nivel por defecto (planificar).
    pub level: Option<model::AgentLevel>,
    /// Claves de los proyectos de su alcance; vacío = todos.
    pub scope: Vec<String>,
    /// Capacidades separadas por comas.
    pub capabilities: String,
    /// Nombre de su interfaz. Al editar se puede cambiar (mover el agente);
    /// vacío solo en agentes antiguos sin interfaz.
    pub interface: String,
}

impl AgentForm {
    /// Un agente nuevo en `interface`.
    pub fn new(interface: &str) -> Self {
        Self {
            color: [0x6e, 0x56, 0xcf],
            interface: interface.to_owned(),
            ..Self::default()
        }
    }

    pub fn edit(a: &model::Agent, snap: &Snapshot) -> Self {
        let c = crate::theme::parse_hex(&a.color);
        Self {
            editing: Some(a.name.clone()),
            name: a.name.clone(),
            model: a.model.clone(),
            description: a.description.clone(),
            color: [c.r(), c.g(), c.b()],
            level: Some(a.level),
            scope: a
                .scope
                .iter()
                .filter_map(|p| snap.project(*p).map(|p| p.key.clone()))
                .collect(),
            capabilities: a.capabilities.join(", "),
            interface: a
                .interface_id
                .and_then(|id| snap.interfaces.iter().find(|i| i.id == id))
                .map(|i| i.name.clone())
                .unwrap_or_default(),
        }
    }

    /// Comando que lo guarda: `agent.create` en su interfaz o `agent.update`.
    pub fn command(&self) -> (&'static str, Args) {
        let level = self.level.unwrap_or(model::AgentLevel::Plan);
        let args = Args::none()
            .with("model", self.model.trim())
            .with("description", self.description.as_str())
            .with("color", crate::theme::to_hex(self.color))
            .with("level", level.as_str())
            .with("scope", self.scope.join(","))
            .with("capabilities", self.capabilities.trim())
            .with("interface", self.interface.as_str());
        match &self.editing {
            None => ("agent.create", args.with("name", self.name.trim())),
            Some(name) => ("agent.update", args.with("agent", name.as_str())),
        }
    }
}

/// Alta o edición de una interfaz (cliente LLM), la única con token (D15, D22).
#[derive(Debug, Clone)]
pub struct InterfaceForm {
    /// `Some(nombre)` = editar esa interfaz; `None` = registrar una nueva.
    pub editing: Option<String>,
    pub name: String,
    pub kind: String,
    pub description: String,
    pub color: [u8; 3],
    pub level: model::AgentLevel,
    /// Claves de los proyectos de su alcance; vacío = todos.
    pub scope: Vec<String>,
    pub max_agents: u32,
}

impl Default for InterfaceForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            kind: "claude-code".into(),
            description: String::new(),
            color: [0x3b, 0x82, 0xf6],
            level: model::AgentLevel::Admin,
            scope: Vec::new(),
            max_agents: 50,
        }
    }
}

impl InterfaceForm {
    pub fn edit(i: &model::Interface, snap: &Snapshot) -> Self {
        let c = crate::theme::parse_hex(&i.color);
        Self {
            editing: Some(i.name.clone()),
            name: i.name.clone(),
            kind: i.kind.clone(),
            description: i.description.clone(),
            color: [c.r(), c.g(), c.b()],
            level: i.level,
            scope: i
                .scope
                .iter()
                .filter_map(|p| snap.project(*p).map(|p| p.key.clone()))
                .collect(),
            max_agents: i.max_agents,
        }
    }

    /// Alta (emite el token) o `interface.update`.
    pub fn intent(&self) -> Intent {
        let args = Args::none()
            .with("name", self.name.trim())
            .with("kind", self.kind.trim())
            .with("description", self.description.as_str())
            .with("color", crate::theme::to_hex(self.color))
            .with("level", self.level.as_str())
            .with("scope", self.scope.join(","))
            .with("max_agents", self.max_agents.to_string());
        match &self.editing {
            None => Intent::IssueToken {
                id: "interface.register",
                args,
            },
            Some(name) => Intent::Submit {
                id: "interface.update",
                args: args.with("interface", name.as_str()),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LabelsForm {
    /// (nombre original, nombre nuevo, color)
    pub rows: Vec<(String, String, [u8; 3])>,
    pub new_name: String,
    pub new_color: [u8; 3],
}

/// Exportar o importar un `.taskcraft` (RF-EXP-09).
#[derive(Debug, Clone)]
pub struct ExchangeForm {
    pub path: String,
    /// `workspace` (vacío), `project` (proyectos nuevos) o `merge` (fusión).
    pub mode: &'static str,
    /// Exportar solo el proyecto que se está viendo.
    pub only_current: bool,
    /// Respuesta de la última vista previa, exportación o importación.
    pub result: Option<Result<serde_json::Value, String>>,
    /// La última respuesta fue una importación ya hecha (no una vista previa).
    pub imported: bool,
}

impl Default for ExchangeForm {
    fn default() -> Self {
        Self {
            path: String::new(),
            mode: "workspace",
            only_current: false,
            result: None,
            imported: false,
        }
    }
}

impl ExchangeForm {
    /// `~/taskcraft-AAAA-MM-DD.taskcraft`.
    pub fn suggested(today: jiff::civil::Date) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            path: format!("{home}/taskcraft-{today}.taskcraft"),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum Dialog {
    /// Token recién emitido para una interfaz, con Copiar (D09, D22).
    Token {
        interface: String,
        token: String,
    },
    /// Resumen desde la última visita (RF-SUP-09).
    Summary(serde_json::Value),
    Workflow(WorkflowForm),
    Templates(TemplateForm),
    Export(ExchangeForm),
    Import(ExchangeForm),
    Settings(SettingsForm),
    Project(ProjectForm),
    Labels(LabelsForm),
    Agent(AgentForm),
    Interface(InterfaceForm),
    Shortcuts,
    /// Nombre con el que guardar el filtro actual como vista (RF-VIS-12).
    SaveView(String),
    Workspaces(WorkspaceForm),
    ConfirmRevoke(String),
    /// Revocar una interfaz: su token deja de valer para todos sus agentes.
    ConfirmInterfaceRevoke(String),
    /// Generar un token nuevo para una interfaz: el anterior deja de valer.
    ConfirmNewToken(String),
    /// Eliminar información (D23): papelera, historial, un proyecto o todo.
    DeleteData(DeleteForm),
    /// Alta o edición de un ciclo o sprint (RF-INT-05).
    Cycle(CycleForm),
    /// Borrar un ciclo: sus tareas quedan sin ciclo.
    ConfirmCycleDelete(model::CycleId, String),
}

/// Formulario de ciclo: `id` = `None` crea uno nuevo en `project`.
#[derive(Debug, Clone)]
pub struct CycleForm {
    pub id: Option<model::CycleId>,
    pub project: ProjectId,
    pub name: String,
    pub start: Date,
    pub end: Date,
}

impl CycleForm {
    /// Ciclo nuevo a continuación del último del proyecto: mismo largo y el
    /// número siguiente (`Sprint 4` → `Sprint 5`); sin ciclos, 14 días desde hoy.
    pub fn after_last(snap: &Snapshot, project: ProjectId, today: Date) -> Self {
        use crate::widgets::date_picker::add_days;
        let last = snap
            .cycles
            .iter()
            .filter(|c| c.project_id == project)
            .max_by_key(|c| c.end);
        let (name, start, days) = match last {
            Some(c) => (
                next_cycle_name(&c.name),
                add_days(c.end, 1).max(today),
                i64::from((c.end - c.start).get_days()) + 1,
            ),
            None => ("Sprint 1".to_owned(), today, 14),
        };
        Self {
            id: None,
            project,
            name,
            start,
            end: add_days(start, days - 1),
        }
    }

    pub fn edit(c: &model::Cycle) -> Self {
        Self {
            id: Some(c.id),
            project: c.project_id,
            name: c.name.clone(),
            start: c.start,
            end: c.end,
        }
    }
}

/// `Sprint 3` → `Sprint 4`; si no termina en número, se le añade ` 2`.
pub fn next_cycle_name(previous: &str) -> String {
    let digits = previous
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count();
    let (head, tail) = previous.split_at(previous.len() - digits);
    match tail.parse::<u64>() {
        Ok(n) => format!("{head}{}", n + 1),
        Err(_) => format!("{previous} 2"),
    }
}

/// Paleta de comandos (RF-VIS-13): todo el registro, con formulario generado.
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub params: &'static [Param],
}

#[derive(Debug, Default)]
pub struct Palette {
    pub query: String,
    pub commands: Vec<PaletteCommand>,
    pub chosen: Option<usize>,
    pub values: HashMap<&'static str, String>,
    pub result: Option<Result<String, String>>,
}

pub struct UiState {
    pub lang: Lang,
    pub theme: eframe::egui::ThemePreference,
    pub text_size: crate::theme::TextSize,
    pub scope: Scope,
    pub view: ViewKind,
    pub lanes: Lanes,
    pub roots_only: bool,
    pub show_archived: bool,
    pub show_activity: bool,
    pub activity_filter: ActivityFilter,
    pub filter: Filter,
    pub collapsed: HashSet<TaskId>,
    pub selected: Option<TaskId>,
    pub calendar_month: Date,
    pub calendar_week: bool,
    pub new_task: String,
    pub focus_new_task: bool,
    /// Subtarea que se está escribiendo bajo una fila de la lista.
    pub add_child: Option<(TaskId, String)>,
    /// El grafo de dependencias muestra también las tareas cerradas.
    pub graph_closed: bool,
    /// Llevar el foco al campo de subtarea nueva del detalle (fuera de la lista).
    pub focus_subtask: bool,
    /// Texto del buscador de padre o de subtarea existente.
    pub parent_query: String,
    /// Tarea rápida en un día del calendario.
    pub quick_add: Option<(Date, String)>,
    pub edit: DetailEdit,
    pub dialog: Option<Dialog>,
    /// Error del último guardado del diálogo abierto (se muestra dentro).
    pub dialog_error: Option<String>,
    pub palette: Option<Palette>,
    /// Motivo de rechazo en la cola de revisión: (tarea, motivo, devolver al mismo agente).
    pub rejecting: Option<(TaskId, String, bool)>,
    pub answers: HashMap<TaskId, String>,
    pub inbox_target: HashMap<InboxId, ProjectId>,
    pub epic_form: EpicForm,
    pub diagrams: DiagramsState,
    pub metrics: MetricsState,
    /// Vista guardada en uso (su nombre), para volver a guardarla o borrarla.
    pub active_view: Option<String>,
    pub table: TableState,
    pub gantt: GanttState,
    pub keymap: crate::keymap::Keymap,
    /// Acción a la que se le está asignando un atajo en el editor.
    pub capturing: Option<crate::keymap::Action>,
    pub focus_search: bool,
    pub panels: PanelsState,
}

/// Paneles que se pliegan según el ancho de la ventana (ver `layout`).
#[derive(Debug, Clone, Default)]
pub struct PanelsState {
    /// El supervisor plegó la barra lateral a iconos.
    pub sidebar_collapsed: bool,
    /// Barra lateral completa abierta encima, cuando no cabe al lado.
    pub sidebar_peek: bool,
    /// Actividad abierta encima, cuando no cabe al lado.
    pub activity_peek: bool,
    /// Filtros desplegados en la cabecera compacta.
    pub filters_open: bool,
    /// Calendario estrecho: la lista sin fecha en lugar de la cuadrícula.
    pub calendar_unscheduled: bool,
}

/// Columnas de la tabla (RF-VIS-05) y la celda que se está editando.
#[derive(Debug, Clone)]
pub struct TableState {
    /// Códigos de columna; los campos propios van como `field.<clave>`.
    pub columns: Vec<String>,
    pub editing: Option<(TaskId, String, String)>,
}

impl Default for TableState {
    fn default() -> Self {
        Self {
            columns: [
                "key", "title", "status", "priority", "assignee", "due", "estimate",
            ]
            .map(str::to_owned)
            .to_vec(),
            editing: None,
        }
    }
}

/// Escala y arrastre del Gantt (RF-VIS-10).
#[derive(Debug, Clone)]
pub struct GanttState {
    /// Píxeles por día.
    pub day_width: f32,
    /// Barra arrastrada: tarea, borde (`None` = la barra entera) y días movidos.
    pub drag: Option<(TaskId, Option<GanttEdge>, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GanttEdge {
    Start,
    End,
}

impl Default for GanttState {
    fn default() -> Self {
        Self {
            day_width: 24.0,
            drag: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsTab {
    Charts,
    Hours,
}

/// Métricas calculadas para la vista (se piden al entrar y al refrescar).
#[derive(Debug, Clone)]
pub struct MetricsData {
    pub burndown: serde_json::Value,
    pub cfd: serde_json::Value,
    pub agents: serde_json::Value,
    pub sheet: serde_json::Value,
    /// Coste por agente (RF-HIS-06).
    pub cost: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct MetricsState {
    pub tab: MetricsTab,
    pub days: u32,
    pub period: &'static str,
    pub data: Option<MetricsData>,
    /// Alcance con el que se calcularon (al cambiarlo se recalculan).
    pub scope: Option<Scope>,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            tab: MetricsTab::Charts,
            days: 30,
            period: "week",
            data: None,
            scope: None,
        }
    }
}

/// Vista Diagramas (RF-MOD-19): el elegido, el editor abierto y el zoom.
#[derive(Debug, Clone)]
pub struct DiagramsState {
    pub selected: Option<model::DiagramId>,
    pub editor: Option<DiagramEditor>,
    /// 1 = tamaño natural.
    pub zoom: f32,
    /// Borrado pendiente de confirmar.
    pub confirm_delete: Option<model::DiagramId>,
}

impl Default for DiagramsState {
    fn default() -> Self {
        Self {
            selected: None,
            editor: None,
            zoom: 1.0,
            confirm_delete: None,
        }
    }
}

/// Alta (`id = None`) o edición de un diagrama, con la vista previa de la
/// fuente que se está escribiendo.
#[derive(Debug, Clone)]
pub struct DiagramEditor {
    pub id: Option<model::DiagramId>,
    pub project: ProjectId,
    pub title: String,
    pub description: String,
    pub source: String,
    /// Fuente de la vista previa: se pone al día un rato después de dejar
    /// de escribir.
    pub preview: String,
    pub edited_at: Option<std::time::Instant>,
}

impl DiagramEditor {
    pub fn new(project: ProjectId) -> Self {
        let source = "flowchart TD\n    A[Inicio] --> B[Fin]\n".to_owned();
        Self {
            id: None,
            project,
            title: String::new(),
            description: String::new(),
            preview: source.clone(),
            source,
            edited_at: None,
        }
    }

    pub fn edit(d: &model::Diagram) -> Self {
        Self {
            id: Some(d.id),
            project: d.project_id,
            title: d.title.clone(),
            description: d.description.clone(),
            source: d.source.clone(),
            preview: d.source.clone(),
            edited_at: None,
        }
    }

    /// Comando y argumentos para guardarlo.
    pub fn command(&self, project_key: &str) -> (&'static str, Args) {
        let args = Args::none()
            .with("title", self.title.as_str())
            .with("description", self.description.as_str())
            .with("source", self.source.as_str());
        match self.id {
            Some(id) => ("diagram.update", args.with("diagram", id.to_string())),
            None => ("diagram.create", args.with("project", project_key)),
        }
    }
}

/// Qué se elimina desde «Información → Eliminar» (D23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteWhat {
    Trash,
    History,
    Project,
    Workspace,
}

impl DeleteWhat {
    pub const ALL: [DeleteWhat; 4] = [
        DeleteWhat::Trash,
        DeleteWhat::History,
        DeleteWhat::Project,
        DeleteWhat::Workspace,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Trash => "ui-delete-trash",
            Self::History => "ui-delete-history",
            Self::Project => "ui-delete-project",
            Self::Workspace => "ui-delete-workspace",
        }
    }

    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Trash => "ui-delete-trash-hint",
            Self::History => "ui-delete-history-hint",
            Self::Project => "ui-delete-project-hint",
            Self::Workspace => "ui-delete-workspace-hint",
        }
    }
}

/// Eliminar información: qué, de qué proyecto o hasta qué fecha, y lo que
/// hay que escribir para confirmarlo. Antes se guarda siempre una copia.
#[derive(Debug, Clone)]
pub struct DeleteForm {
    pub what: DeleteWhat,
    /// Clave del proyecto (para `Project`).
    pub project: Option<String>,
    /// Se borra el historial anterior a este día (para `History`).
    pub before: Date,
    pub confirm: String,
}

impl DeleteForm {
    pub fn new(what: DeleteWhat, project: Option<String>) -> Self {
        Self {
            what,
            project,
            before: crate::widgets::date_picker::add_months(
                crate::widgets::date_picker::today(),
                -6,
            ),
            confirm: String::new(),
        }
    }

    /// Lo que hay que escribir para confirmar; `None` si no hace falta.
    pub fn expected(&self, snap: &Snapshot) -> Option<String> {
        match self.what {
            DeleteWhat::Trash | DeleteWhat::History => None,
            DeleteWhat::Project => Some(self.project.clone().unwrap_or_default()),
            DeleteWhat::Workspace => Some(
                snap.workspace
                    .as_ref()
                    .map(|w| w.name.clone())
                    .unwrap_or_default(),
            ),
        }
    }

    /// ¿Está todo listo para eliminar?
    pub fn ready(&self, snap: &Snapshot) -> bool {
        let project_ok = self.what != DeleteWhat::Project || self.project.is_some();
        let confirm_ok = self
            .expected(snap)
            .is_none_or(|e| !e.is_empty() && self.confirm.trim().eq_ignore_ascii_case(&e));
        project_ok && confirm_ok
    }

    pub fn intent(&self) -> Intent {
        let (id, args) = match self.what {
            DeleteWhat::Trash => (
                "trash.purge",
                Args::none().with("days", "0").with("backup", "true"),
            ),
            DeleteWhat::History => (
                "activity.prune",
                Args::none().with("before", self.before.to_string()),
            ),
            DeleteWhat::Project => (
                "project.delete",
                Args::none()
                    .with("project", self.project.clone().unwrap_or_default())
                    .with("confirm", self.confirm.trim()),
            ),
            DeleteWhat::Workspace => (
                "workspace.wipe",
                Args::none().with("confirm", self.confirm.trim()),
            ),
        };
        Intent::Submit { id, args }
    }
}

/// Alta de una épica o un hito.
#[derive(Debug, Clone)]
pub struct EpicForm {
    pub name: String,
    pub kind: model::EpicKind,
    pub project: Option<ProjectId>,
    pub target: Option<Date>,
}

impl Default for EpicForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: model::EpicKind::Epic,
            project: None,
            target: None,
        }
    }
}

/// Lo que el usuario pidió en este frame.
pub enum Intent {
    /// Calcular (o recalcular) las métricas de la vista.
    LoadMetrics,
    /// Registrar un agente o renovar su token, y mostrar el token.
    IssueToken {
        id: &'static str,
        args: Args,
    },
    /// Abrir una tarea por su clave (enlaces `[[API-42]]` del Markdown).
    OpenKey(String),
    /// Guardar desde un diálogo: si sale bien se cierra; si no, el error se
    /// muestra dentro del diálogo, que sigue abierto.
    Submit {
        id: &'static str,
        args: Args,
    },
    /// Copiar al portapapeles el token guardado de una interfaz (D22).
    CopyToken(String),
    /// Abrir un diagrama por su clave (enlaces `[[API-D3]]`, RF-MOD-19).
    OpenDiagram(String),
    /// La vista Diagramas de un proyecto.
    Diagrams(ProjectId),
    /// Crear o guardar un diagrama; si sale bien, queda elegido y se cierra
    /// el editor.
    SaveDiagram {
        id: &'static str,
        args: Args,
    },
    /// Exportación, vista previa o importación: el resultado se muestra en el diálogo.
    Exchange {
        id: &'static str,
        args: Args,
    },
    /// Cambia los filtros del feed de actividad y vuelve a leerlo.
    FilterActivity(ActivityFilter),
    Run {
        id: &'static str,
        args: Args,
    },
    /// Comando elegido en la paleta (ID del registro).
    RunPalette {
        id: &'static str,
        args: Args,
    },
    /// Eliminación con aviso "Deshacer" de 10 s (D07).
    Delete {
        id: &'static str,
        args: Args,
        undo: (&'static str, Args),
        message: Msg,
    },
    Undo,
    /// Abrir otro workspace (otra ventana de la app) y cerrar esta.
    OpenWorkspace(String),
    Select(Option<TaskId>),
    /// Ir a las tareas de un proyecto (o de todos), de un ciclo o de una épica.
    GoTo(GoTo),
    OpenPalette,
    /// Tarea nueva con vencimiento en un día del calendario.
    CreateOnDay {
        title: String,
        day: Date,
    },
}

/// Destino de `Intent::GoTo`. Los filtros de ciclo y épica se sustituyen
/// por estos (`None` = sin ese filtro).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoTo {
    pub scope: Scope,
    pub cycle: Option<model::CycleId>,
    pub epic: Option<model::EpicId>,
}

impl GoTo {
    pub fn project(scope: Scope) -> Self {
        Self {
            scope,
            cycle: None,
            epic: None,
        }
    }

    pub fn cycle(c: &model::Cycle) -> Self {
        Self {
            cycle: Some(c.id),
            ..Self::project(Scope::Project(c.project_id))
        }
    }

    pub fn epic(e: &model::Epic) -> Self {
        Self {
            epic: Some(e.id),
            ..Self::project(Scope::Project(e.project_id))
        }
    }
}

impl Intent {
    pub fn run(id: &'static str, pairs: &[(&str, &str)]) -> Self {
        Self::Run {
            id,
            args: pairs.iter().fold(Args::none(), |a, (k, v)| a.with(*k, *v)),
        }
    }
}

impl Default for DetailEdit {
    fn default() -> Self {
        Self {
            task: None,
            title: String::new(),
            description: String::new(),
            editing_description: false,
            comment: String::new(),
            new_check: String::new(),
            new_criterion: String::new(),
            new_subtask: String::new(),
            new_label: String::new(),
            editing_item: None,
            release_reason: String::new(),
            answer: String::new(),
            estimate: None,
            field_values: HashMap::new(),
            relate_kind: model::RelationKind::Relates,
            attach_path: String::new(),
            log_minutes: 0,
            log_note: String::new(),
            recurrence: None,
        }
    }
}
