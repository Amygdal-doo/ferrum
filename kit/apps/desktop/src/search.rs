//! The shared search modal in FerrumKit: Search Everywhere (Shift twice)
//! and Go to (⌘P) list the tools and the saved inputs (by name, never their
//! contents); Find in files (⌘⇧F) searches the tools' names and
//! descriptions. ⌘K stays the tool palette, ⌘F the sidebar filter.

use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumkit_core::Tool;
use ferrumkit_core::store::KitStore;

use crate::app::FerrumKit;

const TOOL: &str = "tool:";
const SAVED: &str = "saved:";

fn tool_by_id(id: &str) -> Option<Tool> {
    Tool::ALL.into_iter().find(|t| t.id() == id)
}

/// Every tool, then every saved input as "name" with its tool as detail.
pub(crate) fn items(store: &KitStore) -> Vec<SearchItem> {
    let mut items: Vec<SearchItem> = Tool::ALL
        .into_iter()
        .map(|tool| {
            SearchItem::new(
                SearchCategory::Items,
                tool.name(),
                format!("{TOOL}{}", tool.id()),
            )
            .icon(IconName::ToolHammer)
        })
        .collect();
    for tool in Tool::ALL {
        for saved in store.saved(tool) {
            items.push(
                SearchItem::new(
                    SearchCategory::Items,
                    saved.name.clone(),
                    format!("{SAVED}{}:{}", tool.id(), saved.name),
                )
                .detail(format!("Saved · {}", tool.name()))
                .icon(IconName::Bookmark),
            );
        }
    }
    items
}

/// Searches each tool's name and description.
pub(crate) fn searcher() -> TextSearcher {
    Arc::new(|query, _cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for tool in Tool::ALL {
            if !matcher.allows(tool.name()) {
                continue;
            }
            let text = format!("{}\n{}", tool.name(), tool.description());
            let id = format!("{TOOL}{}", tool.id());
            if !matcher.find_lines(&text, &id, tool.name(), limit, &mut hits) {
                break;
            }
        }
        Ok(hits)
    })
}

impl FerrumKit {
    /// Opens the modal in `mode`, or closes it when it is open in that mode.
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.search_modal.take()
            && open.read(cx).mode() == mode
        {
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        let items = items(&self.store);
        let modal =
            cx.new(|cx| SearchModal::new(mode, "Tools", items, Some(searcher()), window, cx));
        let subscription = cx.subscribe_in(&modal, window, Self::on_search_event);
        self._subscriptions.push(subscription);
        self.search_modal = Some(modal);
        cx.notify();
    }

    #[cfg(test)]
    pub fn search_modal(&self) -> Option<&Entity<SearchModal>> {
        self.search_modal.as_ref()
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
        if let Some(tool) = id.strip_prefix(TOOL).and_then(tool_by_id) {
            self.select(tool, cx);
            self.focus_tool(tool, window, cx);
        } else if let Some((tool, name)) = id.strip_prefix(SAVED).and_then(|rest| {
            let (tool, name) = rest.split_once(':')?;
            Some((tool_by_id(tool)?, name.to_string()))
        }) {
            self.select(tool, cx);
            self.focus_tool(tool, window, cx);
            let values = self
                .store
                .saved(tool)
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.inputs.clone());
            if let Some(values) = values {
                self.restore(values, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn items_are_the_tools_and_saved_inputs_by_name() {
        let mut store = KitStore::default();
        store
            .save_named(Tool::Base64, "Login header", &["secret value".into()])
            .expect("save");
        let items = items(&store);
        assert_eq!(items.len(), Tool::ALL.len() + 1);
        assert_eq!(items[0].id.as_ref(), "tool:json");
        let saved = items.last().expect("saved");
        assert_eq!(saved.title.as_ref(), "Login header");
        assert_eq!(saved.id.as_ref(), "saved:base64:Login header");
        // Saved contents never become searchable text.
        assert!(items.iter().all(|i| !i.title.contains("secret")));
        assert_eq!(tool_by_id("base64"), Some(Tool::Base64));
    }

    #[test]
    fn find_in_files_searches_descriptions() {
        let query = ferrum_ui::search::TextQuery {
            text: "signatures".into(),
            ..Default::default()
        };
        let hits = searcher()(&query, &AtomicBool::new(false), 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source.as_ref(), "tool:jwt");
        assert_eq!(hits[0].line, 1);
    }
}
