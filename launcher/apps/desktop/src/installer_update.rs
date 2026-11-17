//! The launcher's own update on Windows and Linux, when it was installed
//! from its installer: the new release's `setup.exe` or `.deb` is
//! downloaded (checksum verified) and opened; on Windows the launcher then
//! quits so the setup can replace it. Copies run from an archive open the
//! release page instead (see `ferrumlauncher_core::self_update`).

use crate::app::FerrumLauncher;
use crate::services::client_for;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, Platform, self_update};
use semver::Version;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

impl FerrumLauncher {
    /// Whether **Update Launcher** replaces this copy itself (a macOS
    /// bundle, or an installer copy with an installer in the update)
    /// rather than opening the release page.
    pub fn can_update_itself(&self) -> bool {
        if self.running_bundle.is_some() {
            return true;
        }
        let platform = self.services.paths.platform;
        self.from_installer
            && self
                .state
                .update_for(AppId::LAUNCHER)
                .is_some_and(|release| release.launcher_installer(platform).is_some())
    }

    /// Downloads `version`'s installer, opens it and, on Windows, quits.
    pub(crate) fn update_launcher_with_installer(
        &mut self,
        version: Version,
        cx: &mut Context<Self>,
    ) {
        let Some(release) = self
            .state
            .releases
            .iter()
            .find(|r| r.app == AppId::LAUNCHER && r.version == version)
            .cloned()
        else {
            return;
        };
        self.self_update = Some(version.clone());
        let services = self.services.clone();
        let settings = self.state.settings.clone();
        let platform = services.paths.platform;
        cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let opened = services
                .run(async move {
                    let client = tokio::task::spawn_blocking(move || client_for(&settings))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                    let installer = self_update::download_installer(
                        &client,
                        &paths,
                        &release,
                        platform,
                        Arc::new(|_, _| {}),
                        CancellationToken::new(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    self_update::open_installer(platform, &installer).map_err(|e| e.to_string())
                })
                .await
                .and_then(|r| r);
            this.update(cx, |this, cx| {
                this.self_update = None;
                match opened {
                    Ok(()) if platform == Platform::Windows => {
                        this.notify_user(
                            format!("Installing FerrumLauncher {version}…"),
                            false,
                            cx,
                        );
                        // The setup replaces the launcher's files once it quit.
                        cx.spawn(async move |_, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(800))
                                .await;
                            cx.update(|cx| cx.quit());
                        })
                        .detach();
                    }
                    Ok(()) => this.notify_user(
                        format!(
                            "Opened the FerrumLauncher {version} package: install it, then reopen the launcher."
                        ),
                        false,
                        cx,
                    ),
                    Err(e) => {
                        this.notify_user(format!("Could not update the launcher: {e}"), true, cx)
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}
