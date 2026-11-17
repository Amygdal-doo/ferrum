//! The Parallel page: the task and how many agents before a run; during
//! one, a column per worktree with its chat over its diff, and the
//! questions (keep, discard, remove a leftover) as dialogs.

use ferrum_ui::components::{TintColor, Toggleable};
use ferrum_ui::gpui::relative;
use ferrum_ui::prelude::*;
use ferrum_ui::theme::theme_settings;

use super::worktree::Diff;
use super::{COUNTS, Confirm};
use crate::app::FerrumEngineer;
use crate::nav::Page;
use crate::pages::{field, frame, placeholder, row, section, two_lines};

/// A lane's diff as last read.
type LaneDiff = Result<Diff, String>;

/// Patch lines drawn per worktree.
const PATCH_LINES: usize = 400;

impl FerrumEngineer {
    pub(crate) fn parallel_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.parallel.run.is_some() {
            return self.parallel_run(cx);
        }
        let busy = self.parallel.busy.clone();
        let mut page = frame(
            Page::Parallel,
            "The same task for several agents, each in its own git worktree of this project \
             with its own chat. Compare their diffs, then keep one: its changes are applied to \
             your working tree and the worktrees are removed.",
            cx,
        )
        .child(field(&self.parallel.task, cx).min_h(px(64.)));
        let mut counts = h_flex().gap_1().child(
            Label::new("Agents")
                .size(LabelSize::Small)
                .color(Color::Muted),
        );
        for count in COUNTS {
            counts = counts.child(
                Button::new(("parallel-count", count), count.to_string())
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .toggle_state(self.parallel.count == count)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.parallel.count = count;
                        cx.notify();
                    })),
            );
        }
        page = page.child(
            h_flex()
                .gap_3()
                .child(counts)
                .child(div().flex_1())
                .when_some(busy, |row, busy| {
                    row.child(Label::new(busy).size(LabelSize::Small).color(Color::Muted))
                })
                .child(
                    Button::new("parallel-start", "Run in parallel")
                        .style(ButtonStyle::Filled)
                        .size(ButtonSize::Compact)
                        .disabled(self.project.is_none() || self.parallel.busy.is_some())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.start_parallel(window, cx)),
                        ),
                ),
        );
        page = page.child(placeholder(
            "Needs a git repository's top folder with a commit. Worktrees start from HEAD, \
             detached, in ~/.ferrum/vibe/worktrees; your files are not touched until you keep one.",
            cx,
        ));
        if !self.parallel.leftovers.is_empty() {
            page = page.child(section("Left by earlier runs", cx));
            for (i, path) in self.parallel.leftovers.iter().enumerate() {
                let remove = path.clone();
                page = page.child(
                    row(("parallel-leftover", i), cx)
                        .child(two_lines(
                            path.file_name()
                                .map_or_else(String::new, |n| n.to_string_lossy().into()),
                            path.display().to_string(),
                        ))
                        .child(
                            Button::new(("parallel-leftover-remove", i), "Remove…")
                                .style(ButtonStyle::Subtle)
                                .size(ButtonSize::Compact)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.parallel.confirm =
                                        Some(Confirm::RemoveLeftover(remove.clone()));
                                    cx.notify();
                                })),
                        ),
                );
            }
        }
        page.into_any_element()
    }

    fn parallel_run(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(run) = &self.parallel.run else {
            return div().into_any_element();
        };
        let colors = cx.theme().colors().clone();
        let share = 1.0 / run.lanes.len().max(1) as f32;
        let header = h_flex()
            .flex_none()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(colors.border)
            .child(two_lines(
                run.task.clone(),
                format!(
                    "{} agents · from {}",
                    run.lanes.len(),
                    &run.base[..run.base.len().min(8)]
                ),
            ))
            .when_some(self.parallel.busy.clone(), |row, busy| {
                row.child(Label::new(busy).size(LabelSize::Small).color(Color::Muted))
            })
            .child(
                Button::new("parallel-discard", "Discard all…")
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .disabled(self.parallel.busy.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.parallel.confirm = Some(Confirm::Discard);
                        cx.notify();
                    })),
            );
        let lanes: Vec<(usize, std::path::PathBuf, Option<LaneDiff>)> = run
            .lanes
            .iter()
            .enumerate()
            .map(|(i, lane)| (i, lane.path.clone(), lane.diff.clone()))
            .collect();
        let mut columns = h_flex().flex_1().min_h_0().w_full();
        for (i, path, diff) in lanes {
            let chat = self.chats.get(&path).map(|slot| slot.view.clone());
            columns = columns.child(
                v_flex()
                    .h_full()
                    .flex_basis(relative(share))
                    .flex_shrink(1.)
                    .min_w_0()
                    .when(i > 0, |c| c.border_l_1().border_color(colors.border))
                    .child(self.lane_header(i, diff.as_ref(), cx))
                    .child(
                        div()
                            .flex_basis(relative(0.6))
                            .min_h_0()
                            .overflow_hidden()
                            .children(chat),
                    )
                    .child(lane_diff(i, diff.as_ref(), cx)),
            );
        }
        v_flex()
            .size_full()
            .debug_selector(|| "page-Parallel".into())
            .child(header)
            .child(columns)
            .into_any_element()
    }

    fn lane_header(
        &self,
        i: usize,
        diff: Option<&Result<Diff, String>>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let stats = match diff {
            Some(Ok(diff)) => {
                let (add, del) = diff.totals();
                format!("{} files +{add} −{del}", diff.files.len())
            }
            Some(Err(_)) => "diff failed".into(),
            None => "no changes read yet".into(),
        };
        let busy = self.parallel.busy.is_some();
        h_flex()
            .flex_none()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                Label::new(format!("Worktree {}", i + 1))
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::SEMIBOLD),
            )
            .child(
                Label::new(stats)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(
                ferrum_ui::title_button(
                    ("parallel-refresh", i),
                    IconName::RotateCw,
                    "Read its diff",
                )
                .on_click(cx.listener(move |this, _, _, cx| this.refresh_lane(i, cx))),
            )
            .child(
                Button::new(("parallel-keep", i), "Keep this one…")
                    .style(ButtonStyle::Filled)
                    .size(ButtonSize::Compact)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.ask_keep(i, cx))),
            )
    }

    pub(crate) fn render_parallel_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirm = self.parallel.confirm.as_ref()?;
        let others = self
            .parallel
            .run
            .as_ref()
            .map_or(0, |r| r.lanes.len().saturating_sub(1));
        let (title, body, action, tint): (String, Vec<String>, &str, TintColor) = match confirm {
            Confirm::Keep { lane, dirty } if dirty.is_empty() => (
                format!("Keep worktree {}?", lane + 1),
                vec![
                    "Its changes are applied to your working tree (not committed).".into(),
                    format!("The agents stop, and all {} worktrees are removed: the other {others} agents' changes are lost.", others + 1),
                ],
                "Keep",
                TintColor::Accent,
            ),
            Confirm::Keep { lane, dirty } => (
                format!("Stash your changes and keep worktree {}?", lane + 1),
                vec![
                    format!(
                        "Your working tree has {} uncommitted change{} ({}…). They are stashed first \
                         (git stash push --include-untracked) and stay in git stash list; nothing is lost.",
                        dirty.len(),
                        if dirty.len() == 1 { "" } else { "s" },
                        dirty.iter().take(3).map(|d| d.trim()).collect::<Vec<_>>().join(", ")
                    ),
                    format!("Then worktree {}'s changes are applied and all worktrees are removed: the other {others} agents' changes are lost.", lane + 1),
                ],
                "Stash and keep",
                TintColor::Warning,
            ),
            Confirm::Discard => (
                "Discard the parallel run?".into(),
                vec!["The agents stop and every worktree is removed with its changes. Your working tree is not touched.".into()],
                "Discard all",
                TintColor::Error,
            ),
            Confirm::RemoveLeftover(path) => (
                "Remove this worktree?".into(),
                vec![format!("{} and any changes in it are removed.", path.display())],
                "Remove",
                TintColor::Error,
            ),
        };
        let mut modal = Modal::new(title)
            .on_dismiss(cx.listener(|this, _, window, cx| this.answer_parallel(false, window, cx)));
        for line in body {
            modal = modal.child(Label::new(line).size(LabelSize::Small));
        }
        let modal = modal
            .footer(Button::new("parallel-confirm-cancel", "Cancel").on_click(
                cx.listener(|this, _, window, cx| this.answer_parallel(false, window, cx)),
            ))
            .footer(
                Button::new("parallel-confirm", action)
                    .style(ButtonStyle::Tinted(tint))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.answer_parallel(true, window, cx)),
                    ),
            );
        Some(modal.into_any_element())
    }
}

/// A worktree's changes: files with their counts, then the patch.
fn lane_diff(i: usize, diff: Option<&Result<Diff, String>>, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let mono = theme_settings(cx).buffer_font.family.clone();
    let mut body = v_flex()
        .id(("parallel-diff", i))
        .flex_basis(relative(0.4))
        .min_h_0()
        .overflow_y_scroll()
        .border_t_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .px_2()
        .py_1()
        .font_family(mono)
        .text_xs();
    match diff {
        None => {
            return body
                .child(Label::new("Waiting for the agent's first changes.").color(Color::Muted))
                .into_any_element();
        }
        Some(Err(error)) => {
            return body
                .child(Label::new(error.clone()).color(Color::Error))
                .into_any_element();
        }
        Some(Ok(diff)) if diff.files.is_empty() => {
            return body
                .child(Label::new("No changes yet.").color(Color::Muted))
                .into_any_element();
        }
        Some(Ok(diff)) => {
            for (path, add, del) in &diff.files {
                body = body.child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(path.clone()))
                        .child(
                            div()
                                .text_color(Color::Created.color(cx))
                                .child(format!("+{add}")),
                        )
                        .child(
                            div()
                                .text_color(Color::Deleted.color(cx))
                                .child(format!("−{del}")),
                        ),
                );
            }
            body = body.child(div().h(px(6.)));
            let lines: Vec<&str> = diff.patch.lines().collect();
            for line in lines.iter().take(PATCH_LINES) {
                let color = if line.starts_with("+++") || line.starts_with("---") {
                    colors.text_muted
                } else if line.starts_with('+') {
                    Color::Created.color(cx)
                } else if line.starts_with('-') {
                    Color::Deleted.color(cx)
                } else if line.starts_with("@@") || line.starts_with("diff ") {
                    colors.text_accent
                } else {
                    colors.text
                };
                body = body.child(div().text_color(color).whitespace_nowrap().child(
                    if line.is_empty() {
                        " ".to_string()
                    } else {
                        (*line).to_string()
                    },
                ));
            }
            if lines.len() > PATCH_LINES {
                body = body.child(
                    Label::new(format!("… {} more lines", lines.len() - PATCH_LINES))
                        .color(Color::Muted),
                );
            }
        }
    }
    body.into_any_element()
}
