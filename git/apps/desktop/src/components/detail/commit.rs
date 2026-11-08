//! Commit / stash details (Tower style): metadata, message, and the
//! changeset with expandable inline diffs — or a folder tree of changes.

use crate::commands::{self, repo};
use crate::components::actions::{self, Action};
use crate::components::code::CodeDiff;
use crate::components::graph::RefBadge;
use crate::components::icons::{Icon, Svg};
use crate::components::{avatar_class, change_word};
use crate::state::{AppState, Selection};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::text::initials;
use ferrumgit_domain::{CommitDetails, DiffSource, FileDiff, RepoId};
use std::collections::BTreeMap;
use std::sync::Arc;

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

#[component]
pub fn CommitPanel(id: RepoId, details: Arc<CommitDetails>) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let view = state.view(id);
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
    let short = c.oid.short().to_owned();
    let author = format!("{} <{}>", c.author_name, c.author_email);
    let committer = (details.committer_email != c.author_email)
        .then(|| format!("{} <{}>", details.committer_name, details.committer_email));
    let date = long_date(c.author_time);
    let avatar = initials(&c.author_name);
    let avatar_bg = avatar_class(&c.author_email);
    let paths: Vec<String> = details.files.iter().map(|f| f.path.clone()).collect();
    let svc = services.clone();
    let act = move |a: Action| actions::perform(state, svc.clone(), a);
    let (a1, a2, a3) = (act.clone(), act.clone(), act);

    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-2 border-b px-4",
            button { class: "font-mono text-[12.5px] text-muted-foreground hover:text-foreground", title: "Copy full SHA",
                onclick: move |_| { commands::copy(&sha); state.notify("Copied SHA", false); }, "{short}" }
            div { class: "flex-1" }
            div { class: "flex overflow-hidden rounded-md border text-[12px]",
                button { class: if tree { "px-2.5 py-0.5 hover:bg-accent" } else { "bg-primary-accent px-2.5 py-0.5 text-white" },
                    onclick: move |_| state.update_view(id, |v| v.tree = false), "Changeset" }
                button { class: if tree { "bg-primary-accent px-2.5 py-0.5 text-white" } else { "px-2.5 py-0.5 hover:bg-accent" },
                    onclick: move |_| state.update_view(id, |v| v.tree = true), "Tree" }
            }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto",
            div { class: "flex gap-4 px-5 pt-4 pb-3 select-text",
                div { class: "grid min-w-0 flex-1 grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1 text-[12.5px]",
                    span { class: "text-right text-muted-foreground", "Author" }
                    span { class: "truncate", "{author}" }
                    span { class: "text-right text-muted-foreground", "Author Date" }
                    span { "{date}" }
                    if let Some(cm) = committer {
                        span { class: "text-right text-muted-foreground", "Committer" }
                        span { class: "truncate", "{cm}" }
                    }
                    if !refs.is_empty() || is_head {
                        span { class: "text-right text-muted-foreground", "Refs" }
                        span { class: "flex flex-wrap gap-1",
                            if is_head && !refs.iter().any(|r| r.is_head) { span { class: "rounded bg-primary-accent px-1.5 py-px text-[10.5px] font-medium text-white", "HEAD" } }
                            for r in refs { RefBadge { key: "{r.full_name}", r: r.clone() } }
                        }
                    }
                    if !c.parents.is_empty() {
                        span { class: "text-right text-muted-foreground", if c.parents.len() > 1 { "Parents" } else { "Parent" } }
                        span { class: "flex gap-2",
                            for p in c.parents.clone() {
                                {
                                    let (p2, s, ps) = (p.clone(), services.clone(), p.short().to_owned());
                                    rsx! { button { key: "{p}", class: "font-mono text-[12px] text-info hover:underline",
                                        onclick: move |_| repo::reveal_commit(state, s.clone(), id, &p2), "{ps}" } }
                                }
                            }
                        }
                    }
                }
                span { class: "flex size-14 shrink-0 items-center justify-center rounded-full text-lg font-semibold {avatar_bg}", "{avatar}" }
            }
            if let Some(i) = stash {
                div { class: "flex gap-2 px-5 pb-3",
                    button { class: "rounded-md bg-primary-accent px-3 py-1 text-xs font-medium text-white", onclick: move |_| a1(Action::StashApply(i)), "Apply" }
                    button { class: "rounded-md border px-3 py-1 text-xs hover:bg-accent", onclick: move |_| a2(Action::StashPop(i)), "Pop" }
                    button { class: "rounded-md px-3 py-1 text-xs text-destructive hover:bg-accent", onclick: move |_| a3(Action::StashDrop(i)), "Drop…" }
                }
            }
            div { class: "border-y px-5 py-3 select-text",
                p { class: "text-[13.5px] font-medium", "{c.summary}" }
                if !details.body.is_empty() { pre { class: "mt-2 whitespace-pre-wrap font-sans text-[12.5px] text-muted-foreground", "{details.body}" } }
            }
            div { class: "flex h-9 items-center gap-3 px-5 text-[12px] text-muted-foreground",
                if !tree {
                    button { class: "hover:text-foreground",
                        onclick: move |_| state.update_view(id, |v| v.expanded = if all_open { Default::default() } else { paths.iter().cloned().collect() }),
                        if all_open { "Collapse All" } else { "Expand All" } }
                }
                div { class: "flex-1" }
                span { "Showing {n_files} changed file{plural(n_files)} with {adds} addition{plural(adds as usize)} and {dels} deletion{plural(dels as usize)}" }
            }
            if details.truncated { p { class: "px-5 pb-2 text-[11px] text-warning", "Large commit: only the first files are shown." } }
            if tree {
                FileTree { id, files: details.files.clone() }
            } else {
                for f in details.files.iter() {
                    FileSection { key: "{f.path}", id, file: Arc::new(f.clone()), open: view.expanded.contains(&f.path) }
                }
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[component]
fn FileSection(id: RepoId, file: Arc<FileDiff>, open: bool) -> Element {
    let state = use_context::<AppState>();
    let (word, word_cls, badge_cls) = change_word(file.change);
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
    rsx! {
        div { class: "border-t",
            div { class: "flex h-9 cursor-default items-center gap-2 px-3 text-[12.5px] hover:bg-accent/50",
                onclick: move |_| state.update_view(id, |v| if !v.expanded.remove(&path) { v.expanded.insert(path.clone()); }),
                span { class: "text-muted-foreground", if open { Svg { icon: Icon::ChevronDown, class: "size-3.5" } } else { Svg { icon: Icon::ChevronRight, class: "size-3.5" } } }
                span { class: "w-16 text-right text-[12px] {word_cls}", "{word}" }
                span { class: "flex size-4 items-center justify-center rounded-[3px] font-mono text-[10px] font-bold {badge_cls}", "{file.change.letter()}" }
                span { class: "min-w-0 flex-1 truncate select-text", title: "{shown}", "{shown}" }
                span { class: "font-mono text-[11px] text-green-600 dark:text-green-400", "+{file.additions}" }
                span { class: "font-mono text-[11px] text-red-600 dark:text-red-400", "−{file.deletions}" }
            }
            if open {
                div { class: "border-t bg-background",
                    div { class: "px-8 py-2 font-mono text-[11.5px] leading-5 text-muted-foreground",
                        if let Some(m) = mode { p { "{capitalize(&m)}" } }
                        p { "{chunks} chunk{plural(chunks)}, {file.additions} insertion{plural(file.additions as usize)}, {file.deletions} deletion{plural(file.deletions as usize)}" }
                    }
                    CodeDiff { file: file.clone(), source: DiffSource::Commit }
                }
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Changed files grouped by directory; clicking a file opens its diff in the changeset.
#[component]
fn FileTree(id: RepoId, files: Vec<FileDiff>) -> Element {
    let state = use_context::<AppState>();
    let mut dirs: BTreeMap<String, Vec<FileDiff>> = BTreeMap::new();
    for f in files {
        let dir = f
            .path
            .rsplit_once('/')
            .map_or_else(String::new, |(d, _)| d.to_owned());
        dirs.entry(dir).or_default().push(f);
    }
    rsx! {
        div { class: "border-t py-1 text-[12.5px]",
            for (dir, fs) in dirs {
                div { key: "{dir}",
                    div { class: "flex h-7 items-center gap-2 px-4 text-muted-foreground",
                        Svg { icon: Icon::Folder, class: "size-3.5" }
                        span { class: "truncate", if dir.is_empty() { "/" } else { "{dir}" } }
                    }
                    for f in fs {
                        {
                            let (word, word_cls, _) = change_word(f.change);
                            let name = f.path.rsplit('/').next().unwrap_or(&f.path).to_owned();
                            let path = f.path.clone();
                            rsx! {
                                div { key: "{f.path}", class: "flex h-7 cursor-default items-center gap-2 pr-4 pl-10 hover:bg-accent/50",
                                    onclick: move |_| state.update_view(id, |v| { v.tree = false; v.expanded.insert(path.clone()); }),
                                    span { class: "flex-1 truncate", "{name}" }
                                    span { class: "text-[11px] {word_cls}", "{word}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
