//! Page columns: the middle list (commits / working-copy files / stashes)
//! and the right panel (details / diff) for the active repository.

mod commit;
pub(crate) mod files;
mod pull_detail;
mod pull_list;
mod stashes;
mod trailers;
mod wip;

use crate::app::GitView;
use crate::components::diff::diff_pane;
use crate::components::file_panel::file_panel;
use crate::components::graph::commit_list;
use crate::components::widgets::empty;
use crate::state::{Page, Selection};
use ferrum_ui::components::v_flex;
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Context, Window, div, prelude::*};

pub fn middle_column(
    app: &mut GitView,
    window: &mut Window,
    cx: &mut Context<GitView>,
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
    // Bisecting is shown on every page until reset.
    let bisect = crate::components::bisect::bisect_banner(app, id, cx);
    // In the window the card behind supplies the background.
    let flat = app.options.compact;
    v_flex()
        .size_full()
        .min_w_0()
        .when(flat, |d| d.bg(cx.theme().colors().editor_background))
        .children(bisect)
        .child(body)
        .into_any_element()
}

pub fn right_column(app: &mut GitView, _: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let view = app.state.view(id);
    let body = match view.page {
        // The file editor covers the panel on every page until closed.
        _ if app.editor_shown() => crate::components::editor::pane::render(app, cx),
        // File history / blame covers the panel on every page until closed.
        _ if let Some(f) = view.file.clone() => file_panel(app, id, f, cx),
        Page::PullRequests => pull_detail::pull_detail(app, id, cx),
        Page::WorkingCopy => match (view.conflict, view.diff) {
            (Some(c), _) => crate::components::conflict::conflict_pane(app, id, c, cx),
            (None, Some(_)) => diff_pane(app, cx),
            (None, None) => empty("Select a file to see its changes"),
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
    let flat = app.options.compact;
    v_flex()
        .flex_1()
        .h_full()
        .min_w_0()
        .when(flat, |d| d.bg(cx.theme().colors().editor_background))
        .child(body)
        .into_any_element()
}
