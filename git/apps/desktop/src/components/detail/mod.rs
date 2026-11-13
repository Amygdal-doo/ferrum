//! Page columns: the middle list (commits / working-copy files / stashes)
//! and the right panel (details / diff) for the active repository.

mod commit;
mod files;
mod pull_detail;
mod pull_list;
mod stashes;
mod wip;

use crate::app::FerrumGit;
use crate::components::diff::diff_pane;
use crate::components::file_panel::file_panel;
use crate::components::graph::commit_list;
use crate::components::widgets::empty;
use crate::state::{Page, Selection};
use ferrum_ui::components::v_flex;
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Context, Window, div, prelude::*};

pub fn middle_column(
    app: &mut FerrumGit,
    window: &mut Window,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let view = app.state.view(id);
    let body = match view.page {
        Page::History => commit_list(app, window, cx),
        Page::WorkingCopy if view.data.is_some() => wip::wip_panel(app, id, window, cx),
        Page::Stashes if view.data.is_some() => stashes::stash_list(app, id, cx),
        Page::PullRequests => pull_list::pull_list(app, id, cx),
        _ => empty("Loading…"),
    };
    v_flex()
        .size_full()
        .min_w_0()
        .bg(cx.theme().colors().editor_background)
        .child(body)
        .into_any_element()
}

pub fn right_column(
    app: &mut FerrumGit,
    _: &mut Window,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let view = app.state.view(id);
    let body = match view.page {
        // File history / blame covers the panel on every page until closed.
        _ if let Some(f) = view.file.clone() => file_panel(app, id, f, cx),
        Page::PullRequests => pull_detail::pull_detail(app, id, cx),
        Page::WorkingCopy => match view.diff {
            Some(_) => diff_pane(app, cx),
            None => empty("Select a file to see its changes"),
        },
        Page::History | Page::Stashes => match view.details.clone() {
            Some(d) => commit::commit_panel(app, id, d, cx),
            None if view.page == Page::Stashes
                && view
                    .data
                    .as_ref()
                    .is_some_and(|d| d.snapshot.stashes.is_empty()) =>
            {
                empty("No stashes")
            }
            None if matches!(view.selection, Selection::Wip) => empty("Select a commit"),
            None => empty("Loading…"),
        },
    };
    v_flex()
        .flex_1()
        .h_full()
        .min_w_0()
        .bg(cx.theme().colors().editor_background)
        .child(body)
        .into_any_element()
}
