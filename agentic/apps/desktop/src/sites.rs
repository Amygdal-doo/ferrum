//! Sites: web pages saved per project (docs, dashboards, specs) to hand to
//! the chat as context. Kept in `~/.ferrum/agentic/sites.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sites {
    #[serde(default)]
    pub projects: BTreeMap<PathBuf, Vec<Site>>,
}

/// `example.com/x` → `https://example.com/x`; `None` for text that isn't
/// a web address.
#[must_use]
pub fn normalize_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let url = if text.starts_with("http://") || text.starts_with("https://") {
        text.to_string()
    } else if text.contains("://") {
        return None;
    } else {
        format!("https://{text}")
    };
    let host = url.split("://").nth(1)?.split(['/', '?', '#']).next()?;
    (host.contains('.') || host.starts_with("localhost")).then_some(url)
}

/// A title when none is given: the host and path, without the scheme.
#[must_use]
pub fn default_title(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.trim_end_matches('/').to_string()
}

impl Sites {
    #[must_use]
    pub fn of(&self, project: &Path) -> &[Site] {
        self.projects.get(project).map_or(&[], Vec::as_slice)
    }

    /// Adds (or retitles) `url` in `project`; returns the saved site.
    pub fn add(&mut self, project: &Path, url: &str, title: &str) -> Option<Site> {
        let url = normalize_url(url)?;
        let title = match title.trim() {
            "" => default_title(&url),
            title => title.to_string(),
        };
        let list = self.projects.entry(project.to_path_buf()).or_default();
        list.retain(|s| s.url != url);
        let site = Site { url, title };
        list.push(site.clone());
        Some(site)
    }

    pub fn remove(&mut self, project: &Path, url: &str) {
        if let Some(list) = self.projects.get_mut(project) {
            list.retain(|s| s.url != url);
            if list.is_empty() {
                self.projects.remove(project);
            }
        }
    }
}

#[must_use]
pub fn load(file: &Path) -> Sites {
    ferrum_ui::prefs::read(file)
}

pub fn save(file: &Path, sites: &Sites) -> std::io::Result<()> {
    ferrum_ui::prefs::write(file, sites)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_normalized() {
        assert_eq!(
            normalize_url(" docs.rs/gpui ").as_deref(),
            Some("https://docs.rs/gpui")
        );
        assert_eq!(
            normalize_url("http://localhost:3000/x").as_deref(),
            Some("http://localhost:3000/x")
        );
        for bad in ["", "not a url", "ftp://x.org", "word"] {
            assert_eq!(normalize_url(bad), None, "{bad}");
        }
        assert_eq!(default_title("https://docs.rs/gpui/"), "docs.rs/gpui");
    }

    #[test]
    fn sites_are_kept_per_project_and_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("agentic").join("sites.json");
        let (a, b) = (Path::new("/code/a"), Path::new("/code/b"));
        let mut sites = load(&file);
        assert!(sites.of(a).is_empty());
        sites.add(a, "docs.rs", "Docs").expect("add");
        sites.add(a, "https://github.com/x/y", "").expect("add");
        // The same URL again replaces the first one's title.
        sites.add(a, "https://docs.rs", "Rust docs").expect("add");
        sites.add(b, "example.com", "").expect("add");
        assert!(sites.add(b, "nope", "").is_none());
        let titles: Vec<&str> = sites.of(a).iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["github.com/x/y", "Rust docs"]);
        save(&file, &sites).expect("save");
        assert_eq!(load(&file), sites);

        sites.remove(b, "https://example.com");
        assert!(sites.of(b).is_empty() && !sites.projects.contains_key(b));
    }
}
