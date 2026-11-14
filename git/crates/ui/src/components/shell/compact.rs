//! Toolbar of the compact (embedded) layout: branch drawer toggle, the
//! repository actions as icon buttons, then the page tabs and search.

use crate::app::GitView;
use crate::components::icons::Icon;
use crate::state::{Dialog, Page, PromptKind};
use ferrum_ui::components::{
    Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{RepoId, RepoOp};
use gpui::{AnyElement, ClickEvent, Context, ElementId, Window, div, prelude::*, px};

fn icon_button(
    id: impl Into<ElementId>,
    icon: Icon,
    label: &'static str,
    disabled: bool,
    badge: Option<u32>,
    on_click: impl Fn(&mut GitView, &ClickEvent, &mut Window, &mut Context<GitView>) + 'static,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let on_accent = crate::components::text_on(colors.text_accent, cx);
    div()
        .id(id)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(26.))
        .rounded_md()
        .tooltip(Tooltip::text(label))
        .when(disabled, |d| d.opacity(0.35))
        .when(!disabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(colors.element_hover))
                .on_click(cx.listener(on_click))
        })
        .child(icon.icon().size(IconSize::Small).color(Color::Default))
        .children(badge.filter(|n| *n > 0).map(|n| {
            div()
                .absolute()
                .right(px(-2.))
                .top(px(-2.))
                .px_0p5()
                .rounded_full()
                .bg(colors.text_accent)
                .text_color(on_accent)
                .text_ui_xs(cx)
                .child(n.to_string())
        }))
        .into_any_element()
}

fn page_tab(
    id: RepoId,
    page: Page,
    label: &'static str,
    active: bool,
    badge: usize,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let colors = cx.theme().colors().clone();
    h_flex()
        .id(ElementId::Name(format!("compact-page-{label}").into()))
        .flex_none()
        .h_full()
        .gap_1()
        .px_2()
        .cursor_pointer()
        .border_b_2()
        .border_color(if active {
            colors.text_accent
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(colors.element_hover))
        .child(Label::new(label).size(LabelSize::Small).color(if active {
            Color::Accent
        } else {
            Color::Muted
        }))
        .when(badge > 0, |d| {
            d.child(
                Label::new(badge.to_string())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
        })
        .on_click(cx.listener(move |app, _, _, cx| {
            if page == Page::PullRequests {
                app.open_pull_requests(id, cx);
            } else {
                app.set_page(id, page, cx);
            }
        }))
        .into_any_element()
}

pub fn render(app: &GitView, _: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let bar = h_flex()
        .flex_none()
        .flex_wrap()
        .min_h(px(36.))
        .gap_1()
        .px_1p5()
        .py_1()
        .bg(colors.title_bar_background);
    let Some(entry) = app.state.active_entry() else {
        if !app.options.show_toolbar {
            return div().into_any_element();
        }
        return v_flex()
            .flex_none()
            .border_b_1()
            .border_color(colors.border)
            .child(
                bar.child(
                    div().flex_1().px_1().child(
                        Label::new("FerrumGIT")
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    ),
                )
                .child(icon_button(
                    "compact-profiles",
                    Icon::User,
                    "Profiles",
                    false,
                    None,
                    |app, _, window, cx| app.open_dialog(Dialog::Profiles(None), window, cx),
                    cx,
                )),
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
    let status = &snap.status;
    let detached = status.branch.head.is_none();
    let dirty = !status.is_clean();
    let has_stash = !snap.stashes.is_empty();
    let pull_mode = app.state.settings.pull_mode;
    let prune = app.state.settings.prune_on_fetch;
    let branch = status
        .branch
        .head
        .clone()
        .unwrap_or_else(|| "detached HEAD".to_owned());
    let drawer_open = app.drawer_open;

    let branch_button = h_flex()
        .id("compact-branches")
        .flex_1()
        .min_w(px(96.))
        .h(px(26.))
        .gap_1()
        .px_1p5()
        .rounded_md()
        .cursor_pointer()
        .when(drawer_open, |d| d.bg(colors.element_selected))
        .hover(|s| s.bg(colors.element_hover))
        .tooltip(Tooltip::text("Branches, tags and remotes"))
        .child(
            Icon::Branch
                .icon()
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            div().min_w_0().child(
                Label::new(branch)
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::MEDIUM)
                    .truncate(),
            ),
        )
        .children(busy.as_ref().map(|b| {
            Label::new(format!("{}…", b.label))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate()
        }))
        .on_click(cx.listener(|app, _, _, cx| {
            app.drawer_open = !app.drawer_open;
            cx.notify();
        }));

    let actions = h_flex()
        .flex_none()
        .gap_0p5()
        .child(icon_button(
            "compact-fetch",
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
                );
            },
            cx,
        ))
        .child(icon_button(
            "compact-pull",
            Icon::Pull,
            "Pull",
            is_busy || detached,
            Some(status.branch.behind),
            move |app, _, _, cx| app.op(id, RepoOp::Pull(pull_mode), cx),
            cx,
        ))
        .child(icon_button(
            "compact-push",
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
        .child(icon_button(
            "compact-merge",
            Icon::Merge,
            "Merge…",
            is_busy || detached,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::PickBranch { rebase: false }, window, cx),
            cx,
        ))
        .child(icon_button(
            "compact-rebase",
            Icon::Rebase,
            "Rebase…",
            is_busy || detached,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::PickBranch { rebase: true }, window, cx),
            cx,
        ))
        .child(icon_button(
            "compact-stash",
            Icon::Archive,
            "Stash changes…",
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
        .child(icon_button(
            "compact-unstash",
            Icon::ArchiveRestore,
            "Pop latest stash",
            is_busy || !has_stash,
            None,
            move |app, _, _, cx| app.op(id, RepoOp::StashPop(0), cx),
            cx,
        ))
        .child(icon_button(
            "compact-undo",
            Icon::Undo,
            "Undo",
            is_busy || snap.undo.is_none(),
            None,
            move |app, _, window, cx| app.undo(id, window, cx),
            cx,
        ))
        .child(icon_button(
            "compact-refresh",
            Icon::Refresh,
            "Refresh",
            false,
            None,
            move |app, _, _, cx| app.load(id, cx),
            cx,
        ))
        .child(icon_button(
            "compact-settings",
            Icon::Sliders,
            "Repository settings",
            false,
            None,
            move |app, _, window, cx| app.open_dialog(Dialog::RepoSettings(id), window, cx),
            cx,
        ))
        .child(icon_button(
            "compact-profiles",
            Icon::User,
            "Profiles",
            false,
            None,
            |app, _, window, cx| app.open_dialog(Dialog::Profiles(None), window, cx),
            cx,
        ));

    let open_prs = view.prs.list.ready().map_or(0, |l| l.prs.len());
    let prs_label = match view.prs.info.ready().map(|i| i.repo.kind) {
        Some(ferrumgit_domain::ForgeKind::GitLab) => "MRs",
        _ => "PRs",
    };
    let tabs = h_flex()
        .flex_none()
        .h(px(30.))
        .gap_1()
        .px_1p5()
        .border_t_1()
        .border_color(colors.border_variant)
        .child(page_tab(
            id,
            Page::WorkingCopy,
            "Changes",
            view.page == Page::WorkingCopy,
            status.files.len(),
            cx,
        ))
        .child(page_tab(
            id,
            Page::History,
            "History",
            view.page == Page::History,
            0,
            cx,
        ))
        .child(page_tab(
            id,
            Page::Stashes,
            "Stashes",
            view.page == Page::Stashes,
            snap.stashes.len(),
            cx,
        ))
        .child(page_tab(
            id,
            Page::PullRequests,
            prs_label,
            view.page == Page::PullRequests,
            open_prs,
            cx,
        ))
        .child(
            h_flex()
                .flex_1()
                .min_w(px(72.))
                .h(px(24.))
                .ml_1()
                .gap_1()
                .px_1p5()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(
                    Icon::Search
                        .icon()
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(div().flex_1().min_w_0().child(app.inputs.search.clone())),
        );

    // Hosts that run these actions themselves keep only the page tabs.
    let show_toolbar = app.options.show_toolbar;
    v_flex()
        .flex_none()
        .border_b_1()
        .border_color(colors.border)
        .when(show_toolbar, |col| {
            col.bg(colors.title_bar_background)
                .child(bar.child(branch_button).child(actions))
        })
        .when(!show_toolbar, |col| col.pt_0p5())
        .child(tabs.when(!show_toolbar, |tabs| tabs.border_t_0()))
        .into_any_element()
}
