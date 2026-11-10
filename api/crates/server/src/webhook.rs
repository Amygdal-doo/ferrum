//! The webhook inspector: records every request to any path in a bounded
//! in-memory log and answers with a configurable reply.

use crate::{Handler, Incoming, LocalServer, Outgoing, ServerError, bind_local};
use chrono::Utc;
use ferrumapi_domain::webhook::{CaptureLog, CapturedRequest, WebhookReply};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use tokio::sync::mpsc::UnboundedSender;

/// A capture of `incoming` received now (id assigned by the log).
#[must_use]
pub fn capture(incoming: Incoming) -> CapturedRequest {
    CapturedRequest {
        id: 0,
        received_at: Utc::now(),
        method: incoming.method,
        path: incoming.path,
        query: incoming.query,
        headers: incoming.headers,
        body: incoming.body,
        size: incoming.size,
    }
}

fn reply_to_outgoing(reply: &WebhookReply) -> Outgoing {
    let mut headers = Vec::new();
    if !reply.body.is_empty() && !reply.content_type.trim().is_empty() {
        headers.push((
            "Content-Type".to_owned(),
            reply.content_type.trim().to_owned(),
        ));
    }
    Outgoing {
        status: reply.status,
        headers,
        body: reply.body.clone().into_bytes(),
    }
}

/// A running inspector. The log and the reply are shared with the handler.
#[derive(Debug)]
pub struct WebhookInspector {
    server: LocalServer,
    log: Arc<Mutex<CaptureLog>>,
    reply: Arc<RwLock<WebhookReply>>,
}

impl WebhookInspector {
    /// Starts on `127.0.0.1:port`, keeping at most `limit` captures. The id
    /// of each new capture is sent to `notify` (a closed receiver is ignored).
    pub async fn start(
        port: u16,
        reply: WebhookReply,
        limit: usize,
        notify: UnboundedSender<u64>,
    ) -> Result<Self, ServerError> {
        let listener = bind_local(port).await?;
        let log = Arc::new(Mutex::new(CaptureLog::new(limit)));
        let reply = Arc::new(RwLock::new(reply));
        let (log_h, reply_h) = (log.clone(), reply.clone());
        let handler: Handler = Arc::new(move |incoming: Incoming| {
            let id = log_h
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(capture(incoming));
            let _ = notify.send(id);
            let out = reply_to_outgoing(&reply_h.read().unwrap_or_else(PoisonError::into_inner));
            Box::pin(async move { out })
        });
        Ok(Self {
            server: LocalServer::spawn(listener, handler)?,
            log,
            reply,
        })
    }

    /// Captures, newest first.
    #[must_use]
    pub fn captures(&self) -> Vec<CapturedRequest> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .newest_first()
    }

    pub fn clear(&self) {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub fn set_reply(&self, reply: WebhookReply) {
        *self.reply.write().unwrap_or_else(PoisonError::into_inner) = reply;
    }

    #[must_use]
    pub fn reply(&self) -> WebhookReply {
        self.reply
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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

    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}
