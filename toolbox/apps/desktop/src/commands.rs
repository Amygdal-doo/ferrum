//! UI commands. Network and file work runs on the services runtime; state
//! is only touched on the UI thread.

use crate::app::{Download, FerrumToolbox};
use crate::components::onboarding::{Onboarding, OnboardingEvent};
use crate::components::settings::TokenChange;
use crate::services::client_for;
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::{AppId, Settings, State, UpdateChannel, auth, store};
use semver::Version;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

impl FerrumToolbox {
    /// Starts update checks when a token is available; otherwise asks the
    /// user to sign in first (the release repo is private).
    pub fn detect_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let source = services.blocking(auth::token_source).await;
            this.update_in(cx, |this, window, cx| match source {
                Ok(Some(source)) => {
                    this.token_source = Some(source);
                    this.restart_check_loop(cx);
                }
                Ok(None) => this.show_onboarding(window, cx),
                Err(e) => {
                    this.notify_user(format!("Could not look up a GitHub token: {e}"), true, cx);
                    this.show_onboarding(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn show_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo: SharedString = format!(
            "{}/{}",
            self.state.settings.repo_owner, self.state.settings.repo_name
        )
        .into();
        let onboarding = cx.new(|cx| Onboarding::new(repo, window, cx));
        self.onboarding_subscription =
            Some(cx.subscribe(&onboarding, |this, onboarding, event, cx| {
                this.handle_onboarding(onboarding, event, cx)
            }));
        onboarding.update(cx, |onboarding, cx| onboarding.focus(window, cx));
        self.onboarding = Some(onboarding);
        cx.notify();
    }

    fn handle_onboarding(
        &mut self,
        onboarding: Entity<Onboarding>,
        event: &OnboardingEvent,
        cx: &mut Context<Self>,
    ) {
        let services = self.services.clone();
        match event {
            OnboardingEvent::Skip => self.finish_onboarding(None, cx),
            OnboardingEvent::UseGhCli => {
                cx.spawn(async move |this, cx| {
                    let signed_in = services.blocking(auth::gh_cli_signed_in).await;
                    this.update(cx, |this, cx| match signed_in {
                        Ok(true) => this.finish_onboarding(Some("Signed in with the gh CLI."), cx),
                        Ok(false) => onboarding.update(cx, |onboarding, cx| {
                            onboarding.fail(
                                "gh isn't installed or signed in. Run `gh auth login` in Terminal, then try again.",
                                cx,
                            )
                        }),
                        Err(e) => onboarding.update(cx, |onboarding, cx| onboarding.fail(e, cx)),
                    })
                    .ok();
                })
                .detach();
            }
            OnboardingEvent::SaveToken(token) => {
                let token = token.clone();
                cx.spawn(async move |this, cx| {
                    let saved = services
                        .blocking(move || auth::save_token(&token).map_err(|e| e.to_string()))
                        .await
                        .and_then(|result| result);
                    this.update(cx, |this, cx| match saved {
                        Ok(()) => this.finish_onboarding(Some("Token saved to the keychain."), cx),
                        Err(e) => onboarding.update(cx, |onboarding, cx| {
                            onboarding.fail(format!("Could not save the token: {e}"), cx)
                        }),
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

    fn finish_onboarding(&mut self, message: Option<&'static str>, cx: &mut Context<Self>) {
        self.onboarding = None;
        self.onboarding_subscription = None;
        if let Some(message) = message {
            self.notify_user(message, false, cx);
        }
        self.restart_check_loop(cx);
        cx.notify();
    }

    /// Checks on startup, then every `check_interval_hours`.
    pub fn restart_check_loop(&mut self, cx: &mut Context<Self>) {
        let hours = self.state.settings.check_interval_hours.max(1);
        self.check_loop = cx.spawn(async move |this, cx| {
            loop {
                if this
                    .update(cx, |this, cx| this.check_for_updates(false, cx))
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(u64::from(hours) * 3600))
                    .await;
            }
        });
    }

    /// Fetches the release list. `manual` also reports "up to date".
    pub fn check_for_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.checking {
            return;
        }
        self.checking = true;
        cx.notify();
        let services = self.services.clone();
        let settings = self.state.settings.clone();
        let before = self.state.updates();
        cx.spawn(async move |this, cx| {
            let result = services
                .run(async move {
                    let client = tokio::task::spawn_blocking(move || client_for(&settings))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                    let source = client_token_source(&client);
                    let releases = client.list_releases().await.map_err(|e| e.to_string())?;
                    // The catalog is optional: without it the embedded copy is used.
                    let catalog = client.fetch_catalog().await;
                    Ok::<_, String>((releases, catalog, source))
                })
                .await
                .and_then(|result| result);
            this.update(cx, |this, cx| {
                this.checking = false;
                match result {
                    Ok((releases, catalog, source)) => {
                        this.token_source = source;
                        tracing::info!(count = releases.len(), "fetched releases");
                        match catalog {
                            Ok(catalog) => {
                                tracing::info!(apps = catalog.apps.len(), "fetched catalog.json");
                                this.state.record_catalog(catalog);
                            }
                            Err(e) => tracing::warn!(
                                error = %e,
                                "catalog.json unavailable; using the cached or built-in copy"
                            ),
                        }
                        this.state.record_releases(releases);
                        this.persist(cx);
                        let updates = this.state.updates();
                        let fresh: Vec<_> =
                            updates.iter().filter(|u| !before.contains(u)).collect();
                        if !updates.is_empty() && (manual || !fresh.is_empty()) {
                            let list = updates
                                .iter()
                                .map(|(app, version)| format!("{} {version}", app.info().name))
                                .collect::<Vec<_>>()
                                .join(", ");
                            this.notify_user(format!("Updates available: {list}"), false, cx);
                        } else if manual {
                            this.notify_user("Everything is up to date.", false, cx);
                        }
                    }
                    Err(e) => this.notify_user(format!("Update check failed: {e}"), true, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Downloads and installs `version` of `app`; activates it when it is the
    /// first install or `activate` is set (updates), then prunes old versions.
    pub fn install(
        &mut self,
        app: AppId,
        version: Version,
        activate: bool,
        cx: &mut Context<Self>,
    ) {
        if self.downloads.contains_key(&app) {
            self.notify_user(
                format!("{} is already downloading.", app.info().name),
                true,
                cx,
            );
            return;
        }
        let Some(release) = self
            .state
            .releases
            .iter()
            .find(|r| r.app == app && r.version == version)
            .cloned()
        else {
            self.notify_user(
                format!("{} {version} is not in the release list.", app.info().name),
                true,
                cx,
            );
            return;
        };

        let downloaded = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let cancel = CancellationToken::new();
        let services = self.services.clone();
        let settings = self.state.settings.clone();
        let progress = {
            let downloaded = downloaded.clone();
            let total = total.clone();
            Arc::new(move |done: u64, size: Option<u64>| {
                downloaded.store(done, Ordering::Relaxed);
                total.store(size.unwrap_or(0), Ordering::Relaxed);
            })
        };
        let task_cancel = cancel.clone();
        let task = cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let result = services
                .run(async move {
                    let client = tokio::task::spawn_blocking(move || client_for(&settings))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                    store::install_release(&client, &paths, &release, progress, task_cancel)
                        .await
                        .map_err(|e| e.to_string())
                })
                .await
                .and_then(|result| result);
            this.update(cx, |this, cx| {
                this.downloads.remove(&app);
                match result {
                    Ok(record) => {
                        let name = app.info().name;
                        let version = record.version.clone();
                        this.state.record_install(app, record);
                        let make_active = activate || this.state.active(app) == Some(&version);
                        // Activate and prune in one pass so neither works on a stale copy.
                        this.update_app_state(
                            app,
                            Some(format!("Installed {name} {version}.")),
                            move |state, paths| {
                                if make_active {
                                    state.activate(app, &version, paths)?;
                                }
                                state.prune(app, paths).map(|_| ())
                            },
                            cx,
                        );
                    }
                    Err(e) if e.contains("cancelled") => {
                        this.notify_user("Download cancelled.", false, cx)
                    }
                    Err(e) => this.notify_user(format!("Install failed: {e}"), true, cx),
                }
                cx.notify();
            })
            .ok();
        });
        self.downloads.insert(
            app,
            Download {
                version,
                downloaded,
                total,
                cancel,
                _task: task,
            },
        );
        self.ensure_progress_ticker(cx);
        cx.notify();
    }

    pub fn cancel_download(&mut self, app: AppId, cx: &mut Context<Self>) {
        if let Some(download) = self.downloads.get(&app) {
            download.cancel.cancel();
        }
        cx.notify();
    }

    /// Runs a file-system operation on a copy of the app's state off the UI
    /// thread, then adopts the result for that app.
    fn update_app_state(
        &mut self,
        app: AppId,
        success: Option<String>,
        operation: impl FnOnce(&mut State, &ferrumtoolbox_core::Paths) -> ferrumtoolbox_core::Result<()>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        let mut working = self.state.clone();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let result = services
                .blocking(move || {
                    operation(&mut working, &paths)
                        .map(|()| working.app(app))
                        .map_err(|e| e.to_string())
                })
                .await
                .and_then(|result| result);
            this.update(cx, |this, cx| {
                match result {
                    Ok(mut app_state) => {
                        // The channel may have changed while the work ran.
                        app_state.channel = this.state.channel(app);
                        this.state.apps.insert(app, app_state);
                        this.persist(cx);
                        if let Some(message) = success {
                            this.notify_user(message, false, cx);
                        }
                    }
                    Err(e) => this.notify_user(e, true, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn activate(&mut self, app: AppId, version: Version, cx: &mut Context<Self>) {
        self.update_app_state(
            app,
            None,
            move |state, paths| state.activate(app, &version, paths),
            cx,
        );
    }

    pub fn uninstall(&mut self, app: AppId, version: Version, cx: &mut Context<Self>) {
        let message = format!("Removed {} {version}.", app.info().name);
        self.update_app_state(
            app,
            Some(message),
            move |state, paths| state.uninstall(app, &version, paths),
            cx,
        );
    }

    fn prune(&mut self, app: AppId, cx: &mut Context<Self>) {
        if self.state.prune_candidates(app).is_empty() {
            return;
        }
        self.update_app_state(
            app,
            None,
            move |state, paths| state.prune(app, paths).map(|_| ()),
            cx,
        );
    }

    /// Switches which releases `app` follows; updates are re-evaluated at once.
    pub fn set_channel(&mut self, app: AppId, channel: UpdateChannel, cx: &mut Context<Self>) {
        if self.state.channel(app) == channel {
            return;
        }
        self.state.set_channel(app, channel);
        self.persist(cx);
        if let Some(update) = self.state.update_for(app) {
            let message = format!(
                "{} {} is available on {}.",
                app.info().name,
                update.version,
                channel.label()
            );
            self.notify_user(message, false, cx);
        }
        cx.notify();
    }

    pub fn launch(&mut self, app: AppId, cx: &mut Context<Self>) {
        match store::launch(&self.state, app, &self.services.paths) {
            Ok(()) => self.notify_user(format!("Launched {}.", app.info().name), false, cx),
            Err(e) => self.notify_user(format!("Could not launch: {e}"), true, cx),
        }
    }

    /// Saves settings and (optionally) the token, then re-checks.
    pub fn apply_settings(
        &mut self,
        settings: Settings,
        token: TokenChange,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let interval_changed =
            settings.check_interval_hours != self.state.settings.check_interval_hours;
        let repo_changed = settings.repo_owner != self.state.settings.repo_owner
            || settings.repo_name != self.state.settings.repo_name;
        self.state.settings = settings;
        if repo_changed {
            self.state.releases.clear();
            self.state.last_check = None;
        }
        self.persist(cx);
        for app in AppId::ALL {
            self.prune(app, cx);
        }

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let token_result = match token {
                TokenChange::Keep => Ok(None),
                TokenChange::Set(value) => services
                    .blocking(move || auth::save_token(&value).map_err(|e| e.to_string()))
                    .await
                    .and_then(|r| r)
                    .map(|()| Some("Token saved to the keychain.")),
                TokenChange::Clear => services
                    .blocking(|| auth::clear_token().map_err(|e| e.to_string()))
                    .await
                    .and_then(|r| r)
                    .map(|()| Some("Token removed from the keychain.")),
            };
            this.update(cx, |this, cx| {
                match token_result {
                    Ok(Some(message)) => this.notify_user(message, false, cx),
                    Ok(None) => {}
                    Err(e) => {
                        this.notify_user(format!("Could not update the token: {e}"), true, cx)
                    }
                }
                if interval_changed {
                    this.restart_check_loop(cx);
                } else {
                    this.check_for_updates(false, cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Writes `state.json` off the UI thread.
    pub fn persist(&mut self, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let result = services
                .blocking(move || state.save(&paths).map_err(|e| e.to_string()))
                .await
                .and_then(|r| r);
            if let Err(e) = result {
                this.update(cx, |this, cx| {
                    this.notify_user(format!("Could not save state: {e}"), true, cx)
                })
                .ok();
            }
        })
        .detach();
    }
}

fn client_token_source(
    client: &ferrumtoolbox_core::GitHubClient,
) -> Option<ferrumtoolbox_core::TokenSource> {
    client.token_source()
}
