//! Dónde escucha el daemon (D08, D19): socket Unix con permisos del usuario
//! o, en Windows, tubería con nombre.

use std::path::Path;

use crate::{ServeError, io};

#[cfg(unix)]
pub(crate) struct Listener(tokio::net::UnixListener);

#[cfg(unix)]
pub(crate) type Stream = tokio::net::UnixStream;

#[cfg(unix)]
impl Listener {
    /// Socket con permisos `0600` en un directorio `0700`: solo el mismo
    /// usuario del sistema puede conectarse (D09). Tenemos el lock, así que un
    /// socket que ya exista es de un daemon anterior que terminó mal.
    pub(crate) fn bind(socket: &Path) -> Result<Self, ServeError> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        if let Some(dir) = socket.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(|e| io(dir, e))?;
        }
        if socket.exists() {
            std::fs::remove_file(socket).map_err(|e| io(socket, e))?;
        }
        let listener = tokio::net::UnixListener::bind(socket).map_err(|e| io(socket, e))?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| io(socket, e))?;
        Ok(Self(listener))
    }

    pub(crate) async fn accept(&mut self) -> std::io::Result<Stream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }

    pub(crate) fn remove(socket: &Path) {
        let _ = std::fs::remove_file(socket);
    }
}

#[cfg(windows)]
pub(crate) struct Listener {
    name: std::ffi::OsString,
    next: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
pub(crate) type Stream = tokio::net::windows::named_pipe::NamedPipeServer;

#[cfg(windows)]
impl Listener {
    /// La primera instancia se crea en exclusiva: si ya hay otra tubería con
    /// ese nombre, falla en vez de compartirla (D19).
    pub(crate) fn bind(socket: &Path) -> Result<Self, ServeError> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let name = socket.as_os_str().to_owned();
        let next = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&name)
            .map_err(|e| io(socket, e))?;
        Ok(Self { name, next })
    }

    /// Espera a un cliente y deja preparada otra instancia para el siguiente.
    pub(crate) async fn accept(&mut self) -> std::io::Result<Stream> {
        use tokio::net::windows::named_pipe::ServerOptions;

        self.next.connect().await?;
        let fresh = ServerOptions::new().create(&self.name)?;
        Ok(std::mem::replace(&mut self.next, fresh))
    }

    pub(crate) fn remove(_: &Path) {}
}
