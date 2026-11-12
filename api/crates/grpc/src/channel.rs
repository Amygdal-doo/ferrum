//! Channels: plaintext HTTP/2, or TLS through tokio-rustls with the shared
//! FerrumAPI rustls config (so "Verify TLS" off works for self-signed
//! servers).

use ferrumapi_domain::ExecError;
use ferrumapi_domain::protocol::grpc::endpoint;
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::pki_types::ServerName;
use tonic::transport::{Channel, Endpoint, Uri};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Connects to `raw` (`host:port`, `grpc(s)://…` or `http(s)://…`).
pub(crate) async fn connect(
    raw: &str,
    force_tls: bool,
    verify_tls: bool,
) -> Result<Channel, ExecError> {
    let (url, tls) = endpoint(raw, force_tls);
    let uri: Uri = url
        .parse()
        .map_err(|e| ExecError::InvalidUrl(format!("{e}")))?;
    if uri.host().is_none_or(str::is_empty) {
        return Err(ExecError::InvalidUrl("missing host".into()));
    }
    let endpoint = Endpoint::from_shared(url.clone())
        .map_err(|e| ExecError::InvalidUrl(e.to_string()))?
        .connect_timeout(CONNECT_TIMEOUT);
    let connected = if tls {
        let config = ferrumapi_http::tls::client_config(verify_tls, &[b"h2"])?;
        let connector = tower::service_fn(move |uri: Uri| {
            let config = config.clone();
            async move {
                let host = uri
                    .host()
                    .unwrap_or_default()
                    .trim_matches(['[', ']'])
                    .to_owned();
                let port = uri.port_u16().unwrap_or(443);
                let tcp = TcpStream::connect((host.as_str(), port)).await?;
                let name = ServerName::try_from(host)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
                let tls = TlsConnector::from(config).connect(name, tcp).await?;
                Ok::<_, std::io::Error>(TokioIo::new(tls))
            }
        });
        endpoint.connect_with_connector(connector).await
    } else {
        endpoint.connect().await
    };
    connected.map_err(|e| {
        let detail =
            std::error::Error::source(&e).map_or_else(|| e.to_string(), ToString::to_string);
        ExecError::Connect(detail)
    })
}
