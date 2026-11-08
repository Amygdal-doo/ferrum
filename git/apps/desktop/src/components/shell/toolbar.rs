//! Unified titlebar toolbar (Tower style): repositories / profiles over the
//! sidebar, then quick open, back / forward, the repo title, actions and search.

use crate::commands::{repo, workspace};
use crate::components::icons::{Icon, Svg};
use crate::state::{AppState, Dialog, Page, PromptKind};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::RepoOp;

/// Room for the macOS traffic lights.
const TRAFFIC_LIGHTS: f64 = 78.0;

fn drag(e: MouseEvent) {
    // Only a press on the bar itself moves the window, not on its buttons.
    if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
        dioxus::desktop::window().drag();
    }
}

#[component]
fn Tool(
    icon: Icon,
    label: String,
    #[props(default)] disabled: bool,
    #[props(default)] badge: Option<u32>,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: "relative flex h-11 min-w-12 flex-col items-center justify-center gap-0.5 rounded-md px-1.5 text-[10.5px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-35",
            title: "{label}", disabled,
            onmousedown: move |e| e.stop_propagation(),
            onclick: move |_| onclick.call(()),
            span { class: "text-foreground", Svg { icon, class: "size-[18px]" } }
            "{label}"
            if let Some(n) = badge.filter(|n| *n > 0) {
                span { class: "absolute right-0.5 top-0.5 rounded-full bg-primary-accent px-1 text-[9px] font-semibold text-white", "{n}" }
            }
        }
    }
}

#[component]
pub fn Toolbar() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let sidebar = state.sidebar_width();
    let entry = state.active_entry();
    let s_home = services.clone();
    let left = rsx! {
        div { class: "flex h-full shrink-0 items-center justify-end gap-1 border-r pr-2", style: "width: {sidebar}px; padding-left: {TRAFFIC_LIGHTS}px", onmousedown: drag,
            Tool { icon: Icon::Bookmark, label: "Repositories", onclick: move |_| workspace::activate(state, s_home.clone(), None) }
            Tool { icon: Icon::User, label: "Profiles", onclick: move |_| state.dialog.set(Some(Dialog::Profiles(None))) }
        }
    };
    let Some(entry) = entry else {
        return rsx! {
            header { class: "flex h-[52px] shrink-0 items-center border-b bg-sidenav", onmousedown: drag,
                {left}
                div { class: "flex-1 px-4 text-[13px] font-semibold", "FerrumGIT" }
            }
        };
    };
    let id = entry.id;
    let view = state.view(id);
    let busy = state.busy.read().get(&id).cloned();
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
    let profile = state
        .profile_of(&entry)
        .map_or_else(|| "System default".to_owned(), |p| p.name);
    let pull_mode = state.settings.read().pull_mode;
    let prune = state.settings.read().prune_on_fetch;
    let has_stash = !snap.stashes.is_empty();
    let dirty = !status.is_clean();
    let detached = status.branch.head.is_none();
    let s = services.clone();
    let run = move |op: RepoOp| repo::op(state, s.clone(), id, op);
    let (r_fetch, r_pull, r_push, r_apply) = (run.clone(), run.clone(), run.clone(), run);
    let (s_back, s_fwd, s_sync, s_refresh, s_search) = (
        services.clone(),
        services.clone(),
        services.clone(),
        services.clone(),
        services.clone(),
    );
    let (path_term, path_finder) = (entry.path.clone(), entry.path.clone());
    let search = view.search.clone();

    rsx! {
        header { class: "flex h-[52px] shrink-0 items-center border-b bg-sidenav", onmousedown: drag,
            {left}
            div { class: "flex items-center gap-0.5 px-2",
                Tool { icon: Icon::QuickOpen, label: "Quick Open", onclick: move |_| state.dialog.set(Some(Dialog::QuickOpen)) }
                Tool { icon: Icon::ChevronLeft, label: "Prev", disabled: view.back.is_empty(), onclick: move |_| repo::go(state, s_back.clone(), id, true) }
                Tool { icon: Icon::ChevronRight, label: "Next", disabled: view.forward.is_empty(), onclick: move |_| repo::go(state, s_fwd.clone(), id, false) }
            }
            div { class: "flex min-w-0 flex-1 flex-col justify-center px-2",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-[13.5px] font-semibold", "{entry.name}" }
                    button { class: "flex shrink-0 items-center gap-1 rounded-full border px-2 text-[10.5px] text-muted-foreground hover:bg-accent hover:text-foreground",
                        title: "Identity and SSH key for this repository",
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |_| state.dialog.set(Some(Dialog::RepoSettings(id))),
                        Svg { icon: Icon::User, class: "size-3" } "{profile}" }
                }
                if let Some(b) = busy {
                    div { class: "flex items-center gap-2 text-[11.5px] text-muted-foreground",
                        span { class: "size-1.5 animate-pulse rounded-full bg-primary-accent" }
                        "{b.label}…"
                        if b.cancel.is_some() {
                            button { class: "underline hover:text-foreground", onmousedown: move |e| e.stop_propagation(), onclick: move |_| repo::cancel(state, id), "Cancel" }
                        }
                    }
                } else {
                    span { class: "truncate text-[11.5px] text-muted-foreground", "{subtitle}" }
                }
            }
            div { class: "flex items-center gap-0.5 px-1",
                Tool { icon: Icon::Terminal, label: "Terminal", onclick: move |_| open_terminal(&path_term) }
                Tool { icon: Icon::Folder, label: "Finder", onclick: move |_| workspace::reveal(&path_finder) }
                div { class: "mx-1 h-7 w-px bg-border" }
                Tool { icon: Icon::Fetch, label: "Fetch", disabled: is_busy, onclick: move |_| r_fetch(RepoOp::Fetch { remote: None, prune }) }
                Tool { icon: Icon::Pull, label: "Pull", disabled: is_busy || detached, badge: Some(status.branch.behind), onclick: move |_| r_pull(RepoOp::Pull(pull_mode)) }
                Tool { icon: Icon::Push, label: "Push", disabled: is_busy || detached, badge: Some(status.branch.ahead), onclick: move |_| r_push(RepoOp::Push { remote: None, force_with_lease: false }) }
                Tool { icon: Icon::Sync, label: "Sync", disabled: is_busy || detached, onclick: move |_| sync(state, s_sync.clone(), id) }
                div { class: "mx-1 h-7 w-px bg-border" }
                Tool { icon: Icon::Merge, label: "Merge", disabled: is_busy || detached, onclick: move |_| state.dialog.set(Some(Dialog::PickBranch { rebase: false })) }
                Tool { icon: Icon::Rebase, label: "Rebase", disabled: is_busy || detached, onclick: move |_| state.dialog.set(Some(Dialog::PickBranch { rebase: true })) }
                div { class: "mx-1 h-7 w-px bg-border" }
                Tool { icon: Icon::ArchiveRestore, label: "Apply", disabled: is_busy || !has_stash, onclick: move |_| r_apply(RepoOp::StashPop(0)) }
                Tool { icon: Icon::Archive, label: "Save", disabled: is_busy || !dirty, onclick: move |_| state.dialog.set(Some(Dialog::Prompt { kind: PromptKind::StashSave, initial: String::new() })) }
                Tool { icon: Icon::Refresh, label: "Refresh", onclick: move |_| repo::load(state, s_refresh.clone(), id) }
            }
            div { class: "mr-3 ml-1 flex h-8 w-52 items-center gap-1.5 rounded-md border bg-background px-2", onmousedown: move |e| e.stop_propagation(),
                Svg { icon: Icon::Search, class: "size-3.5 text-muted-foreground" }
                input { class: "h-full min-w-0 flex-1 bg-transparent text-[12.5px] outline-none select-text", placeholder: "Search for Message",
                    "aria-label": "Search commits", value: "{search}",
                    oninput: move |e| {
                        let v = e.value();
                        if state.view(id).page != Page::History { repo::set_page(state, s_search.clone(), id, Page::History); }
                        state.update_view(id, |x| x.search = v);
                    } }
            }
        }
    }
}

fn open_terminal(path: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .args(["-a", "Terminal", path])
            .spawn()
    } else {
        std::process::Command::new("x-terminal-emulator")
            .current_dir(path)
            .spawn()
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not open a terminal");
    }
}

/// Pull, then push once the pull has finished.
fn sync(state: AppState, services: AppServices, id: ferrumgit_domain::RepoId) {
    let mode = state.settings.peek().pull_mode;
    repo::op(state, services.clone(), id, RepoOp::Pull(mode));
    spawn(async move {
        while state.is_busy(id) {
            let _ = crate::commands::background(
                &services,
                tokio::time::sleep(std::time::Duration::from_millis(150)),
            )
            .await;
        }
        repo::op(
            state,
            services,
            id,
            RepoOp::Push {
                remote: None,
                force_with_lease: false,
            },
        );
    });
}
