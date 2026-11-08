//! Refs grouped into folders by `/` (`feature/a`, `feature/b` → feature ▸ a, b).

use super::toggle;
use crate::app::FerrumGit;
use crate::components::actions::{self, Action};
use crate::components::icons::Icon;
use ferrumgit_domain::{GitRef, RefKind};
use gpui::{
    AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent, SharedString, div, prelude::*, px,
};
use std::collections::BTreeMap;
use theme::ActiveTheme;
use ui::{Color, IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex};

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Folder {
        name: String,
        path: String,
        children: Vec<Node>,
    },
    Leaf {
        label: String,
        r: GitRef,
    },
}

/// Builds a folder tree from `(display path, ref)` pairs; folders first, then leaves, by name.
pub fn build(items: Vec<(String, GitRef)>) -> Vec<Node> {
    fn insert(
        dirs: &mut BTreeMap<String, Vec<(String, GitRef)>>,
        leaves: &mut Vec<(String, GitRef)>,
        item: (String, GitRef),
    ) {
        match item.0.split_once('/') {
            Some((dir, rest)) => dirs
                .entry(dir.to_owned())
                .or_default()
                .push((rest.to_owned(), item.1)),
            None => leaves.push(item),
        }
    }
    fn go(items: Vec<(String, GitRef)>, prefix: &str) -> Vec<Node> {
        let mut dirs = BTreeMap::new();
        let mut leaves = Vec::new();
        for it in items {
            insert(&mut dirs, &mut leaves, it);
        }
        let mut out: Vec<Node> = dirs
            .into_iter()
            .map(|(name, children)| {
                let path = format!("{prefix}{name}/");
                Node::Folder {
                    children: go(children, &path),
                    name,
                    path,
                }
            })
            .collect();
        leaves.sort_by_key(|(l, _)| l.to_lowercase());
        out.extend(leaves.into_iter().map(|(label, r)| Node::Leaf { label, r }));
        out
    }
    go(items, "")
}

pub fn render_tree(
    app: &FerrumGit,
    nodes: Vec<Node>,
    scope: &str,
    depth: usize,
    all_refs: &[GitRef],
    cx: &mut Context<FerrumGit>,
) -> Vec<AnyElement> {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut out = Vec::new();
    for n in nodes {
        match n {
            Node::Folder {
                name,
                path,
                children,
            } => {
                let key = format!("{scope}:{path}");
                let open = !app.collapsed.contains(&key);
                let pad = px(4.0 + depth as f32 * 14.0);
                out.push(
                    h_flex()
                        .id(SharedString::from(format!("folder-{key}")))
                        .h(px(28.))
                        .gap_1p5()
                        .pl(pad)
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
                        .child(
                            Icon::Folder
                                .icon()
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .child(Label::new(name).truncate())
                        .on_click(cx.listener(move |app, _, _, cx| toggle(app, &key, cx)))
                        .into_any_element(),
                );
                if open {
                    out.extend(render_tree(app, children, scope, depth + 1, all_refs, cx));
                }
            }
            Node::Leaf { label, r } => {
                let menu_items = actions::for_ref(&r, all_refs);
                out.push(ref_row(label, r, depth, menu_items, cx));
            }
        }
    }
    out
}

fn tag(text: impl Into<SharedString>, color: Color, border: gpui::Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(border)
        .child(Label::new(text.into()).size(LabelSize::XSmall).color(color))
}

fn ref_row(
    label: String,
    r: GitRef,
    depth: usize,
    menu_items: Vec<Action>,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let target = r.target.clone();
    let double_click = match r.kind {
        Some(RefKind::LocalBranch) if !r.is_head => Some(Action::Checkout(r.name.clone())),
        Some(RefKind::RemoteBranch) => Some(Action::CheckoutRemote(r.name.clone())),
        _ => None,
    };
    let icon = if r.is(RefKind::Tag) {
        Icon::Tag
    } else {
        Icon::Branch
    };
    // Leaves line up with folder names (chevron width + gap).
    let pad = px(4.0 + depth as f32 * 14.0 + 20.0);
    let title = match (&r.upstream, r.upstream_gone) {
        (Some(u), true) => format!("{} — upstream {u} is gone", r.name),
        (Some(u), false) => format!("{} → {u}", r.name),
        _ => r.name.clone(),
    };
    let track = match (r.ahead, r.behind) {
        (0, 0) => None,
        (a, 0) => Some(format!("↑{a}")),
        (0, b) => Some(format!("↓{b}")),
        (a, b) => Some(format!("↑{a} | ↓{b}")),
    };
    let label = Label::new(label)
        .truncate()
        .color(if r.merged {
            Color::Muted
        } else {
            Color::Default
        })
        .when(r.is_head, |l| l.weight(gpui::FontWeight::SEMIBOLD));
    h_flex()
        .id(SharedString::from(format!("ref-{}", r.full_name)))
        .h(px(28.))
        .gap_1p5()
        .pl(pad)
        .pr_1p5()
        .rounded_md()
        .hover(|s| s.bg(colors.element_hover))
        .tooltip(Tooltip::text(title))
        .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
            if event.click_count() >= 2 {
                if let Some(action) = double_click.clone() {
                    app.perform(action, window, cx);
                }
            } else {
                app.reveal(&target, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                app.open_menu(event.position, menu_items.clone(), cx);
            }),
        )
        .child(icon.icon().size(IconSize::Small).color(Color::Muted))
        .child(div().flex_1().min_w_0().child(label))
        .when(r.upstream_gone, |d| {
            d.child(tag("gone", Color::Warning, colors.border))
        })
        .when(r.merged, |d| {
            d.child(tag("✓ Merged", Color::Muted, colors.border))
        })
        .when(r.is_head, |d| {
            d.child(tag("HEAD", Color::Default, colors.border))
        })
        .children(track.map(|t| tag(t, Color::Muted, colors.border)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str) -> (String, GitRef) {
        (
            name.to_owned(),
            GitRef {
                name: name.to_owned(),
                full_name: format!("refs/heads/{name}"),
                ..GitRef::default()
            },
        )
    }

    #[test]
    fn groups_by_slash_folders_first() {
        let nodes = build(vec![
            r("main"),
            r("feature/b"),
            r("feature/a"),
            r("fix/deep/x"),
            r("Staging"),
        ]);
        let names: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } => format!("{name}/"),
                Node::Leaf { label, .. } => label.clone(),
            })
            .collect();
        assert_eq!(names, ["feature/", "fix/", "main", "Staging"]);
        let Node::Folder { children, path, .. } = &nodes[1] else {
            panic!()
        };
        assert_eq!(path, "fix/");
        assert!(matches!(&children[0], Node::Folder { path, .. } if path == "fix/deep/"));
        let Node::Folder { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(&children[0], Node::Leaf { label, .. } if label == "a"));
    }
}
