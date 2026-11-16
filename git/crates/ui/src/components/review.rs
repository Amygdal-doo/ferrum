//! Pull request review comments inline in the diff: existing threads under
//! their lines, a click on a line to start one, Reply in a thread. Posting
//! goes through the forge API (GitHub review comments, GitLab discussions).

use crate::app::GitView;
use crate::components::detail::{markdown, unix};
use crate::components::now_secs;
use crate::components::widgets::{ghost_button, primary_button};
use ferrum_ui::TextInput;
use ferrum_ui::components::{
    Clickable, Color, Disableable, Label, LabelCommon, LabelSize, StyledTypography, Tooltip,
    h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_application::forge::{NewLineComment, ReviewThread};
use ferrumgit_domain::text::relative_time;
use ferrumgit_domain::{DiffLine, RepoId};
use gpui::{AnyElement, Context, Div, Entity, SharedString, Stateful, div, prelude::*, px};
use std::sync::Arc;

/// A diff line comments can be attached to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTarget {
    pub path: String,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
}

impl LineTarget {
    #[must_use]
    pub fn of(path: &str, line: &DiffLine) -> Self {
        Self {
            path: path.to_owned(),
            old_line: line.old_no.map(u64::from),
            new_line: line.new_no.map(u64::from),
        }
    }
}

/// The pull request whose diff is shown, with its threads.
#[derive(Clone)]
pub struct ReviewCtx {
    pub repo: RepoId,
    pub number: u64,
    pub threads: Arc<Vec<ReviewThread>>,
}

/// What the comment being written answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftTarget {
    /// A new thread on a line.
    Line(LineTarget),
    /// A reply in thread `id`.
    Reply(String),
}

/// The comment being written (one at a time).
pub struct ReviewDraft {
    pub repo: RepoId,
    pub number: u64,
    pub target: DraftTarget,
    pub input: Entity<TextInput>,
    pub sending: bool,
}

/// Makes a diff line start a comment when clicked.
pub fn comment_on_click(
    row: Stateful<Div>,
    target: LineTarget,
    cx: &mut Context<GitView>,
) -> Stateful<Div> {
    row.cursor_pointer()
        .tooltip(Tooltip::text("Comment on this line"))
        .on_click(cx.listener(move |app, _, window, cx| {
            app.start_review_draft(DraftTarget::Line(target.clone()), window, cx);
        }))
}

/// Threads on `line` and, when one is being written there, the composer.
pub fn review_notes(
    app: &GitView,
    review: &ReviewCtx,
    line: &LineTarget,
    cx: &mut Context<GitView>,
) -> Vec<AnyElement> {
    let mut out: Vec<AnyElement> = review
        .threads
        .iter()
        .filter(|t| t.is_on(&line.path, line.old_line, line.new_line))
        .map(|t| thread_box(app, review, t, cx))
        .collect();
    let draft_here = app.review_draft.as_ref().is_some_and(|d| {
        d.repo == review.repo
            && d.number == review.number
            && d.target == DraftTarget::Line(line.clone())
    });
    if draft_here {
        out.push(note_frame(composer(app, "Comment", cx), cx));
    }
    out
}

fn note_frame(child: AnyElement, cx: &mut Context<GitView>) -> AnyElement {
    let colors = cx.theme().colors();
    div()
        .pl(px(104.))
        .pr_4()
        .py_1()
        .font_ui(cx)
        .child(
            v_flex()
                .gap_2()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.elevated_surface_background)
                .child(child),
        )
        .into_any_element()
}

fn thread_box(
    app: &GitView,
    review: &ReviewCtx,
    thread: &ReviewThread,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let now = now_secs();
    let comments: Vec<AnyElement> = thread
        .comments
        .iter()
        .map(|c| {
            v_flex()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Label::new(c.author.clone())
                                .size(LabelSize::Small)
                                .weight(gpui::FontWeight::SEMIBOLD),
                        )
                        .child(
                            Label::new(relative_time(unix(&c.created_at), now))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(markdown(&c.body, cx))
                .into_any_element()
        })
        .collect();
    let replying = app.review_draft.as_ref().is_some_and(|d| {
        d.repo == review.repo
            && d.number == review.number
            && d.target == DraftTarget::Reply(thread.id.clone())
    });
    let id = thread.id.clone();
    let footer = if replying {
        composer(app, "Reply", cx)
    } else {
        h_flex()
            .gap_2()
            .child(
                ghost_button(SharedString::from(format!("reply-{}", thread.id)), "Reply").on_click(
                    cx.listener(move |app, _, window, cx| {
                        app.start_review_draft(DraftTarget::Reply(id.clone()), window, cx);
                    }),
                ),
            )
            .when(thread.resolved, |d| {
                d.child(
                    Label::new("Resolved")
                        .size(LabelSize::XSmall)
                        .color(Color::Success),
                )
            })
            .into_any_element()
    };
    note_frame(
        v_flex()
            .gap_2()
            .children(comments)
            .child(footer)
            .into_any_element(),
        cx,
    )
}

fn composer(app: &GitView, label: &'static str, cx: &mut Context<GitView>) -> AnyElement {
    let Some(draft) = &app.review_draft else {
        return div().into_any_element();
    };
    let sending = draft.sending;
    let empty = draft.input.read(cx).text().trim().is_empty();
    v_flex()
        .gap_2()
        .child(draft.input.clone())
        .child(
            h_flex()
                .gap_2()
                .justify_end()
                .child(
                    ghost_button("review-draft-cancel", "Cancel")
                        .disabled(sending)
                        .on_click(cx.listener(|app, _, _, cx| app.cancel_review_draft(cx))),
                )
                .child(
                    primary_button(
                        "review-draft-send",
                        if sending { "Sending…" } else { label },
                    )
                    .disabled(sending || empty)
                    .on_click(cx.listener(|app, _, _, cx| app.send_review_draft(cx))),
                ),
        )
        .into_any_element()
}

impl ReviewDraft {
    /// The forge request for this draft (`None` for a reply).
    #[must_use]
    pub fn line_comment(&self, body: String) -> Option<NewLineComment> {
        match &self.target {
            DraftTarget::Line(t) => Some(NewLineComment {
                path: t.path.clone(),
                old_line: t.old_line,
                new_line: t.new_line,
                body,
            }),
            DraftTarget::Reply(_) => None,
        }
    }
}
