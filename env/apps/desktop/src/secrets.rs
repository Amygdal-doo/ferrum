//! Handling decrypted values in the UI: masking, and copying to the
//! clipboard with a timed clear.

use std::time::Duration;

use ferrum_ui::gpui::ClipboardItem;
use ferrum_ui::prelude::*;
use sha2::{Digest, Sha256};

/// How long a copied secret may stay on the clipboard.
pub const CLIPBOARD_CLEAR_AFTER: Duration = Duration::from_secs(30);

/// Shown instead of a masked value.
pub const MASK: &str = "••••••••••••";

/// Remembers the last secret we put on the clipboard (as a hash, so the
/// value itself isn't kept around) and wipes the clipboard after
/// [`CLIPBOARD_CLEAR_AFTER`] if it still holds that value.
#[derive(Default)]
pub struct ClipboardGuard {
    digest: Option<[u8; 32]>,
    task: Option<Task<()>>,
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

impl ClipboardGuard {
    /// Remembers `text` (hashed) and the task that will clear it.
    pub fn arm(&mut self, text: &str, clear_task: Task<()>) {
        self.digest = Some(digest(text));
        self.task = Some(clear_task);
    }

    /// Empties the clipboard if it still holds the copied secret (the user
    /// may have copied something else since: that is left alone).
    pub fn clear_if_ours(&mut self, cx: &mut App) {
        self.task = None;
        let Some(expected) = self.digest.take() else {
            return;
        };
        let current = cx.read_from_clipboard().and_then(|item| item.text());
        if current.is_some_and(|text| digest(&text) == expected) {
            cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
        }
    }

    #[cfg(test)]
    pub fn is_armed(&self) -> bool {
        self.digest.is_some()
    }
}

/// The first line of a value, shortened for a table cell.
pub fn preview(value: &str) -> String {
    let mut lines = value.lines();
    let first = lines.next().unwrap_or("");
    let mut out: String = first.chars().take(80).collect();
    if first.chars().count() > 80 || lines.next().is_some() {
        out.push('…');
    }
    out
}
