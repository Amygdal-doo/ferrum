//! Opening and closing the panel's window.

use std::time::{Duration, Instant};

use ferrum_ui::gpui::{
    Bounds, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};
use ferrum_ui::prelude::*;

use super::LauncherPanel;
use super::layout::{IconRect, PANEL_SIZE, panel_origin};
use crate::app::FerrumLauncher;

/// A click on the icon this soon after the panel closed itself (losing
/// focus to that very click) closes it rather than reopening it.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// Opens the panel under the menu bar icon, or closes it when open.
pub fn toggle(launcher: Entity<FerrumLauncher>, icon: IconRect, cx: &mut App) {
    let (panel, closed_at) = {
        let integration = &launcher.read(cx).integration;
        (integration.panel, integration.panel_closed_at)
    };
    if let Some(panel) = panel {
        launcher.update(cx, |launcher, _| {
            launcher.integration.panel = None;
            launcher.integration.panel_closed_at = Some(Instant::now());
        });
        if panel
            .update(cx, |_, window, _| window.remove_window())
            .is_ok()
        {
            return;
        }
    } else if closed_at.is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
        return;
    }
    open(launcher, Some(icon), cx);
}

/// Opens the panel: under `icon`, or (without one, as a development aid)
/// as a regular window in the middle of the screen.
pub fn open(launcher: Entity<FerrumLauncher>, icon: Option<IconRect>, cx: &mut App) {
    let preview = icon.is_none();
    let display = cx.primary_display().filter(|_| !preview);
    let bounds = match (icon, &display) {
        (Some(icon), Some(display)) => Bounds {
            origin: panel_origin(icon.logical(), display.bounds(), PANEL_SIZE),
            size: PANEL_SIZE,
        },
        _ => Bounds::centered(None, PANEL_SIZE, cx),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: true,
        show: true,
        kind: if preview {
            WindowKind::Normal
        } else {
            WindowKind::PopUp
        },
        is_movable: preview,
        is_resizable: false,
        is_minimizable: false,
        display_id: display.map(|display| display.id()),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("ferrum launcher".into()),
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        let launcher = launcher.clone();
        cx.new(|cx| LauncherPanel::new(launcher, preview, window, cx))
    });
    match opened {
        Ok(panel) => launcher.update(cx, |launcher, _| launcher.integration.panel = Some(panel)),
        Err(error) => tracing::warn!(%error, "could not open the menu bar panel"),
    }
}
