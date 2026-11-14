//! The cards' 40px header rows. The list card: (◧ ⌘ + while the sidebar is
//! hidden), back / forward / undo, the page title and search. The detail
//! card: the repository and its identity, then the repository actions as
//! small icon buttons. The traffic lights sit over the sidebar's own row.

use crate::app::GitView;
use crate::commands::workspace::{open_terminal, reveal};
use crate::components::icons::Icon;
use crate::state::{Dialog, Page, PromptKind};
use ferrum_ui::components::{
    Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{RepoId, RepoOp};
use gpui::{AnyElement, ClickEvent, Context, ElementId, Window, div, prelude::*, px};

fn tool(
    id: impl Into<ElementId>,
    icon: Icon,
    label: &'static str,
    disabled: bool,
    badge: Option<u32>,
    on_click: impl Fn(&mut GitView, &ClickEvent, &mut Window, &mut Context<GitView>) + 'static,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let on_accent = theme.readable_text_on(colors.text_accent);
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(28.))
        .rounded(px(7.))
        .text_color(colors.text_muted)
        .tooltip(Tooltip::text(label))
        .when(disabled, |d| d.opacity(0.35))
        .when(!disabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(ferrum_ui::hover_fill(cx)).text_color(colors.text))
                .on_click(cx.listener(on_click))
        })
        .child(icon.icon().size(IconSize::Small).color(Color::Muted))
        .children(badge.filter(|n| *n > 0).map(|n| {
            div()
                .absolute()
                .right(px(-2.))
                .top(px(-2.))
                .px(px(3.))
                .rounded_full()
                .bg(colors.text_accent)
                .text_color(on_accent)
                .text_size(px(9.))
                .child(n.to_string())
        }))
        .into_any_element()
}

fn separator(cx: &Context<GitView>) -> impl IntoElement {
    div()
        .mx_1()
        .h(px(16.))
        .w(px(1.))
        .bg(cx.theme().colors().border_variant)
}

/// A card's 40px header row (it drags the window too). The first card's
/// header holds the window controls while the sidebar is hidden.
fn card_header(
    id: &'static str,
    first: bool,
    app: &GitView,
    window: &Window,
    cx: &mut Context<GitView>,
) -> gpui::Stateful<gpui::Div> {
    let controls = first && !app.sidebar_shown();
    ferrum_ui::title_row(id, controls, window)
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .when(controls, |row| {
            row.child(title_controls(app, cx)).child(div().w(px(6.)))
        })
}

/// Header of the single card shown while no repository is open (home,
/// loading): window controls and a title.
pub fn home_header(
    app: &GitView,
    title: &'static str,
    window: &Window,
    cx: &mut Context<GitView>,
) -> AnyElement {
    card_header("home-header", true, app, window, cx)
        .child(
            div()
                .flex_1()
                .px_1()
                .child(Label::new(title).weight(gpui::FontWeight::SEMIBOLD)),
        )
        .into_any_element()
}

/// The list card's header: back / forward / undo, the page title with its
/// count, and search.
pub fn list_header(app: &GitView, window: &Window, cx: &mut Context<GitView>) -> AnyElement {
    let row = card_header("list-header", true, app, window, cx);
    let Some(id) = app.state.active else {
        return row.into_any_element();
    };
    let view = app.state.view(id);
    let busy = app.state.busy.contains_key(&id);
    let (back_empty, forward_empty) = (view.back.is_empty(), view.forward.is_empty());
    let no_undo = view.data.as_ref().is_none_or(|d| d.snapshot.undo.is_none());
    let title = page_title(app, id);

    let nav = h_flex()
        .flex_none()
        .gap_0p5()
        .child(tool(
            "tool-prev",
            Icon::ChevronLeft,
            "Prev",
            back_empty,
            None,
            move |app, _, _, cx| app.go(id, true, cx),
            cx,
        ))
        .child(tool(
            "tool-next",
            Icon::ChevronRight,
            "Next",
            forward_empty,
            None,
            move |app, _, _, cx| app.go(id, false, cx),
            cx,
        ))
        .child(tool(
            "tool-undo",
            Icon::Undo,
            "Undo",
            busy || no_undo,
            None,
            move |app, _, window, cx| app.undo(id, window, cx),
            cx,
        ));

    let search = h_flex()
        .min_w(px(90.))
        .w(px(180.))
        .h(px(28.))
        .gap_1p5()
        .px_2()
        .rounded(px(7.))
        .bg(ferrum_ui::hover_fill(cx))
        .child(
            Icon::Search
                .icon()
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(div().flex_1().min_w_0().child(app.inputs.search.clone()));

    row.child(nav)
        .child(
            div().flex_1().min_w_0().px_1().child(
                Label::new(title)
                    .weight(gpui::FontWeight::SEMIBOLD)
                    .truncate(),
            ),
        )
        .child(search)
        .into_any_element()
}

/// "History (58 Commits)" and the like for the active page.
fn page_title(app: &GitView, id: RepoId) -> String {
    let view = app.state.view(id);
    let snap = view.data.as_ref().map(|d| &d.snapshot);
    let commits = snap.map_or(0, |s| s.commits.len());
    let more = snap.is_some_and(|s| s.has_more);
    match view.page {
        Page::History => format!("History ({commits}{} Commits)", if more { "+" } else { "" }),
        Page::WorkingCopy => format!(
            "Working Copy ({} changes)",
            snap.map_or(0, |s| s.status.files.len())
        ),
        Page::Stashes => format!("Stashes ({})", snap.map_or(0, |s| s.stashes.len())),
        Page::PullRequests => match view.prs.info.ready() {
            Some(info) => {
                let noun = info.repo.kind.pr_noun();
                let n = view.prs.list.ready().map_or(0, |l| l.prs.len());
                let mut title = noun.to_owned();
                if let Some(first) = title.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                format!("{title}s ({n} open)")
            }
            None => "Pull Requests".to_owned(),
        },
    }
}

/// The detail card's header: the repository's identity (or the running
/// operation; with the repository's name while the sidebar is hidden), then
/// the repository actions.
pub fn detail_header(app: &GitView, window: &Window, cx: &mut Context<GitView>) -> AnyElement {
    let row = card_header("detail-header", false, app, window, cx);
    let Some(entry) = app.state.active_entry() else {
        return row.into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let id = entry.id;
    let view = app.state.view(id);
    let busy = app.state.busy.get(&id).cloned();
    let is_busy = busy.is_some();
    let snap = view
        .data
        .as_ref()
        .map(|d| d.snapshot.clone())
        .unwrap_or_default();
    let status = snap.status.clone();
    let profile = app
        .state
        .profile_of(&entry)
        .map_or_else(|| "System default".to_owned(), |p| p.name);
    let pull_mode = app.state.settings.pull_mode;
    let prune = app.state.settings.prune_on_fetch;
    let has_stash = !snap.stashes.is_empty();
    let dirty = !status.is_clean();
    let detached = status.branch.head.is_none();
    let (path_term, path_finder) = (entry.path.clone(), entry.path.clone());

    let identity = h_flex()
        .id("tool-repo-profile")
        .min_w(px(64.))
        .gap_1()
        .px_2()
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .cursor_pointer()
        .hover(|s| s.bg(colors.element_hover))
        .tooltip(Tooltip::text("Identity and SSH key for this repository"))
        .child(Icon::User.icon().size(IconSize::XSmall).color(Color::Muted))
        .child(
            Label::new(profile)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        )
        .on_click(cx.listener(move |app, _, window, cx| {
            app.open_dialog(Dialog::RepoSettings(id), window, cx);
        }));
    let state = match busy {
        Some(b) => h_flex()
            .min_w_0()
            .gap_2()
            .child(div().size(px(6.)).rounded_full().bg(colors.text_accent))
            .child(
                Label::new(format!("{}…", b.label))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            )
            .when(b.cancel.is_some(), |d| {
                d.child(
                    div()
                        .id("tool-cancel")
                        .flex_none()
                        .cursor_pointer()
                        .text_ui_sm(cx)
                        .text_color(colors.text_muted)
                        .hover(|s| s.text_color(colors.text))
                        .child("Cancel")
                        .on_click(cx.listener(move |app, _, _, _| app.cancel(id))),
                )
            })
            .into_any_element(),
        None => identity.into_any_element(),
    };
    // The project bar (naming the repository) hides with the sidebar.
    let title = h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .px_1()
        .overflow_hidden()
        .when(!app.sidebar_shown(), |row| {
            row.child(
                div().min_w_0().child(
                    Label::new(entry.name.clone())
                        .weight(gpui::FontWeight::SEMIBOLD)
                        .truncate(),
                ),
            )
        })
        .child(state);

    let actions = h_flex()
        .flex_none()
        .gap_0p5()
        .child(tool(
            "tool-fetch",
            Icon::Fetch,
            "Fetch",
            is_busy,
            None,
            move |app, _, _, cx| {
                app.op(
                    id,
                    RepoOp::Fetch {
                        remote: None,
                        prune,
                    },
                    cx,
                )
            },
            cx,
        ))
        .child(tool(
            "tool-pull",
            Icon::Pull,
            "Pull",
            is_busy || detached,
            Some(status.branch.behind),
            move |app, _, _, cx| app.op(id, RepoOp::Pull(pull_mode), cx),
            cx,
        ))
        .child(tool(
            "tool-push",
            Icon::Push,
            "Push",
            is_busy || detached,
            Some(status.branch.ahead),
            move |app, _, _, cx| {
                app.op(
                    id,
                    RepoOp::Push {
                        remote: None,
                        force_with_lease: false,
                    },
                    cx,
                );
            },
            cx,
        ))
        .child(tool(
            "tool-sync",
            Icon::Sync,
            "Sync",
            is_busy || detached,
            None,
            move |app, _, _, cx| app.sync(id, cx),
            cx,
        ))
        .child(separator(cx))
        .child(tool(
            "tool-merge",
            Icon::Merge,
            "Merge",
            is_busy || detached,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::PickBranch { rebase: false }, window, cx),
            cx,
        ))
        .child(tool(
            "tool-rebase",
            Icon::Rebase,
            "Rebase",
            is_busy || detached,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::PickBranch { rebase: true }, window, cx),
            cx,
        ))
        .child(separator(cx))
        .child(tool(
            "tool-apply",
            Icon::ArchiveRestore,
            "Apply stash",
            is_busy || !has_stash,
            None,
            move |app, _, _, cx| app.op(id, RepoOp::StashPop(0), cx),
            cx,
        ))
        .child(tool(
            "tool-save",
            Icon::Archive,
            "Save stash",
            is_busy || !dirty,
            None,
            |app, _, window, cx| {
                app.open_dialog(
                    Dialog::Prompt {
                        kind: PromptKind::StashSave,
                        initial: String::new(),
                    },
                    window,
                    cx,
                );
            },
            cx,
        ))
        .child(separator(cx))
        .child(tool(
            "tool-refresh",
            Icon::Refresh,
            "Refresh",
            false,
            None,
            move |app, _, _, cx| app.load(id, cx),
            cx,
        ))
        .child(separator(cx))
        .child(tool(
            "tool-terminal",
            Icon::Terminal,
            "Terminal",
            false,
            None,
            move |_, _, _, _| open_terminal(&path_term),
            cx,
        ))
        .child(tool(
            "tool-finder",
            Icon::Folder,
            "Finder",
            false,
            None,
            move |_, _, _, _| reveal(&path_finder),
            cx,
        ))
        .child(tool(
            "tool-profiles",
            Icon::User,
            "Profiles",
            false,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::Profiles(None), window, cx),
            cx,
        ));

    row.child(title).child(actions).into_any_element()
}

/// ◧ (sidebar), ⌘ (quick open) and + (repositories): in the sidebar's
/// title row, or at the start of the list header while the sidebar is hidden.
pub fn title_controls(app: &GitView, cx: &mut Context<GitView>) -> impl IntoElement {
    use ferrum_ui::components::{Clickable, IconName};
    h_flex()
        .flex_none()
        .gap_0p5()
        .child(
            ferrum_ui::sidebar_toggle("toggle-sidebar", app.sidebar_open).on_click(cx.listener(
                |app, _, _, cx| {
                    app.sidebar_open = !app.sidebar_open;
                    cx.notify();
                },
            )),
        )
        .when(app.options.show_repo_tabs, |row| {
            row.child(ferrum_ui::settings_button("open-settings"))
        })
        .when(app.options.show_repo_tabs, |row| {
            row.child(
                ferrum_ui::command_button("quick-open", "Quick Open (⌘P)").on_click(
                    cx.listener(|app, _, window, cx| {
                        app.open_dialog(Dialog::QuickOpen, window, cx)
                    }),
                ),
            )
            .child(
                ferrum_ui::title_button(
                    "repositories",
                    IconName::Plus,
                    "Repositories: add, clone or init",
                )
                .on_click(cx.listener(|app, _, _, cx| app.activate(None, cx))),
            )
        })
}
