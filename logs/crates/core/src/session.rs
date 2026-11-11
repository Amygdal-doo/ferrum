//! One project's live state: its sources and what they are doing, the
//! merged line history, counters and the filtered view.

use crate::buffer::{DEFAULT_CAPACITY, LogBuffer};
use crate::filter::{Filter, Matcher, View};
use crate::level::Level;
use crate::line::{LogLine, Stream};
use crate::runner::{self, Event, EventSender, RunHandle, RunId, RunSpec, RunStatus};
use crate::source::SourceConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Starts sources: the event channel plus how commands are run.
#[derive(Debug, Clone)]
pub struct Launcher {
    pub tx: EventSender,
    pub shell: PathBuf,
    pub login: bool,
    pub home: Option<PathBuf>,
    next_run: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Launcher {
    #[must_use]
    pub fn new(tx: EventSender, shell: PathBuf, login: bool, home: Option<PathBuf>) -> Self {
        Self {
            tx,
            shell,
            login,
            home,
            next_run: std::sync::Arc::default(),
        }
    }

    fn launch(&self, slot: u32, config: &SourceConfig, project: &Path) -> RunHandle {
        let run = self
            .next_run
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        runner::spawn(
            RunSpec {
                run,
                slot,
                kind: config.kind.clone(),
                project: project.to_path_buf(),
                shell: self.shell.clone(),
                login: self.login,
                home: self.home.clone(),
            },
            self.tx.clone(),
        )
    }
}

/// The sidebar's status dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Idle,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug)]
pub struct SourceState {
    pub config: SourceConfig,
    /// Written into each of its lines; stable for the session.
    pub slot: u32,
    pub status: Option<RunStatus>,
    pub errors: u64,
    pub warns: u64,
    pub lines: u64,
    handle: Option<RunHandle>,
}

impl SourceState {
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.handle.is_some() && !matches!(&self.status, Some(s) if !s.is_running())
    }

    #[must_use]
    pub fn health(&self) -> Health {
        match &self.status {
            _ if self.is_running() => Health::Running,
            None => Health::Idle,
            Some(status) if status.is_failure() => Health::Failed,
            Some(_) => Health::Stopped,
        }
    }

    /// `running`, `exited 1`, `stopped`, …
    #[must_use]
    pub fn status_text(&self) -> String {
        if self.is_running() {
            return "running".into();
        }
        match &self.status {
            None => "not started".into(),
            Some(RunStatus::Running) => "starting".into(),
            Some(RunStatus::Exited(Some(code))) => format!("exited {code}"),
            Some(RunStatus::Exited(None)) => "killed".into(),
            Some(RunStatus::Stopped) => "stopped".into(),
            Some(RunStatus::Failed(e)) => format!("failed: {e}"),
        }
    }
}

pub struct Session {
    pub project: PathBuf,
    pub sources: Vec<SourceState>,
    pub buffer: LogBuffer,
    pub view: View,
    filter: Filter,
    matcher: Matcher,
    /// Why the current query is not applied (bad regex).
    pub filter_error: Option<String>,
    next_slot: u32,
    runs: HashMap<RunId, u32>,
    /// Last level seen per source stream, for indented continuation lines
    /// (stack traces) that have none of their own.
    last_level: HashMap<(u32, Stream), Level>,
}

impl Session {
    #[must_use]
    pub fn new(project: &Path, sources: Vec<SourceConfig>) -> Self {
        Self::with_capacity(project, sources, DEFAULT_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(project: &Path, sources: Vec<SourceConfig>, capacity: usize) -> Self {
        let mut session = Self {
            project: project.to_path_buf(),
            sources: Vec::new(),
            buffer: LogBuffer::new(capacity),
            view: View::default(),
            filter: Filter::default(),
            matcher: Matcher::all(),
            filter_error: None,
            next_slot: 0,
            runs: HashMap::new(),
            last_level: HashMap::new(),
        };
        session.set_sources(sources);
        session
    }

    /// Takes a new source list (after add/edit/remove), keeping the state
    /// of sources that remain; removed sources are stopped.
    pub fn set_sources(&mut self, configs: Vec<SourceConfig>) {
        let mut old: Vec<SourceState> = std::mem::take(&mut self.sources);
        for config in configs {
            match old.iter().position(|s| s.config.id == config.id) {
                Some(index) => {
                    let mut state = old.remove(index);
                    state.config = config;
                    self.sources.push(state);
                }
                None => {
                    let slot = self.next_slot;
                    self.next_slot += 1;
                    self.sources.push(SourceState {
                        config,
                        slot,
                        status: None,
                        errors: 0,
                        warns: 0,
                        lines: 0,
                        handle: None,
                    });
                }
            }
        }
        for removed in old {
            if let Some(handle) = removed.handle {
                handle.stop();
            }
        }
    }

    #[must_use]
    pub fn configs(&self) -> Vec<SourceConfig> {
        self.sources.iter().map(|s| s.config.clone()).collect()
    }

    #[must_use]
    pub fn source(&self, id: &str) -> Option<&SourceState> {
        self.sources.iter().find(|s| s.config.id == id)
    }

    #[must_use]
    pub fn by_slot(&self, slot: u32) -> Option<&SourceState> {
        self.sources.iter().find(|s| s.slot == slot)
    }

    /// Whether `run` was started by this session (events are routed by it).
    #[must_use]
    pub fn owns_run(&self, run: RunId) -> bool {
        self.runs.contains_key(&run)
    }

    #[must_use]
    pub fn running_count(&self) -> usize {
        self.sources.iter().filter(|s| s.is_running()).count()
    }

    /// Starts (or restarts) source `id`.
    pub fn start(&mut self, id: &str, launcher: &Launcher) {
        let project = self.project.clone();
        let Some(state) = self.sources.iter_mut().find(|s| s.config.id == id) else {
            return;
        };
        if let Some(handle) = state.handle.take() {
            handle.stop();
        }
        let handle = launcher.launch(state.slot, &state.config, &project);
        self.runs.insert(handle.run, state.slot);
        state.status = Some(RunStatus::Running);
        state.handle = Some(handle);
        let what = match state.config.kind.command_line() {
            Some(line) => format!("$ {line}"),
            None => format!("following {}", state.config.kind.target()),
        };
        let slot = state.slot;
        self.push(LogLine::system(slot, what, None));
        self.view.update(&self.buffer, &self.matcher);
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(state) = self.sources.iter_mut().find(|s| s.config.id == id)
            && let Some(handle) = &state.handle
        {
            handle.stop();
        }
    }

    /// Starts every source that isn't running.
    pub fn start_all(&mut self, launcher: &Launcher) {
        let ids: Vec<String> = self
            .sources
            .iter()
            .filter(|s| !s.is_running())
            .map(|s| s.config.id.clone())
            .collect();
        for id in ids {
            self.start(&id, launcher);
        }
    }

    pub fn stop_all(&mut self) {
        for state in &self.sources {
            if let Some(handle) = &state.handle {
                handle.stop();
            }
        }
    }

    /// Applies a batch from the runners; returns how many lines became visible.
    pub fn apply(&mut self, events: Vec<Event>) -> usize {
        for event in events {
            match event {
                Event::Lines { run, lines } => {
                    if self.runs.contains_key(&run) {
                        for line in lines {
                            self.push(line);
                        }
                    }
                }
                Event::Status { run, status } => self.apply_status(run, status),
            }
        }
        self.view.update(&self.buffer, &self.matcher)
    }

    fn apply_status(&mut self, run: RunId, status: RunStatus) {
        let Some(&slot) = self.runs.get(&run) else {
            return;
        };
        let Some(state) = self.sources.iter_mut().find(|s| s.slot == slot) else {
            return;
        };
        // A restarted source's old run reports late: ignore it.
        if state.handle.as_ref().map(|h| h.run) != Some(run) {
            return;
        }
        let note = match &status {
            RunStatus::Running => None,
            RunStatus::Exited(Some(0)) => Some(("exited with code 0".to_owned(), None)),
            RunStatus::Exited(Some(code)) => {
                Some((format!("exited with code {code}"), Some(Level::Error)))
            }
            RunStatus::Exited(None) => Some(("killed by a signal".to_owned(), Some(Level::Error))),
            RunStatus::Stopped => Some(("stopped".to_owned(), None)),
            RunStatus::Failed(e) => Some((format!("failed to start: {e}"), Some(Level::Error))),
        };
        if !status.is_running() {
            state.handle = None;
        }
        state.status = Some(status);
        if let Some((text, level)) = note {
            self.push(LogLine::system(slot, text, level));
        }
    }

    fn push(&mut self, mut line: LogLine) {
        let key = (line.source, line.stream);
        match line.level {
            Some(level) => {
                self.last_level.insert(key, level);
            }
            None if line.stream != Stream::System && is_continuation(&line.text) => {
                line.level = self.last_level.get(&key).copied();
            }
            None => {
                self.last_level.remove(&key);
            }
        }
        if let Some(state) = self.sources.iter_mut().find(|s| s.slot == line.source) {
            state.lines += 1;
            // Continuations share their first line's count.
            if line.stream == Stream::System || !is_continuation(&line.text) {
                match line.level {
                    Some(Level::Error) => state.errors += 1,
                    Some(Level::Warn) => state.warns += 1,
                    _ => {}
                }
            }
        }
        self.buffer.push(line);
    }

    #[must_use]
    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    #[must_use]
    pub fn matcher(&self) -> &Matcher {
        &self.matcher
    }

    /// Applies `filter`; with an invalid regex the previous one stays.
    pub fn set_filter(&mut self, filter: Filter) {
        match filter.compile() {
            Ok(matcher) => {
                self.matcher = matcher;
                self.filter_error = None;
            }
            Err(e) => self.filter_error = Some(e),
        }
        self.filter = filter;
        self.view.rebuild(&self.buffer, &self.matcher);
    }

    pub fn update_filter(&mut self, f: impl FnOnce(&mut Filter)) {
        let mut filter = self.filter.clone();
        f(&mut filter);
        self.set_filter(filter);
    }

    /// Forgets all lines and counters.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.view.clear(&self.buffer);
        for state in &mut self.sources {
            state.errors = 0;
            state.warns = 0;
            state.lines = 0;
        }
    }

    /// The line on visible row `row`.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&LogLine> {
        self.buffer.get(self.view.seq(row)?)
    }

    /// `[source] [tag] text`, as copied and exported.
    #[must_use]
    pub fn format_line(&self, line: &LogLine) -> String {
        let mut out = String::new();
        if let Some(state) = self.by_slot(line.source) {
            out.push('[');
            out.push_str(&state.config.name);
            out.push_str("] ");
        }
        if let Some(tag) = &line.tag {
            out.push_str(tag);
            out.push_str(" | ");
        }
        out.push_str(&line.copy_text());
        out
    }

    /// Visible rows `range` (clamped), one per line.
    #[must_use]
    pub fn rows_text(&self, range: std::ops::Range<usize>) -> String {
        let end = range.end.min(self.view.len());
        let mut out = String::new();
        for row in range.start.min(end)..end {
            if let Some(line) = self.row(row) {
                out.push_str(&self.format_line(line));
                out.push('\n');
            }
        }
        out
    }

    /// Every visible line, for export.
    #[must_use]
    pub fn visible_text(&self) -> String {
        self.rows_text(0..self.view.len())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Indented lines (`    at foo()`, `\tat …`) continue the previous one.
fn is_continuation(text: &str) -> bool {
    text.starts_with([' ', '\t']) && !text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Levels;
    use crate::line::{ParseOptions, parse};
    use crate::source::SourceKind;
    use futures::StreamExt;

    fn command(name: &str, line: &str) -> SourceConfig {
        SourceConfig::new(
            name,
            SourceKind::Command {
                command: line.into(),
            },
        )
    }

    fn launcher() -> (Launcher, runner::EventReceiver) {
        let (tx, rx) = runner::channel();
        (Launcher::new(tx, PathBuf::from("/bin/sh"), false, None), rx)
    }

    /// Feeds runner events into the session until `done`.
    fn pump(
        session: &mut Session,
        rx: &mut runner::EventReceiver,
        done: impl Fn(&Session) -> bool,
    ) {
        while !done(session) {
            let Some(event) = futures::executor::block_on(rx.next()) else {
                break;
            };
            session.apply(vec![event]);
        }
    }

    #[test]
    fn runs_sources_and_counts_levels() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = command(
            "a",
            "echo 'ERROR bad'; echo '    at x()'; echo 'WARN meh'; exit 2",
        );
        let b = command("b", "echo 'INFO hi'");
        let (ids, (launcher, mut rx)) = ((a.id.clone(), b.id.clone()), launcher());
        let mut session = Session::new(dir.path(), vec![a, b]);
        session.start_all(&launcher);
        assert_eq!(session.running_count(), 2);
        pump(&mut session, &mut rx, |s| s.running_count() == 0);
        let a = session.source(&ids.0).expect("a");
        assert_eq!((a.errors, a.warns), (2, 1), "exit 2 counts as an error");
        assert_eq!(a.health(), Health::Failed);
        assert_eq!(a.status_text(), "exited 2");
        let b = session.source(&ids.1).expect("b");
        assert_eq!(b.health(), Health::Stopped);
        // The stack-trace line inherits ERROR.
        session.set_filter(Filter {
            levels: Levels::ERRORS_ONLY,
            ..Filter::default()
        });
        let texts: Vec<String> = (0..session.view.len())
            .filter_map(|r| session.row(r).map(|l| l.text.clone()))
            .collect();
        assert_eq!(texts, ["ERROR bad", "    at x()", "exited with code 2"]);
        assert!(session.visible_text().starts_with("[a] ERROR bad\n"));
        session.clear();
        assert!(session.view.is_empty());
        assert_eq!(session.source(&ids.0).map(|s| s.errors), Some(0));
    }

    #[test]
    fn restart_ignores_the_old_runs_status_and_removal_stops() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slow = command("slow", "echo up; sleep 30");
        let id = slow.id.clone();
        let (launcher, mut rx) = launcher();
        let mut session = Session::new(dir.path(), vec![slow]);
        session.start(&id, &launcher);
        pump(&mut session, &mut rx, |s| {
            s.buffer.iter().any(|l| l.text == "up")
        });
        session.start(&id, &launcher);
        // The first run's "stopped" arrives but doesn't touch the new run.
        pump(&mut session, &mut rx, |s| {
            s.buffer.iter().filter(|l| l.text == "up").count() == 2
        });
        assert!(session.source(&id).is_some_and(SourceState::is_running));
        session.set_sources(Vec::new());
        assert!(session.sources.is_empty());
        assert_eq!(session.running_count(), 0);
    }

    #[test]
    fn hidden_sources_and_bad_regex() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = Session::with_capacity(dir.path(), vec![command("a", "x")], 10);
        let slot = session.sources[0].slot;
        session.runs.insert(99, slot);
        let lines = ["one", "two"]
            .map(|t| parse(slot, Stream::Stdout, t, ParseOptions::default()))
            .to_vec();
        assert_eq!(session.apply(vec![Event::Lines { run: 99, lines }]), 2);
        // Unknown runs are ignored.
        let stray = vec![parse(slot, Stream::Stdout, "x", ParseOptions::default())];
        assert_eq!(
            session.apply(vec![Event::Lines {
                run: 5,
                lines: stray
            }]),
            0
        );
        session.update_filter(|f| {
            f.regex = true;
            f.query = "t(wo".into();
        });
        assert!(session.filter_error.is_some());
        assert_eq!(session.view.len(), 2, "previous filter kept");
        session.update_filter(|f| f.query = "t.o".into());
        assert_eq!(session.view.len(), 1);
        session.update_filter(|f| {
            f.query.clear();
            f.hidden.insert(slot);
        });
        assert!(session.view.is_empty());
        assert_eq!(session.rows_text(0..5), "");
    }
}
