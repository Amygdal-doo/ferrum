use crate::app::FerrumData;
use crate::state::ConnStatus;
use ferrum_ui::prelude::*;

impl FerrumData {
    pub fn render_status_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.state.active_profile();
        let status = profile.and_then(|p| self.state.conn_status.get(&p.id));
        let run_label = self
            .state
            .active_tab
            .and_then(|id| self.state.runs.get(&id))
            .map(|r| r.status_label())
            .unwrap_or_default();

        let conn_label = match (profile, status) {
            (None, _) => "No connection selected".to_owned(),
            (Some(p), None) => format!("{} · not connected", p.display_name),
            (Some(p), Some(ConnStatus::Connecting)) => format!("{} · connecting…", p.display_name),
            (Some(p), Some(ConnStatus::Ready(v))) => format!("{} · {v}", p.display_name),
            (Some(p), Some(ConnStatus::Failed(_))) => {
                format!("{} · connection failed", p.display_name)
            }
        };
        let flags = profile
            .map(|p| {
                let mut f = vec![p.env.label().to_owned(), p.tls.label().to_owned()];
                if p.read_only {
                    f.push("Read only".into());
                }
                f.join(" · ")
            })
            .unwrap_or_default();
        let vault = format!("Secrets: {}", self.services.connections.vault_name());

        // On the backdrop under the cards: no fill or rule of its own.
        status_bar(cx)
            .h(px(24.))
            .bg(ferrum_ui::gpui::transparent_black())
            .border_t_0()
            .child(
                h_flex()
                    .gap_4()
                    .child(conn_label)
                    .when(!flags.is_empty(), |this| this.child(flags)),
            )
            .child(
                h_flex()
                    .gap_4()
                    .when(!run_label.is_empty(), |this| this.child(run_label))
                    .child(
                        div()
                            .id("vault")
                            .tooltip(Tooltip::text("Where saved passwords are stored"))
                            .child(vault),
                    ),
            )
            .into_any_element()
    }
}
