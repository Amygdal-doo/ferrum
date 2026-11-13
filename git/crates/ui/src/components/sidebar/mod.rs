//! Left sidebar (Tower layout): Workspace pages, branches grouped into
//! folders by `/`, tags, remotes; filter and "new branch" at the bottom.

mod tree;

use crate::app::GitView;
use crate::components::actions::Action;
use crate::components::icons::Icon;
use crate::state::{Dialog, Page, PromptKind};
use ferrum_ui::components::{
    Color, IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{GitRef, RefKind};
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, SharedString, Window,
    div, prelude::*, px,
};
use tree::{build, render_tree};

pub(crate) fn toggle(app: &mut GitView, key: &str, cx: &mut Context<GitView>) {
    if !app.collapsed.remove(key) {
        app.collapsed.insert(key.to_owned());
    }
    cx.notify();
}

fn section_title(
    title: &'static str,
    count: Option<usize>,
    action: Option<AnyElement>,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let key = format!("section:{title}");
    h_flex()
        .group("section")
        .mt_3()
        .h(px(24.))
        .px_2()
        .child(
            h_flex()
                .id(ElementId::Name(format!("sec-{title}").into()))
                .flex_1()
                .gap_1p5()
                .cursor_pointer()
                .text_color(colors.text_muted)
                .hover(|s| s.text_color(colors.text))
                .child(
                    Label::new(title)
                        .size(LabelSize::Small)
                        .weight(gpui::FontWeight::SEMIBOLD)
                        .color(Color::Muted),
                )
                .children(count.filter(|n| *n > 0).map(|n| {
                    Label::new(n.to_string())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                }))
                .on_click(cx.listener(move |app, _, _, cx| toggle(app, &key, cx))),
        )
        .children(action)
        .into_any_element()
}

fn nav_item(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    active: bool,
    badge: Option<String>,
    on_click: impl Fn(&mut GitView, &ClickEvent, &mut Window, &mut Context<GitView>) + 'static,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    h_flex()
        .id(id)
        .h(px(28.))
        .w_full()
        .gap_2p5()
        .px_2p5()
        .rounded_md()
        .cursor_pointer()
        .when(active, |d| d.bg(colors.element_selected))
        .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
        .child(icon.icon().size(IconSize::Small).color(if active {
            Color::Accent
        } else {
            Color::Default
        }))
        .child(
            div().flex_1().min_w_0().child(
                Label::new(label)
                    .color(if active {
                        Color::Accent
                    } else {
                        Color::Default
                    })
                    .truncate(),
            ),
        )
        .children(badge.map(|b| {
            div()
                .px_1p5()
                .rounded_full()
                .bg(colors.element_active)
                .child(Label::new(b).size(LabelSize::XSmall))
        }))
        .on_click(cx.listener(on_click))
        .into_any_element()
}

fn remote_node(
    app: &GitView,
    name: String,
    branches: Vec<(String, GitRef)>,
    all_refs: &[GitRef],
    cx: &mut Context<GitView>,
) -> Vec<AnyElement> {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let key = format!("remote:{name}");
    let open = !app.collapsed.contains(&key);
    let menu_name = name.clone();
    let mut out = vec![
        h_flex()
            .id(SharedString::from(format!("remote-{name}")))
            .h(px(28.))
            .gap_1p5()
            .px_1()
            .rounded_md()
            .hover(|s| s.bg(colors.element_hover))
            .child(
                div().w(px(14.)).child(
                    if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    }
                    .icon()
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
                ),
            )
            .child(Icon::Cloud.icon().size(IconSize::Small).color(Color::Muted))
            .child(Label::new(name.clone()).truncate())
            .on_click(cx.listener(move |app, _, _, cx| toggle(app, &key, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                    app.open_menu(
                        event.position,
                        vec![
                            Action::FetchRemote(menu_name.clone()),
                            Action::RemoveRemote(menu_name.clone()),
                        ],
                        cx,
                    );
                }),
            )
            .into_any_element(),
    ];
    if open {
        out.extend(render_tree(
            app,
            build(branches),
            &format!("remote:{name}"),
            1,
            all_refs,
            cx,
        ));
    }
    out
}

pub fn render(app: &GitView, _: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let view = app.state.view(id);
    let page = view.page;
    let snap = view
        .data
        .as_ref()
        .map(|d| d.snapshot.clone())
        .unwrap_or_default();
    let needle = app.inputs.ref_filter.read(cx).text().to_lowercase();
    let keep = |r: &&GitRef| needle.is_empty() || r.name.to_lowercase().contains(&needle);
    let local: Vec<(String, GitRef)> = snap
        .refs
        .iter()
        .filter(|r| r.is(RefKind::LocalBranch))
        .filter(keep)
        .map(|r| (r.name.clone(), r.clone()))
        .collect();
    let tags: Vec<(String, GitRef)> = snap
        .refs
        .iter()
        .filter(|r| r.is(RefKind::Tag))
        .filter(keep)
        .map(|r| (r.name.clone(), r.clone()))
        .collect();
    let remotes: Vec<(String, Vec<(String, GitRef)>)> = snap
        .remotes
        .iter()
        .map(|rm| {
            let prefix = format!("{}/", rm.name);
            let branches = snap
                .refs
                .iter()
                .filter(|r| r.is(RefKind::RemoteBranch))
                .filter(keep)
                .filter_map(|r| {
                    r.name
                        .strip_prefix(&prefix)
                        .map(|n| (n.to_owned(), r.clone()))
                })
                .collect();
            (rm.name.clone(), branches)
        })
        .collect();
    let all_refs = snap.refs.clone();
    let changes = snap.status.files.len();
    let stashes = snap.stashes.len();
    let tag_count = tags.len();

    let mut items: Vec<AnyElement> = Vec::new();
    items.push(section_title("Workspace", None, None, cx));
    if !app.collapsed.contains("section:Workspace") {
        items.push(nav_item(
            "nav-wip",
            Icon::FolderOpen,
            "Working Copy",
            page == Page::WorkingCopy,
            (changes > 0).then(|| changes.to_string()),
            move |app, _, _, cx| app.set_page(id, Page::WorkingCopy, cx),
            cx,
        ));
        items.push(nav_item(
            "nav-history",
            Icon::History,
            "History",
            page == Page::History,
            None,
            move |app, _, _, cx| app.set_page(id, Page::History, cx),
            cx,
        ));
        items.push(nav_item(
            "nav-stashes",
            Icon::Layers,
            "Stashes",
            page == Page::Stashes,
            (stashes > 0).then(|| stashes.to_string()),
            move |app, _, _, cx| app.set_page(id, Page::Stashes, cx),
            cx,
        ));
        let open_prs = view
            .prs
            .list
            .ready()
            .map(|l| l.prs.len())
            .filter(|n| *n > 0)
            .map(|n| n.to_string());
        let prs_label = match view.prs.info.ready().map(|i| i.repo.kind) {
            Some(ferrumgit_domain::ForgeKind::GitLab) => "Merge Requests",
            _ => "Pull Requests",
        };
        items.push(nav_item(
            "nav-prs",
            Icon::Merge,
            prs_label,
            page == Page::PullRequests,
            open_prs,
            move |app, _, _, cx| app.open_pull_requests(id, cx),
            cx,
        ));
        items.push(nav_item(
            "nav-settings",
            Icon::Sliders,
            "Settings",
            false,
            None,
            move |app, _, window, cx| app.open_dialog(Dialog::RepoSettings(id), window, cx),
            cx,
        ));
    }
    items.push(section_title("Branches", None, None, cx));
    if !app.collapsed.contains("section:Branches") {
        items.extend(render_tree(app, build(local), "local", 0, &all_refs, cx));
    }
    items.push(section_title("Tags", Some(tag_count), None, cx));
    if !app.collapsed.contains("section:Tags") {
        items.extend(render_tree(app, build(tags), "tags", 0, &all_refs, cx));
    }
    let add_remote = div()
        .id("add-remote")
        .px_1()
        .rounded_sm()
        .invisible()
        .group_hover("section", |s| s.visible())
        .hover(|s| s.bg(colors.element_hover))
        .tooltip(Tooltip::text("Add remote"))
        .child(Icon::Plus.icon().size(IconSize::Small))
        .on_click(cx.listener(|app, _, window, cx| {
            cx.stop_propagation();
            app.open_dialog(Dialog::AddRemote, window, cx);
        }))
        .into_any_element();
    items.push(section_title("Remotes", None, Some(add_remote), cx));
    if !app.collapsed.contains("section:Remotes") {
        for (name, branches) in remotes {
            items.extend(remote_node(app, name, branches, &all_refs, cx));
        }
    }

    v_flex()
        .size_full()
        .bg(colors.panel_background)
        .child(
            v_flex()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .pb_3()
                .children(items),
        )
        .child(
            h_flex()
                .flex_none()
                .h(px(40.))
                .gap_1()
                .px_2()
                .border_t_1()
                .border_color(colors.border)
                .child(
                    div().ml_1().child(
                        Icon::Search
                            .icon()
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(app.inputs.ref_filter.clone()),
                )
                .child(
                    div()
                        .id("new-branch")
                        .p_1p5()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .tooltip(Tooltip::text("New branch at HEAD"))
                        .child(Icon::Plus.icon().size(IconSize::Small).color(Color::Muted))
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_dialog(
                                Dialog::Prompt {
                                    kind: PromptKind::CreateBranch { start: None },
                                    initial: String::new(),
                                },
                                window,
                                cx,
                            );
                        })),
                ),
        )
        .into_any_element()
}
