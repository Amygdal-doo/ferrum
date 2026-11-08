//! Connection profiles and engine capabilities.

use crate::{ProfileId, Secret};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseEngine {
    Postgres,
    MySql,
    Sqlite,
}

impl DatabaseEngine {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::MySql => "MySQL",
            Self::Sqlite => "SQLite",
        }
    }

    #[must_use]
    pub const fn default_port(self) -> Option<u16> {
        match self {
            Self::Postgres => Some(5432),
            Self::MySql => Some(3306),
            Self::Sqlite => None,
        }
    }
}

/// Environment tag; drives colouring and safety prompts in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EnvironmentTag {
    #[default]
    Local,
    Development,
    Staging,
    Production,
}

impl EnvironmentTag {
    pub const ALL: [Self; 4] = [
        Self::Local,
        Self::Development,
        Self::Staging,
        Self::Production,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Development => "Development",
            Self::Staging => "Staging",
            Self::Production => "Production",
        }
    }

    #[must_use]
    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

/// TLS policy. Certificate verification is on unless the user explicitly
/// picks a weaker mode, which the UI must label as such.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TlsMode {
    Disable,
    /// Use TLS if the server offers it, without verifying the certificate.
    Prefer,
    /// Require TLS, do not verify the certificate (insecure against MITM).
    Require,
    /// Require TLS and verify the CA chain.
    VerifyCa,
    /// Require TLS, verify the CA chain and the host name.
    #[default]
    VerifyFull,
}

impl TlsMode {
    pub const ALL: [Self; 5] = [
        Self::Disable,
        Self::Prefer,
        Self::Require,
        Self::VerifyCa,
        Self::VerifyFull,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disable => "Disabled",
            Self::Prefer => "Prefer (unverified)",
            Self::Require => "Require (unverified)",
            Self::VerifyCa => "Verify CA",
            Self::VerifyFull => "Verify CA + host",
        }
    }

    #[must_use]
    pub const fn verifies_server(self) -> bool {
        matches!(self, Self::VerifyCa | Self::VerifyFull)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endpoint {
    Tcp { host: String, port: u16 },
    File { path: String },
}

/// Pointer into the OS credential vault. Holds no secret material.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef {
    pub service: String,
    pub account: String,
}

impl SecretRef {
    pub const SERVICE: &'static str = "dev.ferrumdata.connection";

    #[must_use]
    pub fn for_profile(id: ProfileId) -> Self {
        Self {
            service: Self::SERVICE.to_owned(),
            account: id.to_string(),
        }
    }
}

/// A saved connection. Serializable and safe to export: never contains secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: ProfileId,
    pub display_name: String,
    pub engine: DatabaseEngine,
    pub endpoint: Endpoint,
    pub database: Option<String>,
    pub username: Option<String>,
    pub tls: TlsMode,
    pub read_only: bool,
    pub env: EnvironmentTag,
    pub connect_timeout_secs: u32,
    /// `None` means the password is not stored and is asked per session.
    pub secret_ref: Option<SecretRef>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub folder: Option<String>,
}

impl ConnectionProfile {
    #[must_use]
    pub fn new_postgres(display_name: impl Into<String>) -> Self {
        Self {
            id: ProfileId::new(),
            display_name: display_name.into(),
            engine: DatabaseEngine::Postgres,
            endpoint: Endpoint::Tcp {
                host: "localhost".into(),
                port: 5432,
            },
            database: Some("postgres".into()),
            username: Some("postgres".into()),
            tls: TlsMode::Prefer,
            read_only: false,
            env: EnvironmentTag::Local,
            connect_timeout_secs: 10,
            secret_ref: None,
            favorite: false,
            folder: None,
        }
    }

    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.connect_timeout_secs.max(1)))
    }

    /// Human readable target without credentials, e.g. `postgres@db:5432/app`.
    #[must_use]
    pub fn target_label(&self) -> String {
        let user = self
            .username
            .as_deref()
            .map(|u| format!("{u}@"))
            .unwrap_or_default();
        let db = self
            .database
            .as_deref()
            .map(|d| format!("/{d}"))
            .unwrap_or_default();
        match &self.endpoint {
            Endpoint::Tcp { host, port } => format!("{user}{host}:{port}{db}"),
            Endpoint::File { path } => path.clone(),
        }
    }

    /// Validation errors for the profile form.
    #[must_use]
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.display_name.trim().is_empty() {
            errors.push("Name is required".into());
        }
        match &self.endpoint {
            Endpoint::Tcp { host, port } => {
                if host.trim().is_empty() {
                    errors.push("Host is required".into());
                }
                if *port == 0 {
                    errors.push("Port must be between 1 and 65535".into());
                }
            }
            Endpoint::File { path } if path.trim().is_empty() => {
                errors.push("File path is required".into());
            }
            Endpoint::File { .. } => {}
        }
        errors
    }
}

/// A profile paired with its secret, handed to a driver for one connect call.
#[derive(Debug, Clone)]
pub struct ResolvedConnectionConfig {
    pub profile: ConnectionProfile,
    pub password: Option<Secret>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Ready { server_version: String },
    Failed { message: String },
}

/// What a driver supports. The UI hides or explains unavailable features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct CapabilitySet {
    pub transactions: bool,
    pub query_cancel: bool,
    pub schema_introspection: bool,
    pub edit_rows: bool,
    pub multi_result: bool,
    pub explain: bool,
    pub parameters: bool,
    pub import: bool,
    pub ddl: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_profile_has_no_password_field() {
        let mut p = ConnectionProfile::new_postgres("local");
        p.secret_ref = Some(SecretRef::for_profile(p.id));
        let json = serde_json::to_string(&p).expect("serialize");
        assert!(!json.to_lowercase().contains("password"));
        let back: ConnectionProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, p);
    }

    #[test]
    fn validation_reports_missing_fields() {
        let mut p = ConnectionProfile::new_postgres(" ");
        p.endpoint = Endpoint::Tcp {
            host: String::new(),
            port: 0,
        };
        assert_eq!(p.validate().len(), 3);
    }

    #[test]
    fn target_label_omits_secrets() {
        let p = ConnectionProfile::new_postgres("x");
        assert_eq!(p.target_label(), "postgres@localhost:5432/postgres");
    }

    #[test]
    fn default_tls_verifies() {
        assert!(TlsMode::default().verifies_server());
    }
}
