//! The non-blocking notice shown when the shared server is older than this
//! app (it outlived an update, like tmux): "Restart now" (after a warning
//! with what will end) or "Later" (hidden for the rest of this app run, in
//! every view of the process).

use super::TermView;
use ferrum_ui::components::{StyledTypography, h_flex, v_flex};
use ferrum_ui::gpui::{self, Context, ElementId, Hsla, SharedString, div, prelude::*, px};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::SessionInfo;
use std::sync::atomic::{AtomicBool, Ordering};

/// "Later" was chosen in this app run.
static LATER: AtomicBool = AtomicBool::new(false);
/// "Restart now" ran in this app run (so a still-older server means the
/// `fterm` that started it is old too).
static RESTARTED: AtomicBool = AtomicBool::new(false);

/// An older shared server: its build and ours.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StaleServer {
    pub server: String,
    pub app: String,
    /// "Restart now" was clicked; waiting for the user to confirm.
    pub confirming: bool,
}

impl StaleServer {
    /// The notice for `client`'s server, when it is older than this app
    /// and the user hasn't chosen "Later".
    pub(crate) fn check(client: &ferrumterm_client::Client) -> Option<Self> {
        if LATER.load(Ordering::Relaxed) || !client.server_age().needs_restart() {
            return None;
        }
        Some(Self {
            server: server_label(client.server.as_ref()),
            app: ferrumterm_protocol::build_version().to_owned(),
            confirming: false,
        })
    }
}

fn server_label(info: Option<&ferrumterm_protocol::ServerInfo>) -> String {
    match info {
        Some(i) if i.revision > 0 => format!("{} r{}", i.version, i.revision),
        Some(i) => format!("{}, pre-0.12", i.version),
        None => "unknown".to_owned(),
    }
}

/// The notice's text.
pub(crate) fn headline(stale: &StaleServer, after_restart: bool) -> String {
    if after_restart {
        format!(
            "The FerrumTerm server is still older than this app after restarting (server {}, app {}). \
             The fterm that starts it is out of date: update FerrumTerm.",
            stale.server, stale.app
        )
    } else {
        format!(
            "The FerrumTerm server is older than this app (server {}, app {}). \
             Agent status and some features need a restart.",
            stale.server, stale.app
        )
    }
}

/// The confirmation shown after "Restart now".
pub(crate) fn restart_warning(sessions: &[SessionInfo]) -> String {
    let blocks: usize = sessions
        .iter()
        .flat_map(|s| &s.windows)
        .map(|w| w.blocks.len())
        .sum();
    let n = sessions.len();
    format!(
        "Restarting ends the programs running in all terminals ({n} {}, {blocks} {}), in every app using this server.",
        if n == 1 { "session" } else { "sessions" },
        if blocks == 1 { "terminal" } else { "terminals" }
    )
}

impl TermView {
    pub(crate) fn render_stale_server(
        &self,
        stale: StaleServer,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if LATER.load(Ordering::Relaxed) {
            return None;
        }
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let after_restart = RESTARTED.load(Ordering::Relaxed);
        let text = if stale.confirming {
            restart_warning(&self.app.sessions)
        } else {
            headline(&stale, after_restart)
        };
        let accent = colors.text_accent;
        let on_accent = cx.theme().readable_text_on(accent);
        let buttons = if stale.confirming {
            h_flex()
                .gap_2()
                .child(
                    button("stale-cancel", "Cancel", None, colors.text, cx).on_click(cx.listener(
                        |view, _, _, cx| {
                            if let Some(s) = view.stale_server.as_mut() {
                                s.confirming = false;
                            }
                            cx.notify();
                        },
                    )),
                )
                .child(
                    button(
                        "stale-confirm",
                        "Restart server",
                        Some(accent),
                        on_accent,
                        cx,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        RESTARTED.store(true, Ordering::Relaxed);
                        view.stale_server = None;
                        view.app.notify("Restarting the FerrumTerm server…");
                        view.app.restart_server();
                        cx.notify();
                    })),
                )
        } else {
            let later = button(
                "stale-later",
                if after_restart { "Dismiss" } else { "Later" },
                None,
                colors.text,
                cx,
            )
            .on_click(cx.listener(|view, _, _, cx| {
                LATER.store(true, Ordering::Relaxed);
                view.stale_server = None;
                cx.notify();
            }));
            h_flex()
                .gap_2()
                .when(!after_restart, |row| {
                    row.child(
                        button("stale-restart", "Restart now", Some(accent), on_accent, cx)
                            .on_click(cx.listener(|view, _, _, cx| {
                                if let Some(s) = view.stale_server.as_mut() {
                                    s.confirming = true;
                                }
                                cx.notify();
                            })),
                    )
                })
                .child(later)
        };
        Some(
            div()
                .absolute()
                .top_2()
                .left_2()
                .right_2()
                .flex()
                .justify_center()
                .child(
                    h_flex()
                        .max_w(px(720.))
                        .gap_3()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(status.warning_border)
                        .bg(colors.elevated_surface_background)
                        .text_color(colors.text)
                        .font_ui(cx)
                        .text_size(px(12.5))
                        .child(
                            div()
                                .flex_none()
                                .size(px(8.))
                                .rounded_full()
                                .bg(status.warning),
                        )
                        .child(v_flex().flex_1().min_w_0().child(SharedString::from(text)))
                        .child(buttons.flex_none()),
                ),
        )
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    bg: Option<Hsla>,
    fg: Hsla,
    cx: &Context<TermView>,
) -> gpui::Stateful<gpui::Div> {
    let border = cx.theme().colors().border_variant;
    div()
        .id(ElementId::Name(id.into()))
        .px_2p5()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .text_color(fg)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .map(|b| match bg {
            Some(bg) => b.bg(bg),
            None => b.border_1().border_color(border),
        })
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::ServerInfo;

    fn stale() -> StaleServer {
        StaleServer {
            server: "0.1.0, pre-0.12".into(),
            app: "0.12.0".into(),
            confirming: false,
        }
    }

    #[test]
    fn headline_names_both_builds() {
        assert_eq!(
            headline(&stale(), false),
            "The FerrumTerm server is older than this app (server 0.1.0, pre-0.12, app 0.12.0). \
             Agent status and some features need a restart."
        );
        assert!(headline(&stale(), true).contains("update FerrumTerm"));
    }

    #[test]
    fn labels_old_and_new_servers() {
        let mut info: ServerInfo = serde_json::from_str(
            r#"{"version":"0.1.0","protocol":2,"pid":1,"started_ms":0,"socket":"/s",
                "listeners":[],"sessions":0,"blocks":0,"clients":0,
                "awake_mode":"auto","awake":false}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(server_label(Some(&info)), "0.1.0, pre-0.12");
        info.version = "0.12.0".into();
        info.revision = 1;
        assert_eq!(server_label(Some(&info)), "0.12.0 r1");
        assert_eq!(server_label(None), "unknown");
    }

    #[test]
    fn warning_counts_sessions_and_terminals() {
        use crate::app_tests::session;
        assert_eq!(
            restart_warning(&[session(1, "a", &[1, 2]), session(2, "b", &[3])]),
            "Restarting ends the programs running in all terminals (2 sessions, 3 terminals), \
             in every app using this server."
        );
        assert!(restart_warning(&[session(1, "a", &[1])]).contains("(1 session, 1 terminal)"));
    }
}
