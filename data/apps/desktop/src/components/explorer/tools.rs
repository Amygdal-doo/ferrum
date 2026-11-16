//! The "…" menu of a SQL connection's table list: schema tools.

use crate::app::FerrumData;
use ferrum_ui::prelude::*;
use ferrumdata_domain::ProfileId;

impl FerrumData {
    pub(super) fn render_schema_tools(
        &mut self,
        profile_id: ProfileId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity().downgrade();
        PopoverMenu::new(SharedString::from(format!("schema-tools-{profile_id}")))
            .menu(move |window, cx| {
                let this = this.clone();
                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                    let entry = |menu: ContextMenu,
                                 label: &'static str,
                                 run: fn(&mut FerrumData, ProfileId, &mut Context<FerrumData>)| {
                        let this = this.clone();
                        menu.entry(label, None, move |_, cx| {
                            if let Err(e) = this.update(cx, |this, cx| run(this, profile_id, cx)) {
                                tracing::debug!(error = %e, "explorer dropped");
                            }
                        })
                    };
                    entry(menu, "ER diagram", |this, id, cx| this.open_diagram(id, cx))
                }))
            })
            .trigger(
                IconButton::new(
                    SharedString::from(format!("schema-tools-button-{profile_id}")),
                    IconName::Ellipsis,
                )
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text("Schema tools")),
            )
            .into_any_element()
    }
}
