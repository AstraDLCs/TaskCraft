//! L5 — Cliente del daemon (D01, D08): conexión bloqueante por el socket
//! local, autoarranque del daemon y suscripción a eventos.
//!
//! Sin runtime asíncrono: la CLI y la app de escritorio lo usan desde hilos
//! normales.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use i18n::{Lang, Localize, Msg};
use protocol::{ArgMap, ClientMsg, Dispatch, Event, ServerMsg, WireError};
use serde_json::Value;

mod transport;

use transport::Stream;

/// Binario que se lanza como daemon si no hay ninguno (por defecto, el
/// `taskcraft-cli` junto al ejecutable actual).
pub const DAEMON_BIN_ENV: &str = "TASKCRAFT_DAEMON_BIN";
/// Token de agente para conexiones locales (D09).
pub const TOKEN_ENV: &str = "TASKCRAFT_AGENT_TOKEN";
/// Minutos sin clientes tras los que se detiene un daemon autoarrancado.
pub const AUTOSTART_IDLE_MINUTES: u32 = 30;

const STARTUP_WAIT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum ClientError {
    /// No hay daemon y no se pudo (o no se quiso) arrancar uno.
    Unavailable {
        socket: PathBuf,
        detail: String,
    },
    /// El daemon rechazó la conexión (token inválido, versión…).
    Rejected(WireError),
    Io(String),
    Protocol(String),
}

impl Localize for ClientError {
    fn msg(&self) -> Msg {
        match self {
            Self::Unavailable { socket, detail } => Msg::new("client-unavailable")
                .arg("socket", socket.display().to_string())
                .arg("detail", detail),
            Self::Rejected(e) => Msg::new("client-rejected").arg("detail", &e.message),
            Self::Io(detail) => Msg::new("client-io").arg("detail", detail),
            Self::Protocol(detail) => Msg::new("client-protocol").arg("detail", detail),
        }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.msg().fmt(f)
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    /// El mismo problema como error de comando, para quien solo maneja `WireError`.
    pub fn to_wire(&self, lang: Lang) -> WireError {
        match self {
            Self::Rejected(e) => e.clone(),
            other => WireError::new("UNAVAILABLE", "client-unavailable", other.localize(lang)),
        }
    }
}

/// Cómo conectarse.
#[derive(Debug, Clone)]
pub struct Connect {
    pub db: PathBuf,
    /// `cli`, `app`, `mcp-stdio`…; queda como transporte de la sesión.
    pub client: String,
    pub lang: Lang,
    pub token: Option<String>,
    pub agent: Option<String>,
    /// Si no hay daemon, lanzarlo en segundo plano.
    pub autostart: bool,
}

impl Connect {
    pub fn new(db: PathBuf, client: &str) -> Self {
        Self {
            db,
            client: client.to_owned(),
            lang: Lang::En,
            token: None,
            agent: None,
            autostart: true,
        }
    }

    pub fn socket(&self) -> PathBuf {
        protocol::socket_path(&self.db)
    }
}

/// Conexión abierta con el daemon.
pub struct Client {
    reader: BufReader<Stream>,
    writer: Stream,
    next_id: u64,
    lang: Lang,
    actor: String,
    session: Option<String>,
}

impl Client {
    /// Conecta (arrancando el daemon si hace falta) y se identifica (D09).
    pub fn connect(opts: &Connect) -> Result<Self, ClientError> {
        let stream = open_stream(opts)?;
        let writer = stream
            .try_clone()
            .map_err(|e| ClientError::Io(e.to_string()))?;
        let mut client = Self {
            reader: BufReader::new(stream),
            writer,
            next_id: 1,
            lang: opts.lang,
            actor: String::new(),
            session: None,
        };
        client.send(&ClientMsg::Hello {
            client: opts.client.clone(),
            version: protocol::VERSION,
            lang: Some(opts.lang.code().to_owned()),
            token: opts.token.clone(),
            agent: opts.agent.clone(),
        })?;
        match client.recv()? {
            ServerMsg::Welcome { actor, session, .. } => {
                client.actor = actor;
                client.session = session;
                Ok(client)
            }
            ServerMsg::Rejected { err } => Err(ClientError::Rejected(err)),
            other => Err(ClientError::Protocol(format!("unexpected {other:?}"))),
        }
    }

    /// Actor con el que el daemon atribuye lo que hace esta conexión.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    /// Ejecuta un comando en el daemon. Los errores de transporte llegan
    /// como `WireError` con código `UNAVAILABLE`.
    pub fn request(
        &mut self,
        command: &str,
        args: &ArgMap,
        lang: Lang,
    ) -> Result<Value, WireError> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = ClientMsg::Request {
            id,
            command: command.to_owned(),
            args: args.clone(),
            lang: Some(lang.code().to_owned()),
        };
        self.send(&msg).map_err(|e| e.to_wire(lang))?;
        loop {
            match self.recv().map_err(|e| e.to_wire(lang))? {
                ServerMsg::Response { id: got, ok, err } if got == id => {
                    return match (ok, err) {
                        (_, Some(e)) => Err(e),
                        (Some(v), None) => Ok(v),
                        (None, None) => Ok(Value::Null),
                    };
                }
                ServerMsg::Rejected { err } => return Err(err),
                // Respuestas viejas o eventos sueltos no son de esta petición.
                _ => continue,
            }
        }
    }

    /// Pide al daemon que se detenga (solo supervisor).
    pub fn shutdown(&mut self) -> Result<(), ClientError> {
        self.send(&ClientMsg::Shutdown)?;
        match self.recv()? {
            ServerMsg::Bye => Ok(()),
            ServerMsg::Rejected { err } => Err(ClientError::Rejected(err)),
            other => Err(ClientError::Protocol(format!("unexpected {other:?}"))),
        }
    }

    fn send(&mut self, msg: &ClientMsg) -> Result<(), ClientError> {
        self.writer
            .write_all(protocol::encode(msg).as_bytes())
            .map_err(|e| ClientError::Io(e.to_string()))
    }

    fn recv(&mut self) -> Result<ServerMsg, ClientError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| ClientError::Io(e.to_string()))?;
        if n == 0 {
            return Err(ClientError::Io("daemon closed the connection".into()));
        }
        protocol::decode(&line).map_err(|e| ClientError::Protocol(e.to_string()))
    }
}

impl Dispatch for Client {
    fn dispatch(&mut self, command: &str, args: &ArgMap, lang: Lang) -> Result<Value, WireError> {
        self.request(command, args, lang)
    }
}

/// ¿Hay un daemon atendiendo este workspace? Devuelve su PID.
pub fn ping(db: &Path) -> Option<u32> {
    let mut stream = transport::connect(&protocol::socket_path(db)).ok()?;
    transport::read_timeout(&stream, Duration::from_secs(2)).ok()?;
    stream
        .write_all(protocol::encode(&ClientMsg::Ping).as_bytes())
        .ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    match protocol::decode(&line).ok()? {
        ServerMsg::Pong { pid, .. } => Some(pid),
        _ => None,
    }
}

/// Recibe en un hilo aparte cada escritura confirmada en el daemon (RNF-08)
/// y llama a `on_change` con el token nuevo. El hilo termina cuando el
/// daemon cierra la conexión o `on_change` devuelve `false`.
pub fn subscribe(
    opts: &Connect,
    mut on_change: impl FnMut(u64) -> bool + Send + 'static,
) -> Result<std::thread::JoinHandle<()>, ClientError> {
    let mut client = Client::connect(opts)?;
    client.send(&ClientMsg::Subscribe)?;
    Ok(std::thread::spawn(move || {
        while let Ok(msg) = client.recv() {
            if let ServerMsg::Event(Event::Changed { token }) = msg
                && !on_change(token)
            {
                return;
            }
        }
    }))
}

/// Detiene el daemon del workspace y espera a que su proceso termine, para
/// sustituirlo (por ejemplo, uno de una versión anterior). Sin token: solo
/// lo acepta un daemon local que trata la conexión como supervisor.
pub fn stop_daemon(db: &Path) -> Result<(), ClientError> {
    let pid = ping(db);
    let opts = Connect {
        autostart: false,
        ..Connect::new(db.to_path_buf(), "cli")
    };
    Client::connect(&opts)?.shutdown()?;
    let deadline = Instant::now() + STARTUP_WAIT;
    // Un daemon en este mismo proceso (tests) no termina con el proceso.
    let own = std::process::id();
    while pid.is_some_and(|p| p != own && transport::alive(p)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

/// ¿El daemon que atiende el workspace corre un binario que ya no existe?
/// Pasa al recompilar o actualizar mientras sigue vivo: hablaría un esquema
/// anterior al de este cliente.
fn stale_daemon(db: &Path) -> bool {
    ping(db).is_some_and(transport::binary_replaced)
}

fn open_stream(opts: &Connect) -> Result<Stream, ClientError> {
    let socket = opts.socket();
    if opts.autostart && stale_daemon(&opts.db) {
        tracing::info!("daemon binary was replaced; restarting it");
        if let Err(e) = stop_daemon(&opts.db) {
            tracing::warn!(error = %e, "could not stop the outdated daemon");
        }
    }
    match transport::connect(&socket) {
        Ok(s) => return Ok(s),
        Err(e) if !opts.autostart => {
            return Err(ClientError::Unavailable {
                socket,
                detail: e.to_string(),
            });
        }
        Err(_) => {}
    }
    spawn_daemon(&opts.db).map_err(|detail| ClientError::Unavailable {
        socket: socket.clone(),
        detail,
    })?;
    let deadline = Instant::now() + STARTUP_WAIT;
    loop {
        match transport::connect(&socket) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= deadline => {
                return Err(ClientError::Unavailable {
                    socket,
                    detail: e.to_string(),
                });
            }
            Err(_) => std::thread::sleep(Duration::from_millis(30)),
        }
    }
}

/// Lanza `taskcraft-cli serve` desacoplado de la terminal (D08).
fn spawn_daemon(db: &Path) -> Result<(), String> {
    let bin = daemon_bin()?;
    tracing::info!(bin = %bin.display(), "starting daemon");
    let mut cmd = Command::new(&bin);
    // Una cadena de conexión puede llevar la contraseña: por el entorno, no
    // en los argumentos que cualquiera ve con `ps` (D13).
    if protocol::is_url(db) {
        cmd.env("TASKCRAFT_DB", db);
    } else {
        cmd.arg("--db").arg(db);
    }
    cmd.arg("serve")
        .arg("--idle-exit")
        .arg(AUTOSTART_IDLE_MINUTES.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    transport::detach(&mut cmd);
    cmd.spawn()
        .map(drop)
        .map_err(|e| format!("{}: {e}", bin.display()))
}

fn daemon_bin() -> Result<PathBuf, String> {
    if let Some(bin) = std::env::var_os(DAEMON_BIN_ENV) {
        return Ok(PathBuf::from(bin));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let sibling = exe.with_file_name(format!("taskcraft-cli{}", std::env::consts::EXE_SUFFIX));
    if sibling.exists() {
        Ok(sibling)
    } else {
        Err(format!("{} not found", sibling.display()))
    }
}
