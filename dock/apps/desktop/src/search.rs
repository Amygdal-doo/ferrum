//! Search Everywhere (Shift twice), Go to (⌘P) and Find in files (⌘⇧F) in
//! the shared search modal: containers, images, volumes, networks and
//! compose projects by name; text in the open container's logs and the
//! project's compose files.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use ferrum_ui::components::IconName;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextHit, TextMatcher,
    TextSearcher,
};
use gpui::{AppContext as _, Context, Entity, Window};

use crate::app::FerrumContainers;
use crate::state::{AppState, DetailTab, Section};

const CONTAINER: &str = "container:";
const IMAGE: &str = "image:";
const VOLUME: &str = "volume:";
const NETWORK: &str = "network:";
const PROJECT: &str = "project:";
const LOG: &str = "log:";
const COMPOSE: &str = "compose:";

/// Everything the engine listed, as Go to / Search Everywhere items.
pub fn items(state: &AppState) -> Vec<SearchItem> {
    let item = |title: &str, id: String, detail: String, icon| {
        SearchItem::new(SearchCategory::Items, title.to_owned(), id)
            .detail(detail)
            .icon(icon)
    };
    let mut items = Vec::new();
    let mut projects = BTreeSet::new();
    for c in &state.containers {
        if let Some(p) = c.project() {
            projects.insert(p.to_owned());
        }
        items.push(item(
            &c.name,
            format!("{CONTAINER}{}", c.id),
            format!("Container · {}", c.image),
            IconName::Box,
        ));
    }
    if let Some(project) = &state.project {
        projects.extend(project.files.iter().map(|f| f.project.clone()));
    }
    for p in projects {
        items.push(item(
            &p,
            format!("{PROJECT}{p}"),
            "Compose project".into(),
            IconName::Blocks,
        ));
    }
    for image in &state.images {
        let detail = format!("Image · {}", image.short_id());
        if image.tags.is_empty() {
            items.push(item(
                image.short_id(),
                format!("{IMAGE}{}", image.short_id()),
                detail.clone(),
                IconName::BoxOpen,
            ));
        }
        for tag in &image.tags {
            items.push(item(
                tag,
                format!("{IMAGE}{tag}"),
                detail.clone(),
                IconName::BoxOpen,
            ));
        }
    }
    for v in &state.volumes {
        items.push(item(
            &v.name,
            format!("{VOLUME}{}", v.name),
            format!("Volume · {}", v.driver),
            IconName::Archive,
        ));
    }
    for n in &state.networks {
        items.push(item(
            &n.name,
            format!("{NETWORK}{}", n.name),
            format!("Network · {}", n.driver),
            IconName::Link,
        ));
    }
    items
}

/// Text search over a snapshot: the open container's log lines (already
/// loaded) and the open project's compose files (read on the search thread).
pub fn searcher(state: &AppState) -> TextSearcher {
    let logs: Option<(String, String, Vec<String>)> = state.detail.as_ref().map(|d| {
        let name = state
            .container(&d.id)
            .map_or_else(|| d.id.clone(), |c| c.name.clone());
        let lines = d.logs.lines().iter().map(|l| l.plain.clone()).collect();
        (d.id.clone(), name, lines)
    });
    let root = state.project.as_ref().map(|p| p.root.clone());
    let files: Vec<PathBuf> = state
        .project
        .as_ref()
        .map(|p| p.files.iter().map(|f| f.path.clone()).collect())
        .unwrap_or_default();
    let logs = Arc::new(logs);
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for path in &files {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let label = root
                .as_ref()
                .and_then(|r| path.strip_prefix(r).ok())
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            if !matcher.allows(&label) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let source = format!("{COMPOSE}{}", path.display());
            if !matcher.find_lines(&text, &source, &label, limit, &mut hits) {
                return Ok(hits);
            }
        }
        if let Some((id, name, lines)) = logs.as_ref() {
            let label = format!("{name} logs");
            if matcher.allows(&label) {
                matcher.find_lines(
                    &lines.join("\n"),
                    &format!("{LOG}{id}"),
                    &label,
                    limit,
                    &mut hits,
                );
            }
        }
        Ok(hits)
    })
}

impl FerrumContainers {
    /// Opens the search modal, or closes it when it is open in that mode.
    pub fn toggle_search(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.search.take()
            && search.read(cx).mode() == mode
        {
            self.search_subscription = None;
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        let list = if mode == SearchMode::FindInFiles {
            Vec::new()
        } else {
            items(&self.state)
        };
        let searcher = (mode != SearchMode::GoTo).then(|| searcher(&self.state));
        let search = cx.new(|cx| SearchModal::new(mode, "Containers", list, searcher, window, cx));
        let subscription = cx.subscribe_in(&search, window, Self::on_search_event);
        self.search_subscription = Some(subscription);
        self.search = Some(search);
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = None;
        self.search_subscription = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        match event {
            SearchEvent::Open(item) => self.open_search_item(&item.id, cx),
            SearchEvent::OpenHit(hit) => self.open_search_hit(hit, cx),
            SearchEvent::Dismissed => {}
        }
    }

    fn open_search_item(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(container) = id.strip_prefix(CONTAINER) {
            self.select_section(Section::Containers, cx);
            self.select_container(container, cx);
        } else if let Some(project) = id.strip_prefix(PROJECT) {
            self.show_filtered(Section::Containers, project, cx);
        } else if let Some(image) = id.strip_prefix(IMAGE) {
            self.show_filtered(Section::Images, image, cx);
        } else if let Some(volume) = id.strip_prefix(VOLUME) {
            self.show_filtered(Section::Volumes, volume, cx);
        } else if let Some(network) = id.strip_prefix(NETWORK) {
            self.show_filtered(Section::Networks, network, cx);
        }
    }

    fn open_search_hit(&mut self, hit: &TextHit, cx: &mut Context<Self>) {
        let matched = hit
            .preview
            .get(hit.range.clone())
            .unwrap_or_default()
            .to_owned();
        if let Some(id) = hit.source.strip_prefix(LOG) {
            self.select_section(Section::Containers, cx);
            self.select_container(id, cx);
            self.set_detail_tab(DetailTab::Logs, cx);
            self.inputs
                .log_search
                .update(cx, |input, cx| input.set_text(matched.clone(), cx));
            self.set_log_search(matched, cx);
        } else if let Some(path) = hit.source.strip_prefix(COMPOSE) {
            let project = self.state.project.as_ref().and_then(|p| {
                p.files
                    .iter()
                    .find(|f| f.path.to_string_lossy() == path)
                    .map(|f| f.project.clone())
            });
            if let Some(project) = project {
                self.show_filtered(Section::Containers, &project, cx);
            }
        }
    }

    /// Shows `section` filtered down to `text`.
    fn show_filtered(&mut self, section: Section, text: &str, cx: &mut Context<Self>) {
        self.select_section(section, cx);
        self.state.filter = text.to_owned();
        let text = text.to_owned();
        self.inputs
            .filter
            .update(cx, |input, cx| input.set_text(text, cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn state() -> AppState {
        let container = ferrumdock_engine::Container {
            id: "abc123".into(),
            name: "web".into(),
            image: "nginx:latest".into(),
            state: ferrumdock_engine::ContainerState::Running,
            status: "Up".into(),
            ports: Vec::new(),
            labels: [(
                ferrumdock_engine::COMPOSE_PROJECT.to_owned(),
                "shop".to_owned(),
            )]
            .into_iter()
            .collect(),
            created: 0,
        };
        AppState {
            containers: vec![container],
            ..AppState::default()
        }
    }

    #[test]
    fn lists_containers_and_their_compose_projects() {
        let items = items(&state());
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_ref()).collect();
        assert_eq!(ids, vec!["container:abc123", "project:shop"]);
        assert!(items.iter().all(|i| i.category == SearchCategory::Items));
        assert_eq!(items[0].title.as_ref(), "web");
    }

    #[test]
    fn searches_compose_files_of_the_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("compose.yaml");
        std::fs::write(&file, "services:\n  web:\n    image: nginx\n").expect("write");
        let state = AppState {
            project: Some(crate::project::ProjectState::load(dir.path().to_path_buf())),
            ..AppState::default()
        };
        let search = searcher(&state);
        let query = ferrum_ui::search::TextQuery {
            text: "nginx".into(),
            ..Default::default()
        };
        let hits = search(&query, &AtomicBool::new(false), 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
        assert_eq!(hits[0].source_label.as_ref(), "compose.yaml");
    }
}
