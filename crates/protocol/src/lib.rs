//! L2 — Protocolo entre clientes y el daemon (D01, D08).
//!
//! JSON por líneas sobre el socket local: el cliente abre con [`ClientMsg::Hello`],
//! envía peticiones y recibe respuestas con el mismo `id`. Una conexión que pide
//! [`ClientMsg::Subscribe`] pasa a recibir solo eventos.

use std::collections::BTreeMap;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use i18n::Lang;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Versión del protocolo; cliente y daemon deben coincidir.
pub const VERSION: u32 = 1;

/// Argumentos de un comando, tal como los recibe `engine::Args`.
pub type ArgMap = BTreeMap<String, String>;

/// Lo que envía un cliente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Primer mensaje de toda conexión: quién es (D09) y en qué idioma lee.
    Hello {
        /// `cli`, `app`, `mcp-stdio`…; queda como transporte de la sesión.
        client: String,
        version: u32,
        #[serde(default)]
        lang: Option<String>,
        #[serde(default)]
        token: Option<String>,
        /// Agente por nombre (solo por el socket local, D09).
        #[serde(default)]
        agent: Option<String>,
    },
    Request {
        id: u64,
        command: String,
        #[serde(default)]
        args: ArgMap,
        /// Idioma de esta respuesta, si difiere del de `Hello`.
        #[serde(default)]
        lang: Option<String>,
    },
    /// A partir de aquí la conexión solo recibe [`ServerMsg::Event`].
    Subscribe,
    Ping,
    /// Detiene el daemon (solo supervisor).
    Shutdown,
}

/// Lo que envía el daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome {
        /// Actor de la conexión: `supervisor`, `agent:<id>`…
        actor: String,
        /// Sesión de agente abierta para esta conexión (RF-ACT-03).
        #[serde(default)]
        session: Option<String>,
        version: u32,
    },
    Response {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        err: Option<WireError>,
    },
    Event(Event),
    Pong {
        version: u32,
        pid: u32,
    },
    /// El daemon rechaza la conexión o el mensaje (por ejemplo, token inválido).
    Rejected {
        err: WireError,
    },
    /// Respuesta a `Shutdown`: el daemon se detiene tras enviarla.
    Bye,
}

impl ServerMsg {
    pub fn response(id: u64, result: Result<Value, WireError>) -> Self {
        match result {
            Ok(v) => Self::Response {
                id,
                ok: Some(v),
                err: None,
            },
            Err(e) => Self::Response {
                id,
                ok: None,
                err: Some(e),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Alguien confirmó una escritura; `token` crece con cada commit.
    Changed { token: u64 },
}

/// Error de un comando tal como viaja: código estable (RF-MCP-07), clave i18n
/// y mensaje ya traducido al idioma del cliente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    pub key: String,
    pub message: String,
    /// Detalle actual del recurso en un `CONFLICT` (RF-COO-06). En caja: un
    /// `Value` con orden de inserción (lo activa `merman`, D21) es grande.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<Box<Value>>,
}

impl WireError {
    pub fn new(code: &str, key: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            key: key.to_owned(),
            message: message.into(),
            current: None,
        }
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for WireError {}

/// Quien sabe ejecutar comandos: el engine en proceso o un cliente del daemon.
/// Los frontends trabajan contra esto y no saben cuál de los dos tienen.
pub trait Dispatch: Send {
    fn dispatch(&mut self, command: &str, args: &ArgMap, lang: Lang) -> Result<Value, WireError>;
}

/// Una línea del protocolo (con su salto de línea).
pub fn encode<T: Serialize>(msg: &T) -> String {
    let mut line = serde_json::to_string(msg).unwrap_or_else(|e| {
        serde_json::json!({ "type": "rejected", "err": { "code": "INTERNAL", "key": "protocol-encode", "message": e.to_string() } }).to_string()
    });
    line.push('\n');
    line
}

pub fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(line.trim_end())
}

/// Archivo de lock junto a la base: solo un daemon por workspace (D01).
pub fn lock_path(db: &Path) -> PathBuf {
    // Un workspace en PostgreSQL (D13) no tiene carpeta: el lock va con los sockets.
    if is_url(db) {
        return runtime_dir().join(format!("{:016x}.lock", fnv1a(db)));
    }
    let mut name = db.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    db.with_file_name(name)
}

/// Socket del daemon de un workspace: `$XDG_RUNTIME_DIR/taskcraft/<hash>.sock`
/// (o el directorio temporal), con un hash estable de la ruta de la base para
/// que dos workspaces no compartan socket.
/// En Windows es una tubería con nombre: `\\.\pipe\taskcraft-<hash>` (D19).
pub fn socket_path(db: &Path) -> PathBuf {
    let canonical = db
        .parent()
        .and_then(|dir| dir.canonicalize().ok())
        .map(|dir| dir.join(db.file_name().unwrap_or_default()))
        .unwrap_or_else(|| db.to_path_buf());
    let hash = fnv1a(&canonical);
    if cfg!(windows) {
        PathBuf::from(format!(r"\\.\pipe\taskcraft-{hash:016x}"))
    } else {
        runtime_dir().join(format!("{hash:016x}.sock"))
    }
}

/// ¿La «ruta» del workspace es una cadena de conexión de PostgreSQL (D13)?
pub fn is_url(db: &Path) -> bool {
    db.to_str()
        .is_some_and(|s| s.starts_with("postgres://") || s.starts_with("postgresql://"))
}

/// Directorio privado de sockets del usuario.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("taskcraft"),
        _ => {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            std::env::temp_dir().join(format!("taskcraft-{user}"))
        }
    }
}

/// FNV-1a de 64 bits: estable entre versiones de Rust (a diferencia de
/// `DefaultHasher`), así todos los binarios calculan el mismo socket.
struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

fn fnv1a(path: &Path) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    h.write(path.to_string_lossy().as_bytes());
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_roundtrip_as_single_lines() {
        let msgs = [
            ClientMsg::Hello {
                client: "cli".into(),
                version: VERSION,
                lang: Some("es".into()),
                token: None,
                agent: Some("claude-1".into()),
            },
            ClientMsg::Request {
                id: 7,
                command: "task.list".into(),
                args: ArgMap::from([("project".into(), "API".into())]),
                lang: None,
            },
            ClientMsg::Subscribe,
        ];
        for m in msgs {
            let line = encode(&m);
            assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
            assert_eq!(decode::<ClientMsg>(&line).unwrap(), m);
        }
        let err = ServerMsg::response(
            3,
            Err(WireError::new("CONFLICT", "conflict_key", "changed")),
        );
        assert_eq!(decode::<ServerMsg>(&encode(&err)).unwrap(), err);
    }

    #[test]
    fn minimal_hello_is_accepted() {
        let m: ClientMsg = decode(r#"{"type":"hello","client":"x","version":1}"#).unwrap();
        assert!(matches!(m, ClientMsg::Hello { token: None, .. }));
    }

    #[test]
    fn socket_depends_on_the_database_path() {
        let a = socket_path(Path::new("/tmp/a/workspace.db"));
        let b = socket_path(Path::new("/tmp/b/workspace.db"));
        assert_ne!(a, b);
        assert_eq!(a, socket_path(Path::new("/tmp/a/workspace.db")));
        assert_eq!(
            lock_path(Path::new("/x/workspace.db")),
            Path::new("/x/workspace.db.lock")
        );
    }
}
