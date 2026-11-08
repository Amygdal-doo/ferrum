//! Toolbar (Tower style): repositories / profiles over the sidebar, then
//! quick open, back / forward, the repo title, actions and search.

use crate::app::FerrumGit;
use crate::commands::workspace::{open_terminal, reveal};
use crate::components::icons::Icon;
use crate::state::{Dialog, Page, PromptKind};
use ferrumgit_domain::RepoOp;
use gpui::{AnyElement, ClickEvent, Context, ElementId, Window, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{
    Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex, v_flex,
};

fn tool(
    id: impl Into<ElementId>,
    icon: Icon,
    label: &'static str,
    disabled: bool,
    badge: Option<u32>,
    on_click: impl Fn(&mut FerrumGit, &ClickEvent, &mut Window, &mut Context<FerrumGit>) + 'static,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_0p5()
        .h(px(44.))
        .min_w(px(48.))
        .px_1p5()
        .rounded_md()
        .text_ui_xs(cx)
        .text_color(colors.text_muted)
        .tooltip(Tooltip::text(label))
        .when(disabled, |d| d.opacity(0.35))
        .when(!disabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                .on_click(cx.listener(on_click))
        })
        .child(icon.icon().size(IconSize::Medium).color(Color::Default))
        .child(label)
        .children(badge.filter(|n| *n > 0).map(|n| {
            div()
                .absolute()
                .right(px(2.))
                .top(px(2.))
                .px_1()
                .rounded_full()
                .bg(colors.text_accent)
                .text_color(colors.background)
                .text_ui_xs(cx)
                .child(n.to_string())
        }))
        .into_any_element()
}

fn separator(cx: &Context<FerrumGit>) -> impl IntoElement {
    div()
        .mx_1()
        .h(px(28.))
        .w(px(1.))
        .bg(cx.theme().colors().border)
}

pub fn render(app: &FerrumGit, _: &mut Window, cx: &mut Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let sidebar = px(app.state.sidebar_width() as f32);
    let left = h_flex()
        .flex_none()
        .h_full()
        .w(sidebar)
        .justify_end()
        .gap_1()
        .pr_2()
        .border_r_1()
        .border_color(colors.border)
        .child(tool(
            "tool-repos",
            Icon::Bookmark,
            "Repositories",
            false,
            None,
            |app, _, _, cx| app.activate(None, cx),
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
    let bar = h_flex()
        .flex_none()
        .h(px(52.))
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.title_bar_background)
        .child(left);

    let Some(entry) = app.state.active_entry() else {
        return bar
            .child(
                div()
                    .flex_1()
                    .px_4()
                    .child(Label::new("FerrumGIT").weight(gpui::FontWeight::SEMIBOLD)),
            )
            .into_any_element();
    };
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
    let subtitle = match view.page {
        Page::History => format!(
            "History ({}{} Commits)",
            snap.commits.len(),
            if snap.has_more { "+" } else { "" }
        ),
        Page::WorkingCopy => format!("Working Copy ({} changes)", status.files.len()),
        Page::Stashes => format!("Stashes ({})", snap.stashes.len()),
    };
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

    let title = v_flex()
        .flex_1()
        .min_w_0()
        .justify_center()
        .px_2()
        .child(
            h_flex()
                .min_w_0()
                .gap_2()
                .child(
                    Label::new(entry.name.clone())
                        .weight(gpui::FontWeight::SEMIBOLD)
                        .truncate(),
                )
                .child(
                    h_flex()
                        .id("tool-repo-profile")
                        .flex_none()
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
                                .color(Color::Muted),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.open_dialog(Dialog::RepoSettings(id), window, cx);
                        })),
                ),
        )
        .child(match busy {
            Some(b) => h_flex()
                .gap_2()
                .child(div().size(px(6.)).rounded_full().bg(colors.text_accent))
                .child(
                    Label::new(format!("{}…", b.label))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .when(b.cancel.is_some(), |d| {
                    d.child(
                        div()
                            .id("tool-cancel")
                            .cursor_pointer()
                            .text_ui_sm(cx)
                            .text_color(colors.text_muted)
                            .hover(|s| s.text_color(colors.text))
                            .child("Cancel")
                            .on_click(cx.listener(move |app, _, _, _| app.cancel(id))),
                    )
                })
                .into_any_element(),
            None => Label::new(subtitle)
                .size(LabelSize::Small)
                .color(Color::Muted)
                .truncate()
                .into_any_element(),
        });

    let nav = h_flex()
        .gap_0p5()
        .px_2()
        .child(tool(
            "tool-quick-open",
            Icon::QuickOpen,
            "Quick Open",
            false,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::QuickOpen, window, cx),
            cx,
        ))
        .child(tool(
            "tool-prev",
            Icon::ChevronLeft,
            "Prev",
            view.back.is_empty(),
            None,
            move |app, _, _, cx| app.go(id, true, cx),
            cx,
        ))
        .child(tool(
            "tool-next",
            Icon::ChevronRight,
            "Next",
            view.forward.is_empty(),
            None,
            move |app, _, _, cx| app.go(id, false, cx),
            cx,
        ));

    let actions = h_flex()
        .gap_0p5()
        .px_1()
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
        .child(separator(cx))
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
            "Apply",
            is_busy || !has_stash,
            None,
            move |app, _, _, cx| app.op(id, RepoOp::StashPop(0), cx),
            cx,
        ))
        .child(tool(
            "tool-save",
            Icon::Archive,
            "Save",
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
        .child(tool(
            "tool-refresh",
            Icon::Refresh,
            "Refresh",
            false,
            None,
            move |app, _, _, cx| app.load(id, cx),
            cx,
        ));

    let search = h_flex()
        .flex_none()
        .mr_3()
        .ml_1()
        .h(px(32.))
        .w(px(208.))
        .gap_1p5()
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(
            Icon::Search
                .icon()
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(div().flex_1().min_w_0().child(app.inputs.search.clone()));

    bar.child(nav)
        .child(title)
        .child(actions)
        .child(search)
        .into_any_element()
}
