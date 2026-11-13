//! Every file the agent writes goes through a [`ChangeSet`]: the write is
//! recorded against the session's [`Snapshot`] (so it shows up in the
//! review with per-hunk accept/reject, see [`crate::review`]) and in the
//! current turn's [`Checkpoint`] (the file as it was before the turn's
//! first write), so a turn can be rolled back.
//!
//! Restoring a checkpoint puts every file touched since back as it was
//! before that turn and drops the later checkpoints; the restore itself is
//! undoable until the next turn starts.
//!
//! Paths are relative to the project root. Writes made outside the change
//! set (e.g. by a shell command) still appear in the review, since it
//! diffs the disk against the snapshot, but checkpoints do not cover them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::review::{self, FileChange, ReviewError, Written};
use crate::snapshot::{Snapshot, read};

pub type CheckpointId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub id: CheckpointId,
    pub label: String,
    pub created: DateTime<Utc>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub label: String,
    pub created: DateTime<Utc>,
    /// Content before this turn's first write (`None`: did not exist).
    before: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

impl Checkpoint {
    pub fn info(&self) -> CheckpointInfo {
        CheckpointInfo {
            id: self.id,
            label: self.label.clone(),
            created: self.created,
            files: self.before.keys().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone)]
struct RestoreUndo {
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    removed: Vec<Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChangeError {
    #[error("no checkpoint {0}")]
    NoCheckpoint(CheckpointId),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("{0}")]
    Io(String),
}

#[derive(Debug)]
pub struct ChangeSet {
    snapshot: Snapshot,
    checkpoints: Vec<Checkpoint>,
    next_id: CheckpointId,
    undo: Option<RestoreUndo>,
}

impl ChangeSet {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            snapshot,
            checkpoints: Vec::new(),
            next_id: 1,
            undo: None,
        }
    }

    /// A git snapshot of `root` when it is in a repository, else a copy of
    /// its text files. Blocks.
    pub fn take(root: &Path) -> Result<Self, String> {
        Snapshot::take(root).map(Self::new)
    }

    pub fn root(&self) -> &Path {
        &self.snapshot.root
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Starts a turn's checkpoint. The previous restore can no longer be
    /// undone.
    pub fn begin_checkpoint(&mut self, label: impl Into<String>) -> CheckpointId {
        let id = self.next_id;
        self.next_id += 1;
        self.undo = None;
        self.checkpoints.push(Checkpoint {
            id,
            label: label.into(),
            created: Utc::now(),
            before: BTreeMap::new(),
        });
        id
    }

    pub fn checkpoints(&self) -> Vec<CheckpointInfo> {
        self.checkpoints.iter().map(Checkpoint::info).collect()
    }

    pub fn read(&self, rel: &Path) -> Option<Vec<u8>> {
        read(&self.root().join(rel))
    }

    /// Writes (`Some`) or deletes (`None`) `rel`, recording what it held
    /// before in the current checkpoint.
    pub fn write(&mut self, rel: &Path, content: Option<&[u8]>) -> Result<(), ChangeError> {
        if self.checkpoints.is_empty() {
            self.begin_checkpoint("edits");
        }
        let previous = self.read(rel);
        if let Some(checkpoint) = self.checkpoints.last_mut() {
            checkpoint
                .before
                .entry(rel.to_path_buf())
                .or_insert(previous);
        }
        put(self.root(), rel, content)
    }

    pub fn changes(&self) -> Result<Vec<FileChange>, String> {
        review::changes(&self.snapshot)
    }

    pub fn file_change(&self, rel: &Path) -> Option<FileChange> {
        review::file_change(&self.snapshot, rel)
    }

    pub fn accept_hunk(&mut self, rel: &Path, hunk: u64, hash: u64) -> Result<(), ReviewError> {
        review::accept_hunk_in(&mut self.snapshot, rel, hunk, hash)
    }

    pub fn reject_hunk(
        &mut self,
        rel: &Path,
        hunk: u64,
        hash: u64,
    ) -> Result<Written, ReviewError> {
        review::reject_hunk_in(&mut self.snapshot, rel, hunk, hash)
    }

    pub fn accept_file(&mut self, rel: &Path, hash: u64) -> Result<(), ReviewError> {
        review::accept_file_in(&mut self.snapshot, rel, hash)
    }

    pub fn reject_file(&mut self, rel: &Path, hash: u64) -> Result<Written, ReviewError> {
        review::reject_file_in(&mut self.snapshot, rel, hash)
    }

    /// Keeps every change. Returns how many files were accepted.
    pub fn accept_all(&mut self) -> Result<usize, String> {
        let changes = self.changes()?;
        for change in &changes {
            self.accept_file(&change.path, change.current_hash)
                .map_err(|e| e.to_string())?;
        }
        Ok(changes.len())
    }

    /// Undoes every change on disk. Returns how many files were restored.
    pub fn reject_all(&mut self) -> Result<usize, String> {
        let changes = self.changes()?;
        for change in &changes {
            self.reject_file(&change.path, change.current_hash)
                .map_err(|e| e.to_string())?;
        }
        Ok(changes.len())
    }

    /// Puts every file touched in checkpoint `id` or later back as it was
    /// before `id`, and drops those checkpoints. Returns the files written.
    pub fn restore(&mut self, id: CheckpointId) -> Result<Vec<PathBuf>, ChangeError> {
        let at = self
            .checkpoints
            .iter()
            .position(|c| c.id == id)
            .ok_or(ChangeError::NoCheckpoint(id))?;
        let removed: Vec<Checkpoint> = self.checkpoints.drain(at..).collect();
        let mut target: BTreeMap<PathBuf, Option<Vec<u8>>> = BTreeMap::new();
        for checkpoint in &removed {
            for (path, before) in &checkpoint.before {
                target.entry(path.clone()).or_insert_with(|| before.clone());
            }
        }
        let current: BTreeMap<PathBuf, Option<Vec<u8>>> = target
            .keys()
            .map(|path| (path.clone(), self.read(path)))
            .collect();
        for (path, content) in &target {
            put(self.root(), path, content.as_deref())?;
        }
        self.undo = Some(RestoreUndo {
            files: current,
            removed,
        });
        Ok(target.into_keys().collect())
    }

    pub fn can_undo_restore(&self) -> bool {
        self.undo.is_some()
    }

    /// Takes back the last [`ChangeSet::restore`]: files and checkpoints
    /// return to how they were.
    pub fn undo_restore(&mut self) -> Result<Vec<PathBuf>, ChangeError> {
        let undo = self.undo.take().ok_or(ChangeError::NothingToUndo)?;
        for (path, content) in &undo.files {
            put(self.root(), path, content.as_deref())?;
        }
        self.checkpoints.extend(undo.removed);
        Ok(undo.files.into_keys().collect())
    }
}

fn put(root: &Path, rel: &Path, content: Option<&[u8]>) -> Result<(), ChangeError> {
    let path = root.join(rel);
    let io = |e: std::io::Error| ChangeError::Io(format!("{}: {e}", rel.display()));
    match content {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            crate::fsutil::write_atomic(&path, bytes).map_err(io)
        }
        None if path.is_file() => std::fs::remove_file(&path).map_err(io),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::FileStatus;

    fn text(root: &Path, rel: &str) -> Option<String> {
        std::fs::read_to_string(root.join(rel)).ok()
    }

    #[test]
    fn checkpoints_restore_and_undo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::write(root.join("a.txt"), "a0\n").expect("write");
        std::fs::write(root.join("keep.txt"), "k\n").expect("write");
        let mut set = ChangeSet::new(Snapshot::take_files(&root).expect("snapshot"));

        let first = set.begin_checkpoint("turn 1");
        set.write(Path::new("a.txt"), Some(b"a1\n")).expect("write");
        set.write(Path::new("a.txt"), Some(b"a1b\n"))
            .expect("write again");
        set.write(Path::new("new/b.txt"), Some(b"b1\n"))
            .expect("create");
        let second = set.begin_checkpoint("turn 2");
        set.write(Path::new("a.txt"), Some(b"a2\n")).expect("write");
        set.write(Path::new("keep.txt"), None).expect("delete");
        assert_eq!(set.checkpoints().len(), 2);
        assert_eq!(
            set.checkpoints()[0].files,
            [PathBuf::from("a.txt"), PathBuf::from("new/b.txt")]
        );

        // Back to before turn 2: a1b, keep.txt returns, b stays.
        let files = set.restore(second).expect("restore");
        assert_eq!(files, [PathBuf::from("a.txt"), PathBuf::from("keep.txt")]);
        assert_eq!(text(&root, "a.txt").as_deref(), Some("a1b\n"));
        assert_eq!(text(&root, "keep.txt").as_deref(), Some("k\n"));
        assert_eq!(set.checkpoints().len(), 1);

        // The restore is undoable.
        assert!(set.can_undo_restore());
        set.undo_restore().expect("undo");
        assert_eq!(text(&root, "a.txt").as_deref(), Some("a2\n"));
        assert_eq!(text(&root, "keep.txt"), None);
        assert_eq!(set.checkpoints().len(), 2);
        assert_eq!(set.undo_restore(), Err(ChangeError::NothingToUndo));

        // Back to before turn 1: everything as the snapshot had it.
        set.restore(first).expect("restore all");
        assert_eq!(text(&root, "a.txt").as_deref(), Some("a0\n"));
        assert_eq!(text(&root, "new/b.txt"), None);
        assert!(set.changes().expect("changes").is_empty());
        assert_eq!(set.restore(99), Err(ChangeError::NoCheckpoint(99)));

        // A new turn ends the undo window.
        set.begin_checkpoint("turn 3");
        assert!(!set.can_undo_restore());
    }

    #[test]
    fn writes_are_reviewable_per_hunk_and_in_bulk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        let base = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n";
        std::fs::write(root.join("f.txt"), base).expect("write");
        let mut set = ChangeSet::new(Snapshot::take_files(&root).expect("snapshot"));
        set.begin_checkpoint("t");
        set.write(
            Path::new("f.txt"),
            Some(b"1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\nELEVEN\n12\n"),
        )
        .expect("write");
        set.write(Path::new("g.txt"), Some(b"g\n")).expect("write");
        let changes = set.changes().expect("changes");
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].hunks.len(), 2);
        assert_eq!(changes[1].status, FileStatus::Added);

        let f = &changes[0];
        set.accept_hunk(&f.path, f.hunks[0].id, f.current_hash)
            .expect("accept");
        let f = set.file_change(Path::new("f.txt")).expect("still changed");
        set.reject_hunk(&f.path, f.hunks[0].id, f.current_hash)
            .expect("reject");
        assert_eq!(
            text(&root, "f.txt").as_deref(),
            Some("1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n")
        );
        assert_eq!(set.reject_all().expect("reject all"), 1);
        assert_eq!(text(&root, "g.txt"), None);
        set.write(Path::new("h.txt"), Some(b"h\n")).expect("write");
        assert_eq!(set.accept_all().expect("accept all"), 1);
        assert!(set.changes().expect("changes").is_empty());
    }
}
