//! Port forwards from remote machines to this Mac: started from the detail
//! pane of a remote listener, listed in a bar under the table header with
//! their status, kept running while the app is open.

use ferrum_ui::prelude::*;
use ferrumports_core::forward::{self, Forward, ForwardStatus};
use ferrumports_core::{Exposure, Listener};

use crate::app::FerrumPorts;

impl FerrumPorts {
    /// The active forward of the remote machine's `port`, if any.
    pub fn forward_of(&self, port: u16) -> Option<&Forward> {
        let host = self.target.as_deref()?;
        self.forwards
            .iter()
            .find(|f| f.status.is_active() && f.host.name == host && f.remote_port == port)
    }

    /// Forwards `listener` (on the remote machine shown) to a local port:
    /// the same number when it is free here.
    pub fn start_forward(&mut self, listener: &Listener, cx: &mut Context<Self>) {
        let Some(host) = self.target_host() else {
            return;
        };
        let taken: Vec<u16> = self
            .forwards
            .iter()
            .filter(|f| f.status.is_active())
            .map(|f| f.local_port)
            .collect();
        let Some(local) = forward::local_port_for(listener.port, &taken) else {
            self.show_notice(Notice::error("No free local port near it"), cx);
            return;
        };
        // A socket bound to one interface is reached at that address.
        let remote_host = match (listener.exposure(), listener.addresses.first()) {
            (Exposure::Specific, Some(address)) => address.clone(),
            _ => "localhost".to_string(),
        };
        match Forward::start(
            host,
            local,
            &remote_host,
            listener.port,
            Some(listener.process.clone()),
        ) {
            Ok(forward) => {
                let label = forward.label();
                self.forwards.push(forward);
                self.show_notice(Notice::info(format!("Forwarding {label}")), cx);
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
    }

    pub fn stop_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(forward) = self.forwards.get_mut(index) {
            forward.stop();
        }
        cx.notify();
    }

    /// Starts a stopped or failed forward again, on the same ports.
    pub fn restart_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(old) = self.forwards.get(index) else {
            return;
        };
        let restarted = Forward::start(
            old.host.clone(),
            old.local_port,
            &old.remote_host.clone(),
            old.remote_port,
            old.process.clone(),
        );
        match restarted {
            Ok(forward) => self.forwards[index] = forward,
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
        cx.notify();
    }

    pub fn remove_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.forwards.len() {
            // Dropping it stops ssh.
            self.forwards.remove(index);
        }
        cx.notify();
    }

    /// Picks up ssh exits and bound ports (each refresh tick).
    pub fn poll_forwards(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for forward in &mut self.forwards {
            changed |= forward.poll();
        }
        if changed {
            cx.notify();
        }
    }

    /// Stops every forward (on quit).
    pub fn stop_forwards(&mut self) {
        for forward in &mut self.forwards {
            forward.stop();
        }
    }
}

fn status_color(status: &ForwardStatus, cx: &App) -> gpui::Hsla {
    let s = cx.theme().status();
    match status {
        ForwardStatus::Running => s.success,
        ForwardStatus::Connecting => s.warning,
        ForwardStatus::Failed(_) => s.error,
        ForwardStatus::Stopped => cx.theme().colors().text_muted,
    }
}

/// One row per forward under the table header; nothing when there are none.
pub fn bar(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> Option<AnyElement> {
    if app.forwards.is_empty() {
        return None;
    }
    let colors = cx.theme().colors().clone();
    let rows: Vec<AnyElement> = app
        .forwards
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let color = status_color(&f.status, cx);
            let detail = match &f.status {
                ForwardStatus::Failed(message) => message.clone(),
                _ => f.process.clone().unwrap_or_default(),
            };
            let url = f.url();
            let running = f.status == ForwardStatus::Running;
            let active = f.status.is_active();
            h_flex()
                .id(("forward", i))
                .px_3()
                .py_0p5()
                .gap_2()
                .child(div().flex_none().size(px(7.)).rounded_full().bg(color))
                .child(
                    Label::new(f.label())
                        .size(LabelSize::Small)
                        .weight(gpui::FontWeight::SEMIBOLD),
                )
                .child(
                    Label::new(f.status.label().to_string())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_ui_xs(cx)
                        .text_color(colors.text_muted)
                        .child(detail),
                )
                .when(running, |row| {
                    row.child(
                        IconButton::new(("forward-open", i), IconName::ArrowUpRight)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(format!("Open {url}")))
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                    )
                })
                .when(active, |row| {
                    row.child(
                        IconButton::new(("forward-stop", i), IconName::Stop)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Stop forwarding"))
                            .on_click(cx.listener(move |app, _, _, cx| app.stop_forward(i, cx))),
                    )
                })
                .when(!active, |row| {
                    row.child(
                        IconButton::new(("forward-restart", i), IconName::RotateCw)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Start again"))
                            .on_click(cx.listener(move |app, _, _, cx| app.restart_forward(i, cx))),
                    )
                    .child(
                        IconButton::new(("forward-remove", i), IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Remove"))
                            .on_click(cx.listener(move |app, _, _, cx| app.remove_forward(i, cx))),
                    )
                })
                .into_any_element()
        })
        .collect();
    Some(
        v_flex()
            .flex_none()
            .py_1()
            .border_b_1()
            .border_color(colors.border_variant)
            .bg(colors.panel_background)
            .children(rows)
            .into_any_element(),
    )
}
