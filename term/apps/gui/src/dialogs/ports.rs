//! "What's using a port?": every listening TCP port, or one port's owner
//! (PID, process, user, command line), with a confirmed kill: SIGTERM first,
//! then SIGKILL only if the user asks again. Nothing is killed automatically.

use super::{Dialog, error_text, field, hint, modal};
use crate::ssh::{KillOutcome, PortsResult};
use crate::view::TermView;
use ferrum_ui::TextInput;
use ferrum_ui::gpui::{
    self, AnyElement, Context, Entity, SharedString, Window, div, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::ui::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, TintColor, h_flex, v_flex,
};
use ferrumterm_net::ports::Signal;

pub struct PortsDialog {
    pub query: Entity<TextInput>,
    pub result: Option<PortsResult>,
    pub loading: bool,
    /// A signal the user is being asked to confirm.
    pub confirm: Option<(u32, String, Signal)>,
    pub note: Option<String>,
}

/// Empty = all listening ports; else a port number.
pub fn parse_query(text: &str) -> Result<Option<u16>, String> {
    let t = text.trim().trim_start_matches(':');
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .map(Some)
        .ok_or_else(|| format!("“{t}” is not a port number (1–65535)"))
}

impl TermView {
    pub(crate) fn open_ports(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._dialog_events.clear();
        let query = self.dialog_input("Port, e.g. 3000 (empty = all)", "", false, window, cx);
        let dialog = PortsDialog {
            query: query.clone(),
            result: None,
            loading: true,
            confirm: None,
            note: None,
        };
        self.app.lookup_ports(None);
        self.open_dialog(Dialog::Ports(dialog), Some(query), window, cx);
    }

    pub(crate) fn run_port_query(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Ports(d)) = &mut self.dialog else {
            return;
        };
        match parse_query(d.query.read(cx).text()) {
            Ok(port) => {
                d.loading = true;
                d.confirm = None;
                d.note = None;
                self.app.lookup_ports(port);
            }
            Err(e) => d.note = Some(e),
        }
        cx.notify();
    }

    pub(crate) fn on_ports_result(&mut self, result: PortsResult) {
        if let Some(Dialog::Ports(d)) = &mut self.dialog {
            d.result = Some(result);
            d.loading = false;
        }
    }

    pub(crate) fn on_killed(&mut self, o: KillOutcome, cx: &mut Context<Self>) {
        let Some(Dialog::Ports(d)) = &mut self.dialog else {
            return;
        };
        let who = format!("{} (pid {})", o.name, o.pid);
        match o.result {
            Ok(true) => d.note = Some(format!("{who} exited.")),
            Ok(false) if o.signal == Signal::Term => {
                d.note = Some(format!("{who} ignored SIGTERM and is still running."));
                d.confirm = Some((o.pid, o.name, Signal::Kill));
            }
            Ok(false) => d.note = Some(format!("{who} is still running after SIGKILL.")),
            Err(e) => d.note = Some(format!("Could not signal {who}: {e}")),
        }
        // Refresh the list, keeping the note and any follow-up question.
        if let Ok(port) = parse_query(d.query.read(cx).text()) {
            d.loading = true;
            self.app.lookup_ports(port);
        }
        cx.notify();
    }

    fn confirm_signal(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Ports(d)) = &mut self.dialog else {
            return;
        };
        let Some((pid, name, signal)) = d.confirm.take() else {
            return;
        };
        let sig = if signal == Signal::Term {
            "SIGTERM"
        } else {
            "SIGKILL"
        };
        d.note = Some(format!("Sent {sig} to {name} (pid {pid})…"));
        self.app.signal_process(pid, name, signal);
        cx.notify();
    }

    pub(crate) fn render_ports(&self, d: &PortsDialog, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let mut m = modal("What's using a port?", cx).width(px(680.)).child(
            h_flex()
                .gap_2()
                .items_end()
                .child(field("Port", d.query.clone()).flex_1())
                .child(
                    Button::new("ports-find", "Find")
                        .style(ButtonStyle::Filled)
                        .on_click(cx.listener(|view, _, _, cx| view.run_port_query(cx))),
                ),
        );
        if let Some((pid, name, signal)) = &d.confirm {
            let (question, action) = match signal {
                Signal::Term => (
                    format!("Send SIGTERM to {name} (pid {pid})? It may lose unsaved work."),
                    "Terminate",
                ),
                Signal::Kill => (
                    format!(
                        "Force-kill {name} (pid {pid}) with SIGKILL? It gets no chance to clean up."
                    ),
                    "Force kill",
                ),
            };
            m = m.child(
                h_flex()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().status().warning_background)
                    .child(div().flex_1().text_size(px(12.)).child(question))
                    .child(Button::new("kill-cancel", "Cancel").on_click(cx.listener(
                        |view, _, _, cx| {
                            if let Some(Dialog::Ports(d)) = &mut view.dialog {
                                d.confirm = None;
                            }
                            cx.notify();
                        },
                    )))
                    .child(
                        Button::new("kill-confirm", action)
                            .style(ButtonStyle::Tinted(TintColor::Error))
                            .on_click(cx.listener(|view, _, _, cx| view.confirm_signal(cx))),
                    ),
            );
        }
        if let Some(note) = &d.note {
            m = m.child(hint(note.clone()));
        }
        let body: AnyElement = match (&d.result, d.loading) {
            (_, true) => hint("Looking…").into_any_element(),
            (None, false) => hint("").into_any_element(),
            (Some(r), false) => match &r.listeners {
                Err(e) => error_text(e.clone()).into_any_element(),
                Ok(list) if list.is_empty() => hint(match (r.port, r.in_use_by_other) {
                    (Some(p), true) => format!(
                        "Port {p} is taken by a process you can't see (another user's). \
                         Try: sudo lsof -nP -iTCP:{p} -sTCP:LISTEN"
                    ),
                    (Some(p), false) => format!("Nothing is listening on TCP port {p}."),
                    (None, _) => "No listening TCP ports found.".to_owned(),
                })
                .into_any_element(),
                Ok(list) => {
                    let mut rows = v_flex().gap_1();
                    for (i, l) in list.iter().enumerate() {
                        let target = l.pid.map(|pid| (pid, l.process.clone()));
                        rows = rows.child(
                            h_flex()
                                .gap_2()
                                .py_1()
                                .border_b_1()
                                .border_color(colors.border_variant)
                                .child(
                                    div()
                                        .w(px(56.))
                                        .flex_none()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(l.port.to_string()),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(div().truncate().child(SharedString::from(format!(
                                            "{} · pid {} · {} · {}",
                                            if l.process.is_empty() {
                                                "?"
                                            } else {
                                                &l.process
                                            },
                                            l.pid.map_or("?".into(), |p| p.to_string()),
                                            l.user.as_deref().unwrap_or("?"),
                                            l.address,
                                        ))))
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.))
                                                .text_color(colors.text_muted)
                                                .child(l.command.clone().unwrap_or_default()),
                                        ),
                                )
                                .when_some(target, |row, (pid, name)| {
                                    row.child(
                                        Button::new(("kill", i), "Kill…")
                                            .size(ButtonSize::Compact)
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                if let Some(Dialog::Ports(d)) = &mut view.dialog {
                                                    d.confirm =
                                                        Some((pid, name.clone(), Signal::Term));
                                                    d.note = None;
                                                }
                                                cx.notify();
                                            })),
                                    )
                                }),
                        );
                    }
                    rows.into_any_element()
                }
            },
        };
        m.child(body)
            .footer(
                Button::new("ports-close", "Close")
                    .on_click(cx.listener(|view, _, window, cx| view.close_dialog(window, cx))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_query;

    #[test]
    fn port_queries() {
        assert_eq!(parse_query(""), Ok(None));
        assert_eq!(parse_query(" 3000 "), Ok(Some(3000)));
        assert_eq!(parse_query(":8080"), Ok(Some(8080)));
        assert!(parse_query("0").is_err());
        assert!(parse_query("http").is_err());
        assert!(parse_query("70000").is_err());
    }
}
