//! Image references and pull progress.

use crate::model::PullEvent;
use std::collections::BTreeMap;

/// Splits `reference` into (`from_image`, `tag`) for `POST /images/create`.
/// A missing tag means `latest` (an empty tag would pull *every* tag);
/// digests (`name@sha256:…`) are passed as the tag.
/// Returns `None` for an empty or malformed reference.
#[must_use]
pub fn split_reference(reference: &str) -> Option<(String, String)> {
    let reference = reference.trim();
    if reference.is_empty() || reference.contains(char::is_whitespace) {
        return None;
    }
    if let Some((name, digest)) = reference.split_once('@') {
        return (!name.is_empty() && !digest.is_empty())
            .then(|| (name.to_owned(), digest.to_owned()));
    }
    // A colon after the last slash is a tag; before it, a registry port.
    let last_slash = reference.rfind('/').map_or(0, |i| i + 1);
    match reference[last_slash..].rfind(':') {
        Some(i) => {
            let split = last_slash + i;
            let (name, tag) = (&reference[..split], &reference[split + 1..]);
            (!name.is_empty() && !tag.is_empty() && !name.ends_with('/'))
                .then(|| (name.to_owned(), tag.to_owned()))
        }
        None => (!reference.ends_with('/')).then(|| (reference.to_owned(), "latest".to_owned())),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layer {
    current: u64,
    total: u64,
    done: bool,
}

/// Aggregates pull events into an overall progress line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullProgress {
    layers: BTreeMap<String, Layer>,
    /// Latest non-layer status (e.g. "Digest: sha256:…", "Status: Downloaded newer image…").
    pub status: String,
    pub error: Option<String>,
    pub finished: bool,
}

impl PullProgress {
    pub fn apply(&mut self, event: &PullEvent) {
        if let Some(error) = &event.error {
            self.error = Some(error.clone());
            return;
        }
        match event.id.as_deref() {
            Some(id) if !event.status.starts_with("Pulling from") => {
                let layer = self.layers.entry(id.to_owned()).or_default();
                match event.status.as_str() {
                    "Downloading" => {
                        layer.current = event.current.unwrap_or(layer.current);
                        layer.total = event.total.unwrap_or(layer.total).max(layer.total);
                    }
                    "Download complete" | "Pull complete" | "Already exists" => {
                        layer.current = layer.total;
                        layer.done = true;
                    }
                    _ => {}
                }
            }
            _ => self.status.clone_from(&event.status),
        }
    }

    /// Layers known so far and how many are finished.
    #[must_use]
    pub fn layers(&self) -> (usize, usize) {
        let done = self.layers.values().filter(|l| l.done).count();
        (done, self.layers.len())
    }

    /// Downloaded and total bytes of layers whose size is known.
    #[must_use]
    pub fn bytes(&self) -> (u64, u64) {
        self.layers
            .values()
            .filter(|l| l.total > 0)
            .fold((0, 0), |(c, t), l| {
                (c + l.current.min(l.total), t + l.total)
            })
    }

    /// 0.0–1.0, or `None` before any size is known.
    #[must_use]
    pub fn fraction(&self) -> Option<f32> {
        if self.finished {
            return Some(1.0);
        }
        let (current, total) = self.bytes();
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        (total > 0).then(|| (current as f64 / total as f64) as f32)
    }

    /// One-line summary for the UI.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(error) = &self.error {
            return error.clone();
        }
        let (done, count) = self.layers();
        let (current, total) = self.bytes();
        let mut parts = Vec::new();
        if count > 0 {
            parts.push(format!("{done}/{count} layers"));
        }
        if total > 0 && !self.finished {
            parts.push(format!(
                "{} / {}",
                crate::format::bytes(current),
                crate::format::bytes(total)
            ));
        }
        if !self.status.is_empty() {
            parts.push(self.status.clone());
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(r: &str) -> Option<(String, String)> {
        split_reference(r)
    }

    fn pair(a: &str, b: &str) -> Option<(String, String)> {
        Some((a.to_owned(), b.to_owned()))
    }

    #[test]
    fn splits_references() {
        assert_eq!(split("nginx"), pair("nginx", "latest"));
        assert_eq!(split(" redis:7-alpine "), pair("redis", "7-alpine"));
        assert_eq!(split("ghcr.io/org/app:1.2"), pair("ghcr.io/org/app", "1.2"));
        assert_eq!(
            split("localhost:5000/app"),
            pair("localhost:5000/app", "latest")
        );
        assert_eq!(
            split("localhost:5000/app:dev"),
            pair("localhost:5000/app", "dev")
        );
        assert_eq!(split("alpine@sha256:abc"), pair("alpine", "sha256:abc"));
        for bad in ["", "  ", "nginx:", ":tag", "a b", "@sha256:x", "repo/"] {
            assert_eq!(split(bad), None, "{bad:?}");
        }
    }

    fn event(
        id: Option<&str>,
        status: &str,
        current: Option<u64>,
        total: Option<u64>,
    ) -> PullEvent {
        PullEvent {
            id: id.map(str::to_owned),
            status: status.to_owned(),
            current,
            total,
            error: None,
        }
    }

    #[test]
    fn aggregates_layer_progress() {
        let mut progress = PullProgress::default();
        progress.apply(&event(
            Some("latest"),
            "Pulling from library/nginx",
            None,
            None,
        ));
        progress.apply(&event(Some("a"), "Pulling fs layer", None, None));
        progress.apply(&event(Some("b"), "Already exists", None, None));
        progress.apply(&event(Some("a"), "Downloading", Some(250), Some(1000)));
        assert_eq!(progress.layers(), (1, 2));
        assert_eq!(progress.bytes(), (250, 1000));
        assert_eq!(progress.fraction(), Some(0.25));
        assert_eq!(
            progress.summary(),
            "1/2 layers · 250 B / 1000 B · Pulling from library/nginx"
        );
        progress.apply(&event(Some("a"), "Pull complete", None, None));
        progress.apply(&event(
            None,
            "Status: Downloaded newer image for nginx:latest",
            None,
            None,
        ));
        progress.finished = true;
        assert_eq!(progress.fraction(), Some(1.0));
        assert_eq!(
            progress.summary(),
            "2/2 layers · Status: Downloaded newer image for nginx:latest"
        );
    }

    #[test]
    fn reports_errors() {
        let mut progress = PullProgress::default();
        assert_eq!(progress.fraction(), None);
        progress.apply(&PullEvent {
            error: Some("manifest unknown".into()),
            ..PullEvent::default()
        });
        assert_eq!(progress.summary(), "manifest unknown");
    }
}
