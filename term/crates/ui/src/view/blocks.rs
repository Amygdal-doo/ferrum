//! Command blocks in the view (shell integration; see `crate::blocks`): the
//! toolbar on the command under the pointer (collapse, copy the command or
//! its output, run it again), the exit code chip and "Ask agent" on a
//! failed one (see `agent_pane`), and ⌘↑ / ⌘↓ between commands. The server
//! knows every command in the scrollback (`Request::Commands`) and its
//! output (`Request::CommandOutput`).

use super::TermView;
use crate::app::UiEvent;
use crate::blocks::{self, Jump};
use ferrum_ui::components::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Icon, IconButton, IconName,
    IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex,
};
use ferrum_ui::gpui::{AnyElement, Context, ElementId, IntoElement, div, prelude::*, px};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::{BlockId, CommandInfo, CommandMark, Input, Request, Response};

/// The first server revision with command blocks.
const BLOCKS_REVISION: u32 = 3;

impl TermView {
    /// The command under window point `(x, y)`, if any (for the toolbar).
    pub(crate) fn command_under(&self, x: f32, y: f32) -> Option<(BlockId, u64)> {
        let metrics = self.metrics?;
        let &(block, _, py, _, h) = self
            .app
            .panes
            .iter()
            .find(|(_, px, py, w, h)| x >= *px && x < px + w && y >= *py && y < py + h)?;
        let at = ((y - py) / f32::from(metrics.cell_height)).floor();
        if at < 0. || y >= py + h {
            return None;
        }
        let row = self.frame_row(block, at as usize)?;
        let frame = self.app.frames.get(&block)?;
        blocks::at_row(frame, row).map(|c| (block, c.id))
    }

    /// The frame row painted at screen row `at` of `block` (rows of
    /// collapsed outputs are left out, see `paint`).
    pub(crate) fn frame_row(&self, block: BlockId, at: usize) -> Option<u16> {
        match self.row_maps.get(&block) {
            Some(rows) => rows.get(at).copied(),
            None => u16::try_from(at).ok(),
        }
    }

    /// Tracks the command under the pointer; `true` when it changed.
    pub(crate) fn hover_command(&mut self, x: f32, y: f32) -> bool {
        let under = self.command_under(x, y);
        if under == self.hovered_command {
            return false;
        }
        self.hovered_command = under;
        true
    }

    fn command(&self, block: BlockId, id: u64) -> Option<&CommandMark> {
        self.app
            .frames
            .get(&block)?
            .commands
            .iter()
            .find(|c| c.id == id)
    }

    pub(crate) fn toggle_collapsed(&mut self, block: BlockId, id: u64, cx: &mut Context<Self>) {
        let folded = self.collapsed.entry(block).or_default();
        if !folded.remove(&id) {
            folded.insert(id);
        }
        cx.notify();
    }

    pub(crate) fn copy_command(&mut self, block: BlockId, id: u64, cx: &mut Context<Self>) {
        let Some(command) = self.command(block, id).map(|c| c.command.clone()) else {
            return;
        };
        cx.write_to_clipboard(ferrum_ui::gpui::ClipboardItem::new_string(command));
        self.app.notify("Copied the command");
        self.after_change(cx);
    }

    /// Asks the server for the output of command `id` of `block`, then `then`.
    pub(crate) fn with_output(
        &self,
        block: BlockId,
        id: u64,
        then: impl FnOnce(String) -> UiEvent + Send + 'static,
    ) {
        let Some(client) = self.app.client.clone() else {
            return;
        };
        let proxy = self.app.proxy.clone();
        self.app.rt.spawn(async move {
            let request = Request::CommandOutput {
                target: Some(block.to_string()),
                command: id,
            };
            let event = match client.request(request).await {
                Ok(Response::Text { text }) => then(text),
                Ok(_) => return,
                Err(e) => UiEvent::Message(e.to_string()),
            };
            let _ = proxy.unbounded_send(event);
        });
    }

    pub(crate) fn copy_output(&mut self, block: BlockId, id: u64) {
        self.with_output(block, id, |text| {
            UiEvent::Copied(text, "Copied the command's output".into())
        });
    }

    /// Types the command again at the prompt (clearing what is typed there).
    pub(crate) fn rerun(&mut self, block: BlockId, id: u64, cx: &mut Context<Self>) {
        let Some(frame) = self.app.frames.get(&block) else {
            return;
        };
        if frame.commands.iter().any(|c| c.running) {
            self.app.notify("A command is still running in this pane");
            self.after_change(cx);
            return;
        }
        let Some(command) = self.command(block, id).map(|c| c.command.clone()) else {
            return;
        };
        self.app.send(Request::Input {
            target: Some(block.to_string()),
            input: vec![
                Input::Key("C-e".into()),
                Input::Key("C-u".into()),
                Input::Text(command),
                Input::Key("Enter".into()),
            ],
        });
        self.after_change(cx);
    }

    /// ⌘↑ / ⌘↓: asks for the pane's commands, then scrolls (see `on_commands`).
    pub(crate) fn jump_command(&mut self, to: Jump, cx: &mut Context<Self>) {
        let (Some(block), Some(client)) = (self.app.focused(), self.app.client.clone()) else {
            return;
        };
        if client.server_revision() < BLOCKS_REVISION {
            self.app
                .notify("Restart the FerrumTerm server to move between commands");
            self.after_change(cx);
            return;
        }
        let proxy = self.app.proxy.clone();
        self.app.rt.spawn(async move {
            let request = Request::Commands {
                target: Some(block.to_string()),
            };
            if let Ok(Response::Commands { commands }) = client.request(request).await {
                let _ = proxy.unbounded_send(UiEvent::Commands(block, commands, to));
            }
        });
    }

    pub(crate) fn on_commands(
        &mut self,
        block: BlockId,
        commands: &[CommandInfo],
        to: Jump,
        cx: &mut Context<Self>,
    ) {
        let Some(frame) = self.app.frames.get(&block) else {
            return;
        };
        if commands.is_empty() {
            self.app.notify(
                "No commands to move between: this shell has no FerrumTerm shell integration",
            );
            self.after_change(cx);
            return;
        }
        let Some((id, lines)) = blocks::jump(frame, commands, to) else {
            return;
        };
        self.selected_command = id.map(|id| (block, id));
        if lines != 0 {
            self.app.send(Request::Scroll {
                target: Some(block.to_string()),
                lines: Some(lines),
            });
        }
        cx.notify();
    }

    /// The toolbar of the hovered command, and the chips of failed and
    /// collapsed ones, over the panes.
    pub(crate) fn render_command_bars(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(metrics) = self.metrics else {
            return Vec::new();
        };
        let cell_height = f32::from(metrics.cell_height);
        let (origin_x, origin_y) = self.canvas_origin;
        let mut out = Vec::new();
        for &(block, x, y, w, _) in &self.app.panes {
            let Some(frame) = self.app.frames.get(&block) else {
                continue;
            };
            let rows = self.row_maps.get(&block);
            let place = |row: u16| match rows {
                Some(rows) => rows.iter().position(|r| *r == row),
                None => Some(usize::from(row)),
            };
            for command in &frame.commands {
                let hovered = self.hovered_command == Some((block, command.id));
                let collapsed = self
                    .collapsed
                    .get(&block)
                    .is_some_and(|ids| ids.contains(&command.id));
                if !(hovered || collapsed || blocks::failed(command)) {
                    continue;
                }
                let Some(at) = (command.top..=command.bottom).find_map(place) else {
                    continue;
                };
                let bar = self.command_bar(block, command, hovered, collapsed, cx);
                out.push(
                    h_flex()
                        .absolute()
                        .top(px(y - origin_y + cell_height * at as f32))
                        .left(px(x - origin_x))
                        .w(px(w))
                        .justify_end()
                        .pr_1()
                        .child(bar)
                        .into_any_element(),
                );
            }
        }
        out
    }

    fn command_bar(
        &self,
        block: BlockId,
        command: &CommandMark,
        full: bool,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let id = command.id;
        let key = |what: &str| ElementId::Name(format!("cmd-{block}-{id}-{what}").into());
        let status = if command.running {
            Some(Label::new("running").color(Color::Accent))
        } else {
            command
                .exit
                .filter(|c| *c != 0)
                .map(|code| Label::new(format!("exit {code}")).color(Color::Error))
        };
        let small = |id: ElementId, label: &'static str| {
            Button::new(id, label)
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::XSmall)
        };
        let foldable = command.output.is_some() && !command.running;
        let has_command = !command.command.is_empty();
        h_flex()
            .id(key("bar"))
            .occlude()
            .gap_0p5()
            .px_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.elevated_surface_background)
            .shadow_sm()
            .children(status.map(|s| div().px_1().child(s.size(LabelSize::XSmall))))
            .when(collapsed && !full, |bar| {
                bar.child(
                    div().px_1().child(
                        Label::new("output folded")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
                )
            })
            .when(foldable && (full || collapsed), |bar| {
                bar.child(
                    IconButton::new(
                        key("fold"),
                        if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        },
                    )
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(if collapsed {
                        "Show the output"
                    } else {
                        "Collapse the output"
                    }))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_collapsed(block, id, cx);
                    })),
                )
            })
            .when(full && has_command, |bar| {
                bar.child(
                    small(key("copy"), "Copy")
                        .tooltip(Tooltip::text("Copy the command"))
                        .on_click(
                            cx.listener(move |view, _, _, cx| view.copy_command(block, id, cx)),
                        ),
                )
            })
            .when(full && command.output.is_some(), |bar| {
                bar.child(
                    small(key("output"), "Output")
                        .tooltip(Tooltip::text("Copy the output"))
                        .on_click(cx.listener(move |view, _, _, _| view.copy_output(block, id))),
                )
            })
            .when(full && has_command && !command.running, |bar| {
                bar.child(
                    small(key("rerun"), "Run again")
                        .tooltip(Tooltip::text("Type the command again and run it"))
                        .on_click(cx.listener(move |view, _, _, cx| view.rerun(block, id, cx))),
                )
            })
            // A failed command offers the agent, hovered or not.
            .when(blocks::failed(command) && self.can_ask_agent(), |bar| {
                bar.child(
                    small(key("ask"), "Ask agent")
                        .start_icon(
                            Icon::new(IconName::Sparkle)
                                .size(IconSize::XSmall)
                                .color(Color::Accent),
                        )
                        .tooltip(Tooltip::text(
                            "Open the Agent pane with this command, its exit code and its output",
                        ))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.ask_agent_about_command(block, id, window, cx);
                        })),
                )
            })
    }
}
