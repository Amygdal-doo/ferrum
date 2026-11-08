//! A string that never appears in `Debug` output or logs.

use std::fmt;

#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The plain value. Only pass it to the credential store or the askpass file.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_is_redacted() {
        assert_eq!(format!("{:?}", Secret::new("hunter2")), "Secret(***)");
    }
}
