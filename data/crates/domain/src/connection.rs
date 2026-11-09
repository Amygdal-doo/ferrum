//! Connection profiles and engine capabilities.

use crate::{ProfileId, Secret};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Supported engines. Variant names are persisted in saved profiles: never
/// rename one, only add.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseEngine {
    Postgres,
    MySql,
    Sqlite,
    SurrealDb,
    MongoDb,
}

/// How a profile locates its server, which decides the connection form fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    /// Host + port.
    Tcp,
    /// A database file on local disk.
    File,
    /// A driver connection string (`mongodb://…`).
    Uri,
}

impl DatabaseEngine {
    pub const ALL: [Self; 5] = [
        Self::Postgres,
        Self::MySql,
        Self::Sqlite,
        Self::SurrealDb,
        Self::MongoDb,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::MySql => "MySQL",
            Self::Sqlite => "SQLite",
            Self::SurrealDb => "SurrealDB",
            Self::MongoDb => "MongoDB",
        }
    }

    #[must_use]
    pub const fn default_port(self) -> Option<u16> {
        match self {
            Self::Postgres => Some(5432),
            Self::MySql => Some(3306),
            Self::SurrealDb => Some(8000),
            Self::MongoDb => Some(27017),
            Self::Sqlite => None,
        }
    }

    #[must_use]
    pub const fn endpoint_kind(self) -> EndpointKind {
        match self {
            Self::Postgres | Self::MySql | Self::SurrealDb => EndpointKind::Tcp,
            Self::Sqlite => EndpointKind::File,
            Self::MongoDb => EndpointKind::Uri,
        }
    }

    /// True for engines whose editor input is SQL (statement splitting, SQL
    /// safety keywords, `SELECT … LIMIT` browsing).
    #[must_use]
    pub const fn is_sql(self) -> bool {
        matches!(self, Self::Postgres | Self::MySql | Self::Sqlite)
    }

    /// Name of the editor's input language.
    #[must_use]
    pub const fn query_language(self) -> &'static str {
        match self {
            Self::Postgres | Self::MySql | Self::Sqlite => "SQL",
            Self::SurrealDb => "SurrealQL",
            Self::MongoDb => "MongoDB query",
        }
    }

    /// Placeholder shown in an empty editor; documents the accepted input.
    #[must_use]
    pub const fn editor_placeholder(self) -> &'static str {
        match self {
            Self::Postgres | Self::MySql | Self::Sqlite => {
                "SELECT * FROM … ;  Run with ⌘/Ctrl+Enter"
            }
            Self::SurrealDb => {
                "SELECT * FROM person LIMIT 10;  INFO FOR DB;  Run with ⌘/Ctrl+Enter"
            }
            Self::MongoDb => {
                "db.users.find({ age: { $gt: 30 } }).sort({ name: 1 }).limit(20)  ·  \
                 db.orders.aggregate([...])  ·  db.users.countDocuments({})  ·  \
                 or a JSON command: { \"count\": \"users\" }   Run with ⌘/Ctrl+Enter"
            }
        }
    }

    /// Whether a profile for this engine needs a password field.
    #[must_use]
    pub const fn uses_password(self) -> bool {
        !matches!(self, Self::Sqlite)
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
    Tcp {
        host: String,
        port: u16,
    },
    File {
        path: String,
    },
    /// Connection string without credentials (validated, see `validate`).
    Uri {
        uri: String,
    },
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
    /// SurrealDB namespace. Absent in profiles saved before SurrealDB support.
    #[serde(default)]
    pub namespace: Option<String>,
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
        Self::new(DatabaseEngine::Postgres, display_name)
    }

    /// A fresh profile with sensible local defaults for `engine`.
    #[must_use]
    pub fn new(engine: DatabaseEngine, display_name: impl Into<String>) -> Self {
        let tcp = |port: u16| Endpoint::Tcp {
            host: "localhost".into(),
            port,
        };
        let (endpoint, database, username, tls, namespace) = match engine {
            DatabaseEngine::Postgres => (
                tcp(5432),
                Some("postgres"),
                Some("postgres"),
                TlsMode::Prefer,
                None,
            ),
            DatabaseEngine::MySql => (tcp(3306), None, Some("root"), TlsMode::Prefer, None),
            DatabaseEngine::Sqlite => (
                Endpoint::File {
                    path: String::new(),
                },
                None,
                None,
                TlsMode::Disable,
                None,
            ),
            // A local SurrealDB server speaks plain `ws://`; pick TLS for remote ones.
            DatabaseEngine::SurrealDb => (
                tcp(8000),
                Some("test"),
                Some("root"),
                TlsMode::Disable,
                Some("test".to_owned()),
            ),
            // TLS for MongoDB is configured in the connection string.
            DatabaseEngine::MongoDb => (
                Endpoint::Uri {
                    uri: "mongodb://localhost:27017".into(),
                },
                Some("test"),
                None,
                TlsMode::Disable,
                None,
            ),
        };
        Self {
            id: ProfileId::new(),
            display_name: display_name.into(),
            engine,
            endpoint,
            database: database.map(str::to_owned),
            namespace,
            username: username.map(str::to_owned),
            tls,
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
        let ns = self
            .namespace
            .as_deref()
            .filter(|_| self.engine == DatabaseEngine::SurrealDb)
            .map(|n| format!("/{n}"))
            .unwrap_or_default();
        match &self.endpoint {
            Endpoint::Tcp { host, port } => format!("{user}{host}:{port}{ns}{db}"),
            Endpoint::File { path } => path.clone(),
            Endpoint::Uri { uri } => format!("{user}{}{db}", uri.trim_end_matches('/')),
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
            Endpoint::Uri { uri } => {
                if let Err(e) = validate_mongo_uri(uri) {
                    errors.push(e.to_owned());
                }
            }
        }
        let blank = |v: &Option<String>| v.as_deref().is_none_or(|s| s.trim().is_empty());
        match self.engine {
            DatabaseEngine::SurrealDb => {
                if blank(&self.namespace) {
                    errors.push("Namespace is required".into());
                }
                if blank(&self.database) {
                    errors.push("Database is required".into());
                }
            }
            DatabaseEngine::MongoDb if blank(&self.database) => {
                errors.push("Database is required".into());
            }
            _ => {}
        }
        if !matches_engine(self.engine, &self.endpoint) {
            errors.push(format!(
                "{} profiles need a {} endpoint",
                self.engine.label(),
                match self.engine.endpoint_kind() {
                    EndpointKind::Tcp => "host and port",
                    EndpointKind::File => "file path",
                    EndpointKind::Uri => "connection string",
                }
            ));
        }
        errors
    }
}

fn matches_engine(engine: DatabaseEngine, endpoint: &Endpoint) -> bool {
    matches!(
        (engine.endpoint_kind(), endpoint),
        (EndpointKind::Tcp, Endpoint::Tcp { .. })
            | (EndpointKind::File, Endpoint::File { .. })
            | (EndpointKind::Uri, Endpoint::Uri { .. })
    )
}

/// Checks a MongoDB connection string. Credentials must go in the user and
/// password fields so the password lands in the OS vault, not in the profile.
fn validate_mongo_uri(uri: &str) -> Result<(), &'static str> {
    let uri = uri.trim();
    let Some(rest) = uri
        .strip_prefix("mongodb://")
        .or_else(|| uri.strip_prefix("mongodb+srv://"))
    else {
        return Err("Connection string must start with mongodb:// or mongodb+srv://");
    };
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err("Connection string needs a host");
    }
    if let Some((userinfo, _)) = authority.rsplit_once('@')
        && userinfo.contains(':')
    {
        return Err(
            "Remove the password from the connection string and use the Password field instead",
        );
    }
    Ok(())
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

    /// A profile exactly as written by FerrumData before MySQL/SQLite/SurrealDB/
    /// MongoDB support (no `namespace`, no `Uri` endpoint) must keep loading.
    #[test]
    fn legacy_postgres_profile_json_still_loads() {
        let json = r#"{
            "id": "6f0e8a52-58b7-4d2c-9a43-3f1f3c0f7b11",
            "display_name": "Local PG",
            "engine": "Postgres",
            "endpoint": {"Tcp": {"host": "localhost", "port": 5432}},
            "database": "app",
            "username": "postgres",
            "tls": "Prefer",
            "read_only": false,
            "env": "Local",
            "connect_timeout_secs": 10,
            "secret_ref": {"service": "dev.ferrumdata.connection", "account": "6f0e8a52-58b7-4d2c-9a43-3f1f3c0f7b11"}
        }"#;
        let p: ConnectionProfile = serde_json::from_str(json).expect("legacy profile");
        assert_eq!(p.engine, DatabaseEngine::Postgres);
        assert_eq!(p.namespace, None);
        assert!(!p.favorite);
        assert!(p.validate().is_empty());
    }

    #[test]
    fn every_engine_default_round_trips_and_validates_except_empty_sqlite_path() {
        for engine in DatabaseEngine::ALL {
            let p = ConnectionProfile::new(engine, "x");
            let back: ConnectionProfile =
                serde_json::from_str(&serde_json::to_string(&p).expect("ser")).expect("de");
            assert_eq!(back, p);
            let errors = p.validate();
            if engine == DatabaseEngine::Sqlite {
                assert_eq!(errors, vec!["File path is required".to_owned()]);
            } else {
                assert!(errors.is_empty(), "{engine:?}: {errors:?}");
            }
            assert!(matches_engine(engine, &p.endpoint));
        }
    }

    #[test]
    fn mongo_uri_rules() {
        assert!(validate_mongo_uri("mongodb://localhost:27017").is_ok());
        assert!(validate_mongo_uri("mongodb+srv://cluster0.example.net/?retryWrites=true").is_ok());
        assert!(validate_mongo_uri("mongodb://alice@host").is_ok());
        assert!(validate_mongo_uri("postgres://x").is_err());
        assert!(validate_mongo_uri("mongodb://").is_err());
        assert!(validate_mongo_uri("mongodb://alice:pw@host/db").is_err());
    }

    #[test]
    fn engine_specific_required_fields_and_endpoint_kind() {
        let mut p = ConnectionProfile::new(DatabaseEngine::SurrealDb, "s");
        p.namespace = None;
        assert_eq!(p.validate(), vec!["Namespace is required".to_owned()]);
        let mut p = ConnectionProfile::new(DatabaseEngine::MongoDb, "m");
        p.endpoint = Endpoint::Tcp {
            host: "h".into(),
            port: 1,
        };
        assert_eq!(p.validate().len(), 1);
    }

    #[test]
    fn target_labels_per_engine() {
        let s = ConnectionProfile::new(DatabaseEngine::SurrealDb, "s");
        assert_eq!(s.target_label(), "root@localhost:8000/test/test");
        let m = ConnectionProfile::new(DatabaseEngine::MongoDb, "m");
        assert_eq!(m.target_label(), "mongodb://localhost:27017/test");
    }

    #[test]
    fn default_tls_verifies() {
        assert!(TlsMode::default().verifies_server());
    }
}
