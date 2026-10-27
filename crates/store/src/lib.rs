//! L2 — Almacenamiento abstracto (RF-ALM-01).
//!
//! El engine solo conoce estos traits; el backend concreto (SQLite hoy,
//! PostgreSQL después) se elige al arrancar. Lectura y escritura van separadas
//! para que quien solo lee (la UI) no pueda escribir.

use std::collections::{HashMap, HashSet};

use model::{
    Action, ActivityEntry, ActivityId, Actor, Agent, AgentId, AgentSession, Comment, Dependency,
    EntityKind, InboxId, InboxItem, InboxStatus, Label, LabelId, Project, ProjectId, SessionId,
    Status, Task, TaskId, WorkspaceInfo,
};

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
    /// Fallo del motor de base de datos; el detalle es técnico (inglés).
    Backend(String),
    Corrupt(String),
    NewerSchema {
        found: u32,
        supported: u32,
    },
    /// La base aún no está migrada a este esquema: quien la tiene abierta
    /// (un daemon de una versión anterior) debe reiniciarse.
    OlderSchema {
        found: u32,
        supported: u32,
    },
    Io(String),
}

impl i18n::Localize for StoreError {
    fn msg(&self) -> i18n::Msg {
        use i18n::Msg;
        match self {
            Self::Backend(detail) => Msg::new("store-backend").arg("detail", detail),
            Self::Corrupt(detail) => Msg::new("store-corrupt").arg("detail", detail),
            Self::NewerSchema { found, supported } => Msg::new("store-newer-schema")
                .arg("found", *found)
                .arg("supported", *supported),
            Self::OlderSchema { found, supported } => Msg::new("store-older-schema")
                .arg("found", *found)
                .arg("supported", *supported),
            Self::Io(detail) => Msg::new("store-io").arg("detail", detail),
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use i18n::Localize;
        self.msg().fmt(f)
    }
}

impl std::error::Error for StoreError {}

/// Qué tareas devolver en un listado.
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub project: Option<ProjectId>,
    /// `Some(None)` = solo tareas raíz; `Some(Some(id))` = hijas directas de `id`.
    pub parent: Option<Option<TaskId>>,
    /// Vacío = cualquier estado.
    pub statuses: Vec<Status>,
    pub include_archived: bool,
    /// `true` = solo la papelera; `false` = excluirla.
    pub deleted: bool,
    /// `true` = solo las que esperan respuesta del supervisor (RF-COO-10).
    pub needs_human: bool,
    /// Solo las de esta épica o este hito (RF-MOD-13).
    pub group: Option<model::EpicId>,
    /// Solo las de este ciclo (RF-INT-05).
    pub cycle: Option<model::CycleId>,
}

impl TaskFilter {
    pub fn project(id: ProjectId) -> Self {
        Self {
            project: Some(id),
            ..Self::default()
        }
    }
}

/// Lo mínimo de una tarea para elegir la siguiente (RF-COO-05) sin cargarla
/// entera: con 50 000 tareas, cargarlas todas no cabe en 50 ms (RNF-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextCandidate {
    pub id: TaskId,
    pub priority: model::Priority,
    pub due: Option<model::When>,
    pub number: u64,
    /// Fin del lease si alguien la tiene tomada (vigente o vencido).
    pub lease_until: Option<model::Timestamp>,
    /// Quién la tiene tomada (vigente o vencido).
    pub claimed_by: Option<model::AgentId>,
    /// Sus asignados (D20).
    pub assignees: Vec<Actor>,
    /// Proyecto y columna propia: si la columna es de un agente, la tarea es
    /// para él primero.
    pub project_id: model::ProjectId,
    pub stage: Option<String>,
}

/// Qué tiempo registrado devolver.
#[derive(Debug, Clone, Default)]
pub struct TimeFilter {
    pub task: Option<TaskId>,
    pub actor: Option<Actor>,
    /// Solo los cronómetros en marcha.
    pub running: bool,
    pub since: Option<model::Timestamp>,
    pub until: Option<model::Timestamp>,
}

/// Qué costes se leen (RF-HIS-06).
#[derive(Debug, Clone, Default)]
pub struct CostFilter {
    pub task: Option<TaskId>,
    /// Los de cualquier tarea de este proyecto.
    pub project: Option<model::ProjectId>,
    pub actor: Option<model::Actor>,
    pub since: Option<model::Timestamp>,
    pub until: Option<model::Timestamp>,
}

/// Qué candidatas busca `task.next`.
#[derive(Debug, Clone)]
pub struct NextFilter {
    pub status: Status,
    /// Estados en los que un bloqueador ya no bloquea.
    pub closed: Vec<Status>,
    /// Solo en estos proyectos; vacío = todos (alcance del agente, D11).
    pub projects: Vec<ProjectId>,
    /// Capacidades del agente (RF-ACT-04): si no está vacío, solo tareas sin
    /// etiquetas o con alguna de estas.
    pub labels: Vec<LabelId>,
}

/// Qué entradas del historial devolver, de la más reciente a la más antigua.
#[derive(Debug, Clone)]
pub struct ActivityFilter {
    pub task: Option<TaskId>,
    pub project: Option<ProjectId>,
    pub actor: Option<Actor>,
    /// Vacío = cualquier acción.
    pub actions: Vec<Action>,
    pub entity: Option<EntityKind>,
    /// Solo lo que hizo un mismo comando.
    pub correlation: Option<model::CorrelationId>,
    /// Cursor: solo entradas anteriores a esta.
    pub before: Option<ActivityId>,
    /// Solo entradas posteriores a este instante.
    pub since: Option<model::Timestamp>,
    pub limit: usize,
}

impl Default for ActivityFilter {
    fn default() -> Self {
        Self {
            task: None,
            project: None,
            actor: None,
            actions: Vec::new(),
            entity: None,
            correlation: None,
            before: None,
            since: None,
            limit: 50,
        }
    }
}

pub trait Reader {
    fn workspace(&self) -> StoreResult<WorkspaceInfo>;

    fn project(&self, id: ProjectId) -> StoreResult<Option<Project>>;
    fn project_by_key(&self, key: &str) -> StoreResult<Option<Project>>;
    fn projects(&self, include_archived: bool) -> StoreResult<Vec<Project>>;

    /// Incluye archivadas y borradas; quien llama decide qué hacer con ellas.
    fn task(&self, id: TaskId) -> StoreResult<Option<Task>>;
    fn task_by_number(&self, project: ProjectId, number: u64) -> StoreResult<Option<Task>>;
    /// Ordenadas por número dentro de cada proyecto.
    fn tasks(&self, filter: &TaskFilter) -> StoreResult<Vec<Task>>;
    /// Hijas directas, incluidas archivadas y borradas.
    fn children(&self, parent: TaskId) -> StoreResult<Vec<Task>>;
    /// Por cada tarea con hijas fuera de la papelera: (hijas, hijas en uno de
    /// los estados `closed`). Una sola consulta para toda la vista (RNF-06).
    fn children_summary(&self, closed: &[Status]) -> StoreResult<HashMap<TaskId, (usize, usize)>>;
    /// Por proyecto, sus tareas vivas y sin archivar que no están en `closed`.
    /// Una sola consulta para la barra lateral.
    fn open_counts(&self, closed: &[Status]) -> StoreResult<HashMap<ProjectId, usize>>;
    /// Tareas con al menos un bloqueador vivo que no está en `closed` (RF-REL-01).
    fn blocked_tasks(&self, closed: &[Status]) -> StoreResult<HashSet<TaskId>>;
    /// Tareas bloqueadas por el supervisor y todo lo que cuelga de ellas (RF-SUP-05).
    fn locked_subtrees(&self) -> StoreResult<HashSet<TaskId>>;
    /// Tareas en `status`, vivas, sin pregunta pendiente, sin bloqueo humano
    /// (propio, de un ancestro o del proyecto) y sin bloqueadores abiertos.
    /// El lease lo decide quien llama, con su reloj.
    fn next_candidates(&self, filter: &NextFilter) -> StoreResult<Vec<NextCandidate>>;

    /// Respuesta guardada para la clave de idempotencia de un actor
    /// (RF-MCP-09): `(comando, respuesta JSON)`.
    fn idempotent(&self, actor: &Actor, key: &str) -> StoreResult<Option<(String, String)>>;

    fn label(&self, id: LabelId) -> StoreResult<Option<Label>>;
    fn label_by_name(&self, name: &str) -> StoreResult<Option<Label>>;
    fn labels(&self) -> StoreResult<Vec<Label>>;

    fn dependencies(&self) -> StoreResult<Vec<Dependency>>;
    /// Tareas que bloquean a `task`.
    fn blockers_of(&self, task: TaskId) -> StoreResult<Vec<TaskId>>;
    /// Tareas que `task` bloquea.
    fn blocks(&self, task: TaskId) -> StoreResult<Vec<TaskId>>;

    fn agent(&self, id: AgentId) -> StoreResult<Option<Agent>>;
    fn agent_by_name(&self, name: &str) -> StoreResult<Option<Agent>>;
    /// Agente dueño del token cuyo SHA-256 es `token_hash` (D09).
    fn interface(&self, id: model::InterfaceId) -> StoreResult<Option<model::Interface>>;
    fn interface_by_name(&self, name: &str) -> StoreResult<Option<model::Interface>>;
    /// Interfaz por el SHA-256 de su token (D15).
    fn interface_by_token(&self, token_hash: &str) -> StoreResult<Option<model::Interface>>;
    /// Por nombre.
    fn interfaces(&self) -> StoreResult<Vec<model::Interface>>;
    /// Ordenados por nombre, incluidos desactivados y revocados.
    fn agents(&self) -> StoreResult<Vec<Agent>>;

    fn session(&self, id: SessionId) -> StoreResult<Option<AgentSession>>;
    /// Sesiones sin cerrar; de un agente o de todos.
    fn open_sessions(&self, agent: Option<AgentId>) -> StoreResult<Vec<AgentSession>>;

    /// En orden de creación.
    fn comments(&self, task: TaskId) -> StoreResult<Vec<Comment>>;
    fn comment_count(&self, task: TaskId) -> StoreResult<usize>;
    /// Comentarios por tarea, solo de las que tienen alguno.
    fn comment_counts(&self) -> StoreResult<HashMap<TaskId, usize>>;

    /// Búsqueda de texto completo en título, descripción y comentarios
    /// (RF-VIS-11), por relevancia. Excluye la papelera; incluye archivadas.
    fn search_tasks(
        &self,
        query: &str,
        project: Option<ProjectId>,
        limit: usize,
    ) -> StoreResult<Vec<TaskId>>;

    fn activity(&self, filter: &ActivityFilter) -> StoreResult<Vec<ActivityEntry>>;
    fn activity_entry(&self, id: ActivityId) -> StoreResult<Option<ActivityEntry>>;

    /// Tareas con claim (vigente o vencido), por vencimiento.
    fn claimed_tasks(&self) -> StoreResult<Vec<Task>>;

    fn inbox_item(&self, id: InboxId) -> StoreResult<Option<InboxItem>>;
    /// Más recientes primero; `None` = cualquier estado.
    fn inbox_items(&self, status: Option<InboxStatus>) -> StoreResult<Vec<InboxItem>>;

    fn epic(&self, id: model::EpicId) -> StoreResult<Option<model::Epic>>;
    /// Épicas e hitos de un proyecto (o de todos), por fecha objetivo y nombre.
    fn epics(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Epic>>;
    fn cycle(&self, id: model::CycleId) -> StoreResult<Option<model::Cycle>>;
    /// Ciclos de un proyecto (o de todos), por fecha de inicio (RF-INT-05).
    fn cycles(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Cycle>>;
    fn diagram(&self, id: model::DiagramId) -> StoreResult<Option<model::Diagram>>;
    fn diagram_by_number(
        &self,
        project: ProjectId,
        number: u64,
    ) -> StoreResult<Option<model::Diagram>>;
    /// Diagramas de un proyecto (o de todos), por proyecto y número (RF-MOD-19).
    fn diagrams(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Diagram>>;

    fn template(&self, id: model::TemplateId) -> StoreResult<Option<model::Template>>;
    fn template_by_name(&self, name: &str) -> StoreResult<Option<model::Template>>;
    fn templates(&self) -> StoreResult<Vec<model::Template>>;

    fn attachment(&self, id: model::AttachmentId) -> StoreResult<Option<model::Attachment>>;
    /// En orden de creación.
    fn attachments(&self, task: TaskId) -> StoreResult<Vec<model::Attachment>>;
    fn attachment_data(&self, sha256: &str) -> StoreResult<Option<Vec<u8>>>;

    /// Relaciones donde participa `task` (como `a` o como `b`); `None` = todas.
    fn relations(&self, task: Option<TaskId>) -> StoreResult<Vec<model::Relation>>;
    /// Quién menciona a `target` (RF-REL-04).
    fn mentions_of(&self, target: TaskId) -> StoreResult<Vec<model::Mention>>;
    /// Quién cita al diagrama `target` (RF-MOD-19).
    fn diagram_mentions_of(
        &self,
        target: model::DiagramId,
    ) -> StoreResult<Vec<model::DiagramMention>>;
    /// Diagramas que cita `source`, en su descripción o en sus comentarios.
    fn diagram_mentions_from(&self, source: TaskId) -> StoreResult<Vec<model::DiagramMention>>;

    /// Tiempo registrado (RF-HIS-04), por inicio; filtros opcionales.
    fn time_entries(&self, filter: &TimeFilter) -> StoreResult<Vec<model::TimeEntry>>;
    /// Costes reportados (RF-HIS-06), del más antiguo al más reciente.
    fn cost_entries(&self, filter: &CostFilter) -> StoreResult<Vec<model::CostEntry>>;
    /// Vínculos con git (RF-INT-04) de una tarea o de todas, por fecha.
    fn git_links(&self, task: Option<TaskId>) -> StoreResult<Vec<model::GitLink>>;

    /// Vistas guardadas (RF-VIS-12), por nombre.
    fn views(&self) -> StoreResult<Vec<model::SavedView>>;
    /// Tareas marcadas «para hoy» y el día en que se marcaron (RF-VIS-08).
    fn today_marks(&self) -> StoreResult<HashMap<TaskId, model::Date>>;

    /// Automatizaciones (RF-INT-03), por nombre.
    fn automations(&self) -> StoreResult<Vec<model::Automation>>;
    /// Webhooks (RF-INT-02), con su secreto.
    fn webhooks(&self) -> StoreResult<Vec<model::Webhook>>;
    /// Envíos pendientes cuya hora llegó, del más antiguo al más nuevo.
    fn deliveries_due(
        &self,
        now: model::Timestamp,
        limit: usize,
    ) -> StoreResult<Vec<model::Delivery>>;
    fn delivery(&self, id: model::DeliveryId) -> StoreResult<Option<model::Delivery>>;
    /// Últimos envíos de un webhook (o de todos), del más nuevo al más antiguo.
    fn deliveries(
        &self,
        webhook: Option<model::WebhookId>,
        limit: usize,
    ) -> StoreResult<Vec<model::Delivery>>;
}

pub trait Writer: Reader {
    fn update_workspace(&mut self, workspace: &WorkspaceInfo) -> StoreResult<()>;

    fn insert_project(&mut self, project: &Project) -> StoreResult<()>;
    fn update_project(&mut self, project: &Project) -> StoreResult<()>;

    /// Guarda la tarea completa, con checklist, criterios y etiquetas.
    fn insert_task(&mut self, task: &Task) -> StoreResult<()>;
    fn update_task(&mut self, task: &Task) -> StoreResult<()>;
    /// Borrado físico de la tarea y su subárbol (purga de la papelera).
    fn purge_task(&mut self, id: TaskId) -> StoreResult<()>;
    /// Borra definitivamente un proyecto con todo lo suyo, historial incluido
    /// (D23). Quien llama hace antes la copia de seguridad.
    fn delete_project(&mut self, id: ProjectId) -> StoreResult<()>;
    /// Vacía el workspace (D23): queda solo su nombre y su configuración.
    fn wipe_workspace(&mut self) -> StoreResult<()>;
    /// Borra el historial anterior a `before` (D23); devuelve cuántas
    /// entradas. Es lo único que puede quitar historial: nunca se modifica.
    fn prune_activity(&mut self, before: model::Timestamp) -> StoreResult<usize>;

    fn insert_label(&mut self, label: &Label) -> StoreResult<()>;
    fn update_label(&mut self, label: &Label) -> StoreResult<()>;

    fn add_dependency(&mut self, dep: Dependency) -> StoreResult<()>;
    /// Devuelve `false` si no existía.
    fn remove_dependency(&mut self, dep: Dependency) -> StoreResult<bool>;

    fn insert_interface(&mut self, interface: &model::Interface) -> StoreResult<()>;
    fn update_interface(&mut self, interface: &model::Interface) -> StoreResult<()>;
    fn insert_agent(&mut self, agent: &Agent) -> StoreResult<()>;
    fn update_agent(&mut self, agent: &Agent) -> StoreResult<()>;
    fn upsert_session(&mut self, session: &AgentSession) -> StoreResult<()>;
    fn insert_comment(&mut self, comment: &Comment) -> StoreResult<()>;
    fn insert_inbox_item(&mut self, item: &InboxItem) -> StoreResult<()>;
    fn update_inbox_item(&mut self, item: &InboxItem) -> StoreResult<()>;

    fn insert_epic(&mut self, epic: &model::Epic) -> StoreResult<()>;
    fn update_epic(&mut self, epic: &model::Epic) -> StoreResult<()>;
    /// Sus tareas se quedan sin épica o hito.
    fn delete_epic(&mut self, id: model::EpicId) -> StoreResult<()>;
    fn insert_cycle(&mut self, cycle: &model::Cycle) -> StoreResult<()>;
    fn update_cycle(&mut self, cycle: &model::Cycle) -> StoreResult<()>;
    /// Borra el ciclo y deja sin ciclo a sus tareas.
    fn delete_cycle(&mut self, id: model::CycleId) -> StoreResult<()>;
    fn insert_diagram(&mut self, diagram: &model::Diagram) -> StoreResult<()>;
    fn update_diagram(&mut self, diagram: &model::Diagram) -> StoreResult<()>;
    /// Borra el diagrama y las citas que apuntan a él.
    fn delete_diagram(&mut self, id: model::DiagramId) -> StoreResult<()>;

    fn insert_template(&mut self, template: &model::Template) -> StoreResult<()>;
    fn update_template(&mut self, template: &model::Template) -> StoreResult<()>;
    fn delete_template(&mut self, id: model::TemplateId) -> StoreResult<()>;

    /// Guarda el contenido (si no estaba ya, por hash) y el adjunto.
    fn insert_attachment(&mut self, attachment: &model::Attachment, data: &[u8])
    -> StoreResult<()>;
    fn delete_attachment(&mut self, id: model::AttachmentId) -> StoreResult<()>;

    fn add_relation(&mut self, relation: model::Relation) -> StoreResult<()>;
    /// Devuelve `false` si no existía.
    fn remove_relation(&mut self, relation: model::Relation) -> StoreResult<bool>;
    /// Reemplaza las menciones que salen de la descripción de `source`
    /// (`comment = None`) o de uno de sus comentarios.
    fn set_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[TaskId],
    ) -> StoreResult<()>;
    /// Como [`Writer::set_mentions`], para las citas de diagramas.
    fn set_diagram_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[model::DiagramId],
    ) -> StoreResult<()>;

    fn insert_time_entry(&mut self, entry: &model::TimeEntry) -> StoreResult<()>;
    fn update_time_entry(&mut self, entry: &model::TimeEntry) -> StoreResult<()>;
    fn insert_cost_entry(&mut self, entry: &model::CostEntry) -> StoreResult<()>;
    /// `false` si ya existía un vínculo igual (tarea, tipo y referencia).
    fn insert_git_link(&mut self, link: &model::GitLink) -> StoreResult<bool>;
    fn delete_git_link(&mut self, id: model::GitLinkId) -> StoreResult<bool>;

    /// Crea o reemplaza (por id).
    fn save_view(&mut self, view: &model::SavedView) -> StoreResult<()>;
    fn delete_view(&mut self, id: model::ViewId) -> StoreResult<()>;
    /// `None` quita la marca.
    fn set_today_mark(&mut self, task: TaskId, day: Option<model::Date>) -> StoreResult<()>;

    /// Crea o reemplaza (por id).
    fn save_automation(&mut self, automation: &model::Automation) -> StoreResult<()>;
    fn delete_automation(&mut self, id: model::AutomationId) -> StoreResult<()>;
    /// Crea o reemplaza (por id).
    fn save_webhook(&mut self, webhook: &model::Webhook) -> StoreResult<()>;
    /// Con sus envíos.
    fn delete_webhook(&mut self, id: model::WebhookId) -> StoreResult<()>;
    /// Crea o reemplaza (por id).
    fn save_delivery(&mut self, delivery: &model::Delivery) -> StoreResult<()>;

    fn save_idempotent(
        &mut self,
        actor: &Actor,
        key: &str,
        command: &str,
        response: &str,
        at: model::Timestamp,
    ) -> StoreResult<()>;
    /// Olvida las claves anteriores a `before`.
    fn purge_idempotency(&mut self, before: model::Timestamp) -> StoreResult<usize>;
}

/// Unidad atómica de escritura. Si se descarta sin `commit`, nada se guarda.
pub trait Transaction: Writer {
    /// Añade al historial. Solo existe aquí, no en `Writer`: los comandos no
    /// pueden escribir historial; lo hace el engine con lo que registró.
    fn append_activity(&mut self, entry: &ActivityEntry) -> StoreResult<()>;

    fn commit(self: Box<Self>) -> StoreResult<()>;
}

pub trait Store: Send {
    /// Abre una transacción de escritura (RF-ALM-04).
    fn begin(&mut self) -> StoreResult<Box<dyn Transaction + '_>>;

    /// Vista de lectura consistente.
    fn read(&mut self) -> StoreResult<Box<dyn Reader + '_>>;

    /// Cambia cada vez que alguien (este proceso u otro) confirma una escritura.
    fn change_token(&self) -> StoreResult<u64>;
}

/// Un backend elegido en tiempo de ejecución (SQLite o PostgreSQL, D13).
impl Store for Box<dyn Store> {
    fn begin(&mut self) -> StoreResult<Box<dyn Transaction + '_>> {
        (**self).begin()
    }

    fn read(&mut self) -> StoreResult<Box<dyn Reader + '_>> {
        (**self).read()
    }

    fn change_token(&self) -> StoreResult<u64> {
        (**self).change_token()
    }
}
