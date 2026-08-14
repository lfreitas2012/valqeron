//! Hand-rolled `sd_notify` readiness signaling (no dependency).
//!
//! When systemd starts the engine as a `Type=notify` unit it exports
//! `NOTIFY_SOCKET`; the engine reports `READY=1` once it is serving and
//! `STOPPING=1` when shutdown begins. Everywhere else (foreground runs,
//! launchd, tests) the variable is absent and both calls are silent no-ops.
//!
//! Notification failures are logged at debug and never affect the engine:
//! readiness reporting is advisory, the engine itself is the source of truth.

use std::ffi::OsStr;
use std::os::unix::net::UnixDatagram;

/// Report "started and serving" (`READY=1`).
pub(crate) fn notify_ready() {
    notify("READY=1");
}

/// Report "shutdown began" (`STOPPING=1`).
pub(crate) fn notify_stopping() {
    notify("STOPPING=1");
}

fn notify(state: &str) {
    let Some(socket) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    match send_state(socket.as_os_str(), state) {
        Ok(()) => tracing::debug!(state, "sd_notify state sent"),
        Err(e) => tracing::debug!(state, error = %e, "sd_notify send failed"),
    }
}

/// Send one datagram to the notification socket. A leading `@` means a Linux
/// abstract-namespace address (systemd uses these in containers).
fn send_state(socket: &OsStr, state: &str) -> std::io::Result<()> {
    let sender = UnixDatagram::unbound()?;
    if socket.as_encoded_bytes().first() == Some(&b'@') {
        return send_abstract(&sender, socket, state);
    }
    sender.send_to(state.as_bytes(), std::path::Path::new(socket))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn send_abstract(sender: &UnixDatagram, socket: &OsStr, state: &str) -> std::io::Result<()> {
    use std::os::linux::net::SocketAddrExt;
    let name = socket.as_encoded_bytes().get(1..).unwrap_or_default();
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(name)?;
    sender.send_to_addr(state.as_bytes(), &addr)?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn send_abstract(_sender: &UnixDatagram, _socket: &OsStr, _state: &str) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "abstract NOTIFY_SOCKET addresses are only supported on Linux",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_state_delivers_the_datagram_to_a_path_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notify.sock");
        let receiver = UnixDatagram::bind(&path).unwrap();

        send_state(path.as_os_str(), "READY=1").unwrap();

        let mut buf = [0u8; 64];
        let received = receiver.recv(&mut buf).unwrap();
        assert_eq!(buf.get(..received), Some(b"READY=1".as_slice()));
    }

    #[test]
    fn send_state_to_a_missing_socket_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.sock");
        assert!(send_state(path.as_os_str(), "READY=1").is_err());
    }
}
