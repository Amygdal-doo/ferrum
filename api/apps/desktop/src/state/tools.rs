//! UI state of the local tools: mock servers (one per collection), the
//! webhook inspector and the load test. Pure reducers, unit tested.

use ferrumapi_application::{CancellationToken, LoadEnd, LoadEvent, MockServer, WebhookInspector};
use ferrumapi_domain::load::{LoadConfig, LoadTally};
use ferrumapi_domain::mock::{CollectionMocks, MockLogEntry, MockResponse};
use ferrumapi_domain::webhook::{CapturedRequest, WebhookReply};
use ferrumapi_domain::{CollectionId, ItemId, MockId, TabId};
use std::collections::VecDeque;

/// Log lines kept per mock server.
pub const MOCK_LOG_LIMIT: usize = 200;
/// Default webhook inspector port.
pub const DEFAULT_WEBHOOK_PORT: u16 = 9000;

/// Parses a TCP port typed by the user.
pub fn parse_port(text: &str) -> Result<u16, String> {
    match text.trim().parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err("Port must be a number between 1 and 65535".into()),
    }
}

/// Parses an HTTP status code typed by the user.
pub fn parse_status(text: &str) -> Result<u16, String> {
    match text.trim().parse::<u16>() {
        Ok(status) if (100..=599).contains(&status) => Ok(status),
        _ => Err("Status must be between 100 and 599".into()),
    }
}

/// Mock server of one collection: the editable definitions and, while it
/// runs, the server handle (dropping it stops the server) and its log.
#[derive(Debug)]
pub struct MockState {
    pub collection: CollectionId,
    pub draft: CollectionMocks,
    pub port_text: String,
    /// Saved request whose mock responses are being edited.
    pub selected: Option<ItemId>,
    pub server: Option<MockServer>,
    pub starting: bool,
    pub error: Option<String>,
    /// Newest first.
    pub log: VecDeque<MockLogEntry>,
    /// Bumped on each start so log lines of an old server are ignored.
    pub session: u64,
    /// Edited since last saved.
    pub dirty: bool,
    pub loaded: bool,
}

impl MockState {
    #[must_use]
    pub fn new(collection: CollectionId) -> Self {
        let draft = CollectionMocks::default();
        Self {
            collection,
            port_text: draft.port.to_string(),
            draft,
            selected: None,
            server: None,
            starting: false,
            error: None,
            log: VecDeque::new(),
            session: 0,
            dirty: false,
            loaded: false,
        }
    }

    /// Installs definitions loaded from the store (unless already edited).
    pub fn set_loaded(&mut self, mocks: CollectionMocks) {
        if !self.dirty {
            self.port_text = mocks.port.to_string();
            self.draft = mocks;
        }
        self.loaded = true;
    }

    pub fn push_log(&mut self, entry: MockLogEntry) {
        self.log.push_front(entry);
        self.log.truncate(MOCK_LOG_LIMIT);
    }

    /// Appends a mock response for `item` and returns its id.
    pub fn add_mock(&mut self, item: ItemId) -> MockId {
        let n = self.draft.for_item(item).count() + 1;
        let mut mock = MockResponse::for_item(item);
        mock.name = format!("Response {n}");
        let id = mock.id;
        self.draft.responses.push(mock);
        self.dirty = true;
        id
    }

    pub fn edit_mock(&mut self, id: MockId, f: impl FnOnce(&mut MockResponse)) {
        if let Some(m) = self.draft.responses.iter_mut().find(|m| m.id == id) {
            f(m);
            self.dirty = true;
        }
    }

    pub fn remove_mock(&mut self, id: MockId) {
        let before = self.draft.responses.len();
        self.draft.responses.retain(|m| m.id != id);
        self.dirty |= self.draft.responses.len() != before;
    }

    pub fn set_port_text(&mut self, text: String) {
        if let Ok(port) = parse_port(&text) {
            self.draft.port = port;
        }
        self.port_text = text;
        self.dirty = true;
    }

    #[must_use]
    pub fn running_url(&self) -> Option<String> {
        self.server.as_ref().map(MockServer::base_url)
    }
}

/// The webhook inspector (one per app).
#[derive(Debug)]
pub struct WebhookState {
    pub port_text: String,
    pub status_text: String,
    pub content_type: String,
    pub body: String,
    pub inspector: Option<WebhookInspector>,
    /// Snapshot of the inspector's log, newest first.
    pub captures: Vec<CapturedRequest>,
    pub selected: Option<u64>,
    pub starting: bool,
    pub error: Option<String>,
    pub session: u64,
}

impl Default for WebhookState {
    fn default() -> Self {
        let reply = WebhookReply::default();
        Self {
            port_text: DEFAULT_WEBHOOK_PORT.to_string(),
            status_text: reply.status.to_string(),
            content_type: reply.content_type,
            body: reply.body,
            inspector: None,
            captures: Vec::new(),
            selected: None,
            starting: false,
            error: None,
            session: 0,
        }
    }
}

impl WebhookState {
    /// The reply described by the form.
    pub fn reply(&self) -> Result<WebhookReply, String> {
        Ok(WebhookReply {
            status: parse_status(&self.status_text)?,
            content_type: self.content_type.trim().to_owned(),
            body: self.body.clone(),
        })
    }

    /// The selected capture, or the newest when none is selected.
    #[must_use]
    pub fn current(&self) -> Option<&CapturedRequest> {
        match self.selected {
            Some(id) => self.captures.iter().find(|c| c.id == id),
            None => self.captures.first(),
        }
    }

    /// Re-reads the inspector's log; drops a selection that fell out of it.
    pub fn refresh(&mut self) {
        if let Some(inspector) = &self.inspector {
            self.captures = inspector.captures();
        }
        if let Some(id) = self.selected
            && !self.captures.iter().any(|c| c.id == id)
        {
            self.selected = None;
        }
    }
}

/// The load-test dialog: form, and the current or last run.
#[derive(Debug, Clone)]
pub struct LoadTestState {
    pub tab_id: TabId,
    pub title: String,
    pub total_text: String,
    pub concurrency_text: String,
    pub duration_text: String,
    /// Shown when the target is not localhost.
    pub warning: Option<String>,
    pub run: u64,
    pub running: bool,
    pub cancel: Option<CancellationToken>,
    pub config: Option<LoadConfig>,
    pub tally: Option<LoadTally>,
    pub end: Option<LoadEnd>,
    pub error: Option<String>,
}

impl LoadTestState {
    #[must_use]
    pub fn new(tab_id: TabId, title: String, warning: Option<String>) -> Self {
        Self {
            tab_id,
            title,
            total_text: "100".into(),
            concurrency_text: "10".into(),
            duration_text: String::new(),
            warning,
            run: 0,
            running: false,
            cancel: None,
            config: None,
            tally: None,
            end: None,
            error: None,
        }
    }

    /// Validates the form and marks a new run started.
    pub fn begin(&mut self, run: u64, cancel: CancellationToken) -> Result<LoadConfig, String> {
        let config = LoadConfig::parse(
            &self.total_text,
            &self.concurrency_text,
            &self.duration_text,
        )?;
        self.run = run;
        self.running = true;
        self.cancel = Some(cancel);
        self.config = Some(config);
        self.tally = Some(LoadTally::default());
        self.end = None;
        self.error = None;
        Ok(config)
    }

    pub fn apply(&mut self, event: LoadEvent) {
        match event {
            LoadEvent::Progress(tally) => self.tally = Some(tally),
            LoadEvent::Done(tally, end) => {
                self.tally = Some(tally);
                self.end = Some(end);
                self.running = false;
                self.cancel = None;
            }
        }
    }

    pub fn stop(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }

    /// Completed share of the configured total, 0.0 ..= 1.0.
    #[must_use]
    pub fn progress(&self) -> f32 {
        match (&self.tally, self.config) {
            (Some(t), Some(c)) if c.total > 0 => (t.completed() as f32 / c.total as f32).min(1.0),
            _ => 0.0,
        }
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
