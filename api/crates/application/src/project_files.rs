//! Project collections as files in the repository: `<project>/.ferrum/api/`
//! holds one FerrumAPI collection JSON per collection (every request kind,
//! folders, auth, variables), so a team can commit and share them.
//!
//! Values of variables marked secret are written empty.

use crate::AppServices;
use crate::transfer::TransferError;
use ferrumapi_domain::Collection;
use std::path::{Path, PathBuf};

/// `<project>/.ferrum/api`.
#[must_use]
pub fn project_api_dir(project: &Path) -> PathBuf {
    project.join(".ferrum").join("api")
}

/// `users-api-1a2b3c4d.json`: a readable slug plus the id prefix (stable
/// across renames of other collections, unique per collection).
#[must_use]
pub fn file_name(c: &Collection) -> String {
    let slug: String = c
        .name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let id = c.id.to_string();
    let short = id.get(..8).unwrap_or(&id);
    if slug.is_empty() {
        format!("collection-{short}.json")
    } else {
        format!("{slug}-{short}.json")
    }
}

/// The collection as written to the repository.
#[must_use]
pub fn for_repository(c: &Collection) -> Collection {
    let mut out = c.clone();
    out.project = None;
    for v in out.variables.iter_mut().filter(|v| v.secret) {
        v.value.clear();
    }
    out
}

impl AppServices {
    /// Writes `collections` to `<project>/.ferrum/api/`, replacing older
    /// files of the same collections. Returns the files written.
    pub async fn export_project_collections(
        &self,
        project: PathBuf,
        collections: Vec<Collection>,
    ) -> Result<Vec<PathBuf>, TransferError> {
        let dir = project_api_dir(&project);
        tokio::fs::create_dir_all(&dir).await?;
        let mut written = Vec::new();
        for c in &collections {
            // Remove a file left by an earlier name of this collection.
            let id = c.id.to_string();
            let suffix = format!("-{}.json", id.get(..8).unwrap_or(&id));
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(&suffix) && name != file_name(c) {
                    tokio::fs::remove_file(entry.path()).await?;
                }
            }
            let json = serde_json::to_string_pretty(&for_repository(c))
                .map_err(|e| ferrumapi_interop::InteropError::Json(e.to_string()))?;
            let path = dir.join(file_name(c));
            tokio::fs::write(&path, json + "\n").await?;
            written.push(path);
        }
        Ok(written)
    }

    /// Reads every collection in `<project>/.ferrum/api/`, assigns it to
    /// `project` and saves it (replacing a stored collection with the same
    /// id). Unreadable files are skipped and named in the second list.
    pub async fn import_project_collections(
        &self,
        project: PathBuf,
    ) -> Result<(Vec<Collection>, Vec<String>), TransferError> {
        let dir = project_api_dir(&project);
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return Ok((imported, skipped));
        };
        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        let project_text = project.to_string_lossy().into_owned();
        for path in paths {
            let text = tokio::fs::read_to_string(&path).await?;
            match serde_json::from_str::<Collection>(&text) {
                Ok(mut c) => {
                    c.project = Some(project_text.clone());
                    self.save_collection(&c).await?;
                    imported.push(c);
                }
                Err(_) => skipped.push(
                    path.file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                ),
            }
        }
        Ok((imported, skipped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumapi_domain::{HttpRequest, Item, RequestKind, SavedRequest, Variable};

    #[test]
    fn file_names_are_slugs_with_the_id_prefix() {
        let c = Collection::new("Users API (v2)!");
        let name = file_name(&c);
        assert!(
            name.starts_with("users-api-v2-") && name.ends_with(".json"),
            "{name}"
        );
        assert_eq!(name.len(), "users-api-v2-".len() + 8 + 5);
        assert!(file_name(&Collection::new("??")).starts_with("collection-"));
    }

    #[test]
    fn export_then_import_round_trips_without_secrets() {
        let dir = std::env::temp_dir().join(format!("ferrumapi-proj-{}", uuid::Uuid::new_v4()));
        let svc = AppServices::start(&dir.join("data")).expect("start");
        let project = dir.join("repo");
        let mut c = Collection::new("Mixed");
        c.project = Some(project.display().to_string());
        let mut secret = Variable::new("token", "s3cret");
        secret.secret = true;
        c.variables = vec![Variable::new("base", "http://x"), secret];
        c.items.push(Item::Request(SavedRequest::new(
            "gql",
            HttpRequest::of_kind(RequestKind::Graphql),
        )));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(async {
            let written = svc
                .export_project_collections(project.clone(), vec![c.clone()])
                .await
                .expect("export");
            assert_eq!(written.len(), 1);
            let text = std::fs::read_to_string(&written[0]).expect("read");
            assert!(
                !text.contains("s3cret") && !text.contains("\"project\": \""),
                "{text}"
            );

            // Renaming writes a new file and removes the old one.
            c.name = "Renamed".into();
            svc.export_project_collections(project.clone(), vec![c.clone()])
                .await
                .expect("re-export");
            let files: Vec<_> = std::fs::read_dir(project_api_dir(&project))
                .expect("dir")
                .collect();
            assert_eq!(files.len(), 1);

            std::fs::write(project_api_dir(&project).join("broken.json"), "{").expect("write");
            let (imported, skipped) = svc
                .import_project_collections(project.clone())
                .await
                .expect("import");
            assert_eq!(skipped, ["broken.json"]);
            assert_eq!(imported.len(), 1);
            assert_eq!(imported[0].name, "Renamed");
            assert_eq!(
                imported[0].project.as_deref(),
                Some(project.display().to_string().as_str())
            );
            assert_eq!(imported[0].variables[1].value, "");
            let stored = svc.store.list_collections().await.expect("list");
            assert_eq!(stored.len(), 1);
        });
        drop(svc);
        let _ = std::fs::remove_dir_all(dir);
    }
}
