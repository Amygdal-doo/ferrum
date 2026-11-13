//! "AI Diff Review": the files an agent session changed (added, modified,
//! deleted, with +/- counts) on the left, the selected file's diff against
//! the session's snapshot on the right, drawn like the Git view's diff.
//! Every hunk and file has Accept / Reject; the header has Accept All /
//! Reject All. Files the agent wrote since the last scan are marked stale
//! (their buttons wait for the rescan). Actions go to the workspace as
//! [`ReviewEvent`]s.

use std::path::PathBuf;

use ferrum_ui::components::{Button, ButtonStyle};
use ferrum_ui::prelude::*;
use ferrumeditor_core::agents::review::{FileChange, FileStatus, LineKind};
use gpui::{FontWeight, ScrollHandle};

use super::{AgentSessions, ReviewAction};
use crate::git::diff::{self, RowKind};
use crate::settings::Settings;
use crate::theme_apply::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEvent {
    Apply {
        session: u64,
        action: ReviewAction,
    },
    /// Open a changed file at a 0-based line.
    Open {
        path: PathBuf,
        line: usize,
    },
    Refresh {
        session: u64,
    },
    Close,
}

pub struct AgentReview {
    sessions: Entity<AgentSessions>,
    pub session: Option<u64>,
    pub selected: Option<PathBuf>,
    files_scroll: ScrollHandle,
    diff_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ReviewEvent> for AgentReview {}

impl AgentReview {
    pub fn new(sessions: Entity<AgentSessions>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&sessions, |_, _, cx| cx.notify());
        Self {
            sessions,
            session: None,
            selected: None,
            files_scroll: ScrollHandle::new(),
            diff_scroll: ScrollHandle::new(),
            _subscriptions: vec![subscription],
        }
    }

    pub fn show(&mut self, session: u64, cx: &mut Context<Self>) {
        if self.session != Some(session) {
            self.selected = None;
        }
        self.session = Some(session);
        cx.notify();
    }

    fn changes(&self, cx: &App) -> (Vec<FileChange>, std::collections::BTreeSet<PathBuf>) {
        self.session
            .and_then(|id| self.sessions.read(cx).session(id))
            .map(|s| (s.changes.clone(), s.stale.clone()))
            .unwrap_or_default()
    }

    fn apply(&mut self, action: ReviewAction, cx: &mut Context<Self>) {
        if let Some(session) = self.session {
            cx.emit(ReviewEvent::Apply { session, action });
        }
    }

    fn render_header(&self, changes: &[FileChange], cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let sessions = self.sessions.read(cx);
        let title = match self.session.and_then(|id| sessions.session(id)) {
            Some(s) => format!("AI Diff Review — {}", s.agent.name),
            None => "AI Diff Review".to_string(),
        };
        let added: usize = changes.iter().map(|c| c.additions).sum();
        let removed: usize = changes.iter().map(|c| c.deletions).sum();
        let summary = format!(
            "{} file{} · +{added} −{removed}",
            changes.len(),
            if changes.len() == 1 { "" } else { "s" }
        );
        let any = !changes.is_empty();
        h_flex()
            .flex_none()
            .h(px(36.))
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.panel_background)
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                Label::new(summary)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(
                Button::new("review-refresh", "Refresh")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(session) = this.session {
                            cx.emit(ReviewEvent::Refresh { session });
                        }
                    })),
            )
            .child(
                Button::new("review-accept-all", "Accept All")
                    .style(ButtonStyle::Filled)
                    .disabled(!any)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.apply(ReviewAction::AcceptAll, cx)),
                    ),
            )
            .child(
                Button::new("review-reject-all", "Reject All")
                    .style(ButtonStyle::Subtle)
                    .disabled(!any)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.apply(ReviewAction::RejectAll, cx)),
                    ),
            )
            .child(
                Button::new("review-close", "Back to Editor")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ReviewEvent::Close))),
            )
    }

    fn render_files(
        &self,
        changes: &[FileChange],
        stale: &std::collections::BTreeSet<PathBuf>,
        selected: Option<&PathBuf>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let theme = cx.global::<Settings>().theme.clone();
        let rows: Vec<AnyElement> = changes
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let path = change.path.clone();
                let is_selected = selected == Some(&path);
                let is_stale = stale.contains(&path);
                let status_color = match change.status {
                    FileStatus::Added => ui(&theme, "git_added"),
                    FileStatus::Modified => ui(&theme, "git_modified"),
                    FileStatus::Deleted => ui(&theme, "git_deleted"),
                };
                let hash = change.current_hash;
                let (accept_path, reject_path) = (path.clone(), path.clone());
                h_flex()
                    .id(("review-file", i))
                    .px_2()
                    .h(px(26.))
                    .gap_2()
                    .cursor_pointer()
                    .when(is_selected, |d| d.bg(colors.element_selected))
                    .hover(|d| d.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(path.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(12.))
                            .text_color(status_color)
                            .child(change.status.letter()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(change.path.to_string_lossy().to_string()),
                    )
                    .when(is_stale, |d| {
                        d.child(
                            Label::new("stale")
                                .size(LabelSize::XSmall)
                                .color(Color::Warning),
                        )
                    })
                    .child(
                        Label::new(format!("+{} −{}", change.additions, change.deletions))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new(("review-file-accept", i), "✓")
                            .style(ButtonStyle::Subtle)
                            .disabled(is_stale)
                            .tooltip(Tooltip::text("Accept file"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let path = accept_path.clone();
                                this.apply(ReviewAction::AcceptFile { path, hash }, cx);
                            })),
                    )
                    .child(
                        Button::new(("review-file-reject", i), "✗")
                            .style(ButtonStyle::Subtle)
                            .disabled(is_stale)
                            .tooltip(Tooltip::text("Reject file (restore the snapshot)"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let path = reject_path.clone();
                                this.apply(ReviewAction::RejectFile { path, hash }, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .id("review-files")
            .w(px(320.))
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(colors.border)
            .overflow_y_scroll()
            .track_scroll(&self.files_scroll)
            .when(rows.is_empty(), |d| {
                d.child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(colors.text_muted)
                        .child("No changes since the session started."),
                )
            })
            .children(rows)
    }

    fn render_diff(
        &self,
        change: Option<&FileChange>,
        stale: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let settings = cx.global::<Settings>();
        let theme = settings.theme.clone();
        let font = SharedString::from(settings.config.font_family.clone());
        let font_size = px(settings.font_size());
        let row_height = (font_size * 1.5).round();
        let Some(change) = change else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors.text_muted)
                .child("Select a changed file to see its diff.")
                .into_any_element();
        };
        let muted = ui(&theme, "line_number");
        let mut rows: Vec<AnyElement> = Vec::new();
        let (bg, fg) = diff::colors(RowKind::File, &theme);
        rows.push(
            h_flex()
                .h(row_height)
                .pl_2()
                .when_some(bg, |d, bg| d.bg(bg))
                .text_color(fg)
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!(
                    "{}   +{} −{}",
                    change.path.display(),
                    change.additions,
                    change.deletions
                ))
                .when(stale, |d| {
                    d.child(
                        div()
                            .pl_3()
                            .text_color(ui(&theme, "warning"))
                            .child("the agent changed this file — refreshing"),
                    )
                })
                .into_any_element(),
        );
        if change.binary {
            rows.push(
                div()
                    .pl_2()
                    .text_color(muted)
                    .child("Binary file")
                    .into_any_element(),
            );
        }
        let hash = change.current_hash;
        for (h, hunk) in change.hunks.iter().enumerate() {
            let (bg, fg) = diff::colors(RowKind::Hunk, &theme);
            let (accept_path, reject_path) = (change.path.clone(), change.path.clone());
            let id = hunk.id;
            rows.push(
                h_flex()
                    .h(row_height + px(4.))
                    .pl_2()
                    .gap_2()
                    .when_some(bg, |d, bg| d.bg(bg))
                    .text_color(fg)
                    .child(hunk.header())
                    .child(div().flex_1())
                    .child(
                        Button::new(("hunk-accept", h), "Accept")
                            .style(ButtonStyle::Subtle)
                            .disabled(stale)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let path = accept_path.clone();
                                this.apply(
                                    ReviewAction::AcceptHunk {
                                        path,
                                        hunk: id,
                                        hash,
                                    },
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new(("hunk-reject", h), "Reject")
                            .style(ButtonStyle::Subtle)
                            .disabled(stale)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let path = reject_path.clone();
                                this.apply(
                                    ReviewAction::RejectHunk {
                                        path,
                                        hunk: id,
                                        hash,
                                    },
                                    cx,
                                );
                            })),
                    )
                    .into_any_element(),
            );
            for (l, line) in hunk.lines.iter().enumerate() {
                let kind = match line.kind {
                    LineKind::Context => RowKind::Context,
                    LineKind::Added => RowKind::Added,
                    LineKind::Removed => RowKind::Removed,
                };
                let (bg, fg) = diff::colors(kind, &theme);
                let marker = match kind {
                    RowKind::Added => "+",
                    RowKind::Removed => "−",
                    _ => " ",
                };
                let number = |n: Option<usize>| n.map_or(String::new(), |n| n.to_string());
                let path = change.path.clone();
                let target = line.new_no.or(line.old_no).unwrap_or(1).saturating_sub(1);
                rows.push(
                    h_flex()
                        .id(("hunk-line", h * 100_000 + l))
                        .h(row_height)
                        .w_full()
                        .when_some(bg, |d, bg| d.bg(bg))
                        .text_color(fg)
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(ReviewEvent::Open {
                                path: path.clone(),
                                line: target,
                            })
                        }))
                        .child(
                            div()
                                .w(px(44.))
                                .flex_none()
                                .text_right()
                                .pr_1()
                                .text_color(muted)
                                .child(number(line.old_no)),
                        )
                        .child(
                            div()
                                .w(px(44.))
                                .flex_none()
                                .text_right()
                                .pr_1()
                                .text_color(muted)
                                .child(number(line.new_no)),
                        )
                        .child(div().w(px(16.)).flex_none().child(marker))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(line.text.replace('\t', "    ")),
                        )
                        .into_any_element(),
                );
            }
        }
        v_flex()
            .id("review-diff")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.diff_scroll)
            .font_family(font)
            .text_size(font_size)
            .children(rows)
            .into_any_element()
    }
}

impl Render for AgentReview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let (changes, stale) = self.changes(cx);
        // Keep the selection on a file that is still changed.
        if self
            .selected
            .as_ref()
            .is_none_or(|p| !changes.iter().any(|c| &c.path == p))
        {
            self.selected = changes.first().map(|c| c.path.clone());
        }
        let selected = self.selected.clone();
        let change = changes.iter().find(|c| Some(&c.path) == selected.as_ref());
        let is_stale = selected.as_ref().is_some_and(|p| stale.contains(p));
        let diff = self.render_diff(change, is_stale, cx);
        v_flex()
            .size_full()
            .bg(colors.editor_background)
            .child(self.render_header(&changes, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_files(&changes, &stale, selected.as_ref(), cx))
                    .child(diff),
            )
    }
}
