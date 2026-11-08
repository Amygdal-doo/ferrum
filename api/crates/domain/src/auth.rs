//! Request authorization. `Debug` never prints credentials.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// Use the nearest folder / collection auth. Outside a collection this is
    /// the same as [`Auth::None`].
    #[default]
    Inherit,
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
}

impl Auth {
    #[must_use]
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Inherit => "Inherit from parent",
            Self::None => "No Auth",
            Self::Basic { .. } => "Basic Auth",
            Self::Bearer { .. } => "Bearer Token",
            Self::ApiKey { .. } => "API Key",
        }
    }

    #[must_use]
    pub const fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const R: &str = "***";
        match self {
            Self::Inherit => f.write_str("Inherit"),
            Self::None => f.write_str("None"),
            Self::Basic { username, .. } => f
                .debug_struct("Basic")
                .field("username", username)
                .field("password", &R)
                .finish(),
            Self::Bearer { .. } => f.debug_struct("Bearer").field("token", &R).finish(),
            Self::ApiKey { key, location, .. } => f
                .debug_struct("ApiKey")
                .field("key", key)
                .field("value", &R)
                .field("location", location)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secrets() {
        let a = Auth::Basic {
            username: "u".into(),
            password: "hunter2".into(),
        };
        let b = Auth::Bearer {
            token: "tok123".into(),
        };
        let c = Auth::ApiKey {
            key: "X-Key".into(),
            value: "sekret".into(),
            location: ApiKeyLocation::Header,
        };
        let s = format!("{a:?}{b:?}{c:?}");
        assert!(!s.contains("hunter2") && !s.contains("tok123") && !s.contains("sekret"));
    }

    #[test]
    fn serde_round_trip() {
        let a = Auth::Bearer { token: "t".into() };
        let json = serde_json::to_string(&a).expect("ser");
        assert_eq!(json, r#"{"type":"bearer","token":"t"}"#);
        assert_eq!(serde_json::from_str::<Auth>(&json).expect("de"), a);
    }
}
