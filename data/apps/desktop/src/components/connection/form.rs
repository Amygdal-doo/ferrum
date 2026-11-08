//! Create/edit PostgreSQL connection profile. Passwords go to the OS vault
//! (or session memory) and are never written into the profile.

use crate::commands::background;
use ferrum_ui::prelude::*;
use ferrumdata_application::{AppServices, PasswordChange};
use ferrumdata_domain::{ConnectionProfile, Endpoint, EnvironmentTag, Secret, TlsMode};

#[derive(Clone, PartialEq)]
enum TestState {
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
    base: ConnectionProfile,
    is_new: bool,
    has_stored: bool,
    name: Entity<TextInput>,
    host: Entity<TextInput>,
    port: Entity<TextInput>,
    database: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    remember: bool,
    tls: TlsMode,
    env: EnvironmentTag,
    read_only: bool,
    test: TestState,
    errors: Vec<String>,
    saving: bool,
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
        let (host, port) = match &base.endpoint {
            Endpoint::Tcp { host, port } => (host.clone(), port.to_string()),
            Endpoint::File { .. } => ("localhost".into(), "5432".into()),
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
        let name = field(base.display_name.clone(), "e.g. Local Postgres");
        let host = field(host, "");
        let port = field(port, "");
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
            name,
            host,
            port,
            database,
            username,
            password,
            test: TestState::Idle,
            errors: Vec::new(),
            saving: false,
            _tasks: Vec::new(),
        }
    }

    fn build(&self, cx: &App) -> Result<ConnectionProfile, Vec<String>> {
        let text = |input: &Entity<TextInput>| input.read(cx).text().trim().to_owned();
        let mut p = self.base.clone();
        p.display_name = text(&self.name);
        p.endpoint = Endpoint::Tcp {
            host: text(&self.host),
            port: text(&self.port).parse::<u16>().unwrap_or(0),
        };
        p.database = Some(text(&self.database)).filter(|s| !s.is_empty());
        p.username = Some(text(&self.username)).filter(|s| !s.is_empty());
        p.tls = self.tls;
        p.env = self.env;
        p.read_only = self.read_only;
        let errs = p.validate();
        if errs.is_empty() { Ok(p) } else { Err(errs) }
    }

    fn typed_password(&self, cx: &App) -> Option<Secret> {
        Some(self.password.read(cx).text().to_owned())
            .filter(|s| !s.is_empty())
            .map(Secret::new)
    }

    fn test_connection(&mut self, cx: &mut Context<Self>) {
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

    fn save(&mut self, cx: &mut Context<Self>) {
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

    fn field(label: &'static str, input: &Entity<TextInput>) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .child(Label::new(label).size(LabelSize::Small))
            .child(input.clone())
    }

    fn picker<T: Copy + PartialEq + 'static>(
        id: &'static str,
        options: &[T],
        current: T,
        label: fn(T) -> &'static str,
        apply: fn(&mut Self, T),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity().downgrade();
        let options = options.to_vec();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for option in options {
                let this = this.clone();
                menu = menu.toggleable_entry(
                    label(option),
                    option == current,
                    ferrum_ui::ui::IconPosition::Start,
                    None,
                    move |_, cx| {
                        if let Err(e) = this.update(cx, |this, cx| {
                            apply(this, option);
                            cx.notify();
                        }) {
                            tracing::debug!(error = %e, "connection form closed");
                        }
                    },
                );
            }
            menu
        });
        DropdownMenu::new(id, label(current), menu)
            .full_width(true)
            .into_any_element()
    }
}

impl Render for ConnectionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = cx.theme().status().clone();
        let title = if self.is_new {
            "New PostgreSQL connection"
        } else {
            "Edit connection"
        };
        let tls = Self::picker(
            "cf-tls",
            &TlsMode::ALL,
            self.tls,
            TlsMode::label,
            |this, mode| this.tls = mode,
            window,
            cx,
        );
        let env = Self::picker(
            "cf-env",
            &EnvironmentTag::ALL,
            self.env,
            EnvironmentTag::label,
            |this, tag| this.env = tag,
            window,
            cx,
        );
        let test_line = match &self.test {
            TestState::Idle => None,
            TestState::Running => Some(
                Label::new("Testing connection…")
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
            ),
            TestState::Ok(v) => Some(
                Label::new(format!("Connected: {v}"))
                    .size(LabelSize::Small)
                    .color(Color::Success)
                    .into_any_element(),
            ),
            TestState::Err(e) => Some(
                Label::new(e.clone())
                    .size(LabelSize::Small)
                    .color(Color::Error)
                    .into_any_element(),
            ),
        };
        let toggle = |checked: bool| {
            if checked {
                ToggleState::Selected
            } else {
                ToggleState::Unselected
            }
        };

        Modal::new(title)
            .width(px(560.))
            .on_dismiss(cx.listener(|_, _, _, cx| cx.emit(ConnectionFormEvent::Cancelled)))
            .child(Self::field("Name", &self.name))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().flex_1().child(Self::field("Host", &self.host)))
                    .child(div().w(px(110.)).child(Self::field("Port", &self.port))),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().flex_1().child(Self::field("Database", &self.database)))
                    .child(div().flex_1().child(Self::field("User", &self.username))),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(Self::field("Password", &self.password))
                    .child(
                        ferrum_ui::ui::Checkbox::new("cf-remember", toggle(self.remember))
                            .label("Save password in the OS keychain (otherwise kept for this session only)")
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
                                this.remember = state.selected();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1p5()
                            .child(Label::new("TLS").size(LabelSize::Small))
                            .child(tls)
                            .when(!self.tls.verifies_server(), |this| {
                                this.child(
                                    Label::new("Server certificate is not verified.")
                                        .size(LabelSize::XSmall)
                                        .color(Color::Warning),
                                )
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1p5()
                            .child(Label::new("Environment").size(LabelSize::Small))
                            .child(env),
                    ),
            )
            .child(
                ferrum_ui::ui::Checkbox::new("cf-read-only", toggle(self.read_only))
                    .label("Read-only session (server-enforced default; use a read-only role for real protection)")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
                        this.read_only = state.selected();
                        cx.notify();
                    })),
            )
            .children(self.errors.iter().map(|e| {
                div()
                    .text_sm()
                    .text_color(status.error)
                    .child(e.clone())
            }))
            .children(test_line)
            .footer(
                Button::new("cf-test", "Test connection")
                    .style(ButtonStyle::Outlined)
                    .disabled(self.test == TestState::Running)
                    .on_click(cx.listener(|this, _, _, cx| this.test_connection(cx))),
            )
            .footer(
                Button::new("cf-cancel", "Cancel")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ConnectionFormEvent::Cancelled))),
            )
            .footer(
                Button::new("cf-save", "Save")
                    .style(ButtonStyle::Filled)
                    .disabled(self.saving)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
    }
}
