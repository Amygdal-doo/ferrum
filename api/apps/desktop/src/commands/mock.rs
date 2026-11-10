//! Mock server per collection: load / save definitions, start / stop the
//! server on the services runtime, and stream its log into the state.

use crate::app::FerrumApi;
use crate::state::Dialog;
use crate::state::tools::{MockState, parse_port};
use ferrumapi_application::plan_mock_routes;
use ferrumapi_domain::CollectionId;
use ferrumapi_domain::mock::MockRoute;
use gpui::Context;

impl FerrumApi {
    /// Shows the mock server dialog of `collection`, loading its saved
    /// definitions the first time.
    pub fn open_mock_server(&mut self, collection: CollectionId, cx: &mut Context<Self>) {
        let first = self
            .state
            .collection(collection)
            .and_then(|c| c.requests().first().map(|r| r.id));
        let state = self
            .state
            .mocks
            .entry(collection)
            .or_insert_with(|| MockState::new(collection));
        if state.selected.is_none() {
            state.selected = first;
        }
        let load = !state.loaded;
        self.state.dialog = Some(Dialog::MockServer(collection));
        if load {
            let svc = self.services.clone();
            self.io(
                cx,
                async move { svc.load_mocks(collection).await },
                move |this, result, cx| match result {
                    Ok(mocks) => {
                        if let Some(s) = this.state.mocks.get_mut(&collection) {
                            s.set_loaded(mocks);
                        }
                    }
                    Err(e) => this.notify(format!("Could not load mocks: {e}"), true, cx),
                },
            );
        }
    }

    /// Routes from the current draft and variables.
    fn mock_routes(&self, collection: CollectionId) -> Vec<MockRoute> {
        let state = &self.state;
        match (state.collection(collection), state.mocks.get(&collection)) {
            (Some(c), Some(m)) => {
                plan_mock_routes(c, &m.draft, &state.globals, state.active_environment())
            }
            _ => Vec::new(),
        }
    }

    /// Applies `f` to the collection's mock state; a running server picks
    /// up the new routes immediately.
    pub fn edit_mocks(&mut self, collection: CollectionId, f: impl FnOnce(&mut MockState)) {
        if let Some(s) = self.state.mocks.get_mut(&collection) {
            f(s);
        }
        let routes = self.mock_routes(collection);
        if let Some(server) = self
            .state
            .mocks
            .get(&collection)
            .and_then(|s| s.server.as_ref())
        {
            server.set_routes(routes);
        }
    }

    /// Persists the draft when it has unsaved edits.
    pub fn save_mocks(&mut self, collection: CollectionId, cx: &mut Context<Self>) {
        let Some(s) = self.state.mocks.get_mut(&collection).filter(|s| s.dirty) else {
            return;
        };
        s.dirty = false;
        let mocks = s.draft.clone();
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.save_mocks(collection, mocks).await },
            Self::report("Could not save mocks"),
        );
    }

    pub fn close_mock_dialog(&mut self, collection: CollectionId, cx: &mut Context<Self>) {
        self.save_mocks(collection, cx);
        self.state.dialog = None;
    }

    pub fn start_mock_server(&mut self, collection: CollectionId, cx: &mut Context<Self>) {
        let routes = self.mock_routes(collection);
        let Some(s) = self.state.mocks.get_mut(&collection) else {
            return;
        };
        if s.server.is_some() || s.starting {
            return;
        }
        let port = match parse_port(&s.port_text) {
            Ok(port) => port,
            Err(e) => {
                s.error = Some(e);
                return;
            }
        };
        s.starting = true;
        s.error = None;
        s.session += 1;
        let session = s.session;
        self.save_mocks(collection, cx);

        let (log, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.start_mock_server(port, routes, log).await },
            move |this, result, cx| {
                let Some(s) = this.state.mocks.get_mut(&collection) else {
                    return;
                };
                s.starting = false;
                match result {
                    Ok(server) if s.session == session => s.server = Some(server),
                    // Stopped or restarted while starting: dropping stops it.
                    Ok(_) => {}
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            },
        );
        cx.spawn(async move |this, cx| {
            while let Some(entry) = rx.recv().await {
                let update = this.update(cx, |this, cx| {
                    if let Some(s) = this
                        .state
                        .mocks
                        .get_mut(&collection)
                        .filter(|s| s.session == session)
                    {
                        s.push_log(entry);
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

    pub fn stop_mock_server(&mut self, collection: CollectionId) {
        if let Some(s) = self.state.mocks.get_mut(&collection) {
            s.session += 1;
            s.starting = false;
            if let Some(server) = s.server.take() {
                server.stop();
            }
        }
    }

    /// Running mock servers as (collection name, base URL).
    #[must_use]
    pub fn running_mocks(&self) -> Vec<(CollectionId, String)> {
        self.state
            .mocks
            .values()
            .filter_map(|s| s.running_url().map(|url| (s.collection, url)))
            .collect()
    }
}
