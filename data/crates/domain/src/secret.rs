//! Secret values that cannot leak through `Debug`, `Display` or serde.

use std::fmt;

/// A credential held in memory only.
///
/// Deliberately does not implement `Serialize`, `Display` or `Clone`-into-string
/// helpers; `Debug` prints a fixed placeholder so secrets never reach logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub const REDACTED: &'static str = "***";

    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicit, greppable access to the plaintext.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Replaces every occurrence of this secret in `text` with a placeholder.
    /// Used to scrub driver error messages before they reach UI or logs.
    #[must_use]
    pub fn scrub(&self, text: &str) -> String {
        if self.0.is_empty() {
            text.to_owned()
        } else {
            text.replace(&self.0, Self::REDACTED)
        }
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{:?}", Some(&s)).contains("hunter2"));
    }

    #[test]
    fn scrub_removes_all_occurrences() {
        let s = Secret::new("pw");
        assert_eq!(s.scrub("a pw b pw"), "a *** b ***");
        assert_eq!(Secret::new("").scrub("keep"), "keep");
    }
}
