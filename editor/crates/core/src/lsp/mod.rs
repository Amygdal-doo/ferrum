//! Language Server Protocol support without GPUI: the JSON-RPC transport,
//! a threaded client, position conversion, the protocol shapes the editor
//! uses and the table of known servers. The desktop app owns the running
//! servers and turns their answers into completions, diagnostics, jumps,
//! renames, references, code actions and formatting.

pub mod client;
pub mod edit;
pub mod features;
pub mod install;
pub mod position;
pub mod preview;
pub mod servers;
pub mod transport;
pub mod types;

use std::path::{Path, PathBuf};

pub use client::{Client, Launch, ServerEvent};
pub use edit::{EditOp, WorkspaceEdit};
pub use position::{LspRange, Position, PositionEncoding};
pub use types::{CompletionItem, Diagnostic, Location, Severity, TextEdit};

/// `file://` URI for an absolute path.
pub fn path_to_uri(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map(String::from)
        .unwrap_or_else(|()| format!("file://{}", path.to_string_lossy()))
}

/// Path of a `file://` URI.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip_with_escapes() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\Rust Dev\a#b.rs")
        } else {
            PathBuf::from("/Users/me/Rust Dev Tools/a#b.rs")
        };
        let uri = path_to_uri(&path);
        assert!(uri.starts_with("file:///"));
        assert!(uri.contains("%20") && uri.contains("%23"));
        assert_eq!(uri_to_path(&uri), Some(path));
        assert_eq!(uri_to_path("https://example.com"), None);
    }
}
