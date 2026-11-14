//! Extensions: FerrumChat's MCP servers (`~/.ferrum/agent/mcp.json`) with
//! their status in this project's chat; add, remove, enable.

use ferrum_ui::components::{Checkbox, IconSize};
use ferrum_ui::prelude::*;

use super::{field, frame, placeholder, row, section, two_lines};
use crate::app::FerrumAgentic;
use crate::nav::Page;

impl FerrumAgentic {
    fn mcp_file(&self) -> std::path::PathBuf {
        ferrumagent_ui::core::paths::mcp_config(&self.agent_home)
    }

    /// Runs an edit of `mcp.json`, then re-reads it.
    fn edit_mcp(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&std::path::Path) -> Result<(), String>,
    ) {
        let file = self.mcp_file();
        match f(&file) {
            Ok(()) => self.show_notice(
                Notice::info("Saved. New chats use the change; restart open ones to pick it up."),
                cx,
            ),
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
        self.pages.servers = crate::extensions::list(&file);
        cx.notify();
    }

    fn add_server(&mut self, cx: &mut Context<Self>) {
        let name = self.pages.mcp_name.read(cx).text().to_string();
        let command = self.pages.mcp_command.read(cx).text().to_string();
        let mut added = false;
        self.edit_mcp(cx, |file| {
            crate::extensions::add(file, &name, &command)?;
            added = true;
            Ok(())
        });
        if added {
            for input in [self.pages.mcp_name.clone(), self.pages.mcp_command.clone()] {
                input.update(cx, |input, cx| input.set_text("", cx));
            }
        }
    }

    /// "12 tools", the error, or "Off" / "Not started".
    fn server_status(&self, name: &str, disabled: bool) -> (String, Color) {
        if disabled {
            return ("Off".into(), Color::Muted);
        }
        match self.pages.mcp_status.iter().find(|s| s.name == name) {
            Some(status) => match &status.result {
                Ok(1) => ("1 tool".into(), Color::Success),
                Ok(n) => (format!("{n} tools"), Color::Success),
                Err(e) => (e.clone(), Color::Error),
            },
            None => ("Not started in this chat".into(), Color::Muted),
        }
    }

    pub(crate) fn extensions_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = frame(
            Page::Extensions,
            "MCP servers give the agent more tools: docs, databases, browsers, issue trackers.",
            cx,
        )
        .child(
            h_flex()
                .gap_2()
                .max_w(px(820.))
                .child(field(&self.pages.mcp_name, cx).max_w(px(180.)))
                .child(field(&self.pages.mcp_command, cx))
                .child(
                    Button::new("add-server", "Add")
                        .style(ButtonStyle::Filled)
                        .size(ButtonSize::Compact)
                        .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                        .on_click(cx.listener(|this, _, _, cx| this.add_server(cx))),
                ),
        )
        .child(section("MCP servers", cx));
        let servers = match self.pages.servers.clone() {
            Ok(servers) => servers,
            Err(e) => {
                return page
                    .child(placeholder(&format!("mcp.json could not be read: {e}"), cx))
                    .into_any_element();
            }
        };
        if servers.is_empty() {
            page = page.child(placeholder("No MCP servers yet.", cx));
        }
        for (i, server) in servers.into_iter().enumerate() {
            let (status, color) = self.server_status(&server.name, server.disabled);
            let (toggle, remove) = (server.name.clone(), server.name.clone());
            let state = if server.disabled {
                ToggleState::Unselected
            } else {
                ToggleState::Selected
            };
            page = page.child(
                row(("server", i), cx)
                    .child(
                        Checkbox::new(("server-enabled", i), state).on_click(cx.listener(
                            move |this, state: &ToggleState, _, cx| {
                                let disabled = *state != ToggleState::Selected;
                                this.edit_mcp(cx, |file| {
                                    crate::extensions::set_disabled(file, &toggle, disabled)
                                });
                            },
                        )),
                    )
                    .child(two_lines(server.name.clone(), server.detail))
                    .child(
                        div().max_w(px(260.)).child(
                            Label::new(status)
                                .size(LabelSize::Small)
                                .color(color)
                                .truncate(),
                        ),
                    )
                    .child(
                        IconButton::new(("remove-server", i), IconName::Trash)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Remove"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_mcp(cx, |file| crate::extensions::remove(file, &remove));
                            })),
                    ),
            );
        }
        page.into_any_element()
    }
}
