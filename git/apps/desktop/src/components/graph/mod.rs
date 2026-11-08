//! History: commits grouped by month, with graph rails, virtualized with a GPUI `list`.

mod lanes;
mod row;

pub use row::ref_badge;

use crate::app::FerrumGit;
use crate::components::icons::Icon;
use crate::components::widgets::small_button;
use crate::state::Selection;
use ferrumgit_application::RepoData;
use ferrumgit_domain::{Commit, GitRef, RepoId};
use gpui::{AnyElement, Context, Window, div, list, prelude::*, px};
use row::{CommitRowProps, commit_row, month_row};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{Clickable, Color, IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex, v_flex};

pub const ROW_H: f32 = 50.0;
pub const MONTH_H: f32 = 34.0;
pub const LANE_W: f32 = 11.0;
const MAX_LANES: usize = 16;

/// What the history list was last built from; a change resets the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryKey {
    repo: RepoId,
    data: usize,
    search: String,
}

enum Item {
    Month(String),
    Commit(usize),
    More,
}

struct History {
    repo: RepoId,
    items: Vec<Item>,
    data: Arc<RepoData>,
    by_oid: HashMap<String, Vec<GitRef>>,
    selection: Selection,
    head: Option<ferrumgit_domain::Oid>,
    lanes: usize,
    graph_px: f32,
    searching: bool,
}

fn month(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map_or_else(String::new, |t| {
            t.format("%B %Y").to_string().to_uppercase()
        })
}

fn matches(c: &Commit, needle: &str) -> bool {
    c.summary.to_lowercase().contains(needle)
        || c.author_name.to_lowercase().contains(needle)
        || c.oid.as_str().starts_with(needle)
}

pub fn commit_list(app: &mut FerrumGit, _: &mut Window, cx: &mut Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let view = app.state.view(id);
    let Some(data) = view.data.clone() else {
        return v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .map(|d| match view.error.clone() {
                Some(e) => d.child(Label::new(e).color(Color::Error)).child(
                    small_button("history-retry", "Retry")
                        .on_click(cx.listener(move |app, _, _, cx| app.load(id, cx))),
                ),
                None => d.child(Label::new("Loading repository…").color(Color::Muted)),
            })
            .into_any_element();
    };
    let snap = &data.snapshot;
    let needle = view.search.trim().to_lowercase();
    let searching = !needle.is_empty();
    let mut by_oid: HashMap<String, Vec<GitRef>> = HashMap::new();
    for r in &snap.refs {
        by_oid
            .entry(r.target.as_str().to_owned())
            .or_default()
            .push(r.clone());
    }
    let mut items = Vec::new();
    let mut last_month = String::new();
    let mut matched = 0;
    for (i, c) in snap.commits.iter().enumerate() {
        if searching && !matches(c, &needle) {
            continue;
        }
        matched += 1;
        let m = month(c.author_time);
        if m != last_month {
            items.push(Item::Month(m.clone()));
            last_month = m;
        }
        items.push(Item::Commit(i));
    }
    if snap.has_more && !searching {
        items.push(Item::More);
    }
    let lanes = if searching {
        0
    } else {
        data.graph
            .iter()
            .map(|r| r.width)
            .max()
            .unwrap_or(1)
            .min(MAX_LANES)
    };
    let graph_px = if searching {
        0.0
    } else {
        lanes as f32 * LANE_W + 12.0
    };
    let count = if searching {
        format!("{matched} of {}", snap.commits.len())
    } else {
        snap.commits.len().to_string()
    };

    let key = HistoryKey {
        repo: id,
        data: Arc::as_ptr(&data) as usize,
        search: needle.clone(),
    };
    if app.history_key.as_ref() != Some(&key) || app.history.item_count() != items.len() {
        app.history.reset(items.len());
        app.history_key = Some(key);
    }
    if let Some(row) = view.scroll_to {
        app.state.update_view(id, |v| v.scroll_to = None);
        if let Some(ix) = items
            .iter()
            .position(|it| matches!(it, Item::Commit(n) if *n == row))
        {
            app.history.scroll_to_reveal_item(ix);
        }
    }

    let history = Rc::new(History {
        repo: id,
        items,
        head: snap.status.branch.oid.clone(),
        data: data.clone(),
        by_oid,
        selection: view.selection.clone(),
        lanes,
        graph_px,
        searching,
    });
    let weak = cx.entity().downgrade();
    let item_theme = theme.clone();
    let hide_invisibles = app.state.settings.hide_invisibles;

    v_flex()
        .size_full()
        .child(
            h_flex()
                .flex_none()
                .h(px(36.))
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(colors.border)
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(if searching {
                            "Search results"
                        } else {
                            "All Branches, Remotes, Tags"
                        })
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                    ),
                )
                .child(
                    Label::new(count)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    div()
                        .id("toggle-invisibles")
                        .p_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .tooltip(Tooltip::text(if hide_invisibles {
                            "Show whitespace in diffs"
                        } else {
                            "Hide whitespace in diffs"
                        }))
                        .child(
                            Icon::Sliders
                                .icon()
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.state.settings.hide_invisibles =
                                !app.state.settings.hide_invisibles;
                            app.save_settings(cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
            list(app.history.clone(), move |ix, _, cx| {
                render_item(&history, ix, weak.clone(), item_theme.clone(), cx)
            })
            .flex_1()
            .min_h_0()
            .pt_1p5(),
        )
        .into_any_element()
}

fn render_item(
    history: &History,
    ix: usize,
    view: gpui::WeakEntity<FerrumGit>,
    theme: Arc<theme::Theme>,
    cx: &mut gpui::App,
) -> AnyElement {
    let Some(item) = history.items.get(ix) else {
        return div().into_any_element();
    };
    match item {
        Item::Month(label) => {
            let rails = match history.items.get(ix + 1) {
                Some(Item::Commit(n)) if !history.searching => history
                    .data
                    .graph
                    .get(*n)
                    .map(|g| {
                        g.up.iter()
                            .filter(|e| e.from < history.lanes)
                            .map(|e| (e.from, e.color))
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            month_row(label.clone(), history.graph_px, rails, &theme, cx)
        }
        Item::Commit(n) => {
            let Some(commit) = history.data.snapshot.commits.get(*n).cloned() else {
                return div().into_any_element();
            };
            let refs = history
                .by_oid
                .get(commit.oid.as_str())
                .cloned()
                .unwrap_or_default();
            let props = CommitRowProps {
                repo: history.repo,
                is_head: history.head.as_ref() == Some(&commit.oid),
                selected: history.selection == Selection::Commit(commit.oid.clone()),
                graph: (!history.searching)
                    .then(|| history.data.graph.get(*n).cloned())
                    .flatten(),
                commit,
                refs,
                graph_px: history.graph_px,
                lanes: history.lanes,
            };
            commit_row(props, view, theme, cx)
        }
        Item::More => {
            let repo = history.repo;
            div()
                .w_full()
                .flex()
                .justify_center()
                .py_3()
                .child(
                    small_button("load-more", "Load 2000 more commits").on_click(
                        move |_, _, cx| {
                            if let Err(error) = view.update(cx, |app, cx| {
                                let limit = app.state.log_limit() + 2000;
                                app.state.settings.log_limit =
                                    u32::try_from(limit).unwrap_or(u32::MAX);
                                app.save_settings(cx);
                                app.load(repo, cx);
                            }) {
                                tracing::debug!(error = %error, "view closed");
                            }
                        },
                    ),
                )
                .into_any_element()
        }
    }
}
