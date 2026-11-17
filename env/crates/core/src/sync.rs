//! Syncing a project through a shared folder (Dropbox, iCloud Drive, a git
//! checkout…). The folder only ever holds team bundles (format v2,
//! encrypted to the project's members and you), one file per project:
//! `<project>.fenv`. Copies a sync service makes on a clash (`shop
//! (Ana's conflicted copy).fenv`, `shop 2.fenv`, `shop.sync-conflict-….fenv`)
//! are read too.
//!
//! Every change is a three-way merge against [`SyncState::base`], the
//! environments as last agreed with the folder: a key changed on one side
//! only takes that side, a key changed differently on both sides is a
//! [`Conflict`] for the user to decide. Nothing is overwritten silently.
//! Only bundles sealed by you or a member of the project are accepted (the
//! sender is authenticated by the bundle format).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bundle::{self, BundleContents};
use crate::identity::{Identity, PublicKey};
use crate::model::{Environment, Project, Variable};
use crate::{Error, Result, write_private};

/// A project's link to a shared folder (stored in the encrypted vault).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    pub folder: PathBuf,
    /// The environments as last agreed with the folder.
    #[serde(default)]
    pub base: Vec<Environment>,
    /// SHA-256 (hex) of each bundle file already merged, by file name.
    #[serde(default)]
    pub seen: BTreeMap<String, String>,
}

impl SyncState {
    #[must_use]
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            base: Vec::new(),
            seen: BTreeMap::new(),
        }
    }
}

/// `<project>.fenv`, with characters that are awkward in file names
/// replaced.
#[must_use]
pub fn file_name(project: &str) -> String {
    let stem: String = project
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches(|c| c == '.' || c == ' ');
    let stem = if stem.is_empty() { "project" } else { stem };
    format!("{stem}.{}", bundle::FILE_EXTENSION)
}

/// The main file, or a sync service's copy of it.
fn is_candidate(name: &str, main: &str) -> bool {
    let Some(stem) = main.strip_suffix(".fenv") else {
        return false;
    };
    name == main
        || (name.ends_with(".fenv")
            && name
                .strip_prefix(stem)
                .is_some_and(|rest| rest.starts_with([' ', '.', '('])))
}

#[must_use]
pub fn hash_text(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A bundle file in the folder that hasn't been merged yet.
#[derive(Debug)]
pub struct Incoming {
    pub file: String,
    pub hash: String,
    pub contents: BundleContents,
    pub sender: PublicKey,
}

/// A file that could not be used, with the reason (no values inside).
#[derive(Debug)]
pub struct Rejected {
    pub file: String,
    pub hash: String,
    pub reason: String,
}

/// Reads the project's files in the folder that changed since the last
/// merge, decrypting each as `me`. Files from a key that is neither yours
/// nor a member's, or for another project, are rejected.
pub fn read_incoming(
    state: &SyncState,
    project: &Project,
    me: &Identity,
) -> Result<(Vec<Incoming>, Vec<Rejected>)> {
    let main = file_name(&project.name);
    let my_key = me.public_key()?;
    let mut names: Vec<String> = std::fs::read_dir(&state.folder)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_candidate(name, &main))
        .collect();
    names.sort();
    let (mut incoming, mut rejected) = (Vec::new(), Vec::new());
    for file in names {
        let Ok(text) = std::fs::read_to_string(state.folder.join(&file)) else {
            continue;
        };
        let hash = hash_text(&text);
        if state.seen.get(&file) == Some(&hash) {
            continue;
        }
        let opened = bundle::parse(&text).and_then(|sealed| {
            if sealed.project != project.name {
                return Err(Error::Invalid(format!("it is for {}", sealed.project)));
            }
            let (contents, sender) = sealed.open_as(me)?;
            if sender != my_key && project.member(&sender).is_none() {
                return Err(Error::Invalid(format!(
                    "it was sealed by {}, who is not in the project's team",
                    sender.fingerprint()
                )));
            }
            Ok((contents, sender))
        });
        match opened {
            Ok((contents, sender)) => incoming.push(Incoming {
                file,
                hash,
                contents,
                sender,
            }),
            Err(e) => rejected.push(Rejected {
                file,
                hash,
                reason: e.to_string(),
            }),
        }
    }
    Ok((incoming, rejected))
}

/// Seals the project's environments to you and its members and writes
/// `<folder>/<project>.fenv` (atomically, ciphertext only). Returns the
/// file name and its hash.
pub fn write(state: &SyncState, project: &Project, me: &Identity) -> Result<(String, String)> {
    if !state.folder.is_dir() {
        return Err(Error::Invalid(format!(
            "The sync folder {} is not there (unmounted or moved?).",
            state.folder.display()
        )));
    }
    let contents = BundleContents {
        project: project.name.clone(),
        environments: project.environments.clone(),
        created_at: Utc::now(),
        note: String::new(),
    };
    let armored = bundle::seal_for(&contents, me, &project.recipients(&me.public_key()?))?;
    let file = file_name(&project.name);
    write_private(&state.folder.join(&file), armored.as_bytes())?;
    Ok((file, hash_text(&armored)))
}

/// One key changed differently on both sides since the last sync. `None`
/// means deleted on that side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub environment: String,
    pub key: String,
    pub mine: Option<Variable>,
    pub theirs: Option<Variable>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merged {
    /// The result, with your side kept for every conflict until resolved.
    pub environments: Vec<Environment>,
    pub conflicts: Vec<Conflict>,
    /// Keys taken from the other side (added, changed or removed).
    pub taken: usize,
}

fn find<'a>(envs: &'a [Environment], env: &str, key: &str) -> Option<&'a Variable> {
    envs.iter()
        .find(|e| e.name.eq_ignore_ascii_case(env))
        .and_then(|e| e.get(key))
}

/// Three-way merge of `local` and `remote` against `base`.
#[must_use]
pub fn merge(base: &[Environment], local: &[Environment], remote: &[Environment]) -> Merged {
    let mut out = Merged {
        environments: local.to_vec(),
        ..Merged::default()
    };
    // Every (environment, key) any side knows, local order first.
    let mut pairs: Vec<(String, String)> = Vec::new();
    for env in local.iter().chain(remote).chain(base) {
        for var in &env.variables {
            let pair = (env.name.clone(), var.key.clone());
            if !pairs
                .iter()
                .any(|(e, k)| e.eq_ignore_ascii_case(&pair.0) && *k == pair.1)
            {
                pairs.push(pair);
            }
        }
    }
    for env in remote {
        if !out
            .environments
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&env.name))
        {
            out.environments.push(Environment::new(env.name.clone()));
        }
    }
    for (env_name, key) in pairs {
        let b = find(base, &env_name, &key);
        let l = find(local, &env_name, &key);
        let r = find(remote, &env_name, &key);
        if l == r || r == b {
            continue;
        }
        if l == b {
            set(&mut out.environments, &env_name, &key, r.cloned());
            out.taken += 1;
        } else {
            out.conflicts.push(Conflict {
                environment: env_name,
                key,
                mine: l.cloned(),
                theirs: r.cloned(),
            });
        }
    }
    out
}

fn set(envs: &mut Vec<Environment>, env_name: &str, key: &str, value: Option<Variable>) {
    let index = match envs
        .iter()
        .position(|e| e.name.eq_ignore_ascii_case(env_name))
    {
        Some(i) => i,
        None => {
            envs.push(Environment::new(env_name));
            envs.len() - 1
        }
    };
    let env = &mut envs[index];
    match value {
        Some(var) => env.upsert(var),
        None => env.variables.retain(|v| v.key != key),
    }
}

/// Applies the user's choices (`true` = keep theirs) to a merge result.
pub fn resolve(merged: &mut Merged, take_theirs: &[bool]) {
    for (conflict, theirs) in merged.conflicts.iter().zip(take_theirs) {
        let value = if *theirs {
            conflict.theirs.clone()
        } else {
            conflict.mine.clone()
        };
        set(
            &mut merged.environments,
            &conflict.environment,
            &conflict.key,
            value,
        );
    }
    merged.conflicts.clear();
}

/// The same environments and variables, in any order.
#[must_use]
pub fn same_content(a: &[Environment], b: &[Environment]) -> bool {
    fn canonical(envs: &[Environment]) -> Vec<(String, Vec<&Variable>)> {
        let mut out: Vec<(String, Vec<&Variable>)> = envs
            .iter()
            .map(|e| {
                let mut vars: Vec<&Variable> = e.variables.iter().collect();
                vars.sort_by(|x, y| x.key.cmp(&y.key));
                (e.name.to_lowercase(), vars)
            })
            .collect();
        out.sort_by(|x, y| x.0.cmp(&y.0));
        out
    }
    canonical(a) == canonical(b)
}

/// Whether `<folder>/<project>.fenv` exists.
#[must_use]
pub fn main_file_exists(state: &SyncState, project: &str) -> bool {
    Path::new(&state.folder).join(file_name(project)).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str, pairs: &[(&str, &str)]) -> Environment {
        let mut env = Environment::new(name);
        for (k, v) in pairs {
            env.upsert(Variable::new(*k, *v));
        }
        env
    }

    fn value(envs: &[Environment], env: &str, key: &str) -> Option<String> {
        find(envs, env, key).map(|v| v.value.clone())
    }

    #[test]
    fn file_names_and_conflict_copies() {
        assert_eq!(file_name("shop"), "shop.fenv");
        assert_eq!(file_name("a/b:c"), "a_b_c.fenv");
        assert_eq!(file_name(".."), "project.fenv");
        let main = "shop.fenv";
        for ok in [
            "shop.fenv",
            "shop (Ana's conflicted copy 2026-01-02).fenv",
            "shop 2.fenv",
            "shop.sync-conflict-20260101-120000-ABC.fenv",
        ] {
            assert!(is_candidate(ok, main), "{ok}");
        }
        for no in [
            "shop-api.fenv",
            "shopping.fenv",
            "shop.fenv.tmp",
            "other.fenv",
        ] {
            assert!(!is_candidate(no, main), "{no}");
        }
    }

    #[test]
    fn three_way_merge() {
        let base = vec![env(
            "dev",
            &[("A", "1"), ("B", "2"), ("C", "3"), ("D", "4")],
        )];
        // Local: changed A, removed C, added L.
        let local = vec![env(
            "dev",
            &[("A", "10"), ("B", "2"), ("D", "4"), ("L", "x")],
        )];
        // Remote: changed B, removed D, added R, changed A differently,
        // and a new environment.
        let remote = vec![
            env("dev", &[("A", "11"), ("B", "20"), ("C", "3"), ("R", "y")]),
            env("qa", &[("Q", "q")]),
        ];
        let merged = merge(&base, &local, &remote);
        let envs = &merged.environments;
        assert_eq!(value(envs, "dev", "B").as_deref(), Some("20"));
        assert_eq!(value(envs, "dev", "C"), None, "local removal kept");
        assert_eq!(value(envs, "dev", "D"), None, "remote removal taken");
        assert_eq!(value(envs, "dev", "L").as_deref(), Some("x"));
        assert_eq!(value(envs, "dev", "R").as_deref(), Some("y"));
        assert_eq!(value(envs, "qa", "Q").as_deref(), Some("q"));
        assert_eq!(merged.taken, 4);
        assert_eq!(merged.conflicts.len(), 1);
        let conflict = &merged.conflicts[0];
        assert_eq!(
            (conflict.environment.as_str(), conflict.key.as_str()),
            ("dev", "A")
        );
        assert_eq!(conflict.mine.as_ref().map(|v| v.value.as_str()), Some("10"));
        assert_eq!(
            conflict.theirs.as_ref().map(|v| v.value.as_str()),
            Some("11")
        );
        // Until resolved, your side stays.
        assert_eq!(value(envs, "dev", "A").as_deref(), Some("10"));

        let mut theirs = merged.clone();
        resolve(&mut theirs, &[true]);
        assert_eq!(
            value(&theirs.environments, "dev", "A").as_deref(),
            Some("11")
        );
        assert!(theirs.conflicts.is_empty());
        let mut mine = merged;
        resolve(&mut mine, &[false]);
        assert_eq!(value(&mine.environments, "dev", "A").as_deref(), Some("10"));
    }

    #[test]
    fn content_comparison_ignores_order() {
        let a = vec![env("dev", &[("A", "1"), ("B", "2")]), env("qa", &[])];
        let b = vec![env("QA", &[]), env("dev", &[("B", "2"), ("A", "1")])];
        assert!(same_content(&a, &b));
        assert!(!same_content(&a, &[env("dev", &[("A", "1"), ("B", "2")])]));
        assert!(!same_content(
            &a,
            &[env("dev", &[("A", "1"), ("B", "3")]), env("qa", &[])]
        ));
    }

    #[test]
    fn first_sync_has_an_empty_base() {
        let local = vec![env("dev", &[("A", "1"), ("SAME", "s"), ("MINE", "m")])];
        let remote = vec![env("dev", &[("A", "2"), ("SAME", "s"), ("THEIRS", "t")])];
        let merged = merge(&[], &local, &remote);
        assert_eq!(merged.conflicts.len(), 1, "both added A differently");
        assert_eq!(
            value(&merged.environments, "dev", "MINE").as_deref(),
            Some("m")
        );
        assert_eq!(
            value(&merged.environments, "dev", "THEIRS").as_deref(),
            Some("t")
        );
        // Deleted on one side, changed on the other: a conflict too.
        let base = vec![env("dev", &[("K", "1")])];
        let merged = merge(&base, &[env("dev", &[])], &[env("dev", &[("K", "2")])]);
        assert_eq!(merged.conflicts.len(), 1);
        assert_eq!(merged.conflicts[0].mine, None);
    }

    fn team(names: &[&str]) -> Vec<Identity> {
        names
            .iter()
            .map(|_| Identity::generate().expect("id"))
            .collect()
    }

    #[test]
    fn folder_round_trip_and_strangers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ids = team(&["ana", "bo", "eve"]);
        let (ana, bo, eve) = (&ids[0], &ids[1], &ids[2]);
        let mut project = Project::new("shop", None);
        project
            .environment_mut("development")
            .upsert(Variable::new("TOKEN", "t0ps3cret-value"));
        let bo_key = bo.public_key().expect("pk");
        project
            .add_member("Bo", bo_key, Some(&ana.public_key().expect("pk")))
            .expect("member");
        let state = SyncState::new(dir.path().to_path_buf());

        let (file, hash) = write(&state, &project, ana).expect("write");
        assert_eq!(file, "shop.fenv");
        let text = std::fs::read_to_string(dir.path().join(&file)).expect("read");
        assert!(!text.contains("t0ps3cret-value") && !text.contains("TOKEN"));
        assert_eq!(hash_text(&text), hash);
        // Only the bundle is in the folder (no temp files left).
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("list")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names.len(), 1);

        // Bo (whose project lists Ana) reads it.
        let mut bos = Project::new("shop", None);
        bos.add_member("Ana", ana.public_key().expect("pk"), Some(&bo_key))
            .expect("member");
        let (incoming, rejected) = read_incoming(&state, &bos, bo).expect("read");
        assert!(rejected.is_empty());
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].sender, ana.public_key().expect("pk"));
        assert_eq!(
            value(&incoming[0].contents.environments, "development", "TOKEN").as_deref(),
            Some("t0ps3cret-value")
        );
        // Already merged files are skipped.
        let mut seen = state.clone();
        seen.seen.insert(file.clone(), hash);
        assert!(read_incoming(&seen, &bos, bo).expect("read").0.is_empty());

        // Eve, writing to the folder as a "conflict copy", is refused even
        // though she encrypted to Bo.
        let mut forged = Project::new("shop", None);
        forged
            .environment_mut("development")
            .upsert(Variable::new("TOKEN", "evil"));
        let armored = bundle::seal_for(
            &BundleContents {
                project: "shop".into(),
                environments: forged.environments.clone(),
                created_at: Utc::now(),
                note: String::new(),
            },
            eve,
            &[bo_key],
        )
        .expect("seal");
        std::fs::write(dir.path().join("shop (conflicted copy).fenv"), armored).expect("write");
        let (incoming, rejected) = read_incoming(&seen, &bos, bo).expect("read");
        assert!(incoming.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("not in the project's team"));

        let missing = SyncState::new(dir.path().join("gone"));
        assert!(write(&missing, &project, ana).is_err());
    }
}
