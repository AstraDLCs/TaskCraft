//! Transporte local con el daemon (D08, D19): socket Unix o, en Windows,
//! tubería con nombre. El protocolo de JSON por líneas es el mismo.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
pub(crate) type Stream = std::os::unix::net::UnixStream;

#[cfg(windows)]
pub(crate) type Stream = std::fs::File;

#[cfg(unix)]
pub(crate) fn connect(socket: &Path) -> io::Result<Stream> {
    Stream::connect(socket)
}

/// La tubería se abre como un archivo; si todas sus instancias están
/// ocupadas, se reintenta un momento.
#[cfg(windows)]
pub(crate) fn connect(socket: &Path) -> io::Result<Stream> {
    const ERROR_PIPE_BUSY: i32 = 231;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(socket)
        {
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other,
        }
    }
}

/// Plazo de lectura; las tuberías no lo tienen y se espera sin límite.
pub(crate) fn read_timeout(stream: &Stream, timeout: Duration) -> io::Result<()> {
    #[cfg(unix)]
    {
        stream.set_read_timeout(Some(timeout))
    }
    #[cfg(windows)]
    {
        let _ = (stream, timeout);
        Ok(())
    }
}

/// El daemon se desacopla de la terminal o de la consola que lo lanzó.
pub(crate) fn detach(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
}

/// ¿El proceso corre un binario que ya no existe? Solo se sabe en Linux
/// (`/proc`); en Windows un `.exe` en uso no se puede reemplazar.
pub(crate) fn binary_replaced(pid: u32) -> bool {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .is_ok_and(|exe| exe.to_string_lossy().ends_with(" (deleted)"))
}

/// ¿Sigue vivo el proceso? Sin `/proc`, se da por terminado.
pub(crate) fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}
