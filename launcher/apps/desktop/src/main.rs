//! Ferrum Launcher desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod commands;
mod components;
mod integration;
mod panel;
mod services;
mod settings_page;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;
use ferrumlauncher_core::self_rename::{self, Outcome, RENAME_FAILED_ARG};
use ferrumlauncher_core::{AppId, Paths, State, migrate};
use services::Services;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let Some(paths) = Paths::default_for_user() else {
        eprintln!("Ferrum Launcher: could not determine a home directory for local state");
        std::process::exit(1);
    };
    // Ferrum Toolbox's data (installed apps, channels, cached releases)
    // moves here on the first start; later starts find nothing to do.
    let migration = match migrate::legacy_data_dir() {
        Some(legacy) => migrate::migrate_data_dir(&legacy, &paths.data_dir).unwrap_or_else(|e| {
            tracing::error!(error = %e, "could not move Ferrum Toolbox's data");
            migrate::DataMigration::NotNeeded
        }),
        None => migrate::DataMigration::NotNeeded,
    };
    let mut state = match State::load(&paths) {
        Ok(state) => state,
        Err(e) => {
            eprintln!(
                "Ferrum Launcher: could not read {}: {e}",
                paths.state_file().display()
            );
            std::process::exit(1);
        }
    };
    if migration.happened() {
        tracing::info!(?migration, "moved Ferrum Toolbox's data to Ferrum Launcher");
        for (app, e) in state.relink_active(&paths) {
            tracing::warn!(%app, error = %e, "could not relink after the move");
        }
        if let Err(e) = state.save(&paths) {
            tracing::warn!(error = %e, "could not save the migrated state");
        }
    }
    // FerrumAgentic became FerrumVibe, now FerrumEngineer (new id, binary
    // and bundle): its old install is removed and FerrumEngineer offered.
    if state.retire_agentic(&paths)
        && let Err(e) = state.save(&paths)
    {
        tracing::warn!(error = %e, "could not save after retiring FerrumAgentic");
    }
    // A renamed app (FerrumDock → FerrumContainers, Ferrum Editor →
    // FerrumIDE, …) gets its copy in ~/Applications/Ferrum (or Start Menu
    // shortcut) under the new name.
    for (app, e) in state.retire_former_names(&paths) {
        tracing::warn!(%app, error = %e, "could not replace the copy under the app's former name");
    }
    // Shell commands: current scripts for the enabled ones, none left under
    // a name that moved to another app (`fee` → `fei`; `fen` and `fev`
    // swapped between FerrumEngineer and FerrumVariables).
    if state.sync_shell_commands(&paths)
        && let Err(e) = state.save(&paths)
    {
        tracing::warn!(error = %e, "could not save after syncing the shell commands");
    }
    // Dev builds run outside a bundle and have no release version; they show
    // the launcher as not installed, which is what the store records anyway.
    state.running_launcher = ferrumlauncher_core::platform::running_version();

    // Started as "Ferrum Toolbox.app" (the 0.5.0 bridge update) or as
    // `ferrum-toolbox`: take the new name, relaunching if it's a bundle.
    let rename_failed = std::env::args().any(|arg| arg == RENAME_FAILED_ARG);
    let startup_notice = match self_rename::run(&paths, &state.info(AppId::LAUNCHER), rename_failed)
    {
        Outcome::Relaunch => std::process::exit(0),
        Outcome::Blocked(message) => Some(message),
        Outcome::Nothing | Outcome::Renamed => None,
    };

    let services = match Services::start(paths) {
        Ok(services) => services,
        Err(e) => {
            eprintln!("Ferrum Launcher failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %services.paths.data_dir.display(), "starting Ferrum Launcher");

    let config = AppConfig::new("Ferrum Launcher")
        .size(1040., 680.)
        .min_size(760., 480.)
        .unified_titlebar()
        // Without a Dock icon it lives in its own menu bar extra.
        .without_status_item()
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        let launcher =
            cx.new(|cx| app::FerrumLauncher::new(services, state, startup_notice, window, cx));
        let main_window = window.window_handle();
        launcher.update(cx, |launcher, _| {
            launcher.integration.main_window = Some(main_window);
        });
        // Development aid: the menu bar panel as a regular window at start.
        if std::env::var_os("FERRUM_LAUNCHER_PANEL_PREVIEW").is_some() {
            let launcher = launcher.clone();
            cx.defer(move |cx| panel::open(launcher, None, cx));
        }
        // In the menu bar, closing the window hides it; the launcher keeps
        // running for the panel and the app shortcuts. Minimised first:
        // showing the panel unhides the app, which would bring the window
        // back with it.
        let weak = launcher.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let keep = weak
                .upgrade()
                .is_some_and(|l| l.read(cx).state.settings.menu_bar);
            if keep {
                window.minimize_window();
                cx.hide();
            }
            !keep
        });
        launcher
    });
}

/// Logs never include tokens. Override verbosity with `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,ferrum_launcher=info,ferrumlauncher_core=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
