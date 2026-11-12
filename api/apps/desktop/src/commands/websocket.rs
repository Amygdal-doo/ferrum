//! WebSocket sessions: connect, pump incoming frames into the tab's log,
//! send, disconnect.

use super::background;
use crate::app::FerrumApi;
use crate::state::protocols::{WsSession, WsStatus};
use ferrumapi_application::domain::protocol::WsPayloadKind;
use ferrumapi_application::domain::protocol::websocket::{WsFrame, WsLogEntry};
use ferrumapi_application::http::ws::{WsOutgoing, outgoing};
use ferrumapi_domain::TabId;
use gpui::Context;

impl FerrumApi {
    /// Connect, or disconnect when connected / connecting.
    pub fn ws_toggle(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        match self.state.protocols.ws_status(tab_id) {
            Some(WsStatus::Open | WsStatus::Connecting) => self.ws_disconnect(tab_id),
            _ => self.ws_connect(tab_id, cx),
        }
    }

    pub fn ws_connect(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        if tab.request.url.trim().is_empty() {
            self.notify("Enter a ws:// or wss:// URL first.", true, cx);
            return;
        }
        let prepared = self.state.prepare_tab(&tab);
        let protocols = &mut self.state.protocols;
        protocols.ws_generation += 1;
        let generation = protocols.ws_generation;
        let mut session = WsSession::connecting(generation);
        if let Some(old) = protocols.ws.remove(&tab_id) {
            session.log = old.log;
        }
        session
            .log
            .push(WsLogEntry::event(WsFrame::Info, "Connecting…"));
        protocols.ws.insert(tab_id, session);

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let (authored, origin) = (tab.request.clone(), tab.origin);
            let result = background(&services, async move {
                svc.ws_connect(authored, prepared, origin).await
            })
            .await;
            let rx = this
                .update(cx, |this, cx| {
                    let session = this
                        .state
                        .protocols
                        .ws
                        .get_mut(&tab_id)
                        .filter(|s| s.generation == generation)?;
                    let rx = match result {
                        Ok(Ok((connected, client, rx))) => {
                            session.status = WsStatus::Open;
                            session.client = Some(client);
                            let mut text = format!("Connected (HTTP {})", connected.status);
                            if let Some(p) = &connected.protocol {
                                text.push_str(&format!(" · subprotocol {p}"));
                            }
                            session.protocol = connected.protocol;
                            session.log.push(WsLogEntry::event(WsFrame::Info, text));
                            Some(rx)
                        }
                        Ok(Err(e)) => {
                            session.status = WsStatus::Closed;
                            session
                                .log
                                .push(WsLogEntry::event(WsFrame::Error, e.to_string()));
                            None
                        }
                        Err(e) => {
                            session.status = WsStatus::Closed;
                            session.log.push(WsLogEntry::event(WsFrame::Error, e));
                            None
                        }
                    };
                    this.refresh_history(cx);
                    cx.notify();
                    rx
                })
                .ok()
                .flatten();
            let Some(mut rx) = rx else {
                return;
            };
            while let Some(event) = rx.recv().await {
                let keep = this
                    .update(cx, |this, cx| {
                        let Some(session) = this
                            .state
                            .protocols
                            .ws
                            .get_mut(&tab_id)
                            .filter(|s| s.generation == generation)
                        else {
                            return false;
                        };
                        let ended = session.receive(event);
                        cx.notify();
                        !ended
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    /// Closes the socket (code 1000); the log keeps the "Disconnected" line.
    pub fn ws_disconnect(&mut self, tab_id: TabId) {
        if let Some(session) = self.state.protocols.ws.get_mut(&tab_id) {
            if session.status == WsStatus::Connecting {
                // A later handshake result is ignored (generation changes).
                session.generation = 0;
                session.status = WsStatus::Closed;
                session.log.push(WsLogEntry::event(
                    WsFrame::Info,
                    "Connection attempt cancelled",
                ));
            }
            session.client = None;
        }
    }

    /// Sends a payload (variables substituted) on the open socket.
    pub fn ws_send(
        &mut self,
        tab_id: TabId,
        kind: WsPayloadKind,
        payload: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        let resolved = self.state.prepare_tab(&tab).variables.resolve(payload);
        let message = match outgoing(kind, &resolved) {
            Ok(m) => m,
            Err(e) => {
                self.notify(format!("Not sent: {e}"), true, cx);
                return;
            }
        };
        let Some(session) = self.state.protocols.ws.get_mut(&tab_id) else {
            self.notify("Connect first.", true, cx);
            return;
        };
        let Some(client) = session.client.clone() else {
            self.notify("Connect first.", true, cx);
            return;
        };
        let (frame, text, size) = match &message {
            WsOutgoing::Text(t) => (WsFrame::Text, t.clone(), t.len()),
            WsOutgoing::Binary(b) => (WsFrame::Binary, resolved.trim().to_owned(), b.len()),
            WsOutgoing::Ping(p) => (WsFrame::Ping, String::new(), p.len()),
        };
        if client.send(message) {
            session.sent(frame, text, size);
        } else {
            self.notify("The connection is closed.", true, cx);
        }
    }
}
