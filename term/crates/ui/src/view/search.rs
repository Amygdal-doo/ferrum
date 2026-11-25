//! Search Everywhere (Shift twice), Go to terminal (⌘P) and Find in all
//! terminals (⌘⇧F) in the shared search modal; desktop window only.
//! Terminals are found by label, title and folder, recent commands by their
//! command line, text in every pane's scrollback.

use std::sync::Arc;

use ferrum_ui::components::IconName;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextHit, TextMatcher,
    TextSearcher,
};
use ferrumterm_client::Client;
use ferrumterm_protocol::{BlockId, Request, Response, SessionId, SessionInfo};
use gpui::{AppContext as _, Context, Entity, Window};

use super::TermView;
use crate::sidebar::model::{block_title, tilde};

const BLOCK: &str = "block:";
const COMMAND: &str = "command:";

/// A pane the text search reads: its id and how it is shown.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pane {
    pub(crate) block: BlockId,
    pub(crate) label: String,
}

impl TermView {
    /// Opens the search modal, or closes it when it is open in that mode.
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.options.standalone {
            cx.propagate();
            return;
        }
        if let Some(search) = self.search.take() {
            self._search_events = None;
            window.focus(&self.focus_handle, cx);
            cx.notify();
            if search.read(cx).mode() == mode {
                return;
            }
        }
        let mut items = Vec::new();
        if mode != SearchMode::FindInFiles {
            items = terminal_items(&self.app.sessions);
        }
        if mode == SearchMode::Everywhere {
            items.extend(self.command_items());
        }
        let searcher = self.app.client.clone().map(|client| {
            scrollback_searcher(client, self.app.rt.clone(), panes(&self.app.sessions))
        });
        let search = cx.new(|cx| SearchModal::new(mode, "Terminals", items, searcher, window, cx));
        self._search_events = Some(cx.subscribe_in(&search, window, Self::on_search_event));
        self.search = Some(search);
        cx.notify();
    }

    /// Recent commands of the panes on screen (shell integration).
    fn command_items(&self) -> Vec<SearchItem> {
        let mut items = Vec::new();
        for (block, frame) in &self.app.frames {
            if !self.app.knows_block(*block) {
                continue;
            }
            let pane = self.block_title(*block);
            for mark in frame
                .commands
                .iter()
                .filter(|m| !m.command.trim().is_empty())
            {
                items.push(
                    SearchItem::new(
                        SearchCategory::Symbols,
                        mark.command.trim().to_owned(),
                        format!("{COMMAND}{block}:{}", mark.id),
                    )
                    .detail(pane.clone())
                    .icon(IconName::Terminal),
                );
            }
        }
        items
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = None;
        self._search_events = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        match event {
            SearchEvent::Open(item) => {
                if let Some(rest) = item.id.strip_prefix(BLOCK)
                    && let Some((session, block)) = rest.split_once(':')
                    && let (Ok(session), Ok(block)) =
                        (session.parse::<SessionId>(), block.parse::<BlockId>())
                {
                    self.app.focus_block(session, block);
                } else if let Some(rest) = item.id.strip_prefix(COMMAND)
                    && let Some((block, id)) = rest.split_once(':')
                    && let (Ok(block), Ok(id)) = (block.parse::<BlockId>(), id.parse::<u64>())
                {
                    if let Some(session) = session_of(&self.app.sessions, block) {
                        self.app.focus_block(session, block);
                    }
                    self.selected_command = Some((block, id));
                }
            }
            SearchEvent::OpenHit(hit) => {
                let Ok(block) = hit.source.parse::<BlockId>() else {
                    return;
                };
                if let Some(session) = session_of(&self.app.sessions, block) {
                    self.app.focus_block(session, block);
                }
                // The pane's own find bar takes over, on the matched text.
                let found = hit
                    .preview
                    .get(hit.range.clone())
                    .unwrap_or_default()
                    .to_owned();
                self.open_find(window, cx);
                if let Some(bar) = &self.find {
                    bar.input.update(cx, |input, cx| input.set_text(found, cx));
                }
                self.search_now(true, cx);
            }
            SearchEvent::Dismissed => {}
        }
    }
}

/// Every pane of every session, as Terminals items.
pub(crate) fn terminal_items(sessions: &[SessionInfo]) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for s in sessions {
        for w in &s.windows {
            for b in &w.blocks {
                let mut detail = if sessions.len() > 1 {
                    format!("{} · ", s.name)
                } else {
                    String::new()
                };
                detail.push_str(&b.cwd.as_deref().map(tilde).unwrap_or_default());
                items.push(
                    SearchItem::new(
                        SearchCategory::Items,
                        block_title(b),
                        format!("{BLOCK}{}:{}", s.id, b.id),
                    )
                    .detail(detail)
                    .icon(if b.agent.is_some() {
                        IconName::Sparkle
                    } else {
                        IconName::Terminal
                    }),
                );
            }
        }
    }
    items
}

/// The panes the text search reads, labelled like the Terminals items.
pub(crate) fn panes(sessions: &[SessionInfo]) -> Vec<Pane> {
    sessions
        .iter()
        .flat_map(|s| s.windows.iter())
        .flat_map(|w| w.blocks.iter())
        .map(|b| Pane {
            block: b.id,
            label: block_title(b),
        })
        .collect()
}

fn session_of(sessions: &[SessionInfo], block: BlockId) -> Option<SessionId> {
    sessions
        .iter()
        .find(|s| {
            s.windows
                .iter()
                .any(|w| w.blocks.iter().any(|b| b.id == block))
        })
        .map(|s| s.id)
}

/// Text search over each pane's screen and scrollback, read from the
/// server when the search runs (soft-wrapped lines joined).
fn scrollback_searcher(
    client: Client,
    rt: tokio::runtime::Handle,
    panes: Vec<Pane>,
) -> TextSearcher {
    let panes = Arc::new(panes);
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for pane in panes.iter() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if !matcher.allows(&pane.label) {
                continue;
            }
            let request = Request::Capture {
                target: Some(pane.block.to_string()),
                scrollback: true,
                unwrapped: true,
            };
            let client = client.clone();
            let Ok(Ok(Response::Text { text })) = rt.block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(5), client.request(request))
                    .await
            }) else {
                continue;
            };
            if !search_text(&matcher, &text, pane, limit, &mut hits) {
                break;
            }
        }
        Ok(hits)
    })
}

/// One pane's text into `hits`; false once `limit` is reached.
pub(crate) fn search_text(
    matcher: &TextMatcher,
    text: &str,
    pane: &Pane,
    limit: usize,
    hits: &mut Vec<TextHit>,
) -> bool {
    matcher.find_lines(text, &pane.block.to_string(), &pane.label, limit, hits)
}
