//! Hunks of a file diff with syntax highlighting, visible whitespace and —
//! for working-copy diffs — hunk / line staging.

use crate::commands::repo::{self, HunkAction};
use crate::components::highlight::{Token, highlight};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{DiffLine, DiffSource, FileDiff, Hunk, LineKind, RepoId};
use std::sync::Arc;

/// Lines rendered per file at most.
pub const MAX_LINES: usize = 4000;

/// Renders text with `·` for spaces, `→` for tabs (muted) when `invisibles`.
fn tokens(toks: &[Token], invisibles: bool) -> Element {
    rsx! {
        for (i, t) in toks.iter().enumerate() {
            {
                let style = t.color.as_ref().map(|c| format!("color: {c}")).unwrap_or_default();
                if invisibles && t.text.contains([' ', '\t']) {
                    let parts: Vec<(bool, String)> = t.text.split_inclusive([' ', '\t']).flat_map(|p| {
                        let (body, ws) = p.split_at(p.trim_end_matches([' ', '\t']).len());
                        let mut v = Vec::new();
                        if !body.is_empty() { v.push((false, body.to_owned())); }
                        for c in ws.chars() { v.push((true, if c == '\t' { "→   ".to_owned() } else { "·".to_owned() })); }
                        v
                    }).collect();
                    rsx! {
                        span { key: "{i}", style: "{style}",
                            for (j, (ws, s)) in parts.into_iter().enumerate() {
                                if ws { span { key: "{j}", class: "text-muted-foreground/35", "{s}" } } else { span { key: "{j}", "{s}" } }
                            }
                        }
                    }
                } else {
                    rsx! { span { key: "{i}", style: "{style}", "{t.text}" } }
                }
            }
        }
    }
}

/// All hunks of `file`. `repo` + editable source enables staging actions.
#[component]
pub fn CodeDiff(
    file: Arc<FileDiff>,
    source: DiffSource,
    #[props(default)] repo: Option<RepoId>,
) -> Element {
    let state = use_context::<AppState>();
    let dark = state.is_dark();
    let invisibles = !state.settings.read().hide_invisibles;
    if file.binary {
        return rsx! { p { class: "px-4 py-3 font-sans text-xs text-muted-foreground", "Binary file — no text diff." } };
    }
    if file.hunks.is_empty() {
        return rsx! { p { class: "px-4 py-3 font-sans text-xs text-muted-foreground", "No textual changes (mode change, rename or empty file)." } };
    }
    let mut budget = MAX_LINES;
    let hunks: Vec<(usize, Hunk, usize)> = file
        .hunks
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let shown = h.lines.len().min(budget);
            budget -= shown;
            (i, h.clone(), shown)
        })
        .collect();
    let hidden = file
        .hunks
        .iter()
        .map(|h| h.lines.len())
        .sum::<usize>()
        .saturating_sub(MAX_LINES);
    rsx! {
        div { class: "font-mono text-[12px] leading-[19px] select-text",
            for (i, h, shown) in hunks {
                if shown > 0 {
                    HunkView { key: "{i}", path: file.path.clone(), index: i, hunk: h, shown, source: source.clone(), repo, dark, invisibles }
                }
            }
            if hidden > 0 { p { class: "px-4 py-2 font-sans text-xs text-muted-foreground", "{hidden} more lines not shown." } }
        }
    }
}

#[component]
fn HunkView(
    path: String,
    index: usize,
    hunk: Hunk,
    shown: usize,
    source: DiffSource,
    repo: Option<RepoId>,
    dark: bool,
    invisibles: bool,
) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let editable = repo.is_some() && matches!(source, DiffSource::Unstaged | DiffSource::Staged);
    let selected: Vec<usize> = repo
        .and_then(|id| state.view(id).diff)
        .map(|d| {
            d.lines
                .iter()
                .filter(|(h, _)| *h == index)
                .map(|(_, l)| *l)
                .collect()
        })
        .unwrap_or_default();
    let has_sel = !selected.is_empty();
    let busy = repo.is_some_and(|id| state.is_busy(id));
    let texts: Vec<&str> = hunk
        .lines
        .iter()
        .take(shown)
        .map(|l| l.content.as_str())
        .collect();
    let toks = highlight(&path, &texts, dark);
    let what = if has_sel { "Lines" } else { "Hunk" };
    let s = services.clone();
    let act = move |a: HunkAction| {
        if let Some(id) = repo {
            repo::hunk(state, s.clone(), id, index, a, has_sel);
        }
    };
    let (a1, a2) = (act.clone(), act);
    rsx! {
        div { class: "border-t first:border-t-0",
            div { class: "sticky top-0 z-10 flex h-8 items-center gap-2 bg-muted/95 px-4 text-[11px] text-muted-foreground backdrop-blur",
                span { class: "min-w-0 flex-1 truncate", "{hunk.header}" }
                if editable {
                    if has_sel { span { class: "font-sans", "{selected.len()} selected" } }
                    if source == DiffSource::Unstaged {
                        button { class: "rounded border bg-background px-2 py-0.5 font-sans text-destructive hover:bg-accent", disabled: busy, onclick: move |_| a1(HunkAction::Discard), "Discard {what}" }
                        button { class: "rounded border bg-background px-2 py-0.5 font-sans font-medium text-foreground hover:bg-accent", disabled: busy, onclick: move |_| a2(HunkAction::Stage), "Stage {what}" }
                    } else {
                        button { class: "rounded border bg-background px-2 py-0.5 font-sans font-medium text-foreground hover:bg-accent", disabled: busy, onclick: move |_| a2(HunkAction::Unstage), "Unstage {what}" }
                    }
                }
            }
            for (li, line) in hunk.lines.iter().take(shown).enumerate() {
                Line { key: "{li}", repo, hunk: index, index: li, line: line.clone(), toks: toks.get(li).cloned().unwrap_or_default(), selected: selected.contains(&li), selectable: editable, invisibles }
            }
        }
    }
}

#[component]
fn Line(
    repo: Option<RepoId>,
    hunk: usize,
    index: usize,
    line: DiffLine,
    toks: Vec<Token>,
    selected: bool,
    selectable: bool,
    invisibles: bool,
) -> Element {
    let state = use_context::<AppState>();
    let (tone, gutter, sign) = match line.kind {
        LineKind::Added => (
            "bg-green-500/[0.07]",
            "bg-green-500/[0.12] text-green-700 dark:text-green-300",
            "+",
        ),
        LineKind::Removed => (
            "bg-red-500/[0.07]",
            "bg-red-500/[0.12] text-red-700 dark:text-red-300",
            "-",
        ),
        LineKind::Context => ("", "text-muted-foreground/60", " "),
        LineKind::NoNewline => (
            "italic text-muted-foreground",
            "text-muted-foreground/60",
            "",
        ),
    };
    let changed = matches!(line.kind, LineKind::Added | LineKind::Removed);
    let sel = if selected {
        "outline outline-1 -outline-offset-1 outline-info bg-info-light"
    } else {
        ""
    };
    let cursor = if selectable && changed {
        "cursor-pointer"
    } else {
        ""
    };
    let old = line.old_no.map(|n| n.to_string()).unwrap_or_default();
    let new = line
        .new_no
        .map(|n| format!("{sign}{n}"))
        .unwrap_or_default();
    let is_meta = line.kind == LineKind::NoNewline;
    rsx! {
        div { class: "flex min-w-fit {tone} {sel} {cursor}",
            onclick: move |_| {
                if let Some(id) = repo.filter(|_| selectable && changed) {
                    state.update_view(id, |v| {
                        if let Some(d) = v.diff.as_mut() && !d.lines.remove(&(hunk, index)) {
                            d.lines.insert((hunk, index));
                        }
                    });
                }
            },
            span { class: "w-12 shrink-0 select-none pr-2 text-right {gutter}", "{old}" }
            span { class: "w-14 shrink-0 select-none border-r pr-2 text-right {gutter}", "{new}" }
            span { class: "whitespace-pre pl-3 pr-6",
                if is_meta { "{line.content}" } else { {tokens(&toks, invisibles)} }
                if invisibles && !is_meta { span { class: "text-muted-foreground/35", "¬" } }
            }
        }
    }
}
