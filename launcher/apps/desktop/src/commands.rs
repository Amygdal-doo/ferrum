//! UI commands. Network and file work runs on the services runtime; state
//! is only touched on the UI thread.

use crate::app::{Download, FerrumLauncher, NewAppNotice};
use crate::components::onboarding::{Onboarding, OnboardingEvent};
use crate::services::client_for;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, Settings, State, UpdateChannel, auth, current_target, store};
use semver::Version;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

impl FerrumLauncher {
    /// Starts update checks when a token is available or the public release
    /// repo is used; otherwise asks the user to sign in first.
    pub fn detect_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let services = self.services.clone();
        // A token Ferrum Toolbox saved is copied to the launcher's keychain
        // entry, once (a denied keychain prompt isn't repeated).
        let migrate = !self.state.settings.legacy_keychain_checked;
        if migrate {
            self.state.settings.legacy_keychain_checked = true;
            self.persist(cx);
        }
        cx.spawn_in(window, async move |this, cx| {
            let source = services
                .blocking(move || {
                    if migrate {
                        match auth::migrate_token(&auth::Keychain) {
                            Ok(true) => tracing::info!("copied Ferrum Toolbox's GitHub token"),
                            Ok(false) => {}
                            Err(e) => tracing::warn!(error = %e, "could not copy Ferrum Toolbox's GitHub token"),
                        }
                    }
                    auth::token_source()
                })
                .await;
            this.update_in(cx, |this, window, cx| match source {
                Ok(Some(source)) => {
                    this.token_source = Some(source);
                    this.restart_check_loop(cx);
                }
                Ok(None) if this.state.settings.uses_public_repo() => this.restart_check_loop(cx),
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
                                this.announce_new_apps(cx);
                                this.load_logos(cx);
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
                                .map(|(app, version)| {
                                    format!("{} {version}", this.state.info(*app).name)
                                })
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

    /// Tells the user about apps that appeared since the last check (once
    /// per app; their rows keep a "New" badge until opened).
    pub fn announce_new_apps(&mut self, cx: &mut Context<Self>) {
        let before = self.state.settings.clone();
        let fresh = self.state.discover_apps();
        if self.state.settings != before {
            self.persist(cx);
        }
        let Some(first) = fresh.first().copied() else {
            return;
        };
        let names = fresh
            .iter()
            .map(|app| self.state.info(*app).name)
            .collect::<Vec<_>>()
            .join(", ");
        let message = if fresh.len() == 1 {
            format!("New app: {names} is available")
        } else {
            format!("New apps: {names} are available")
        };
        tracing::info!(apps = %names, "new apps in the catalog");
        self.new_app_notice = Some(NewAppNotice {
            app: first,
            message: message.into(),
        });
        cx.notify();
    }

    /// The new-app banner's **Install**: installs the newest release when
    /// there is a build for this platform, otherwise opens the app's page.
    pub fn install_new_app(&mut self, app: AppId, cx: &mut Context<Self>) {
        self.new_app_notice = None;
        let installable = self.state.latest(app).filter(|latest| {
            self.state.active(app).is_none()
                && current_target().is_some_and(|target| latest.assets_for(target).is_some())
        });
        match installable.map(|latest| latest.version.clone()) {
            Some(version) => self.install(app, version, true, cx),
            None => self.select_app(app, cx),
        }
        cx.notify();
    }

    /// Installs and activates every pending app update at once (the
    /// launcher's own update stays a separate button: it restarts the app).
    pub fn update_all(&mut self, cx: &mut Context<Self>) {
        let updates = self.state.app_updates();
        if updates.is_empty() {
            self.notify_user("Every app is up to date.", false, cx);
            return;
        }
        for (app, version) in updates {
            if !self.downloads.contains_key(&app) {
                self.install(app, version, true, cx);
            }
        }
        cx.notify();
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
        let info = self.state.info(app);
        if self.downloads.contains_key(&app) {
            self.notify_user(format!("{} is already downloading.", info.name), true, cx);
            return;
        }
        if self.state.mark_seen(app) {
            self.persist(cx);
        }
        let Some(release) = self
            .state
            .releases
            .iter()
            .find(|r| r.app == app && r.version == version)
            .cloned()
        else {
            self.notify_user(
                format!("{} {version} is not in the release list.", info.name),
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
        let task_version = version.clone();
        let task = cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let result = services
                .run(async move {
                    let client = tokio::task::spawn_blocking(move || client_for(&settings))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                    store::install_release(&client, &paths, &info, &release, progress, task_cancel)
                        .await
                        .map_err(|e| e.to_string())
                })
                .await
                .and_then(|result| result);
            this.update(cx, |this, cx| {
                this.downloads.remove(&app);
                let self_update = app == AppId::LAUNCHER
                    && this
                        .self_update
                        .take()
                        .is_some_and(|wanted| wanted == task_version);
                match result {
                    Ok(record) if self_update => {
                        let version = record.version.clone();
                        this.state.record_install(app, record);
                        this.persist(cx);
                        this.finish_self_update(&version, cx);
                    }
                    Ok(record) => {
                        let name = this.state.info(app).name;
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

    /// Downloads `version` of the launcher, then swaps it for the running
    /// bundle and restarts (see [`Self::finish_self_update`]).
    pub fn update_launcher(&mut self, version: Version, cx: &mut Context<Self>) {
        self.self_update = Some(version.clone());
        // Not activated: that would publish a second copy into
        // ~/Applications/Ferrum next to the one being replaced.
        self.install(AppId::LAUNCHER, version, false, cx);
        if !self.downloads.contains_key(&AppId::LAUNCHER) {
            self.self_update = None;
        }
        cx.notify();
    }

    fn finish_self_update(&mut self, version: &Version, cx: &mut Context<Self>) {
        let new_bundle = self.services.paths.bundle_path(AppId::LAUNCHER, version);
        let swap = match (new_bundle, self.running_bundle.clone()) {
            (Some(new_bundle), Some(running)) => {
                ferrumlauncher_core::self_update::replace_and_relaunch(&new_bundle, &running)
                    .map_err(|e| e.to_string())
            }
            (None, _) => Err("the release has no macOS app bundle".into()),
            (_, None) => Err("this copy of the launcher isn't running from an app bundle".into()),
        };
        match swap {
            Ok(()) => {
                self.notify_user(
                    format!("Restarting into Ferrum Launcher {version}…"),
                    false,
                    cx,
                );
                // A moment for the notice to show; the swap script waits for
                // this process to exit.
                cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(800))
                        .await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
            Err(e) => self.notify_user(
                format!("Downloaded Launcher {version}, but couldn't replace this app: {e}"),
                true,
                cx,
            ),
        }
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
        operation: impl FnOnce(
            &mut State,
            &ferrumlauncher_core::Paths,
        ) -> ferrumlauncher_core::Result<()>
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
        let message = format!("Removed {} {version}.", self.state.info(app).name);
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
                self.state.info(app).name,
                update.version,
                channel.label()
            );
            self.notify_user(message, false, cx);
        }
        cx.notify();
    }

    pub fn launch(&mut self, app: AppId, cx: &mut Context<Self>) {
        match store::launch(&self.state, app, &self.services.paths) {
            Ok(()) => self.notify_user(
                format!("Launched {}.", self.state.info(app).name),
                false,
                cx,
            ),
            Err(e) => self.notify_user(format!("Could not launch: {e}"), true, cx),
        }
    }

    /// Saves settings, then re-checks.
    pub fn apply_settings(&mut self, settings: Settings, _: &mut Window, cx: &mut Context<Self>) {
        // The form edits these three; everything else (release repository,
        // seen apps, migration flags, channels revision) stays as it is.
        let settings = Settings {
            check_interval_hours: settings.check_interval_hours,
            keep_versions: settings.keep_versions,
            menu_bar: settings.menu_bar,
            ..self.state.settings.clone()
        };
        let menu_bar_changed = settings.menu_bar != self.state.settings.menu_bar;
        let interval_changed =
            settings.check_interval_hours != self.state.settings.check_interval_hours;
        self.state.settings = settings;
        if menu_bar_changed {
            self.refresh_menu_bar(cx);
        }
        self.persist(cx);
        let installed: Vec<AppId> = self.state.apps.keys().copied().collect();
        for app in installed {
            self.prune(app, cx);
        }

        if interval_changed {
            self.restart_check_loop(cx);
        } else {
            self.check_for_updates(false, cx);
        }
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
    client: &ferrumlauncher_core::GitHubClient,
) -> Option<ferrumlauncher_core::TokenSource> {
    client.token_source()
}
