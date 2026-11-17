//! Settings › Launcher › Download updates in the background: after each
//! check the launcher installs app updates beside the active versions
//! (verified like any install), then switches each app whose update is
//! staged as soon as it isn't running. A running app's page and overview
//! row show **Restart to update**, which asks before quitting it (see
//! `ferrumlauncher_core::auto_update`). The launcher's own update keeps its
//! separate button: it replaces the running launcher.

use crate::app::FerrumLauncher;
use ferrum_ui::components::TintColor;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, auto_update, store};
use semver::Version;
use std::time::Duration;

/// How long "Restart to update" waits for the app to quit.
const QUIT_TIMEOUT: Duration = Duration::from_secs(30);
const QUIT_POLL: Duration = Duration::from_millis(500);

impl FerrumLauncher {
    /// Downloads every app update not staged yet (after a check, with the
    /// setting on).
    pub(crate) fn download_updates_in_background(&mut self, cx: &mut Context<Self>) {
        for (app, version) in self.state.background_downloads() {
            if self.downloads.contains_key(&app) {
                continue;
            }
            self.background_downloads.insert(app);
            self.install(app, version, false, cx);
            if !self.downloads.contains_key(&app) {
                self.background_downloads.remove(&app);
            }
        }
    }

    /// A background download finished: stage it, then switch to it unless
    /// the app is running.
    pub(crate) fn stage_downloaded(
        &mut self,
        app: AppId,
        version: Version,
        cx: &mut Context<Self>,
    ) {
        self.state.stage_update(app, version);
        self.persist(cx);
        self.apply_staged(app, false, cx);
    }

    /// Switches every staged app that isn't running (the window became
    /// active, a check finished).
    pub(crate) fn apply_staged_updates(&mut self, cx: &mut Context<Self>) {
        let staged: Vec<AppId> = self
            .state
            .apps
            .keys()
            .copied()
            .filter(|app| self.state.staged_update(*app).is_some())
            .collect();
        for app in staged {
            self.apply_staged(app, false, cx);
        }
    }

    /// Activates `app`'s staged update if the app isn't running; otherwise
    /// says it is ready (once per download, unless `asked`).
    fn apply_staged(&mut self, app: AppId, asked: bool, cx: &mut Context<Self>) {
        if self.state.staged_update(app).is_none() || self.quitting.contains(&app) {
            return;
        }
        let info = self.state.info(app);
        let platform = self.services.paths.platform;
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let running = services
                .blocking(move || auto_update::running_pids(platform, &info))
                .await
                .and_then(|r| r.map_err(|e| e.to_string()));
            this.update(cx, |this, cx| match running {
                Ok(pids) if pids.is_empty() => this.activate_staged(app, false, cx),
                Ok(_) if asked => this.restart_dialog = Some(app),
                Ok(_) => {
                    if let Some(version) = this.state.staged_update(app)
                        && this.ready_told.insert((app, version.clone()))
                    {
                        let name = this.state.info(app).name;
                        this.notify_user(
                            format!("{name} {version} is ready: Restart to update on its page."),
                            false,
                            cx,
                        );
                    }
                }
                Err(e) => tracing::warn!(error = %e, %app, "could not list running apps"),
            })
            .ok();
        })
        .detach();
    }

    /// "Restart to update": switches at once when the app isn't running,
    /// else asks before quitting it.
    pub fn restart_to_update(&mut self, app: AppId, cx: &mut Context<Self>) {
        self.apply_staged(app, true, cx);
    }

    pub fn close_restart_dialog(&mut self, cx: &mut Context<Self>) {
        self.restart_dialog = None;
        cx.notify();
    }

    /// The user confirmed: asks the app to quit, waits for it, then
    /// switches versions and opens it again. An app that doesn't quit (it
    /// asked about unsaved work and the user stayed) keeps its version.
    fn quit_and_update(&mut self, app: AppId, cx: &mut Context<Self>) {
        self.restart_dialog = None;
        if !self.quitting.insert(app) {
            return;
        }
        let info = self.state.info(app);
        let name = info.name.clone();
        let platform = self.services.paths.platform;
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let quit = {
                let info = info.clone();
                services
                    .blocking(move || {
                        let pids = auto_update::running_pids(platform, &info)?;
                        auto_update::request_quit(platform, &info, &pids)
                    })
                    .await
                    .and_then(|r| r.map_err(|e| e.to_string()))
            };
            let mut waited = Duration::ZERO;
            let quit_ok = loop {
                if let Err(e) = &quit {
                    break Err(e.clone());
                }
                let info = info.clone();
                let running = services
                    .blocking(move || auto_update::running_pids(platform, &info))
                    .await
                    .and_then(|r| r.map_err(|e| e.to_string()));
                match running {
                    Ok(pids) if pids.is_empty() => break Ok(()),
                    Ok(_) if waited >= QUIT_TIMEOUT => {
                        break Err(format!("{name} is still running"));
                    }
                    Ok(_) => {}
                    Err(e) => break Err(e),
                }
                cx.background_executor().timer(QUIT_POLL).await;
                waited += QUIT_POLL;
            };
            this.update(cx, |this, cx| {
                this.quitting.remove(&app);
                match quit_ok {
                    Ok(()) => this.activate_staged(app, true, cx),
                    Err(e) => this.notify_user(
                        format!("{e}; quit it to finish the update, or try again."),
                        true,
                        cx,
                    ),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Makes the staged version active (and prunes); `relaunch` opens the
    /// app again afterwards.
    fn activate_staged(&mut self, app: AppId, relaunch: bool, cx: &mut Context<Self>) {
        let Some(version) = self.state.staged_update(app) else {
            return;
        };
        let name = self.state.info(app).name;
        let services = self.services.clone();
        let paths = services.paths.clone();
        let mut state = self.state.clone();
        cx.spawn(async move |this, cx| {
            let result = services
                .blocking(move || {
                    state.activate(app, &version, &paths)?;
                    state.prune(app, &paths)?;
                    Ok::<_, ferrumlauncher_core::LauncherError>((state, version))
                })
                .await
                .and_then(|r| r.map_err(|e| e.to_string()));
            this.update(cx, |this, cx| {
                match result {
                    Ok((state, version)) => {
                        // Only this app's entry (other work may have changed the
                        // rest), keeping a channel picked meanwhile.
                        let mut updated = state.app(app);
                        updated.channel = this.state.channel(app);
                        this.state.apps.insert(app, updated);
                        this.persist(cx);
                        this.notify_user(format!("Updated {name} to {version}."), false, cx);
                        if relaunch
                            && let Err(e) = store::launch(&this.state, app, &this.services.paths)
                        {
                            this.notify_user(format!("Could not reopen {name}: {e}"), true, cx);
                        }
                    }
                    Err(e) => this.notify_user(format!("Could not update {name}: {e}"), true, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "Restart to update" for a staged app, `None` otherwise.
    pub(crate) fn restart_button(
        &self,
        id: &str,
        app: AppId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let version = self.state.staged_update(app)?;
        let quitting = self.quitting.contains(&app);
        Some(
            Button::new(
                SharedString::from(format!("{id}-{app}")),
                "Restart to update",
            )
            .style(ButtonStyle::Filled)
            .start_icon(Icon::new(IconName::RotateCw))
            .disabled(quitting)
            .tooltip(Tooltip::text(format!("v{version} is downloaded")))
            .on_click(cx.listener(move |this, _, _, cx| this.restart_to_update(app, cx)))
            .into_any_element(),
        )
    }

    pub fn render_restart_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let app = self.restart_dialog?;
        let version = self.state.staged_update(app)?;
        let name = self.state.info(app).name;
        let modal = Modal::new(format!("Quit {name} to update?"))
            .on_dismiss(cx.listener(|this, _, _, cx| this.close_restart_dialog(cx)))
            .child(Label::new(format!(
                "{name} is running. The launcher asks it to quit, switches to v{version} and opens it again."
            )))
            .child(
                Label::new(
                    "If it has unsaved work it asks first; until it quits, it keeps its current version.",
                )
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .footer(
                Button::new("restart-cancel", "Not now")
                    .on_click(cx.listener(|this, _, _, cx| this.close_restart_dialog(cx))),
            )
            .footer(
                Button::new("restart-confirm", format!("Quit {name} and update"))
                    .style(ButtonStyle::Tinted(TintColor::Accent))
                    .on_click(cx.listener(move |this, _, _, cx| this.quit_and_update(app, cx))),
            );
        Some(modal.into_any_element())
    }
}
