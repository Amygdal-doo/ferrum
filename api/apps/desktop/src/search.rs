//! Search Everywhere (Shift twice), Go to request (⌘P) and Find in requests
//! (⌘⇧F) in the shared search modal. Requests are found by name, URL and
//! collection; collections and environments by name; text in URLs, params,
//! headers, bodies and responses. Secrets never reach a preview: header
//! values that look like credentials and secret variables are left out.

use std::sync::Arc;

use ferrum_ui::components::IconName;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumapi_domain::{
    Collection, CollectionId, Environment, EnvironmentId, HttpRequest, Item, ItemId, KeyValue,
    RequestBody, RequestOrigin, RequestTab, TabId,
};
use gpui::{AppContext as _, Context, Entity, Window};
use uuid::Uuid;

use crate::app::FerrumApi;
use crate::state::{ResponseSearch, TabResponse};

const REQUEST: &str = "req:";
const TAB: &str = "tab:";
const RESPONSE: &str = "resp:";
const COLLECTION: &str = "col:";
const ENVIRONMENT: &str = "env:";

/// Largest response body searched, in bytes.
const MAX_BODY: usize = 2 * 1024 * 1024;

/// One searchable text: where it is and what it says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Document {
    pub(crate) source: String,
    pub(crate) label: String,
    pub(crate) text: String,
}

impl FerrumApi {
    /// Opens the search modal, or closes it when it is open in that mode.
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search.take() {
            self._search_events = None;
            window.focus(&self.root_focus(), cx);
            cx.notify();
            if search.read(cx).mode() == mode {
                return;
            }
        }
        if self.state.dialog.is_some() {
            return;
        }
        let mut items = Vec::new();
        if mode != SearchMode::FindInFiles {
            items = request_items(&self.state.collections, &self.state.tabs);
        }
        if mode == SearchMode::Everywhere {
            items.extend(place_items(
                &self.state.collections,
                &self.state.environments,
            ));
        }
        let documents = documents(
            &self.state.collections,
            &self.state.tabs,
            &self.state.environments,
            |tab| match self.state.responses.get(&tab) {
                Some(TabResponse::Done(response)) => Some(response.as_ref()),
                _ => None,
            },
        );
        let searcher = Some(documents_searcher(documents));
        let search = cx.new(|cx| SearchModal::new(mode, "Requests", items, searcher, window, cx));
        self._search_events = Some(cx.subscribe_in(&search, window, Self::on_search_event));
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
        self._search_events = None;
        window.focus(&self.root_focus(), cx);
        match event {
            SearchEvent::Open(item) => self.open_found(&item.id, None),
            SearchEvent::OpenHit(hit) => {
                let found = hit.preview.get(hit.range.clone()).map(str::to_owned);
                self.open_found(&hit.source, found);
            }
            SearchEvent::Dismissed => {}
        }
        cx.notify();
    }

    /// Opens what a search found; `found` is the matched text of a hit.
    fn open_found(&mut self, id: &str, found: Option<String>) {
        if let Some(rest) = id.strip_prefix(REQUEST)
            && let Some((c, i)) = rest.split_once(':')
            && let (Ok(c), Ok(i)) = (Uuid::parse_str(c), Uuid::parse_str(i))
        {
            self.open_saved(RequestOrigin {
                collection: CollectionId(c),
                item: ItemId(i),
            });
        } else if let Some(tab) = id.strip_prefix(TAB).and_then(|t| Uuid::parse_str(t).ok()) {
            self.activate(TabId(tab));
        } else if let Some(tab) = id
            .strip_prefix(RESPONSE)
            .and_then(|t| Uuid::parse_str(t).ok())
        {
            self.activate(TabId(tab));
            self.state.response_active = true;
            self.open_response_search();
            if let Some(query) = found {
                self.state.response_search = Some(ResponseSearch {
                    query,
                    reveal: true,
                    focus: true,
                    ..self.state.response_search.take().unwrap_or_default()
                });
            }
        } else if let Some(c) = id
            .strip_prefix(COLLECTION)
            .and_then(|c| Uuid::parse_str(c).ok())
        {
            self.state.sidebar_open = true;
            self.state.sidebar_view = crate::state::SidebarView::Collections;
            self.state.expanded.insert(c);
        } else if let Some(e) = id
            .strip_prefix(ENVIRONMENT)
            .and_then(|e| Uuid::parse_str(e).ok())
        {
            self.set_active_environment(Some(EnvironmentId(e)));
        }
    }
}

/// Saved requests (with their collection and folder) and unsaved tabs.
pub(crate) fn request_items(collections: &[Collection], tabs: &[RequestTab]) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for c in collections {
        walk(&c.items, &c.name, &mut |path, saved| {
            items.push(
                SearchItem::new(
                    SearchCategory::Items,
                    display_name(&saved.name, &saved.request),
                    format!("{REQUEST}{}:{}", c.id, saved.id),
                )
                .detail(format!("{} · {path}", saved.request.method))
                .icon(IconName::ArrowUpRight),
            );
        });
    }
    for tab in tabs.iter().filter(|t| t.origin.is_none()) {
        items.push(
            SearchItem::new(
                SearchCategory::Items,
                display_name(&tab.title, &tab.request),
                format!("{TAB}{}", tab.id),
            )
            .detail(format!("{} · unsaved tab", tab.request.method))
            .icon(IconName::Pencil),
        );
    }
    items
}

/// Collections and environments, listed under Symbols.
pub(crate) fn place_items(
    collections: &[Collection],
    environments: &[Environment],
) -> Vec<SearchItem> {
    let mut items: Vec<SearchItem> = collections
        .iter()
        .map(|c| {
            SearchItem::new(
                SearchCategory::Symbols,
                c.name.clone(),
                format!("{COLLECTION}{}", c.id),
            )
            .detail("Collection")
            .icon(IconName::Folder)
        })
        .collect();
    items.extend(environments.iter().map(|e| {
        SearchItem::new(
            SearchCategory::Symbols,
            e.name.clone(),
            format!("{ENVIRONMENT}{}", e.id),
        )
        .detail("Environment")
        .icon(IconName::Public)
    }));
    items
}

fn display_name(name: &str, request: &HttpRequest) -> String {
    if name.trim().is_empty() {
        request.label()
    } else {
        name.to_owned()
    }
}

/// Calls `f` with the folder path and each saved request under `items`.
fn walk(items: &[Item], path: &str, f: &mut dyn FnMut(&str, &ferrumapi_domain::SavedRequest)) {
    for item in items {
        match item {
            Item::Request(saved) => f(path, saved),
            Item::Folder(folder) => walk(&folder.items, &format!("{path}/{}", folder.name), f),
        }
    }
}

/// Header names whose values are credentials.
fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "authorization",
        "token",
        "secret",
        "password",
        "passwd",
        "api-key",
        "apikey",
        "api_key",
        "cookie",
        "session",
    ]
    .iter()
    .any(|word| key.contains(word))
}

/// Key-value lines, credential values left out.
fn pairs(out: &mut String, pairs: &[KeyValue], separator: &str) {
    for kv in pairs.iter().filter(|kv| !kv.key.is_empty()) {
        out.push_str(&kv.key);
        if !is_secret_key(&kv.key) {
            out.push_str(separator);
            out.push_str(&kv.value);
        }
        out.push('\n');
    }
}

/// A request as searchable text: URL, params, headers, body.
fn request_text(request: &HttpRequest) -> String {
    let mut text = format!("{} {}\n", request.method, request.url);
    pairs(&mut text, &request.params, "=");
    pairs(&mut text, &request.headers, ": ");
    match &request.body {
        RequestBody::Raw { text: body, .. } => {
            text.push_str(body);
            text.push('\n');
        }
        RequestBody::UrlEncoded { fields } => pairs(&mut text, fields, "="),
        RequestBody::FormData { fields } => {
            for field in fields.iter().filter(|f| !f.key.is_empty()) {
                text.push_str(&format!("{}={}\n", field.key, field.value));
            }
        }
        RequestBody::None | RequestBody::Binary { .. } => {}
    }
    text
}

/// Everything the text search reads, copied out of the state.
pub(crate) fn documents<'a>(
    collections: &[Collection],
    tabs: &[RequestTab],
    environments: &[Environment],
    response: impl Fn(TabId) -> Option<&'a ferrumapi_domain::HttpResponse>,
) -> Vec<Document> {
    let mut docs = Vec::new();
    for c in collections {
        walk(&c.items, &c.name, &mut |path, saved| {
            docs.push(Document {
                source: format!("{REQUEST}{}:{}", c.id, saved.id),
                label: format!("{path}/{}", display_name(&saved.name, &saved.request)),
                text: request_text(&saved.request),
            });
        });
    }
    for tab in tabs {
        let title = display_name(&tab.title, &tab.request);
        if tab.origin.is_none() || tab.dirty {
            docs.push(Document {
                source: format!("{TAB}{}", tab.id),
                label: title.clone(),
                text: request_text(&tab.request),
            });
        }
        if let Some(response) = response(tab.id) {
            let mut text = String::new();
            for (key, value) in &response.headers {
                if is_secret_key(key) {
                    continue;
                }
                text.push_str(&format!("{key}: {value}\n"));
            }
            if response.body.len() <= MAX_BODY {
                text.push_str(&response.body_text());
            }
            docs.push(Document {
                source: format!("{RESPONSE}{}", tab.id),
                label: format!("{title} (response)"),
                text,
            });
        }
    }
    for e in environments {
        let mut text = String::new();
        for v in e.variables.iter().filter(|v| !v.key.is_empty()) {
            text.push_str(&v.key);
            if !v.secret && !is_secret_key(&v.key) {
                text.push_str(" = ");
                text.push_str(&v.value);
            }
            text.push('\n');
        }
        docs.push(Document {
            source: format!("{ENVIRONMENT}{}", e.id),
            label: format!("{} (environment)", e.name),
            text,
        });
    }
    docs
}

/// Text search over a snapshot of the documents.
pub(crate) fn documents_searcher(documents: Vec<Document>) -> TextSearcher {
    let documents = Arc::new(documents);
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for doc in documents.iter() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if matcher.allows(&doc.label)
                && !matcher.find_lines(&doc.text, &doc.source, &doc.label, limit, &mut hits)
            {
                break;
            }
        }
        Ok(hits)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::search::TextQuery;
    use ferrumapi_domain::{Folder, SavedRequest, Variable};
    use std::sync::atomic::AtomicBool;

    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            url: url.into(),
            headers: vec![
                KeyValue {
                    key: "Authorization".into(),
                    value: "Bearer hunter2".into(),
                    ..KeyValue::default()
                },
                KeyValue {
                    key: "Accept".into(),
                    value: "application/json".into(),
                    ..KeyValue::default()
                },
            ],
            body: RequestBody::Raw {
                language: ferrumapi_domain::RawLanguage::default(),
                text: "{\"user\": \"ada\"}".into(),
            },
            ..HttpRequest::default()
        }
    }

    fn collection() -> Collection {
        let mut folder = Folder::new("users");
        folder.items.push(Item::Request(SavedRequest::new(
            "List users",
            request("https://api.test/users"),
        )));
        let mut c = Collection::new("Demo");
        c.items.push(Item::Folder(folder));
        c.items.push(Item::Request(SavedRequest::new(
            "",
            request("https://api.test/health"),
        )));
        c
    }

    #[test]
    fn requests_are_items_with_their_folder_path() {
        let c = collection();
        let tab = RequestTab::new("scratch", request("https://x.test"));
        let items = request_items(std::slice::from_ref(&c), &[tab]);
        let titles: Vec<_> = items.iter().map(|i| i.title.to_string()).collect();
        assert_eq!(titles[0], "List users");
        assert_eq!(items[0].detail.as_ref(), "GET · Demo/users");
        assert!(items[0].id.starts_with(REQUEST));
        // An unnamed request is called by its URL; unsaved tabs are listed too.
        assert!(titles[1].contains("health"));
        assert!(items[2].id.starts_with(TAB));

        let env = Environment::new("staging");
        let places = place_items(&[c], &[env]);
        assert_eq!(places.len(), 2);
        assert!(places.iter().all(|p| p.category == SearchCategory::Symbols));
    }

    fn search(docs: Vec<Document>, text: &str) -> Vec<ferrum_ui::search::TextHit> {
        let query = TextQuery {
            text: text.into(),
            ..TextQuery::default()
        };
        documents_searcher(docs)(&query, &AtomicBool::new(false), 100).expect("search")
    }

    #[test]
    fn text_search_reads_urls_headers_and_bodies_but_not_secrets() {
        let mut env = Environment::new("staging");
        env.variables.push(Variable {
            key: "API_TOKEN".into(),
            value: "s3cr3t".into(),
            enabled: true,
            secret: false,
        });
        env.variables.push(Variable {
            key: "host".into(),
            value: "api.test".into(),
            enabled: true,
            secret: false,
        });
        let docs = documents(&[collection()], &[], &[env], |_| None);
        assert_eq!(search(docs.clone(), "users").len(), 1);
        assert_eq!(search(docs.clone(), "application/json").len(), 2);
        assert_eq!(search(docs.clone(), "ada").len(), 2);
        assert!(search(docs.clone(), "hunter2").is_empty());
        assert!(search(docs.clone(), "s3cr3t").is_empty());
        let hits = search(docs, "api.test");
        assert!(hits.iter().any(|h| h.source.starts_with(ENVIRONMENT)));
    }
}
