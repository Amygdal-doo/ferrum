//! Environments and globals.

use crate::ids::EnvironmentId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A `{{key}}` variable. `secret` only affects display (masked) and `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
}

impl Default for Variable {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
            secret: false,
        }
    }
}

impl Variable {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            ..Self::default()
        }
    }
}

impl fmt::Debug for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value: &dyn fmt::Debug = if self.secret { &"***" } else { &self.value };
        f.debug_struct("Variable")
            .field("key", &self.key)
            .field("value", value)
            .field("enabled", &self.enabled)
            .field("secret", &self.secret)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    pub id: EnvironmentId,
    pub name: String,
    pub variables: Vec<Variable>,
}

impl Environment {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: EnvironmentId::new(),
            name: name.into(),
            variables: Vec::new(),
        }
    }

    /// The single globals environment (fixed id).
    #[must_use]
    pub fn globals() -> Self {
        Self {
            id: EnvironmentId::GLOBALS,
            name: "Globals".to_owned(),
            variables: Vec::new(),
        }
    }
}
