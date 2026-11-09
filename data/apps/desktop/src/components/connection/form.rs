//! Create/edit a connection profile for any supported engine. Passwords go to
//! the OS vault (or session memory) and are never written into the profile.
//! Rendering lives in `form_view.rs`.

use crate::commands::background;
use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrumdata_application::{AppServices, PasswordChange};
use ferrumdata_domain::{
    ConnectionProfile, DatabaseEngine, Endpoint, EndpointKind, EnvironmentTag, Secret, TlsMode,
};

#[derive(Clone, PartialEq)]
pub(super) enum TestState {
    Idle,
    Running,
    Ok(String),
    Err(String),
}

pub enum ConnectionFormEvent {
    Saved(Box<ConnectionProfile>),
    Cancelled,
}

pub struct ConnectionForm {
    services: AppServices,
    pub(super) base: ConnectionProfile,
    pub(super) is_new: bool,
    has_stored: bool,
    pub(super) engine: DatabaseEngine,
    pub(super) name: Entity<TextInput>,
    pub(super) host: Entity<TextInput>,
    pub(super) port: Entity<TextInput>,
    /// SQLite database file.
    pub(super) path: Entity<TextInput>,
    /// MongoDB connection string.
    pub(super) uri: Entity<TextInput>,
    pub(super) namespace: Entity<TextInput>,
    pub(super) database: Entity<TextInput>,
    pub(super) username: Entity<TextInput>,
    pub(super) password: Entity<TextInput>,
    pub(super) remember: bool,
    pub(super) tls: TlsMode,
    pub(super) env: EnvironmentTag,
    pub(super) read_only: bool,
    pub(super) test: TestState,
    pub(super) errors: Vec<String>,
    pub(super) saving: bool,
    _tasks: Vec<Task<()>>,
}

impl EventEmitter<ConnectionFormEvent> for ConnectionForm {}

impl ConnectionForm {
    pub fn new(
        services: AppServices,
        existing: Option<ConnectionProfile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let is_new = existing.is_none();
        let base = existing.unwrap_or_else(|| ConnectionProfile::new_postgres(""));
        let engine = base.engine;
        // Seed every endpoint field so switching engines keeps typed values.
        let (host, port) = match &base.endpoint {
            Endpoint::Tcp { host, port } => (host.clone(), port.to_string()),
            _ => ("localhost".to_owned(), port_text(engine)),
        };
        let path = match &base.endpoint {
            Endpoint::File { path } => path.clone(),
            _ => String::new(),
        };
        let uri = match &base.endpoint {
            Endpoint::Uri { uri } => uri.clone(),
            _ => default_uri(),
        };
        let has_stored = base.secret_ref.is_some();
        let mut field = |text: String, placeholder: &str| {
            let placeholder = placeholder.to_owned();
            cx.new(|cx| {
                TextInput::new(window, cx)
                    .with_text(text)
                    .placeholder(placeholder)
            })
        };
        let name = field(base.display_name.clone(), "e.g. Local database");
        let host = field(host, "");
        let port = field(port, "");
        let path = field(path, "/path/to/database.sqlite");
        let uri = field(
            uri,
            "mongodb://host:27017 or mongodb+srv://cluster.example.net",
        );
        let namespace = field(base.namespace.clone().unwrap_or_default(), "");
        let database = field(base.database.clone().unwrap_or_default(), "");
        let username = field(base.username.clone().unwrap_or_default(), "");
        let password = field(
            String::new(),
            if has_stored {
                "Stored in keychain — leave blank to keep"
            } else {
                "Password"
            },
        );
        password.update(cx, |input, cx| input.set_masked(true, cx));
        name.update(cx, |input, cx| input.focus(window, cx));

        Self {
            services,
            remember: is_new || has_stored,
            tls: base.tls,
            env: base.env,
            read_only: base.read_only,
            base,
            is_new,
            has_stored,
            engine,
            name,
            host,
            port,
            path,
            uri,
            namespace,
            database,
            username,
            password,
            test: TestState::Idle,
            errors: Vec::new(),
            saving: false,
            _tasks: Vec::new(),
        }
    }

    /// Switches engine. Fields still holding the previous engine's defaults
    /// (or empty) take the new engine's defaults; typed values are kept.
    pub(super) fn set_engine(&mut self, engine: DatabaseEngine, cx: &mut Context<Self>) {
        if engine == self.engine {
            return;
        }
        let old = ConnectionProfile::new(self.engine, "");
        let new = ConnectionProfile::new(engine, "");
        let swap = |input: &Entity<TextInput>, old: String, new: String, cx: &mut Context<Self>| {
            let current = input.read(cx).text().trim().to_owned();
            if current.is_empty() || current == old {
                input.update(cx, |i, cx| i.set_text(new, cx));
            }
        };
        swap(&self.port, port_text(self.engine), port_text(engine), cx);
        let text = |v: &Option<String>| v.clone().unwrap_or_default();
        swap(&self.database, text(&old.database), text(&new.database), cx);
        swap(&self.username, text(&old.username), text(&new.username), cx);
        swap(
            &self.namespace,
            text(&old.namespace),
            text(&new.namespace),
            cx,
        );
        if self.tls == old.tls {
            self.tls = new.tls;
        }
        self.engine = engine;
        self.test = TestState::Idle;
        self.errors.clear();
    }

    fn build(&self, cx: &App) -> Result<ConnectionProfile, Vec<String>> {
        let text = |input: &Entity<TextInput>| input.read(cx).text().trim().to_owned();
        let optional = |input: &Entity<TextInput>| Some(text(input)).filter(|s| !s.is_empty());
        let mut p = self.base.clone();
        p.engine = self.engine;
        p.display_name = text(&self.name);
        p.endpoint = match self.engine.endpoint_kind() {
            EndpointKind::Tcp => Endpoint::Tcp {
                host: text(&self.host),
                port: text(&self.port).parse::<u16>().unwrap_or(0),
            },
            EndpointKind::File => Endpoint::File {
                path: text(&self.path),
            },
            EndpointKind::Uri => Endpoint::Uri {
                uri: text(&self.uri),
            },
        };
        let is_sqlite = self.engine == DatabaseEngine::Sqlite;
        p.database = optional(&self.database).filter(|_| !is_sqlite);
        p.username = optional(&self.username).filter(|_| !is_sqlite);
        p.namespace =
            optional(&self.namespace).filter(|_| self.engine == DatabaseEngine::SurrealDb);
        p.tls = self.tls;
        p.env = self.env;
        p.read_only = self.read_only;
        let errs = p.validate();
        if errs.is_empty() { Ok(p) } else { Err(errs) }
    }

    fn typed_password(&self, cx: &App) -> Option<Secret> {
        if !self.engine.uses_password() {
            return None;
        }
        Some(self.password.read(cx).text().to_owned())
            .filter(|s| !s.is_empty())
            .map(Secret::new)
    }

    pub(super) fn browse_for_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        self._tasks.push(cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "file dialog failed");
                    None
                }
            };
            let Some(path) = path else { return };
            if let Err(e) = this.update(cx, |this, cx| {
                let path = path.to_string_lossy().into_owned();
                this.path.update(cx, |input, cx| input.set_text(path, cx));
                cx.notify();
            }) {
                tracing::debug!(error = %e, "connection form closed during file dialog");
            }
        }));
    }

    pub(super) fn test_connection(&mut self, cx: &mut Context<Self>) {
        let profile = match self.build(cx) {
            Ok(p) => p,
            Err(e) => {
                self.errors = e;
                cx.notify();
                return;
            }
        };
        self.errors.clear();
        self.test = TestState::Running;
        cx.notify();
        let password = self.typed_password(cx);
        let services = self.services.clone();
        self._tasks.push(cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.test(profile, password).await
            })
            .await;
            let test = match result {
                Ok(Ok(info)) => TestState::Ok(info.version),
                Ok(Err(e)) => TestState::Err(match e.suggestion() {
                    Some(s) => format!("{} — {s}", e.message),
                    None => e.message,
                }),
                Err(e) => TestState::Err(e),
            };
            if let Err(e) = this.update(cx, |this, cx| {
                this.test = test;
                cx.notify();
            }) {
                tracing::debug!(error = %e, "connection form closed during test");
            }
        }));
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let profile = match self.build(cx) {
            Ok(p) => p,
            Err(e) => {
                self.errors = e;
                cx.notify();
                return;
            }
        };
        let change = match (self.typed_password(cx), self.remember) {
            (Some(secret), remember) => PasswordChange::Set { secret, remember },
            // SQLite has no password: drop any stored one.
            (None, _) if !self.engine.uses_password() && self.has_stored => PasswordChange::Clear,
            (None, false) if self.has_stored => PasswordChange::Clear,
            (None, _) => PasswordChange::Keep,
        };
        self.saving = true;
        cx.notify();
        let services = self.services.clone();
        self._tasks.push(cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.save_profile(profile, change).await
            })
            .await;
            if let Err(e) = this.update(cx, |this, cx| match result {
                Ok(Ok(saved)) => cx.emit(ConnectionFormEvent::Saved(Box::new(saved))),
                Ok(Err(e)) => {
                    this.errors = vec![e.message];
                    this.saving = false;
                    cx.notify();
                }
                Err(e) => {
                    this.errors = vec![e];
                    this.saving = false;
                    cx.notify();
                }
            }) {
                tracing::debug!(error = %e, "connection form closed during save");
            }
        }));
    }
}

fn port_text(engine: DatabaseEngine) -> String {
    engine
        .default_port()
        .map(|p| p.to_string())
        .unwrap_or_default()
}

fn default_uri() -> String {
    match ConnectionProfile::new(DatabaseEngine::MongoDb, "").endpoint {
        Endpoint::Uri { uri } => uri,
        _ => String::new(),
    }
}
