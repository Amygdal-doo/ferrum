//! Threads persisted per project as JSON files under
//! `~/.ferrum/agent/threads/<project>-<hash>/<thread id>.json`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::thread::Thread;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub agent: String,
    pub updated: DateTime<Utc>,
    pub messages: usize,
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
                Some(ThreadSummary {
                    id: thread.id,
                    title: thread.title,
                    agent: thread.agent,
                    updated: thread.updated,
                    messages: thread.messages.len(),
                })
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
        assert_eq!(store.load(&a.id).expect("load"), a);
        store.delete(&a.id).expect("delete");
        assert_eq!(store.list().len(), 1);
        assert!(store.load("../escape").is_err());
    }
}
