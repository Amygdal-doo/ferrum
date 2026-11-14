//! Threads persisted per project as JSON files under
//! `~/.ferrum/agent/threads/<project>-<hash>/<thread id>.json`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::modes::Mode;
use crate::thread::Thread;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub agent: String,
    pub updated: DateTime<Utc>,
    pub messages: usize,
    #[serde(default)]
    pub mode: Mode,
    /// Input plus output tokens over every request.
    #[serde(default)]
    pub tokens: u64,
}

impl ThreadSummary {
    pub fn of(thread: &Thread) -> Self {
        Self {
            id: thread.id.clone(),
            title: thread.title.clone(),
            agent: thread.agent.clone(),
            updated: thread.updated,
            messages: thread.messages.len(),
            mode: thread.mode,
            tokens: thread.usage.input_tokens + thread.usage.output_tokens,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThreadStore {
    dir: PathBuf,
}

impl ThreadStore {
    pub fn new(home: &Path, project: &Path) -> Self {
        Self {
            dir: crate::paths::threads_dir(home, project),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, id: &str) -> Option<PathBuf> {
        let safe = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        safe.then(|| self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, thread: &Thread) -> std::io::Result<()> {
        let file = self
            .file(&thread.id)
            .ok_or_else(|| std::io::Error::other("bad thread id"))?;
        std::fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec(thread).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(&file, &json)
    }

    pub fn load(&self, id: &str) -> std::io::Result<Thread> {
        let file = self
            .file(id)
            .ok_or_else(|| std::io::Error::other("bad thread id"))?;
        let text = std::fs::read(file)?;
        serde_json::from_slice(&text).map_err(std::io::Error::other)
    }

    /// Retitles a saved thread (its `updated` time is kept).
    pub fn rename(&self, id: &str, title: &str) -> std::io::Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(std::io::Error::other("empty title"));
        }
        let mut thread = self.load(id)?;
        thread.title = title.to_string();
        self.save(&thread)
    }

    pub fn delete(&self, id: &str) -> std::io::Result<()> {
        match self.file(id) {
            Some(file) if file.exists() => std::fs::remove_file(file),
            _ => Ok(()),
        }
    }

    /// Every thread, most recently updated first.
    pub fn list(&self) -> Vec<ThreadSummary> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<ThreadSummary> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| {
                let thread: Thread = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
                Some(ThreadSummary::of(&thread))
            })
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.updated));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::Mode;
    use crate::thread::ThreadEvent;

    #[test]
    fn save_list_load_delete() {
        let home = tempfile::tempdir().expect("tempdir");
        let store = ThreadStore::new(home.path(), Path::new("/work/my app"));
        assert!(
            store
                .dir()
                .ends_with(store.dir().file_name().map(PathBuf::from).expect("name"))
        );
        assert!(store.dir().to_string_lossy().contains("threads/my_app-"));
        let mut a = Thread::new(
            "/work/my app".into(),
            "anthropic/claude-sonnet-5-5",
            Mode::Agent,
        );
        a.apply(&ThreadEvent::UserMessage {
            id: 1,
            text: "first".into(),
            attachments: vec![],
            checkpoint: None,
        });
        store.save(&a).expect("save");
        let mut b = Thread::new("/work/my app".into(), "acp:claude", Mode::Ask);
        b.apply(&ThreadEvent::TitleChanged("second".into()));
        store.save(&b).expect("save");
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "second");
        assert_eq!(list[0].mode, Mode::Ask);
        assert_eq!(list[1].mode, Mode::Agent);
        assert_eq!(store.load(&a.id).expect("load"), a);
        store.rename(&a.id, "  Renamed  ").expect("rename");
        let renamed = store.load(&a.id).expect("load");
        assert_eq!(renamed.title, "Renamed");
        assert_eq!(renamed.updated, a.updated);
        assert!(store.rename(&a.id, " ").is_err());
        assert!(store.rename("missing", "x").is_err());
        store.delete(&a.id).expect("delete");
        assert_eq!(store.list().len(), 1);
        assert!(store.load("../escape").is_err());
    }

    #[test]
    fn summaries_from_before_mode_and_tokens_load() {
        let old = r#"{"id":"a","title":"t","agent":"acp:claude",
            "updated":"2026-01-02T03:04:05Z","messages":3}"#;
        let summary: ThreadSummary = serde_json::from_str(old).expect("old summary");
        assert_eq!(summary.mode, Mode::Agent);
        assert_eq!(summary.tokens, 0);
        let mut thread = Thread::new("/p".into(), "anthropic/m", Mode::Plan);
        thread.apply(&ThreadEvent::Usage {
            usage: crate::provider::Usage {
                input_tokens: 100,
                output_tokens: 20,
                cache_read_tokens: 5,
            },
            context_tokens: 100,
        });
        let summary = ThreadSummary::of(&thread);
        assert_eq!((summary.mode, summary.tokens), (Mode::Plan, 120));
    }
}
