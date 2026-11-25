//! The shared search modal in FerrumVariables. Search Everywhere (Shift
//! twice) and Go to (⌘P) list the projects and the variables by name; Find
//! in files (⌘⇧F) searches variable names and their comments. Values are
//! never listed or searched, and while the vault is locked only Settings
//! and the menu's actions are (the projects are inside the encrypted vault).
//! ⌘F is the project filter in the sidebar.

use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumenv_core::Vault;

use crate::app::EnvApp;

const PROJECT: &str = "project:";
const VARIABLE: &str = "var:";

/// Projects, then each variable as `KEY` with "project · environment".
pub(crate) fn items(vault: &Vault) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for (p, project) in vault.projects.iter().enumerate() {
        items.push(
            SearchItem::new(
                SearchCategory::Items,
                project.name.clone(),
                format!("{PROJECT}{p}"),
            )
            .detail("Project")
            .icon(IconName::Folder),
        );
    }
    for (p, project) in vault.projects.iter().enumerate() {
        for (e, env) in project.environments.iter().enumerate() {
            for variable in &env.variables {
                items.push(
                    SearchItem::new(
                        SearchCategory::Items,
                        variable.key.clone(),
                        format!("{VARIABLE}{p}:{e}:{}", variable.key),
                    )
                    .detail(format!("{} · {}", project.name, env.name))
                    .icon(if variable.secret {
                        IconName::Lock
                    } else {
                        IconName::Hash
                    }),
                );
            }
        }
    }
    items
}

/// Searches `KEY  comment` lines; a copy of the names and comments only.
pub(crate) fn searcher(vault: &Vault) -> TextSearcher {
    let mut lines: Vec<(String, String, String)> = Vec::new();
    for (p, project) in vault.projects.iter().enumerate() {
        for (e, env) in project.environments.iter().enumerate() {
            for variable in &env.variables {
                let text = if variable.comment.is_empty() {
                    variable.key.clone()
                } else {
                    format!("{}  {}", variable.key, variable.comment.replace('\n', " "))
                };
                lines.push((
                    format!("{VARIABLE}{p}:{e}:{}", variable.key),
                    format!("{} · {}", project.name, env.name),
                    text,
                ));
            }
        }
    }
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for (id, label, text) in &lines {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if matcher.allows(label) && !matcher.find_lines(text, id, label, limit, &mut hits) {
                break;
            }
        }
        Ok(hits)
    })
}

/// `(project, environment)` of an item or hit id.
fn target(id: &str) -> Option<(usize, Option<usize>)> {
    if let Some(p) = id.strip_prefix(PROJECT) {
        return Some((p.parse().ok()?, None));
    }
    let mut parts = id.strip_prefix(VARIABLE)?.splitn(3, ':');
    let p = parts.next()?.parse().ok()?;
    let e = parts.next()?.parse().ok()?;
    Some((p, Some(e)))
}

impl EnvApp {
    /// Opens the modal in `mode`, or closes it when it is open in that mode.
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((open, _)) = self.search_modal.take()
            && open.read(cx).mode() == mode
        {
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        let (items, searcher) = match self.vault() {
            Some(unlocked) => (items(&unlocked.vault), Some(searcher(&unlocked.vault))),
            None if mode == SearchMode::Everywhere => (Vec::new(), None),
            None => {
                self.show_info("Unlock the vault to search its projects and variables.", cx);
                return;
            }
        };
        let modal = cx.new(|cx| SearchModal::new(mode, "Variables", items, searcher, window, cx));
        let subscription = cx.subscribe_in(&modal, window, Self::on_search_event);
        self.search_modal = Some((modal, subscription));
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_modal = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        let id = match event {
            SearchEvent::Open(item) => item.id.to_string(),
            SearchEvent::OpenHit(hit) => hit.source.to_string(),
            SearchEvent::Dismissed => return,
        };
        if let Some((project, env)) = target(&id) {
            self.select_project(project, window, cx);
            if let Some(env) = env {
                self.select_environment(env, cx);
            }
        }
    }

    /// ⌘F: the project filter in the sidebar (shown if hidden).
    pub(crate) fn focus_project_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(search) = self.vault().map(|u| u.search.clone()) else {
            return;
        };
        self.sidebar_open = true;
        search.update(cx, |input, cx| {
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumenv_core::model::{Environment, Project, Variable};
    use std::sync::atomic::AtomicBool;

    fn vault() -> Vault {
        let mut project = Project::new("shop", None);
        let mut env = Environment::new("development");
        let mut url = Variable::new("DATABASE_URL", "postgres://user:hunter2@db/shop");
        url.comment = "primary database".into();
        env.variables.push(url);
        env.variables.push(Variable::new("PORT", "3000"));
        project.environments = vec![env];
        let mut vault = Vault::default();
        vault.projects.push(project);
        vault
    }

    #[test]
    fn items_list_projects_and_variable_names_never_values() {
        let items = items(&vault());
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_ref()).collect();
        assert_eq!(titles, ["shop", "DATABASE_URL", "PORT"]);
        assert_eq!(items[1].detail.as_ref(), "shop · development");
        assert!(items.iter().all(|i| !i.title.contains("hunter2")
            && !i.detail.contains("hunter2")
            && !i.id.contains("hunter2")));
        assert_eq!(target(&items[0].id), Some((0, None)));
        assert_eq!(target(&items[2].id), Some((0, Some(0))));
    }

    #[test]
    fn find_in_files_searches_names_and_comments_but_not_values() {
        let search = searcher(&vault());
        let find = |text: &str| {
            let query = ferrum_ui::search::TextQuery {
                text: text.into(),
                ..Default::default()
            };
            search(&query, &AtomicBool::new(false), 10).expect("search")
        };
        assert_eq!(find("primary").len(), 1);
        assert_eq!(find("port").len(), 1);
        assert!(find("hunter2").is_empty());
        assert!(find("3000").is_empty());
    }
}
