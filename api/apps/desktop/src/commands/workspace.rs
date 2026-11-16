//! Startup, autosave, settings and tab management.

use super::{background, run};
use crate::app::FerrumApi;
use crate::state::tabs;
use ferrumapi_application::DEFAULT_HISTORY_LIMIT;
use ferrumapi_domain::{HttpRequest, RequestOrigin, RequestTab, TabId, WorkspaceState};
use gpui::{Context, Task};
use std::time::Duration;

const AUTOSAVE_INTERVAL: Duration = Duration::from_millis(1500);

impl FerrumApi {
    /// Loads everything. Never fails hard: on error the app starts empty and
    /// shows a notice.
    pub fn bootstrap(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let loaded = background(&services, async move {
                let s = &svc.store;
                let mut collections = s.list_collections().await;
                let mut workspace = s.load_workspace().await;
                // Auth secrets come from the keychain, not the database.
                if let Ok(c) = &mut collections {
                    svc.hydrate_collections(c).await;
                }
                if let Ok(w) = &mut workspace {
                    let auths = w.tabs.iter_mut().map(|t| &mut t.request.auth).collect();
                    svc.hydrate_secrets(auths).await;
                }
                (
                    s.load_settings().await,
                    workspace,
                    collections,
                    s.list_environments().await,
                    s.load_globals().await,
                )
            })
            .await;
            let result = this.update(cx, |this, cx| {
                let state = &mut this.state;
                let mut ws = WorkspaceState::default();
                let mut errors = Vec::new();
                match loaded {
                    Ok((settings, workspace, collections, envs, globals)) => {
                        match settings {
                            Ok(s) => state.settings = s,
                            Err(e) => errors.push(format!("settings: {e}")),
                        }
                        match collections {
                            Ok(c) => state.collections = c,
                            Err(e) => errors.push(format!("collections: {e}")),
                        }
                        match envs {
                            Ok(e) => state.environments = e,
                            Err(e) => errors.push(format!("environments: {e}")),
                        }
                        match globals {
                            Ok(g) => state.globals = g,
                            Err(e) => errors.push(format!("globals: {e}")),
                        }
                        match workspace {
                            Ok(w) => ws = w,
                            Err(e) => errors.push(format!("open tabs: {e}")),
                        }
                    }
                    Err(e) => errors.push(e),
                }
                state.project = ws.project.clone();
                state.active_env = ws
                    .active_environment
                    .filter(|id| state.environments.iter().any(|e| e.id == *id));
                let (mut restored, active) = tabs::restore(ws);
                tabs::reconcile_origins(&mut restored, &state.collections);
                state.tabs = restored;
                state.active_tab = active;
                state.loaded = true;
                if !errors.is_empty() {
                    this.notify(format!("Could not load {}", errors.join(", ")), true, cx);
                }
                this.refresh_history(cx);
                cx.notify();
            });
            if let Err(error) = result {
                tracing::warn!(%error, "window closed during startup");
            }
        })
        .detach();
    }

    /// Persists the workspace about 1.5 s after a change.
    pub fn autosave_loop(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut saved_rev = 0u64;
            loop {
                cx.background_executor().timer(AUTOSAVE_INTERVAL).await;
                let Ok(pending) = this.update(cx, |this, _| {
                    let edits: Vec<_> = this.state.secret_edits.drain().map(|(_, a)| a).collect();
                    let s = &this.state;
                    (s.loaded && (s.ws_rev != saved_rev || !edits.is_empty())).then(|| {
                        (
                            s.ws_rev,
                            this.services.clone(),
                            edits,
                            WorkspaceState {
                                tabs: s.tabs.clone(),
                                active_tab: s.active_tab,
                                active_environment: s.active_env,
                                project: s.project.clone(),
                            },
                        )
                    })
                }) else {
                    return;
                };
                let Some((rev, services, edits, ws)) = pending else {
                    continue;
                };
                let svc = services.clone();
                match run(&services, async move {
                    // Secrets to the keychain: what was typed replaces, the
                    // tabs' (possibly unloaded) secrets only merge.
                    let tabs: Vec<_> = ws.tabs.iter().map(|t| t.request.auth.clone()).collect();
                    for result in [
                        svc.replace_secrets(edits).await,
                        svc.stash_secrets(tabs).await,
                    ] {
                        if let Err(e) = result {
                            tracing::warn!(error = %e, "could not save auth secrets");
                        }
                    }
                    svc.store.save_workspace(&ws).await
                })
                .await
                {
                    Ok(()) => saved_rev = rev,
                    Err(e) => tracing::warn!(error = %e, "failed to save workspace"),
                }
            }
        })
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let settings = self.state.settings;
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.save_settings(&settings).await },
            Self::report("Could not save settings"),
        );
    }

    /// History limit from settings.
    #[must_use]
    pub fn history_limit(&self) -> usize {
        match self.state.settings.history_limit {
            0 => DEFAULT_HISTORY_LIMIT,
            n => n as usize,
        }
    }

    pub fn activate(&mut self, id: TabId) {
        self.state.active_tab = Some(id);
        self.state.touch();
    }

    pub fn new_tab(&mut self) {
        self.push_tab(tabs::blank_tab());
    }

    /// A new, unsaved tab with an empty request of `kind`.
    pub fn new_tab_of_kind(&mut self, kind: ferrumapi_domain::RequestKind) {
        self.push_tab(RequestTab::new(String::new(), HttpRequest::of_kind(kind)));
    }

    pub fn push_tab(&mut self, tab: RequestTab) {
        let id = tab.id;
        self.state.tabs.push(tab);
        self.activate(id);
    }

    /// Opens `request` in a new tab (or focuses the tab already showing `origin`).
    pub fn open_request(
        &mut self,
        title: String,
        request: HttpRequest,
        origin: Option<RequestOrigin>,
    ) {
        if let Some(o) = origin
            && let Some(id) = tabs::find_by_origin(&self.state.tabs, o)
        {
            self.activate(id);
            return;
        }
        let mut tab = RequestTab::new(title, request);
        tab.origin = origin;
        self.push_tab(tab);
    }

    pub fn close_tab(&mut self, id: TabId) {
        let state = &mut self.state;
        if let Some(token) = state.cancels.remove(&id) {
            token.cancel();
        }
        state.responses.remove(&id);
        state.protocols.forget(id);
        let next = tabs::close(&mut state.tabs, state.active_tab, id);
        state.active_tab = next;
        state.touch();
    }
}
