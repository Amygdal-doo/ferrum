//! The mock server: answers from a collection's mock responses, matched by
//! method, path pattern and optional query/header conditions; unmatched
//! requests get a 404 listing the routes.

use crate::{Handler, Incoming, LocalServer, Outgoing, ServerError, bind_local};
use chrono::Utc;
use ferrumapi_domain::mock::{
    MAX_MOCK_DELAY_MS, MockLogEntry, MockRequest, MockRoute, find_route, not_found_body,
};
use ferrumapi_domain::text::pretty_json;
use std::net::SocketAddr;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

type SharedRoutes = Arc<RwLock<Arc<Vec<MockRoute>>>>;

/// The answer for `incoming`, the delay to apply first, and the log line.
#[must_use]
pub fn respond(routes: &[MockRoute], incoming: &Incoming) -> (Outgoing, Duration, MockLogEntry) {
    let req = MockRequest {
        method: &incoming.method,
        path: &incoming.path,
        query: &incoming.query,
        headers: &incoming.headers,
    };
    let target = if incoming.query.is_empty() {
        incoming.path.clone()
    } else {
        format!("{}?{}", incoming.path, incoming.query)
    };
    let (out, delay, matched) = match find_route(routes, &req) {
        Some(m) => {
            let r = &m.route.response;
            let mut headers: Vec<(String, String)> = r
                .headers
                .iter()
                .filter(|h| h.is_active())
                .map(|h| (h.key.trim().to_owned(), h.value.clone()))
                .collect();
            let has_ct = headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
            if !has_ct && !r.body.is_empty() {
                let ct = if pretty_json(&r.body).is_some() {
                    "application/json"
                } else {
                    "text/plain; charset=utf-8"
                };
                headers.push(("Content-Type".into(), ct.into()));
            }
            let label = if r.name.trim().is_empty() {
                m.route.name.clone()
            } else {
                format!("{} · {}", m.route.name, r.name.trim())
            };
            let delay = Duration::from_millis(r.delay_ms.min(MAX_MOCK_DELAY_MS));
            let out = Outgoing {
                status: r.status,
                headers,
                body: r.body.clone().into_bytes(),
            };
            (out, delay, Some(label))
        }
        None => {
            let body = not_found_body(routes, &incoming.method, &incoming.path);
            (Outgoing::json(404, body), Duration::ZERO, None)
        }
    };
    let entry = MockLogEntry {
        at: Utc::now(),
        method: incoming.method.clone(),
        target,
        status: out.status,
        matched,
    };
    (out, delay, entry)
}

/// A running mock server. Routes can be replaced while it runs.
#[derive(Debug)]
pub struct MockServer {
    server: LocalServer,
    routes: SharedRoutes,
}

impl MockServer {
    /// Starts on `127.0.0.1:port`; each answered request is sent to `log`
    /// (a closed receiver is ignored).
    pub async fn start(
        port: u16,
        routes: Vec<MockRoute>,
        log: UnboundedSender<MockLogEntry>,
    ) -> Result<Self, ServerError> {
        let listener = bind_local(port).await?;
        let routes: SharedRoutes = Arc::new(RwLock::new(Arc::new(routes)));
        let shared = routes.clone();
        let handler: Handler = Arc::new(move |incoming: Incoming| {
            let current = shared
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let log = log.clone();
            Box::pin(async move {
                let (out, delay, entry) = respond(&current, &incoming);
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                let _ = log.send(entry);
                out
            })
        });
        Ok(Self {
            server: LocalServer::spawn(listener, handler)?,
            routes,
        })
    }

    /// Replaces the routes; the next request uses them.
    pub fn set_routes(&self, routes: Vec<MockRoute>) {
        *self.routes.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(routes);
    }

    #[must_use]
    pub fn route_count(&self) -> usize {
        self.routes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    #[must_use]
    pub fn base_url(&self) -> String {
        self.server.base_url()
    }

    pub fn stop(&self) {
        self.server.stop();
    }

    /// Stops and waits until the port is released.
    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}
