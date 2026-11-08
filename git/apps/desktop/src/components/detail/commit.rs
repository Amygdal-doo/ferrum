//! Commit / stash details (Tower style): metadata, message, and the
//! changeset with expandable inline diffs — or a folder tree of changes.

use crate::app::FerrumGit;
use crate::components::actions::Action;
use crate::components::code::code_diff;
use crate::components::graph::ref_badge;
use crate::components::icons::Icon;
use crate::components::widgets::{danger_button, ghost_button, primary_button, small_button};
use crate::components::{avatar_color, change_word, plural};
use crate::state::Selection;
use ferrumgit_domain::text::initials;
use ferrumgit_domain::{CommitDetails, DiffSource, FileDiff, RepoId};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};
use std::collections::BTreeMap;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{
    Clickable, Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex,
    v_flex,
};

/// `5 November 2026 at 15:44:54 CET`.
fn long_date(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map_or_else(String::new, |t| {
            t.format("%-d %B %Y at %H:%M:%S %Z").to_string()
        })
}

fn meta_row(label: &str, value: impl IntoElement) -> AnyElement {
    h_flex()
        .gap_3()
        .child(
            div().flex_none().w(px(88.)).flex().justify_end().child(
                Label::new(label.to_owned())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            ),
        )
        .child(div().flex_1().min_w_0().child(value))
        .into_any_element()
}

pub fn commit_panel(
    app: &FerrumGit,
    id: RepoId,
    details: Arc<CommitDetails>,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let view = app.state.view(id);
    let stash = match view.selection {
        Selection::Stash(i) => Some(i),
        _ => None,
    };
    let c = details.commit.clone();
    let refs: Vec<_> = view
        .data
        .as_ref()
        .map(|d| {
            d.snapshot
                .refs
                .iter()
                .filter(|r| r.target == c.oid)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let is_head = view
        .data
        .as_ref()
        .and_then(|d| d.snapshot.status.branch.oid.clone())
        == Some(c.oid.clone());
    let (adds, dels) = details
        .files
        .iter()
        .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions));
    let n_files = details.files.len();
    let all_open = n_files > 0
        && details
            .files
            .iter()
            .all(|f| view.expanded.contains(&f.path));
    let tree = view.tree;
    let sha = c.oid.0.clone();
    let author = format!("{} <{}>", c.author_name, c.author_email);
    let committer = (details.committer_email != c.author_email)
        .then(|| format!("{} <{}>", details.committer_name, details.committer_email));
    let paths: Vec<String> = details.files.iter().map(|f| f.path.clone()).collect();

    let mut grid: Vec<AnyElement> = Vec::new();
    grid.push(meta_row(
        "Author",
        Label::new(author).size(LabelSize::Small).truncate(),
    ));
    grid.push(meta_row(
        "Author Date",
        Label::new(long_date(c.author_time)).size(LabelSize::Small),
    ));
    if let Some(cm) = committer {
        grid.push(meta_row(
            "Committer",
            Label::new(cm).size(LabelSize::Small).truncate(),
        ));
    }
    if !refs.is_empty() || is_head {
        let head_badge = (is_head && !refs.iter().any(|r| r.is_head)).then(|| {
            div()
                .px_1p5()
                .rounded_sm()
                .bg(colors.text_accent)
                .text_color(colors.background)
                .text_ui_xs(cx)
                .child("HEAD")
        });
        let badges: Vec<AnyElement> = refs
            .iter()
            .map(|r| ref_badge(r, false, &theme, cx))
            .collect();
        grid.push(meta_row(
            "Refs",
            h_flex()
                .flex_wrap()
                .gap_1()
                .children(head_badge)
                .children(badges),
        ));
    }
    if !c.parents.is_empty() {
        let parents: Vec<AnyElement> = c
            .parents
            .iter()
            .map(|p| {
                let target = p.clone();
                div()
                    .id(SharedString::from(format!("parent-{p}")))
                    .cursor_pointer()
                    .font_buffer(cx)
                    .text_ui_sm(cx)
                    .text_color(theme.status().info)
                    .hover(|s| s.underline())
                    .child(p.short().to_owned())
                    .on_click(cx.listener(move |app, _, _, cx| app.reveal_commit(id, &target, cx)))
                    .into_any_element()
            })
            .collect();
        grid.push(meta_row(
            if c.parents.len() > 1 {
                "Parents"
            } else {
                "Parent"
            },
            h_flex().gap_2().children(parents),
        ));
    }

    let segment = |label: &'static str, active: bool, on: bool, cx: &mut Context<FerrumGit>| {
        div()
            .id(label)
            .px_2p5()
            .py_0p5()
            .cursor_pointer()
            .text_ui_sm(cx)
            .when(active, |d| {
                d.bg(colors.text_accent).text_color(colors.background)
            })
            .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
            .child(label)
            .on_click(cx.listener(move |app, _, _, cx| {
                app.state.update_view(id, |v| v.tree = on);
                cx.notify();
            }))
    };

    let header = h_flex()
        .flex_none()
        .h(px(36.))
        .gap_2()
        .px_4()
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .id("copy-sha")
                .cursor_pointer()
                .font_buffer(cx)
                .text_ui_sm(cx)
                .text_color(colors.text_muted)
                .hover(|s| s.text_color(colors.text))
                .tooltip(Tooltip::text("Copy full SHA"))
                .child(c.oid.short().to_owned())
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.copy(&sha, cx);
                    app.notify("Copied SHA", false, cx);
                })),
        )
        .child(div().flex_1())
        .child(
            h_flex()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .overflow_hidden()
                .child(segment("Changeset", !tree, false, cx))
                .child(segment("Tree", tree, true, cx)),
        );

    let stash_actions = stash.map(|i| {
        h_flex()
            .gap_2()
            .px_5()
            .pb_3()
            .child(primary_button("stash-apply", "Apply").on_click(cx.listener(
                move |app, _, window, cx| {
                    app.perform(Action::StashApply(i), window, cx);
                },
            )))
            .child(small_button("stash-pop", "Pop").on_click(cx.listener(
                move |app, _, window, cx| {
                    app.perform(Action::StashPop(i), window, cx);
                },
            )))
            .child(danger_button("stash-drop", "Drop…").on_click(cx.listener(
                move |app, _, window, cx| {
                    app.perform(Action::StashDrop(i), window, cx);
                },
            )))
    });

    let summary_line = format!(
        "Showing {n_files} changed file{} with {adds} addition{} and {dels} deletion{}",
        plural(n_files),
        plural(adds as usize),
        plural(dels as usize)
    );
    let files: Vec<AnyElement> = if tree {
        vec![file_tree(id, &details.files, cx)]
    } else {
        details
            .files
            .iter()
            .map(|f| {
                let open = view.expanded.contains(&f.path);
                file_section(app, id, Arc::new(f.clone()), open, cx)
            })
            .collect()
    };

    v_flex()
        .size_full()
        .child(header)
        .child(
            v_flex()
                .id("commit-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex()
                        .items_start()
                        .gap_4()
                        .px_5()
                        .pt_4()
                        .pb_3()
                        .child(v_flex().flex_1().min_w_0().gap_1().children(grid))
                        .child(
                            div()
                                .flex_none()
                                .size(px(56.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(avatar_color(&c.author_email))
                                .text_lg()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(initials(&c.author_name)),
                        ),
                )
                .children(stash_actions)
                .child(
                    v_flex()
                        .px_5()
                        .py_3()
                        .border_t_1()
                        .border_b_1()
                        .border_color(colors.border)
                        .child(Label::new(c.summary.clone()).weight(gpui::FontWeight::MEDIUM))
                        .when(!details.body.is_empty(), |d| {
                            d.child(
                                div()
                                    .mt_2()
                                    .text_ui_sm(cx)
                                    .text_color(colors.text_muted)
                                    .child(details.body.clone()),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .h(px(36.))
                        .gap_3()
                        .px_5()
                        .when(!tree, |d| {
                            d.child(
                                ghost_button(
                                    "expand-all",
                                    if all_open {
                                        "Collapse All"
                                    } else {
                                        "Expand All"
                                    },
                                )
                                .on_click(cx.listener(
                                    move |app, _, _, cx| {
                                        app.state.update_view(id, |v| {
                                            v.expanded = if all_open {
                                                Default::default()
                                            } else {
                                                paths.iter().cloned().collect()
                                            };
                                        });
                                        cx.notify();
                                    },
                                )),
                            )
                        })
                        .child(div().flex_1())
                        .child(
                            Label::new(summary_line)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .when(details.truncated, |d| {
                    d.child(
                        div().px_5().pb_2().child(
                            Label::new("Large commit: only the first files are shown.")
                                .size(LabelSize::XSmall)
                                .color(Color::Warning),
                        ),
                    )
                })
                .children(files),
        )
        .into_any_element()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn file_section(
    app: &FerrumGit,
    id: RepoId,
    file: Arc<FileDiff>,
    open: bool,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = theme.status();
    let (word, word_color) = change_word(file.change, cx);
    let path = file.path.clone();
    let shown = match &file.old_path {
        Some(old) => format!("{old} → {}", file.path),
        None => file.path.clone(),
    };
    let mode = file
        .header
        .iter()
        .find(|l| l.starts_with("new file mode") || l.starts_with("deleted file mode"))
        .cloned();
    let chunks = file.hunks.len();
    v_flex()
        .border_t_1()
        .border_color(colors.border)
        .child(
            h_flex()
                .id(SharedString::from(format!("file-section-{}", file.path)))
                .h(px(36.))
                .gap_2()
                .px_3()
                .text_ui_sm(cx)
                .hover(|s| s.bg(colors.element_hover))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.state.update_view(id, |v| {
                        if !v.expanded.remove(&path) {
                            v.expanded.insert(path.clone());
                        }
                    });
                    cx.notify();
                }))
                .child(
                    if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    }
                    .icon()
                    .size(IconSize::Small)
                    .color(Color::Muted),
                )
                .child(
                    div()
                        .w(px(64.))
                        .flex()
                        .justify_end()
                        .text_color(word_color)
                        .child(word),
                )
                .child(
                    div()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(word_color)
                        .text_color(colors.background)
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(file.change.letter().to_string()),
                )
                .child(div().flex_1().min_w_0().truncate().child(shown))
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.created)
                        .child(format!("+{}", file.additions)),
                )
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.deleted)
                        .child(format!("−{}", file.deletions)),
                ),
        )
        .when(open, |d| {
            d.child(
                v_flex()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(
                        v_flex()
                            .px_8()
                            .py_2()
                            .font_buffer(cx)
                            .text_ui_xs(cx)
                            .text_color(colors.text_muted)
                            .children(mode.map(|m| capitalize(&m)))
                            .child(format!(
                                "{chunks} chunk{}, {} insertion{}, {} deletion{}",
                                plural(chunks),
                                file.additions,
                                plural(file.additions as usize),
                                file.deletions,
                                plural(file.deletions as usize)
                            )),
                    )
                    .child(code_diff(app, file.clone(), DiffSource::Commit, None, cx)),
            )
        })
        .into_any_element()
}

/// Changed files grouped by directory; clicking a file opens its diff in the changeset.
fn file_tree(id: RepoId, files: &[FileDiff], cx: &mut Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut dirs: BTreeMap<String, Vec<FileDiff>> = BTreeMap::new();
    for f in files {
        let dir = f
            .path
            .rsplit_once('/')
            .map_or_else(String::new, |(d, _)| d.to_owned());
        dirs.entry(dir).or_default().push(f.clone());
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    for (dir, fs) in dirs {
        rows.push(
            h_flex()
                .h(px(28.))
                .gap_2()
                .px_4()
                .child(
                    Icon::Folder
                        .icon()
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(if dir.is_empty() { "/".to_owned() } else { dir })
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                )
                .into_any_element(),
        );
        for f in fs {
            let (word, word_color) = change_word(f.change, cx);
            let name = f.path.rsplit('/').next().unwrap_or(&f.path).to_owned();
            let path = f.path.clone();
            rows.push(
                h_flex()
                    .id(SharedString::from(format!("tree-{}", f.path)))
                    .h(px(28.))
                    .gap_2()
                    .pr_4()
                    .pl_10()
                    .text_ui_sm(cx)
                    .hover(|s| s.bg(colors.element_hover))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.state.update_view(id, |v| {
                            v.tree = false;
                            v.expanded.insert(path.clone());
                        });
                        cx.notify();
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .child(div().text_ui_xs(cx).text_color(word_color).child(word))
                    .into_any_element(),
            );
        }
    }
    v_flex()
        .border_t_1()
        .border_color(colors.border)
        .py_1()
        .children(rows)
        .into_any_element()
}
