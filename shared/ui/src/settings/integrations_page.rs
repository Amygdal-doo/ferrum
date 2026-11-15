//! Settings › Integrations: the Ferrum skill for coding agents, the
//! command-line tool, macOS permissions and config migration.

use gpui::{
    AnyElement, App, ClipboardItem, Hsla, IntoElement, ParentElement, Styled, Window,
    prelude::FluentBuilder, px,
};

use super::widgets::{Row, Section, Switch, icon_tile};
use super::{SettingsPage, tint};
use crate::app_info::app_info;
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, IconName, Label, LabelCommon,
    LabelSize, h_flex,
};
use crate::integrations::{self, Agent, PermissionStatus};
use crate::notify;

pub(super) struct IntegrationsPage;

impl SettingsPage for IntegrationsPage {
    fn id(&self) -> &'static str {
        "integrations"
    }
    fn title(&self) -> &'static str {
        "Integrations"
    }
    fn icon(&self) -> IconName {
        IconName::Blocks
    }
    fn tint(&self) -> Hsla {
        tint(0xe8743b)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "agents",
            "claude",
            "codex",
            "opencode",
            "skill",
            "permissions",
            "migrate",
            "cli",
            "path",
        ]
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let info = app_info(cx);
        let found = integrations::installed_agents();
        let skill = integrations::skill_installed();
        let agent_icons = h_flex().gap_1().children(Agent::ALL.map(|agent| {
            let present = found.contains(&agent);
            icon_tile(
                agent.icon(),
                if present {
                    tint(agent.tint())
                } else {
                    tint(0x6b717d)
                },
                px(22.),
            )
        }));
        let found_names = if found.is_empty() {
            "No coding agent found on this Mac yet.".to_owned()
        } else {
            format!(
                "Found: {}.",
                found
                    .iter()
                    .map(|a| a.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let agents = Section::new("Coding Agents")
            .row(
                Row::new("skill", "Integrate with available agents")
                    .detail("Installs a Ferrum skill so agents can open and drive the Ferrum apps.")
                    .icon(agent_icons)
                    .control(Switch::new("skill-switch", skill).on_change(|on, _, cx| {
                        let result = if on {
                            integrations::install_skill()
                                .map(|files| format!("Installed for {} agent(s).", files.len()))
                        } else {
                            integrations::remove_skill().map(|()| "Skill removed.".to_owned())
                        };
                        match result {
                            Ok(message) => notify::notify("settings", "Ferrum skill", message, cx),
                            Err(error) => notify::notify(
                                "settings",
                                "Ferrum skill",
                                format!("Failed: {error}"),
                                cx,
                            ),
                        }
                        cx.refresh_windows();
                    })),
            )
            .footnote(format!(
                "Teaches Claude Code (~/.claude/skills) and Codex / OpenCode (~/.agents/skills) \
                 about the Ferrum apps. {found_names}"
            ));

        let cli = integrations::cli_path(info.binary);
        let command_line = Section::new("Command Line").row(
            Row::new("cli", format!("`{}` command", info.binary))
                .detail(match &cli {
                    Some(path) => format!("Installed at {}", path.display()),
                    None => "Not installed. FerrumLauncher links the apps' commands into \
                             ~/.ferrum/bin."
                        .to_owned(),
                })
                .control(
                    Label::new(if cli.is_some() {
                        "Installed"
                    } else {
                        "Not installed"
                    })
                    .size(LabelSize::Small)
                    .color(if cli.is_some() {
                        Color::Success
                    } else {
                        Color::Muted
                    }),
                ),
        );

        let mut permissions =
            Section::new("System Permissions").detail(integrations::permissions_summary(&info));
        if info.permissions.is_empty() {
            permissions = permissions.row(
                Row::new("no-permissions", "Nothing to grant")
                    .detail(format!("{} needs no special permissions.", info.name)),
            );
        }
        for (i, permission) in info.permissions.iter().enumerate() {
            let kind = permission.kind;
            let status = kind.status();
            permissions = permissions.row(
                Row::new(("permission", i), kind.title())
                    .detail(permission.reason)
                    .icon(icon_tile(kind.icon(), tint(0x4c8bf5), px(28.)))
                    .control(
                        h_flex()
                            .gap_2()
                            .when(status == PermissionStatus::Granted, |this| {
                                this.child(
                                    Label::new("Granted")
                                        .size(LabelSize::Small)
                                        .color(Color::Success),
                                )
                            })
                            .child(
                                Button::new(("grant", i), "Open Settings…")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(move |_, _, cx| crate::integrations::grant(kind, cx)),
                            ),
                    ),
            );
        }

        let mut page = gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .child(agents)
            .child(command_line)
            .child(permissions);

        if let Some(migration) = info.migration.clone() {
            let mut section = Section::new("Migrate Your Config")
                .detail(format!("From {}.", migration.from.join(", ")))
                .footnote(migration.summary);
            for agent in Agent::ALL {
                let present = found.contains(&agent);
                let info = info.clone();
                section = section.row(
                    Row::new(
                        ("migrate", agent as usize),
                        format!("Migrate with {}", agent.name()),
                    )
                    .disabled(!present)
                    .detail(if present {
                        "Opens a terminal where the agent finds your config and asks before \
                             changing anything."
                    } else {
                        "Not installed."
                    })
                    .icon(icon_tile(agent.icon(), tint(agent.tint()), px(28.)))
                    .control(
                        Button::new(("migrate-start", agent as usize), "Start…")
                            .style(ButtonStyle::Subtle)
                            .disabled(!present)
                            .on_click(move |_, _, cx| {
                                if let Err(error) = integrations::run_migration(agent, &info) {
                                    notify::notify("settings", "Migration", error.to_string(), cx);
                                }
                            }),
                    ),
                );
            }
            let prompt_info = info.clone();
            section = section.row(
                Row::new("copy-prompt", "Copy prompt")
                    .detail("Paste it into any agent yourself.")
                    .control(
                        Button::new("copy-prompt-button", "Copy")
                            .style(ButtonStyle::Subtle)
                            .start_icon(crate::components::Icon::new(IconName::Copy))
                            .on_click(move |_, _, cx| {
                                if let Some(prompt) = integrations::migration_prompt(&prompt_info) {
                                    cx.write_to_clipboard(ClipboardItem::new_string(prompt));
                                    notify::notify(
                                        "settings",
                                        "Copied",
                                        "Migration prompt copied.",
                                        cx,
                                    );
                                }
                            }),
                    ),
            );
            page = page.child(section);
        }
        page.into_any_element()
    }
}
