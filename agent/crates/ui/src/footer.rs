//! The line under the composer: project folder, "Local", the branch the
//! host reported (its menu and a Push button emit [`ChatEvent::Git`]), and
//! a thin context-usage bar.

use ferrum_ui::components::{ContentButton, ContextMenu};
use ferrum_ui::prelude::*;
use gpui::{Anchor, Context, Entity, Window};

use crate::composer::tokens;
use crate::host::{GitRequest, GitStatus};
use crate::view::{ChatEvent, ChatView};

/// `feat/x ↑2 ↓1 •`: ahead, behind, and a dot for a dirty tree.
pub(crate) fn branch_label(git: &GitStatus) -> String {
    let mut label = git.branch.clone();
    if git.ahead > 0 {
        label.push_str(&format!(" ↑{}", git.ahead));
    }
    if git.behind > 0 {
        label.push_str(&format!(" ↓{}", git.behind));
    }
    if git.dirty > 0 {
        label.push_str(" •");
    }
    label
}

/// Context used, as a whole percentage of the window (capped at 100).
pub(crate) fn usage_percent(used: u64, window: u64) -> u32 {
    if window == 0 {
        return 0;
    }
    ((used as f64 / window as f64) * 100.0).round().min(100.0) as u32
}

impl ChatView {
    fn git_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        move |window, cx| {
            let this = this.clone();
            Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                for request in GitRequest::ALL {
                    if request == GitRequest::Commit {
                        menu = menu.separator();
                    }
                    let this = this.clone();
                    menu = menu.entry(request.label(), None, move |_, cx| {
                        this.update(cx, |_, cx| cx.emit(ChatEvent::Git(request)))
                            .ok();
                    });
                }
                menu
            }))
        }
    }

    pub(crate) fn render_footer(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let folder = self.project.file_name().map_or_else(
            || self.project.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let item = |icon: IconName, label: String| {
            h_flex()
                .gap_1()
                .min_w_0()
                .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
                .child(
                    Label::new(label)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .single_line()
                        .truncate(),
                )
        };
        let used = self.thread.context_tokens;
        let usage = self
            .context_window()
            .filter(|_| used > 0)
            .map(|size| (size, usage_percent(used, size)));
        h_flex()
            .px_1()
            .pb_1p5()
            .gap_3()
            .min_h(px(22.))
            .child(item(IconName::Folder, folder))
            .child(item(IconName::Monitor, "Local".into()))
            .when_some(self.git_status.clone(), |this, git| {
                this.child(
                    PopoverMenu::new("chat-branch")
                        .trigger(
                            ContentButton::new(
                                "chat-branch-button",
                                item(IconName::GitBranch, branch_label(&git)),
                            )
                            .style(ButtonStyle::Subtle)
                            .tooltip(Tooltip::text("Branch: push, pull, commit…")),
                        )
                        .anchor(Anchor::BottomLeft)
                        .menu(self.git_menu(cx)),
                )
                .when(git.ahead > 0, |this| {
                    this.child(
                        Button::new("chat-push", "Push")
                            .style(ButtonStyle::Outlined)
                            .label_size(LabelSize::XSmall)
                            .start_icon(Icon::new(IconName::ArrowUp).size(IconSize::XSmall))
                            .on_click(
                                cx.listener(|_, _, _, cx| {
                                    cx.emit(ChatEvent::Git(GitRequest::Push))
                                }),
                            ),
                    )
                })
            })
            .child(div().flex_1())
            .when_some(usage, |this, (size, percent)| {
                let fill = match percent {
                    90.. => cx.theme().status().error,
                    75.. => cx.theme().status().warning,
                    _ => colors.text_muted,
                };
                this.child(
                    h_flex()
                        .id("chat-context-usage")
                        .gap_1p5()
                        .tooltip(Tooltip::text(format!(
                            "Context: {} of {} tokens",
                            tokens(used),
                            tokens(size)
                        )))
                        .child(
                            div()
                                .w(px(48.))
                                .h(px(4.))
                                .rounded_full()
                                .bg(colors.text.opacity(0.12))
                                .child(
                                    div()
                                        .h_full()
                                        .rounded_full()
                                        .bg(fill)
                                        .w(relative(percent as f32 / 100.0)),
                                ),
                        )
                        .child(
                            Label::new(format!("{percent}%"))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_labels_and_usage() {
        let mut git = GitStatus {
            branch: "feat/review".into(),
            ..Default::default()
        };
        assert_eq!(branch_label(&git), "feat/review");
        git.ahead = 2;
        git.behind = 1;
        git.dirty = 3;
        assert_eq!(branch_label(&git), "feat/review ↑2 ↓1 •");
        assert_eq!(usage_percent(56_000, 200_000), 28);
        assert_eq!(usage_percent(500_000, 200_000), 100);
        assert_eq!(usage_percent(1, 0), 0);
    }
}
