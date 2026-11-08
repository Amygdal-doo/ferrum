//! Working-copy diff (right panel): file header with file-level actions and
//! the hunks with stage / unstage / discard per hunk or selected lines.

use crate::commands::repo;
use crate::components::change_word;
use crate::components::code::CodeDiff;
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{DiffSource, RepoOp};

#[component]
pub fn DiffPane() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let Some(d) = state.view(id).diff else {
        return rsx! {};
    };
    let file = d.file.clone();
    let source = d.source.clone();
    let busy = state.is_busy(id);
    let (word, word_cls, badge_cls) = change_word(file.change);
    let label = match source {
        DiffSource::Unstaged => "Unstaged",
        DiffSource::Staged => "Staged",
        DiffSource::Untracked => "Untracked",
        DiffSource::Commit => "Commit",
    };
    let path = file.path.clone();
    let p_discard = path.clone();
    let s = services.clone();
    let run = move |op: RepoOp| repo::op(state, s.clone(), id, op);
    let (r1, r2) = (run.clone(), run);
    let shown = match &file.old_path {
        Some(old) => format!("{old} → {}", file.path),
        None => file.path.clone(),
    };
    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-2 border-b px-4 text-[12.5px]",
            span { class: "{word_cls}", "{word}" }
            span { class: "flex size-4 items-center justify-center rounded-[3px] font-mono text-[10px] font-bold {badge_cls}", "{file.change.letter()}" }
            span { class: "min-w-0 truncate font-medium select-text", title: "{shown}", "{shown}" }
            span { class: "rounded bg-muted px-1.5 text-[10.5px] text-muted-foreground", "{label}" }
            span { class: "font-mono text-[11px] text-green-600 dark:text-green-400", "+{file.additions}" }
            span { class: "font-mono text-[11px] text-red-600 dark:text-red-400", "−{file.deletions}" }
            div { class: "flex-1" }
            match source {
                DiffSource::Unstaged | DiffSource::Untracked => rsx! {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 text-xs text-destructive", disabled: busy,
                        onclick: move |_| state.dialog.set(Some(Dialog::Confirm { title: "Discard changes?".into(), body: format!("All unstaged changes to “{p_discard}” will be lost."), confirm: "Discard file".into(), destructive: true, op: RepoOp::Discard(vec![p_discard.clone()]) })),
                        "Discard file" }
                    Button { size: ButtonSize::Sm, class: "h-7 bg-primary-accent text-xs text-white hover:bg-primary-accent/90", disabled: busy, onclick: move |_| r1(RepoOp::Stage(vec![path.clone()])), "Stage file" }
                },
                DiffSource::Staged => rsx! {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, class: "h-7 text-xs", disabled: busy, onclick: move |_| r2(RepoOp::Unstage(vec![path.clone()])), "Unstage file" }
                },
                DiffSource::Commit => rsx! {},
            }
            button { class: "ml-1 rounded px-2 py-1 text-muted-foreground hover:bg-accent", title: "Close (Esc)", "aria-label": "Close diff",
                onclick: move |_| state.update_view(id, |v| v.diff = None), "✕" }
        }
        div { class: "min-h-0 flex-1 overflow-auto",
            CodeDiff { file, source, repo: Some(id) }
        }
    }
}
