//! The shared Ferrum project bar, inline under the sidebar's title row: the active tab's
//! connection (`PostgreSQL · app_db @ localhost:5432`), never credentials.
//!
//! Connections are not folders, so the switcher is off. A SQLite file
//! inside a git repository is shown as that project (path, branch, and
//! "open in Editor / Git / Term"), with the file as the detail.

use crate::app::FerrumData;
use ferrum_ui::ProjectBar;
use ferrum_ui::gpui::{AppContext as _, Context, Entity, SharedString, Window};
use ferrum_ui::project::{display_path, git_root, home_dir};
use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, Endpoint};
use std::path::{Path, PathBuf};

/// The executable name, for the shared bar.
const APP_BINARY: &str = "ferrumdata";

pub fn new_bar(window: &mut Window, cx: &mut Context<FerrumData>) -> Entity<ProjectBar> {
    cx.new(|cx| {
        ProjectBar::new(APP_BINARY, window, cx)
            .switchable(false)
            .inline(true)
    })
}

/// What the bar shows for a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarContent {
    pub title: SharedString,
    pub detail: Option<SharedString>,
    /// The git repository a SQLite file lives in.
    pub project: Option<PathBuf>,
}

/// `host[:port]` of a connection string, without scheme, credentials,
/// database or options.
fn uri_host(uri: &str) -> &str {
    let rest = uri.split_once("://").map_or(uri, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

/// `PostgreSQL · app_db @ localhost:5432`: engine, database, server.
#[must_use]
pub fn connection_detail(profile: &ConnectionProfile, home: Option<&Path>) -> String {
    let engine = profile.engine.label();
    let database = profile
        .database
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let database = match profile.engine {
        DatabaseEngine::Redis => profile.redis_db_index().map(|i| format!("db {i}")),
        DatabaseEngine::SurrealDb => match (profile.namespace.as_deref(), database) {
            (Some(ns), Some(db)) if !ns.trim().is_empty() => Some(format!("{}/{db}", ns.trim())),
            (_, db) => db.map(str::to_owned),
        },
        _ => database.map(str::to_owned),
    };
    let server = match &profile.endpoint {
        Endpoint::Tcp { host, port } => format!("{host}:{port}"),
        Endpoint::Uri { uri } => uri_host(uri).to_owned(),
        Endpoint::File { path } => {
            return format!("{engine} · {}", display_path(Path::new(path), home));
        }
    };
    match database {
        Some(db) => format!("{engine} · {db} @ {server}"),
        None => format!("{engine} @ {server}"),
    }
}

/// The bar for the active tab's connection (`None`: no connection).
#[must_use]
pub fn bar_content(profile: Option<&ConnectionProfile>, home: Option<&Path>) -> BarContent {
    let Some(profile) = profile else {
        return BarContent {
            title: "No connection".into(),
            detail: None,
            project: None,
        };
    };
    let title: SharedString = profile.display_name.clone().into();
    if let Endpoint::File { path } = &profile.endpoint {
        let file = Path::new(path);
        if let Some(root) = file.parent().filter(|p| p.is_absolute()).and_then(git_root)
            && let Ok(inside) = file.strip_prefix(&root)
        {
            return BarContent {
                title,
                detail: Some(format!("{} · {}", profile.engine.label(), inside.display()).into()),
                project: Some(root),
            };
        }
    }
    BarContent {
        title,
        detail: Some(connection_detail(profile, home).into()),
        project: None,
    }
}

impl FerrumData {
    /// Mirrors the active tab's connection into the bar when it changes.
    pub(crate) fn sync_project_bar(&mut self, cx: &mut Context<Self>) {
        let profile = self.state.active_profile();
        let key = profile.map(|p| format!("{}|{}|{}", p.id, p.display_name, p.target_label()));
        if self.project_synced.as_ref() == Some(&key) {
            return;
        }
        let content = bar_content(profile, home_dir().as_deref());
        self.project_synced = Some(key);
        self.project_bar.update(cx, |bar, cx| {
            bar.set_project(content.project, cx);
            bar.set_title(Some(content.title), cx);
            bar.set_detail(content.detail, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(profile: &ConnectionProfile) -> String {
        connection_detail(profile, Some(Path::new("/Users/me")))
    }

    #[test]
    fn server_connections_show_engine_database_and_host() {
        let mut pg = ConnectionProfile::new(DatabaseEngine::Postgres, "App DB");
        pg.database = Some("app_db".into());
        pg.username = Some("admin".into());
        assert_eq!(detail(&pg), "PostgreSQL · app_db @ localhost:5432");
        let content = bar_content(Some(&pg), None);
        assert_eq!(content.title.as_ref(), "App DB");
        assert!(content.project.is_none());

        pg.database = None;
        assert_eq!(detail(&pg), "PostgreSQL @ localhost:5432");

        let redis = ConnectionProfile::new(DatabaseEngine::Redis, "cache");
        assert_eq!(detail(&redis), "Redis · db 0 @ localhost:6379");

        assert_eq!(bar_content(None, None).title.as_ref(), "No connection");
    }

    #[test]
    fn connection_strings_never_show_credentials() {
        let mut mongo = ConnectionProfile::new(DatabaseEngine::MongoDb, "Atlas");
        mongo.endpoint = Endpoint::Uri {
            uri: "mongodb+srv://bob:s3cret@cluster0.example.net/shop?retryWrites=true".into(),
        };
        mongo.database = Some("shop".into());
        let text = detail(&mongo);
        assert_eq!(text, "MongoDB · shop @ cluster0.example.net");
        assert!(!text.contains("s3cret") && !text.contains("bob"));
        assert_eq!(uri_host("redis://localhost:6379/0"), "localhost:6379");
        assert_eq!(uri_host("host-only"), "host-only");
    }

    #[test]
    fn a_sqlite_file_in_a_repository_is_that_project() {
        let dir = std::env::temp_dir().join(format!("ferrumdata-project-{}", std::process::id()));
        let repo = dir.join("shop");
        std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
        std::fs::create_dir_all(repo.join("db")).expect("mkdir");
        let mut sqlite = ConnectionProfile::new(DatabaseEngine::Sqlite, "shop.db");
        sqlite.endpoint = Endpoint::File {
            path: repo.join("db/shop.db").display().to_string(),
        };
        let content = bar_content(Some(&sqlite), None);
        assert_eq!(content.project.as_deref(), Some(repo.as_path()));
        assert_eq!(content.detail.as_deref(), Some("SQLite · db/shop.db"));

        // Outside a repository: the file, from home.
        sqlite.endpoint = Endpoint::File {
            path: "/Users/me/data/notes.sqlite".into(),
        };
        let content = bar_content(Some(&sqlite), Some(Path::new("/Users/me")));
        assert!(content.project.is_none());
        assert_eq!(
            content.detail.as_deref(),
            Some("SQLite · ~/data/notes.sqlite")
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
