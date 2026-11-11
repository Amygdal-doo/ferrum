//! Session restore: the last folder, pane layout, open tabs and cursors,
//! stored as JSON (normally `~/.ferrum/editor/session.json`).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub folder: Option<PathBuf>,
    pub layout: PaneLayout,
    /// Index of the focused pane in depth-first order.
    pub active_pane: usize,
    pub tree_visible: bool,
    pub tree_width: f32,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            folder: None,
            layout: PaneLayout::default(),
            active_pane: 0,
            tree_visible: true,
            tree_width: 240.,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitAxis {
    /// Children side by side, left to right ("split right").
    #[default]
    Horizontal,
    /// Children stacked top to bottom ("split down").
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaneLayout {
    Pane(PaneSession),
    Split {
        #[serde(default)]
        axis: SplitAxis,
        #[serde(default)]
        children: Vec<PaneLayout>,
    },
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self::Pane(PaneSession::default())
    }
}

impl PaneLayout {
    /// Panes in depth-first order (the order `active_pane` indexes).
    pub fn panes(&self) -> Vec<&PaneSession> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a PaneSession>) {
        match self {
            Self::Pane(pane) => out.push(pane),
            Self::Split { children, .. } => {
                for child in children {
                    child.collect(out);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneSession {
    pub tabs: Vec<TabSession>,
    pub active: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSession {
    pub path: PathBuf,
    /// 0-based cursor line.
    pub line: usize,
    /// 0-based cursor column (bytes within the line).
    pub column: usize,
    /// First visible line.
    pub scroll_line: usize,
}

/// Reads a session; any error (missing file, bad JSON) gives the default.
pub fn load(path: &Path) -> Session {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes atomically: a temp file next to `path`, then a rename.
pub fn save(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(session).map_err(io::Error::other)?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        let tab = |path: &str, line| TabSession {
            path: PathBuf::from(path),
            line,
            column: 3,
            scroll_line: 1,
        };
        Session {
            folder: Some(PathBuf::from("/project")),
            layout: PaneLayout::Split {
                axis: SplitAxis::Horizontal,
                children: vec![
                    PaneLayout::Pane(PaneSession {
                        tabs: vec![tab("/project/a.rs", 10), tab("/project/b.rs", 0)],
                        active: 1,
                    }),
                    PaneLayout::Split {
                        axis: SplitAxis::Vertical,
                        children: vec![
                            PaneLayout::Pane(PaneSession::default()),
                            PaneLayout::Pane(PaneSession {
                                tabs: vec![tab("/project/c.md", 4)],
                                active: 0,
                            }),
                        ],
                    },
                ],
            },
            active_pane: 2,
            tree_visible: false,
            tree_width: 300.,
        }
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/session.json");
        let session = sample();
        save(&path, &session).expect("save");
        assert_eq!(load(&path), session);
        assert!(!dir.path().join("nested/session.json.tmp").exists());
    }

    #[test]
    fn panes_in_depth_first_order() {
        let session = sample();
        let panes = session.layout.panes();
        assert_eq!(panes.len(), 3);
        assert_eq!(panes[0].tabs.len(), 2);
        assert!(panes[1].tabs.is_empty());
        assert_eq!(panes[2].tabs[0].path, PathBuf::from("/project/c.md"));
    }

    #[test]
    fn missing_or_corrupt_gives_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(load(&dir.path().join("none.json")), Session::default());
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{not json").expect("write");
        assert_eq!(load(&bad), Session::default());
        assert!(Session::default().tree_visible);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let session: Session =
            serde_json::from_str(r#"{"folder":"/x","future_field":1}"#).expect("parse");
        assert_eq!(session.folder, Some(PathBuf::from("/x")));
        assert_eq!(session.tree_width, 240.);
        assert_eq!(session.layout, PaneLayout::default());
        let layout: PaneLayout =
            serde_json::from_str(r#"{"type":"pane","tabs":[{"path":"/a"}]}"#).expect("parse");
        assert_eq!(layout.panes()[0].tabs[0].line, 0);
    }
}
