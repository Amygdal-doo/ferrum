//! The shared search modal over the diagram: components by name (⌘P and
//! Shift twice), APIs, sockets, databases and flows as symbols, and text in
//! everything the panel shows (⌘⇧F). ⌘F finds a component on the diagram.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumsystem_core::System;

use crate::app::{Dialog, FerrumSystem, Tab};

/// One searchable piece of text: where it opens, what it is called, the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Doc {
    pub id: String,
    pub label: String,
    pub text: String,
}

/// Components as items; APIs, sockets, databases and flows as symbols.
pub(crate) fn items(system: &System) -> Vec<SearchItem> {
    let mut items: Vec<SearchItem> = system
        .nodes
        .iter()
        .map(|n| {
            SearchItem::new(
                SearchCategory::Items,
                n.name.clone(),
                format!("node:{}", n.id),
            )
            .detail(n.kind.clone())
            .icon(IconName::Box)
        })
        .collect();
    items.extend(system.apis.iter().map(|a| {
        SearchItem::new(
            SearchCategory::Symbols,
            format!("{} {}", a.method, a.path),
            "tab:apis",
        )
        .detail(format!("API · {}", a.service))
        .icon(IconName::Link)
    }));
    items.extend(system.sockets.iter().map(|s| {
        SearchItem::new(SearchCategory::Symbols, s.channel.clone(), "tab:sockets")
            .detail(format!("Socket · {}", s.service))
            .icon(IconName::ArrowRightLeft)
    }));
    items.extend(system.databases.iter().map(|d| {
        SearchItem::new(
            SearchCategory::Symbols,
            d.name.clone(),
            format!("db:{}", d.id),
        )
        .detail("Database")
        .icon(IconName::DatabaseZap)
    }));
    items.extend(system.flows.iter().enumerate().map(|(i, f)| {
        SearchItem::new(SearchCategory::Symbols, f.name.clone(), format!("flow:{i}"))
            .detail("Flow")
            .icon(IconName::ArrowRight)
    }));
    items
}

/// Everything the panel shows as text, one [`Doc`] per thing that opens.
pub(crate) fn docs(system: &System) -> Vec<Doc> {
    let doc = |id: String, label: &str, lines: Vec<&str>| Doc {
        id,
        label: label.to_owned(),
        text: lines.join("\n"),
    };
    let mut docs = Vec::new();
    let mut overview = vec![system.tagline.as_str(), system.overview.as_str()];
    overview.extend(system.concepts.iter().map(String::as_str));
    overview.extend(system.functional.iter().map(String::as_str));
    overview.extend(system.non_functional.iter().map(String::as_str));
    docs.push(doc("tab:overview".into(), "Overview", overview));
    for n in &system.nodes {
        docs.push(doc(
            format!("node:{}", n.id),
            &n.name,
            vec![n.name.as_str(), n.description.as_str()],
        ));
    }
    for a in &system.apis {
        docs.push(Doc {
            id: "tab:apis".into(),
            label: format!("{} {}", a.method, a.path),
            text: format!("{} {}\n{}", a.method, a.path, a.summary),
        });
    }
    for s in &system.sockets {
        docs.push(doc(
            "tab:sockets".into(),
            &s.channel,
            vec![s.channel.as_str(), s.summary.as_str()],
        ));
    }
    for d in &system.databases {
        let mut lines = vec![d.name.as_str()];
        for t in &d.tables {
            lines.push(t.name.as_str());
            lines.push(t.note.as_str());
        }
        docs.push(doc(format!("db:{}", d.id), &d.name, lines));
    }
    for (i, f) in system.flows.iter().enumerate() {
        let mut lines = vec![f.name.as_str(), f.summary.as_str()];
        lines.extend(f.steps.iter().map(|s| s.text.as_str()));
        docs.push(doc(format!("flow:{i}"), &f.name, lines));
    }
    for n in &system.reliability {
        docs.push(doc(
            "tab:reliability".into(),
            &n.title,
            vec![n.title.as_str(), n.text.as_str()],
        ));
    }
    docs
}

/// Text search over [`docs`]; the mask matches their labels.
pub(crate) fn searcher(docs: Vec<Doc>) -> TextSearcher {
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for d in &docs {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if matcher.allows(&d.label)
                && !matcher.find_lines(&d.text, &d.id, &d.label, limit, &mut hits)
            {
                break;
            }
        }
        Ok(hits)
    })
}

impl FerrumSystem {
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_sub = None;
        if let Some(search) = self.search.take()
            && search.read(cx).mode() == mode
        {
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        let items = if mode == SearchMode::FindInFiles {
            Vec::new()
        } else {
            let all = items(&self.system);
            if mode == SearchMode::GoTo {
                all.into_iter()
                    .filter(|i| i.category == SearchCategory::Items)
                    .collect()
            } else {
                all
            }
        };
        let searcher = (mode != SearchMode::GoTo).then(|| searcher(docs(&self.system)));
        let search = cx.new(|cx| SearchModal::new(mode, "Components", items, searcher, window, cx));
        let sub = cx.subscribe_in(&search, window, Self::on_search_event);
        self.search_sub = Some(sub);
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
        self.search_sub = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        let id = match event {
            SearchEvent::Open(item) => item.id.to_string(),
            SearchEvent::OpenHit(hit) => hit.source.to_string(),
            SearchEvent::Dismissed => return,
        };
        self.open_search_target(&id, cx);
    }

    /// Shows what a search result points at.
    pub(crate) fn open_search_target(&mut self, id: &str, cx: &mut Context<Self>) {
        let (kind, rest) = id.split_once(':').unwrap_or((id, ""));
        match kind {
            "node" => self.select(Some(rest.to_owned()), cx),
            "db" => self.dialog = Some(Dialog::Schema(rest.to_owned())),
            "flow" => {
                self.tab = Tab::Flows;
                if let Ok(i) = rest.parse() {
                    self.walk_to(Some((i, 0)), cx);
                }
            }
            _ => {
                self.tab = match rest {
                    "apis" => Tab::Apis,
                    "sockets" => Tab::Sockets,
                    "reliability" => Tab::Reliability,
                    _ => Tab::Overview,
                }
            }
        }
        if kind != "db" {
            self.set_panel_open(true, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::search::TextQuery;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn components_are_items_and_the_rest_symbols() {
        let system = ferrumsystem_core::demos::all().remove(0);
        let items = items(&system);
        let nodes = items
            .iter()
            .filter(|i| i.category == SearchCategory::Items)
            .count();
        assert_eq!(nodes, system.nodes.len());
        assert!(
            items
                .iter()
                .all(|i| i.category != SearchCategory::Items || i.id.starts_with("node:"))
        );
        assert_eq!(
            items.len() - nodes,
            system.apis.len() + system.sockets.len() + system.databases.len() + system.flows.len()
        );
    }

    #[test]
    fn text_search_finds_component_descriptions() {
        let system = ferrumsystem_core::demos::all().remove(0);
        let node = system
            .nodes
            .iter()
            .find(|n| n.description.len() > 8)
            .expect("a described node");
        let word = node
            .description
            .split_whitespace()
            .find(|w| w.len() > 4)
            .expect("a word");
        let search = searcher(docs(&system));
        let query = TextQuery {
            text: word.to_owned(),
            ..TextQuery::default()
        };
        let hits = search(&query, &AtomicBool::new(false), 100).expect("search");
        assert!(
            hits.iter()
                .any(|h| h.source.as_ref() == format!("node:{}", node.id))
        );
        let masked = TextQuery {
            mask: "no-such-label".into(),
            ..query
        };
        assert!(
            search(&masked, &AtomicBool::new(false), 100)
                .expect("search")
                .is_empty()
        );
    }
}
