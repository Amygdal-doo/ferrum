//! Program status (Rex-compatible OSC 7501): agents and long-running
//! programs report `working` / `blocked` / `done` / `error` so the terminal can
//! show it in tabs, `fterm ls`, notifications and `fterm wait --status`.
//!
//! Wire form: `ESC ] 7501 ; key=value:key=value (ESC \ | BEL)`. Free text
//! (`message`) is base64-encoded UTF-8.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramState {
    Idle,
    Working,
    Blocked,
    Done,
    Error,
}

impl ProgramState {
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::Working,
        Self::Blocked,
        Self::Done,
        Self::Error,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// One-character badge for tabs and listings.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Idle => "○",
            Self::Working => "◐",
            Self::Blocked => "⚠",
            Self::Done => "✓",
            Self::Error => "✕",
        }
    }

    /// `done` and `error` survive process exit; the others are cleared.
    #[must_use]
    pub const fn sticky(self) -> bool {
        matches!(self, Self::Done | Self::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramStatus {
    pub state: ProgramState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// For `blocked`: `permission`, `question` or `auth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Unix millis of the last change (set by the server).
    #[serde(default)]
    pub updated_ms: u64,
}

impl ProgramStatus {
    #[must_use]
    pub fn new(state: ProgramState) -> Self {
        Self {
            state,
            progress: None,
            kind: None,
            message: None,
            updated_ms: 0,
        }
    }

    /// Encodes as an OSC 7501 sequence (ST-terminated).
    #[must_use]
    pub fn to_osc(&self) -> String {
        let mut parts = vec![format!("state={}", self.state.as_str())];
        if let Some(p) = self.progress {
            parts.push(format!("progress={p}"));
        }
        if let Some(k) = &self.kind {
            parts.push(format!("kind={k}"));
        }
        if let Some(m) = &self.message {
            parts.push(format!("message={}", STANDARD.encode(m)));
        }
        format!("\x1b]7501;{}\x1b\\", parts.join(":"))
    }
}

/// What an OSC 7501 body asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusUpdate {
    Set(ProgramStatus),
    Clear,
}

/// Parses the body after `7501;` (e.g. `state=working:progress=40`).
#[must_use]
pub fn parse_osc_body(body: &str) -> Option<StatusUpdate> {
    let mut status: Option<ProgramStatus> = None;
    let mut fields = Vec::new();
    for pair in body.split(':') {
        let (k, v) = pair.split_once('=')?;
        if k == "state" {
            if v == "clear" {
                return Some(StatusUpdate::Clear);
            }
            status = Some(ProgramStatus::new(ProgramState::parse(v)?));
        } else {
            fields.push((k, v));
        }
    }
    let mut s = status?;
    for (k, v) in fields {
        match k {
            "progress" => s.progress = v.parse::<u8>().ok().map(|p| p.min(100)),
            "kind" => {
                s.kind = Some(
                    v.chars()
                        .filter(char::is_ascii_alphanumeric)
                        .take(32)
                        .collect(),
                )
            }
            "message" => {
                s.message = STANDARD
                    .decode(v)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .map(|m| m.chars().take(500).collect());
            }
            _ => {}
        }
    }
    Some(StatusUpdate::Set(s))
}

/// Incremental scanner that extracts OSC 7501 bodies from a PTY byte stream,
/// across chunk boundaries. Other bytes are ignored (the emulator sees them).
#[derive(Debug, Default)]
pub struct OscScanner {
    state: ScanState,
    buf: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    #[default]
    Ground,
    Esc,
    Osc,
    OscEsc,
}

const PREFIX: &[u8] = b"7501;";
const MAX_BODY: usize = 4096;

impl OscScanner {
    /// Feeds bytes; returns every complete OSC 7501 body found.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            self.state = match (self.state, b) {
                (ScanState::Ground, 0x1b) => ScanState::Esc,
                (ScanState::Ground, _) => ScanState::Ground,
                (ScanState::Esc, b']') => {
                    self.buf.clear();
                    ScanState::Osc
                }
                (ScanState::Esc, 0x1b) => ScanState::Esc,
                (ScanState::Esc, _) => ScanState::Ground,
                (ScanState::Osc, 0x07) => {
                    self.finish(&mut out);
                    ScanState::Ground
                }
                (ScanState::Osc, 0x1b) => ScanState::OscEsc,
                (ScanState::Osc, _) => {
                    // Bail out early on OSCs that are not ours, or that are too long.
                    let not_ours = self.buf.len() < PREFIX.len() && PREFIX[self.buf.len()] != b;
                    if not_ours || self.buf.len() >= MAX_BODY {
                        ScanState::Ground
                    } else {
                        self.buf.push(b);
                        ScanState::Osc
                    }
                }
                (ScanState::OscEsc, b'\\') => {
                    self.finish(&mut out);
                    ScanState::Ground
                }
                (ScanState::OscEsc, b']') => {
                    self.buf.clear();
                    ScanState::Osc
                }
                (ScanState::OscEsc, _) => ScanState::Ground,
            };
        }
        out
    }

    fn finish(&mut self, out: &mut Vec<String>) {
        if let Some(body) = self.buf.strip_prefix(PREFIX) {
            out.push(String::from_utf8_lossy(body).into_owned());
        }
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bodies() {
        let msg = STANDARD.encode("Need approval");
        let Some(StatusUpdate::Set(s)) =
            parse_osc_body(&format!("state=blocked:kind=permission:message={msg}"))
        else {
            panic!("expected set");
        };
        assert_eq!(s.state, ProgramState::Blocked);
        assert_eq!(s.kind.as_deref(), Some("permission"));
        assert_eq!(s.message.as_deref(), Some("Need approval"));
        assert_eq!(parse_osc_body("state=clear"), Some(StatusUpdate::Clear));
        assert_eq!(parse_osc_body("state=bogus"), None);
        assert_eq!(parse_osc_body("progress=4"), None);
    }

    #[test]
    fn round_trips_through_osc() {
        let mut s = ProgramStatus::new(ProgramState::Working);
        s.progress = Some(40);
        s.message = Some("Running tests: 3/10".into());
        let mut scan = OscScanner::default();
        let bodies = scan.feed(s.to_osc().as_bytes());
        assert_eq!(parse_osc_body(&bodies[0]), Some(StatusUpdate::Set(s)));
    }

    #[test]
    fn scanner_handles_splits_bel_and_other_oscs() {
        let mut scan = OscScanner::default();
        let stream =
            b"hello\x1b]0;title\x07 \x1b]7501;state=done\x07 \x1b]7501;state=working\x1b\\";
        let mut found = Vec::new();
        for chunk in stream.chunks(3) {
            found.extend(scan.feed(chunk));
        }
        assert_eq!(found, ["state=done", "state=working"]);
    }
}
