//! The shared search modal in FerrumPorts: Search Everywhere (Shift twice)
//! and Go to (⌘P) list the listening ports by port and process; Find in
//! files (⌘⇧F) searches the processes' command lines. ⌘F stays the table's
//! own filter.

use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumports_core::{Listener, ListenerKey};

use crate::app::FerrumPorts;

/// The id a listener is found by again (`TCP:3000:4242`).
pub(crate) fn key_id(key: &ListenerKey) -> String {
    let pid = key.pid.map_or_else(|| "-".to_string(), |p| p.to_string());
    format!("{}:{}:{pid}", key.protocol.label(), key.port)
}

/// What a listener is called in the modal: `3000 node`.
fn title(listener: &Listener) -> String {
    format!("{} {}", listener.port, listener.process)
}

/// One item per listener; the detail names its project or container.
pub(crate) fn items(listeners: &[Listener]) -> Vec<SearchItem> {
    listeners
        .iter()
        .map(|l| {
            let detail = l
                .container
                .as_ref()
                .map(|c| c.name.clone())
                .or_else(|| l.project_name())
                .unwrap_or_else(|| l.protocol.label().to_string());
            SearchItem::new(SearchCategory::Items, title(l), key_id(&l.key()))
                .detail(detail)
                .icon(IconName::Server)
        })
        .collect()
}

/// Searches the command lines (the process name where `ps` gave none).
pub(crate) fn searcher(listeners: &[Listener]) -> TextSearcher {
    let lines: Vec<(String, String, String)> = listeners
        .iter()
        .map(|l| {
            (
                key_id(&l.key()),
                title(l),
                l.command.clone().unwrap_or_else(|| l.process.clone()),
            )
        })
        .collect();
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

impl FerrumPorts {
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
            window.focus(&self.focus_handle(cx), cx);
            cx.notify();
            return;
        }
        let items = items(&self.listeners);
        let searcher = Some(searcher(&self.listeners));
        let modal = cx.new(|cx| SearchModal::new(mode, "Ports", items, searcher, window, cx));
        let subscription = cx.subscribe_in(&modal, window, Self::on_search_event);
        self._subscriptions.push(subscription);
        self.search_modal = Some(modal);
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
        window.focus(&self.focus_handle(cx), cx);
        let id = match event {
            SearchEvent::Open(item) => item.id.clone(),
            SearchEvent::OpenHit(hit) => hit.source.clone(),
            SearchEvent::Dismissed => {
                cx.notify();
                return;
            }
        };
        self.select_found(&id, cx);
    }

    /// Selects the listener with `id` in the table, clearing a filter that
    /// would hide it.
    fn select_found(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(key) = self
            .listeners
            .iter()
            .map(Listener::key)
            .find(|k| key_id(k) == id)
        else {
            return;
        };
        self.show_history = false;
        self.only_project = false;
        self.search.update(cx, |input, cx| input.set_text("", cx));
        self.selected = Some(key);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumports_core::Protocol;
    use std::sync::atomic::AtomicBool;

    fn listener(port: u16, process: &str, command: Option<&str>) -> Listener {
        Listener {
            protocol: Protocol::Tcp,
            port,
            addresses: vec!["127.0.0.1".into()],
            pid: Some(u32::from(port) + 1),
            process: process.into(),
            user: None,
            command: command.map(str::to_owned),
            started: None,
            cwd: None,
            project: None,
            container: None,
        }
    }

    #[test]
    fn items_name_port_and_process_and_find_their_listener_again() {
        let listeners = vec![
            listener(3000, "node", None),
            listener(5432, "postgres", None),
        ];
        let items = items(&listeners);
        assert_eq!(items[0].title.as_ref(), "3000 node");
        assert_eq!(items[1].detail.as_ref(), "TCP");
        assert_eq!(items[1].id.as_ref(), key_id(&listeners[1].key()));
        assert_eq!(items[1].id.as_ref(), "TCP:5432:5433");
    }

    #[test]
    fn find_in_files_searches_command_lines() {
        let listeners = vec![
            listener(3000, "node", Some("node server.js --port 3000")),
            listener(8080, "java", None),
        ];
        let search = searcher(&listeners);
        let query = ferrum_ui::search::TextQuery {
            text: "server.js".into(),
            ..Default::default()
        };
        let hits = search(&query, &AtomicBool::new(false), 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_label.as_ref(), "3000 node");
        // Without a command line the process name is searched.
        let query = ferrum_ui::search::TextQuery {
            text: "java".into(),
            ..Default::default()
        };
        assert_eq!(
            search(&query, &AtomicBool::new(false), 10)
                .expect("search")
                .len(),
            1
        );
    }
}
