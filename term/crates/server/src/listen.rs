//! Listeners: a private Unix socket (trusted: same user, 0600 in a 0700
//! directory) and optional TCP listeners for remote clients. TCP is only
//! allowed on loopback or Tailscale addresses — WireGuard encrypts tailnet
//! traffic — and every TCP client must present the server token.

use crate::{Server, ServerError, conn};
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use tokio::net::{TcpListener, UnixListener};

/// Tailscale CGNAT range 100.64.0.0/10 or ULA fd7a:115c:a1e0::/48, or loopback.
pub fn check_listen_addr(addr: &SocketAddr) -> Result<(), String> {
    let ok = match addr.ip() {
        IpAddr::V4(v4) => {
            v4.is_loopback() || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "refusing to listen on {addr}: only loopback and Tailscale (100.64.0.0/10, fd7a:115c:a1e0::/48) addresses are allowed — a connection can run any program as you"
        ))
    }
}

/// The token remote clients must send; created (0600) on first start.
pub fn load_or_create_token(dir: &Path) -> Result<String, ServerError> {
    let path = dir.join("token");
    if let Ok(t) = std::fs::read_to_string(&path)
        && t.trim().len() >= 32
    {
        return Ok(t.trim().to_owned());
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| ServerError::Config(format!("no randomness: {e}")))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(&path, &token)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(token)
}

pub async fn start(server: Arc<Server>) -> Result<(), ServerError> {
    let socket = server.config.socket.clone();
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if socket.exists() {
        // A live server answers; a stale socket file is replaced.
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            return Err(ServerError::Config(format!(
                "a server is already running on {}",
                socket.display()
            )));
        }
        std::fs::remove_file(&socket)?;
    }
    let unix = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    crate::lock(&server.listeners).push(format!("unix://{}", socket.display()));
    let s = server.clone();
    tokio::spawn(async move {
        loop {
            match unix.accept().await {
                Ok((stream, _)) => {
                    let (r, w) = stream.into_split();
                    tokio::spawn(conn::serve(s.clone(), r, w, true));
                }
                Err(e) => tracing::warn!(error = %e, "accept failed"),
            }
        }
    });

    for addr in server.config.tcp.clone() {
        check_listen_addr(&addr).map_err(ServerError::Config)?;
        let tcp = TcpListener::bind(addr).await?;
        crate::lock(&server.listeners).push(format!("tcp://{addr}"));
        tracing::info!(%addr, "remote listener ready (token required)");
        let s = server.clone();
        tokio::spawn(async move {
            loop {
                match tcp.accept().await {
                    Ok((stream, peer)) => {
                        tracing::info!(%peer, "remote client connected");
                        let _ = stream.set_nodelay(true);
                        let (r, w) = stream.into_split();
                        tokio::spawn(conn::serve(s.clone(), r, w, false));
                    }
                    Err(e) => tracing::warn!(error = %e, "accept failed"),
                }
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_listen_addr;

    #[test]
    fn only_loopback_and_tailnet() {
        for ok in [
            "127.0.0.1:7397",
            "100.101.102.103:7397",
            "100.64.0.1:1",
            "[::1]:7397",
            "[fd7a:115c:a1e0::1]:7397",
        ] {
            assert!(
                check_listen_addr(&ok.parse().expect("addr")).is_ok(),
                "{ok}"
            );
        }
        for bad in [
            "0.0.0.0:7397",
            "192.168.1.5:7397",
            "100.128.0.1:1",
            "[::]:7397",
        ] {
            assert!(
                check_listen_addr(&bad.parse().expect("addr")).is_err(),
                "{bad}"
            );
        }
    }
}
