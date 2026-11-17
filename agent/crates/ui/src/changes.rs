//! The changes list: every file the agent changed since the session's
//! snapshot, with its diff split into hunks, and Accept / Reject for all,
//! a file or a hunk; "Undo restore" after a checkpoint restore. Also the
//! summary card after a turn ("Edited 3 files +20 −2 · Undo · Review").

use ferrum_ui::prelude::*;
use ferrum_ui::theme::theme_settings;
use ferrumagent_core::review::{FileChange, FileStatus, LineKind};
use gpui::{Context, Window};

use crate::actions::*;
use crate::view::ChatView;

/// Files listed on the summary card before "Show N more files".
const SUMMARY_FILES: usize = 5;

impl ChatView {
    pub(crate) fn render_change_summary(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let additions: usize = self.changes.iter().map(|c| c.additions).sum();
        let deletions: usize = self.changes.iter().map(|c| c.deletions).sum();
        let count = self.changes.len();
        let shown = if self.all_changed_files {
            count
        } else {
            count.min(SUMMARY_FILES)
        };
        let hover = ferrum_ui::hover_fill(cx);
        v_flex()
            .rounded(px(10.))
            .border_1()
            .border_color(colors.border_variant)
            .overflow_hidden()
            .child(
                h_flex()
                    .px_3()
                    .py_1p5()
                    .gap_1p5()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        Label::new(format!(
                            "Edited {count} file{}",
                            if count == 1 { "" } else { "s" }
                        ))
                        .size(LabelSize::Small),
                    )
                    .child(
                        Label::new(format!("+{additions}"))
                            .size(LabelSize::Small)
                            .color(Color::Created),
                    )
                    .child(
                        Label::new(format!("−{deletions}"))
                            .size(LabelSize::Small)
                            .color(Color::Deleted),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("chat-summary-undo", "Undo")
                            .style(ButtonStyle::Subtle)
                            .label_size(LabelSize::XSmall)
                            .start_icon(Icon::new(IconName::Undo).size(IconSize::XSmall))
                            .tooltip(Tooltip::text("Reject every change (⌘⇧⌫)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reject_all(&RejectAll, window, cx)
                            })),
                    )
                    .child(
                        Button::new("chat-summary-review", "Review")
                            .style(ButtonStyle::Subtle)
                            .label_size(LabelSize::XSmall)
                            .toggle_state(self.show_changes)
                            .start_icon(Icon::new(IconName::Eye).size(IconSize::XSmall))
                            .tooltip(Tooltip::text("Review hunk by hunk (⌘⌥R)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_changes(&ToggleChanges, window, cx)
                            })),
                    ),
            )
            .children(
                self.changes
                    .iter()
                    .take(shown)
                    .enumerate()
                    .map(|(i, change)| {
                        let path = change.path.clone();
                        let line = change.hunks.first().map(|h| h.new_start as u32 + 1);
                        h_flex()
                            .id(("chat-summary-file", i))
                            .px_3()
                            .py_0p5()
                            .gap_1p5()
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_file(&path, line, cx)),
                            )
                            .child(
                                div().flex_1().min_w_0().overflow_hidden().child(
                                    Label::new(change.path.to_string_lossy().to_string())
                                        .size(LabelSize::XSmall)
                                        .single_line()
                                        .truncate(),
                                ),
                            )
                            .child(
                                Label::new(format!("+{}", change.additions))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Created),
                            )
                            .child(
                                Label::new(format!("−{}", change.deletions))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Deleted),
                            )
                    }),
            )
            .when(shown < count, |this| {
                this.child(
                    div().px_1p5().py_0p5().child(
                        Button::new(
                            "chat-summary-more",
                            format!(
                                "Show {} more file{}",
                                count - shown,
                                if count - shown == 1 { "" } else { "s" }
                            ),
                        )
                        .style(ButtonStyle::Transparent)
                        .label_size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.all_changed_files = true;
                            cx.notify();
                        })),
                    ),
                )
            })
    }

    pub(crate) fn render_changes(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let additions: usize = self.changes.iter().map(|c| c.additions).sum();
        let deletions: usize = self.changes.iter().map(|c| c.deletions).sum();
        v_flex()
            .flex_none()
            .max_h(gpui::relative(0.45))
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.surface_background)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        div().flex_1().min_w(px(120.)).child(
                            Label::new(format!(
                                "{} file{} changed since the session started  +{additions} −{deletions}",
                                self.changes.len(),
                                if self.changes.len() == 1 { "" } else { "s" }
                            ))
                            .size(LabelSize::XSmall)
                            .single_line(),
                        ),
                    )
                    .when(self.can_undo_restore, |this| {
                        this.child(
                            Button::new("chat-undo-restore", "Undo restore")
                                .style(ButtonStyle::Subtle)
                                .label_size(LabelSize::XSmall)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.run_blocking(cx, |s| s.undo_restore().map(|_| ()))
                                })),
                        )
                    })
                    .child(
                        Button::new("chat-reject-all", "Reject all")
                            .style(ButtonStyle::Subtle)
                            .label_size(LabelSize::XSmall)
                            .tooltip(Tooltip::text("Undo every change (⌘⇧⌫)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reject_all(&RejectAll, window, cx)
                            })),
                    )
                    .child(
                        Button::new("chat-accept-all", "Accept all")
                            .style(ButtonStyle::Filled)
                            .label_size(LabelSize::XSmall)
                            .tooltip(Tooltip::text("Keep every change (⌘⇧⏎)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.accept_all(&AcceptAll, window, cx)
                            })),
                    ),
            )
            .child(
                v_flex()
                    .id("chat-changes-list")
                    .overflow_y_scroll()
                    .children(
                        self.changes
                            .clone()
                            .into_iter()
                            .enumerate()
                            .map(|(i, change)| self.render_file(i, change, cx)),
                    ),
            )
    }

    fn render_file(
        &self,
        i: usize,
        change: FileChange,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let mono = theme_settings(cx).buffer_font.family.clone();
        let open = self.expanded_files.contains(&change.path);
        let (letter, color) = match change.status {
            FileStatus::Added => ("A", Color::Created),
            FileStatus::Modified => ("M", Color::Modified),
            FileStatus::Deleted => ("D", Color::Deleted),
        };
        let path = change.path.clone();
        let toggle_path = change.path.clone();
        let accept_path = change.path.clone();
        let reject_path = change.path.clone();
        let hash = change.current_hash;
        let first_line = change.hunks.first().map(|h| h.new_start as u32 + 1);
        v_flex()
            .border_t_1()
            .border_color(colors.border_variant)
            .child(
                h_flex()
                    .id(("chat-file", i))
                    .px_2()
                    .py_0p5()
                    .gap_1()
                    .cursor_pointer()
                    .hover(|s| s.bg(colors.ghost_element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded_files.remove(&toggle_path) {
                            this.expanded_files.insert(toggle_path.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(Label::new(letter).size(LabelSize::XSmall).color(color))
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().child(
                            Label::new(change.path.to_string_lossy().to_string())
                                .size(LabelSize::XSmall)
                                .single_line()
                                .truncate(),
                        ),
                    )
                    .child(
                        Label::new(format!("+{} −{}", change.additions, change.deletions))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        IconButton::new(("chat-file-open", i), IconName::ArrowUpRight)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Open file"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_file(&path, first_line, cx)
                            })),
                    )
                    .child(
                        IconButton::new(("chat-file-reject", i), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .icon_color(Color::Error)
                            .tooltip(Tooltip::text("Reject file"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let p = reject_path.clone();
                                this.run_blocking(cx, move |s| s.reject_file(&p, hash));
                            })),
                    )
                    .child(
                        IconButton::new(("chat-file-accept", i), IconName::Check)
                            .icon_size(IconSize::XSmall)
                            .icon_color(Color::Success)
                            .tooltip(Tooltip::text("Accept file"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let p = accept_path.clone();
                                this.run_blocking(cx, move |s| s.accept_file(&p, hash));
                            })),
                    ),
            )
            .when(open && change.binary, |this| {
                this.child(
                    div().px_4().py_1().child(
                        Label::new("Binary file")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
                )
            })
            .when(open, |this| {
                this.children(change.hunks.iter().enumerate().map(|(h, hunk)| {
                    let hunk_id = hunk.id;
                    let accept = change.path.clone();
                    let reject = change.path.clone();
                    let jump = change.path.clone();
                    let line = hunk.new_start as u32 + 1;
                    let element = (i * 1000 + h) as u64;
                    v_flex()
                        .mx_2()
                        .mb_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(colors.border_variant)
                        .bg(colors.editor_background)
                        .child(
                            h_flex()
                                .px_1()
                                .gap_1()
                                .border_b_1()
                                .border_color(colors.border_variant)
                                .child(
                                    Button::new(("chat-hunk-jump", element), hunk.header())
                                        .style(ButtonStyle::Transparent)
                                        .label_size(LabelSize::XSmall)
                                        .color(Color::Muted)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_file(&jump, Some(line), cx)
                                        })),
                                )
                                .child(div().flex_1())
                                .child(
                                    Button::new(("chat-hunk-reject", element), "Reject")
                                        .style(ButtonStyle::Subtle)
                                        .label_size(LabelSize::XSmall)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let p = reject.clone();
                                            this.run_blocking(cx, move |s| {
                                                s.reject_hunk(&p, hunk_id, hash)
                                            });
                                        })),
                                )
                                .child(
                                    Button::new(("chat-hunk-accept", element), "Accept")
                                        .style(ButtonStyle::Subtle)
                                        .label_size(LabelSize::XSmall)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let p = accept.clone();
                                            this.run_blocking(cx, move |s| {
                                                s.accept_hunk(&p, hunk_id, hash)
                                            });
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id(("chat-hunk-lines", element))
                                .overflow_x_scroll()
                                .font_family(mono.clone())
                                .text_xs()
                                .children(hunk.lines.iter().map(|line| {
                                    let (sign, bg, fg) = match line.kind {
                                        LineKind::Added => (
                                            "+",
                                            Some(cx.theme().status().created.opacity(0.15)),
                                            colors.text,
                                        ),
                                        LineKind::Removed => (
                                            "-",
                                            Some(cx.theme().status().deleted.opacity(0.15)),
                                            colors.text,
                                        ),
                                        LineKind::Context => (" ", None, colors.text_muted),
                                    };
                                    div()
                                        .px_1()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .when_some(bg, |d, bg| d.bg(bg))
                                        .child(format!("{sign} {}", line.text))
                                })),
                        )
                }))
            })
    }
}
