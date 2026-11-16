//! The anonymous daily usage count (see `ferrumlauncher_core::usage`): a
//! one-time notice and an hourly check that sends at most one ping a day.
//! Failures are silent and nothing here blocks the UI.

use crate::app::FerrumLauncher;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::usage;
use std::time::Duration;

/// How often to look whether today's ping is still due.
const CHECK_EVERY: Duration = Duration::from_secs(3600);

impl FerrumLauncher {
    /// Shows the notice once, the first time the launcher runs with sharing on.
    pub(crate) fn show_usage_notice(&mut self, cx: &mut Context<Self>) {
        let env = |key: &str| std::env::var(key).ok();
        if self.state.usage.notice_shown || !usage::enabled(&self.state.settings, env) {
            return;
        }
        self.state.usage.notice_shown = true;
        self.usage_notice = true;
        self.persist(cx);
        cx.notify();
    }

    /// Checks now (startup) and then every hour, so a launcher left
    /// running in the menu bar still counts once a day. While the first-run
    /// notice is new, the first check waits an hour, so turning it off
    /// right away means nothing was ever sent.
    pub(crate) fn start_usage_pings(&mut self, cx: &mut Context<Self>) {
        let wait_first = self.usage_notice;
        self.usage_loop = cx.spawn(async move |this, cx| {
            if wait_first {
                cx.background_executor().timer(CHECK_EVERY).await;
            }
            loop {
                if this
                    .update(cx, |this, cx| this.send_usage_ping(cx))
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        });
    }

    fn send_usage_ping(&mut self, cx: &mut Context<Self>) {
        let today = usage::today();
        let had_id = self.state.usage.install_id.is_some();
        let Some((url, ping)) = usage::plan(&mut self.state, |k| std::env::var(k).ok(), today)
        else {
            return;
        };
        if !had_id {
            self.persist(cx);
        }
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let sent = services
                .run(async move { usage::send(&url, &ping).await })
                .await;
            match sent {
                Ok(Ok(())) => {
                    this.update(cx, |this, cx| {
                        this.state.usage.last_ping = Some(today);
                        this.persist(cx);
                    })
                    .ok();
                }
                Ok(Err(e)) => tracing::debug!(error = %e, "usage ping not sent"),
                Err(e) => tracing::debug!(error = %e, "usage ping not sent"),
            }
        })
        .detach();
    }

    /// Turns sharing on or off (the notice's **Turn off**, Settings).
    pub(crate) fn set_share_usage_counts(&mut self, on: bool, cx: &mut Context<Self>) {
        self.usage_notice = false;
        if self.state.settings.share_usage_counts != on {
            self.state.settings.share_usage_counts = on;
            self.persist(cx);
            if on {
                self.send_usage_ping(cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn render_usage_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.usage_notice {
            return None;
        }
        let colors = cx.theme().colors();
        Some(
            h_flex()
                .px_4()
                .py_2()
                .gap_3()
                .border_b_1()
                .border_color(colors.border_variant)
                .bg(colors.elevated_surface_background)
                .child(div().flex_1().child(Label::new(usage::NOTICE)))
                .child(
                    Button::new("usage-notice-more", "What is sent")
                        .style(ButtonStyle::Subtle)
                        .on_click(|_, _, cx| cx.open_url(usage::PRIVACY_URL)),
                )
                .child(
                    Button::new("usage-notice-off", "Turn off")
                        .style(ButtonStyle::Subtle)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_share_usage_counts(false, cx);
                        })),
                )
                .child(
                    IconButton::new("usage-notice-dismiss", IconName::Close)
                        .tooltip(Tooltip::text("Dismiss"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.usage_notice = false;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}
