use i18n::{Lang, Localize, Msg, tr};
use rules::RuleError;
use store::StoreError;

/// Error de un comando. Los textos son claves i18n: cada frontend los traduce
/// al idioma de quien los lee (`localize`); `Display` usa inglés, para logs.
#[derive(Debug)]
pub enum EngineError {
    UnknownCommand(String),
    MissingArg(String),
    InvalidArg {
        name: String,
        reason: Msg,
    },
    NotFound(Msg),
    /// Operación válida en forma pero no permitida por el estado actual.
    Invalid(Msg),
    /// El actor no tiene permiso (agente desactivado, acción solo del supervisor…).
    Forbidden(Msg),
    /// La tarea cambió desde la revisión que el cliente creía tener (RF-COO-06).
    /// `current` es el detalle actual, para que el agente reintente sin otra consulta.
    Conflict {
        msg: Msg,
        current: Box<serde_json::Value>,
    },
    /// Tiene bloqueadores abiertos o no está en un estado que se pueda tomar.
    NotReady(Msg),
    /// El supervisor la bloqueó o espera una respuesta suya (RF-SUP-05, RF-COO-10).
    LockedByHuman(Msg),
    /// Otro agente la tiene tomada con el lease vigente.
    ClaimedByOther(Msg),
    /// El supervisor pausó al agente o puso el workspace en solo lectura (RF-SUP-08).
    Paused(Msg),
    Rule(RuleError),
    Panicked(String),
    Store(StoreError),
}

impl EngineError {
    /// Código estable para clientes (MCP, CLI): RF-MCP-07.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) | Self::NotFound(_) => "NOT_FOUND",
            Self::MissingArg(_) | Self::InvalidArg { .. } | Self::Invalid(_) | Self::Rule(_) => {
                "INVALID"
            }
            Self::Forbidden(_) => "FORBIDDEN",
            Self::Conflict { .. } => "CONFLICT",
            Self::NotReady(_) => "NOT_READY",
            Self::LockedByHuman(_) => "LOCKED_BY_HUMAN",
            Self::ClaimedByOther(_) => "CLAIMED_BY_OTHER",
            Self::Paused(_) => "PAUSED",
            Self::Panicked(_) | Self::Store(_) => "INTERNAL",
        }
    }

    /// Clave i18n más específica del error; los tests comparan esto y no el texto.
    pub fn key(&self) -> &'static str {
        match self {
            Self::InvalidArg { reason, .. } => reason.key,
            _ => self.msg().key,
        }
    }

    /// El error tal como viaja a un cliente (D08), explicado en `lang`.
    pub fn to_wire(&self, lang: Lang) -> protocol::WireError {
        protocol::WireError {
            code: self.code().to_owned(),
            key: self.key().to_owned(),
            message: self.localize(lang),
            current: self.current().cloned().map(Box::new),
        }
    }

    pub(crate) fn invalid(msg: Msg) -> Self {
        Self::Invalid(msg)
    }

    pub(crate) fn not_found(msg: Msg) -> Self {
        Self::NotFound(msg)
    }

    pub(crate) fn forbidden(msg: Msg) -> Self {
        Self::Forbidden(msg)
    }

    /// Detalle actual del recurso en conflicto, si lo hay.
    pub fn current(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Conflict { current, .. } => Some(current),
            _ => None,
        }
    }

    pub(crate) fn arg(name: &str, reason: Msg) -> Self {
        Self::InvalidArg {
            name: name.to_owned(),
            reason,
        }
    }

    fn outer(&self, lang: Lang) -> Msg {
        match self {
            Self::UnknownCommand(id) => Msg::new("error-unknown-command").arg("id", id),
            Self::MissingArg(name) => Msg::new("error-missing-arg").arg("name", name),
            Self::InvalidArg { name, reason } => Msg::new("error-invalid-arg")
                .arg("name", name)
                .arg("reason", tr(lang, reason)),
            Self::NotFound(m)
            | Self::Invalid(m)
            | Self::Forbidden(m)
            | Self::NotReady(m)
            | Self::LockedByHuman(m)
            | Self::ClaimedByOther(m)
            | Self::Paused(m)
            | Self::Conflict { msg: m, .. } => m.clone(),
            Self::Rule(e) => e.msg(),
            Self::Panicked(id) => Msg::new("error-panicked").arg("id", id),
            Self::Store(e) => e.msg(),
        }
    }
}

impl Localize for EngineError {
    fn msg(&self) -> Msg {
        self.outer(Lang::En)
    }

    fn localize(&self, lang: Lang) -> String {
        tr(lang, &self.outer(lang))
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.localize(Lang::En))
    }
}

impl std::error::Error for EngineError {}

impl From<RuleError> for EngineError {
    fn from(e: RuleError) -> Self {
        Self::Rule(e)
    }
}

impl From<StoreError> for EngineError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// Error al leer el valor de un argumento, explicable en cualquier idioma.
pub trait ArgError {
    fn reason(&self, value: &str) -> Msg;
}

impl ArgError for std::num::ParseIntError {
    fn reason(&self, value: &str) -> Msg {
        Msg::new("arg-not-number").arg("value", value)
    }
}

impl ArgError for model::ParseEnumError {
    fn reason(&self, _: &str) -> Msg {
        self.msg()
    }
}

impl ArgError for model::ParseActorError {
    fn reason(&self, _: &str) -> Msg {
        self.msg()
    }
}

impl ArgError for jiff::Error {
    fn reason(&self, value: &str) -> Msg {
        Msg::new("arg-bad-date")
            .arg("value", value)
            .arg("detail", self.to_string())
    }
}

impl ArgError for foundation::DecodeError {
    fn reason(&self, value: &str) -> Msg {
        Msg::new("arg-bad-id").arg("value", value)
    }
}
