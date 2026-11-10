//! Webhook inspector: start / stop the listener, refresh the capture list
//! as requests arrive, edit the reply, open a capture as a request.

use crate::app::FerrumApi;
use crate::state::tools::{WebhookState, parse_port};
use ferrumapi_application::WebhookInspector;
use ferrumapi_domain::webhook::DEFAULT_CAPTURE_LIMIT;
use gpui::Context;

impl FerrumApi {
    pub fn start_webhooks(&mut self, cx: &mut Context<Self>) {
        let w = &mut self.state.webhooks;
        if w.inspector.is_some() || w.starting {
            return;
        }
        let checked =
            parse_port(&w.port_text).and_then(|port| w.reply().map(|reply| (port, reply)));
        let (port, reply) = match checked {
            Ok(ok) => ok,
            Err(e) => {
                w.error = Some(e);
                return;
            }
        };
        w.starting = true;
        w.error = None;
        w.session += 1;
        let session = w.session;

        let (notify, mut rx) = tokio::sync::mpsc::unbounded_channel();
        self.io(
            cx,
            async move { WebhookInspector::start(port, reply, DEFAULT_CAPTURE_LIMIT, notify).await },
            move |this, result, cx| {
                let w = &mut this.state.webhooks;
                w.starting = false;
                match result {
                    Ok(inspector) if w.session == session => {
                        w.inspector = Some(inspector);
                        w.refresh();
                    }
                    Ok(_) => {}
                    Err(e) => w.error = Some(e),
                }
                cx.notify();
            },
        );
        cx.spawn(async move |this, cx| {
            while rx.recv().await.is_some() {
                // Coalesce bursts into one refresh.
                while rx.try_recv().is_ok() {}
                let update = this.update(cx, |this, cx| {
                    if this.state.webhooks.session == session {
                        this.state.webhooks.refresh();
                        cx.notify();
                    }
                });
                if update.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn stop_webhooks(&mut self) {
        let w = &mut self.state.webhooks;
        w.session += 1;
        w.starting = false;
        if let Some(inspector) = w.inspector.take() {
            inspector.stop();
        }
    }

    pub fn clear_webhooks(&mut self) {
        let w = &mut self.state.webhooks;
        if let Some(inspector) = &w.inspector {
            inspector.clear();
        }
        w.captures.clear();
        w.selected = None;
    }

    /// Applies a reply-form edit; a running inspector uses it right away.
    pub fn edit_webhook_reply(&mut self, f: impl FnOnce(&mut WebhookState)) {
        let w = &mut self.state.webhooks;
        f(w);
        match w.reply() {
            Ok(reply) => {
                w.error = None;
                if let Some(inspector) = &w.inspector {
                    inspector.set_reply(reply);
                }
            }
            Err(e) => w.error = Some(e),
        }
    }

    /// Copies capture `id` into a new unsaved request tab.
    pub fn open_capture(&mut self, id: u64) {
        let w = &self.state.webhooks;
        let Some(capture) = w.captures.iter().find(|c| c.id == id) else {
            return;
        };
        let base = w.inspector.as_ref().map_or_else(
            || format!("http://127.0.0.1:{}", w.port_text.trim()),
            WebhookInspector::base_url,
        );
        let request = capture.to_request(&base);
        let title = format!("{} {}", capture.method, capture.path);
        self.open_request(title, request, None);
        self.state.dialog = None;
    }
}
