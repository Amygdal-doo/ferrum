//! The detail panel's state: one container or a whole compose project.

use super::LOG_CAPACITY;
use ferrumdock_engine::ContainerDetail;
use ferrumdock_engine::compose::ComposeProject;
use ferrumdock_engine::compose_ps::ServiceStatus;
use ferrumdock_engine::logs::{LogBuffer, Since};
use ferrumdock_engine::shell::Shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Logs,
    Inspect,
    Stats,
    /// `docker compose ps` of a project.
    Services,
}

/// The key of a compose project's panel in [`DetailState::id`].
pub fn project_key(name: &str) -> String {
    format!("project:{name}")
}

/// The detail panel: one container, or a whole compose project (combined logs and `ps`).
pub struct DetailState {
    /// The container id, or [`project_key`] for a project.
    pub id: String,
    pub project: Option<ComposeProject>,
    /// Containers whose logs are being followed (the project's, or just `id`).
    pub sources: Vec<String>,
    pub tab: DetailTab,
    pub logs: LogBuffer,
    /// Logs are still streaming.
    pub streaming: bool,
    pub log_error: Option<String>,
    /// Keep the view scrolled to the newest line.
    pub follow: bool,
    pub timestamps: bool,
    /// Render ANSI colours (else show plain text).
    pub colors: bool,
    pub search: String,
    /// `search` is a regular expression.
    pub regex: bool,
    /// The regular expression did not compile (every line is shown).
    pub search_error: Option<String>,
    pub since: Since,
    /// Line indexes matching `search`; `None` shows every line.
    pub matches: Option<Vec<usize>>,
    pub inspect: Option<Result<ContainerDetail, String>>,
    /// Shell for "Open shell", and the ones the container has (once asked).
    pub shell: Shell,
    pub shells: Option<Vec<Shell>>,
    pub services: Option<Result<Vec<ServiceStatus>, String>>,
    /// Container refreshes that may still miss the container before the
    /// panel closes (a just-created one can be absent from an older poll).
    pub grace: u8,
}

impl DetailState {
    pub fn new(id: String) -> Self {
        Self {
            sources: vec![id.clone()],
            id,
            project: None,
            tab: DetailTab::Logs,
            logs: LogBuffer::new(LOG_CAPACITY, true),
            streaming: true,
            log_error: None,
            follow: true,
            timestamps: false,
            colors: true,
            search: String::new(),
            regex: false,
            search_error: None,
            since: Since::default(),
            matches: None,
            inspect: None,
            shell: Shell::default(),
            shells: None,
            services: None,
            grace: 0,
        }
    }

    /// The combined panel of a compose project following `sources`.
    pub fn for_project(project: ComposeProject, sources: Vec<String>) -> Self {
        Self {
            sources,
            project: Some(project.clone()),
            ..Self::new(project_key(&project.name))
        }
    }

    /// Recomputes `matches` after the search text or the lines changed.
    pub fn refilter(&mut self) {
        if self.search.trim().is_empty() {
            self.matches = None;
            self.search_error = None;
            return;
        }
        match self.logs.search_with(&self.search, self.regex) {
            Ok(matches) => {
                self.matches = Some(matches);
                self.search_error = None;
            }
            Err(error) => {
                self.matches = None;
                self.search_error = Some(error);
            }
        }
    }

    pub fn visible_count(&self) -> usize {
        self.matches.as_ref().map_or(self.logs.len(), Vec::len)
    }

    /// Buffer index of the `row`-th visible line.
    pub fn line_index(&self, row: usize) -> Option<usize> {
        match &self.matches {
            Some(m) => m.get(row).copied(),
            None => (row < self.logs.len()).then_some(row),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_log_lines() {
        use ferrumdock_engine::{LogChunk, LogStream};
        let mut detail = DetailState::new("1".into());
        detail.logs.push(&LogChunk {
            stream: LogStream::Stdout,
            text:
                "2024-05-01T10:00:00Z boot\n2024-05-01T10:00:01Z ERROR x\n2024-05-01T10:00:02Z ok\n"
                    .into(),
        });
        detail.refilter();
        assert_eq!(detail.visible_count(), 3);
        assert_eq!(detail.line_index(2), Some(2));
        assert_eq!(detail.line_index(3), None);
        detail.search = "error".into();
        detail.refilter();
        assert_eq!(detail.visible_count(), 1);
        assert_eq!(detail.line_index(0), Some(1));
    }

    #[test]
    fn regex_search_reports_bad_patterns_and_shows_all_lines() {
        use ferrumdock_engine::{LogChunk, LogStream};
        let mut detail = DetailState::new("1".into());
        detail.logs.push(&LogChunk {
            stream: LogStream::Stdout,
            text: "GET /a 200\nGET /b 500\n".into(),
        });
        detail.regex = true;
        detail.search = r"5\d\d".into();
        detail.refilter();
        assert_eq!(detail.matches, Some(vec![1]));
        detail.search = "(".into();
        detail.refilter();
        assert!(detail.search_error.is_some());
        assert_eq!(
            detail.visible_count(),
            2,
            "an invalid pattern hides nothing"
        );
        detail.search.clear();
        detail.refilter();
        assert!(detail.search_error.is_none() && detail.matches.is_none());
    }

    #[test]
    fn project_panels_follow_every_service() {
        let project = ComposeProject {
            name: "web".into(),
            working_dir: None,
            config_files: Vec::new(),
        };
        let detail = DetailState::for_project(project, vec!["1".into(), "2".into()]);
        assert_eq!(detail.id, "project:web");
        assert_eq!(detail.sources, ["1", "2"]);
        assert_eq!(DetailState::new("abc".into()).sources, ["abc"]);
    }
}
