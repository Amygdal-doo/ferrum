//! Rendering of the connection form: the fields shown depend on the engine
//! (host/port, database file, or connection string).

use super::form::{ConnectionForm, ConnectionFormEvent, TestState};
use ferrum_ui::prelude::*;
use ferrumdata_domain::{DatabaseEngine, EndpointKind, EnvironmentTag, TlsMode};

impl ConnectionForm {
    fn field(label: &'static str, input: &Entity<TextInput>) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .child(Label::new(label).size(LabelSize::Small))
            .child(input.clone())
    }

    fn hint(text: &'static str) -> AnyElement {
        Label::new(text)
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn picker<T: Copy + PartialEq + 'static>(
        id: &'static str,
        options: &[T],
        current: T,
        label: fn(T) -> &'static str,
        apply: fn(&mut Self, T, &mut Context<Self>),
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
                            apply(this, option, cx);
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

    /// Host/port, database file, or connection string.
    fn endpoint_fields(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.engine.endpoint_kind() {
            EndpointKind::Tcp if self.engine == DatabaseEngine::Redis => v_flex()
                .gap_1p5()
                .child(
                    h_flex()
                        .gap_3()
                        .items_start()
                        .child(div().flex_1().child(Self::field("Host or URL", &self.host)))
                        .child(div().w(px(110.)).child(Self::field("Port", &self.port))),
                )
                .child(Self::hint(
                    "A redis:// or rediss:// URL replaces host and port (rediss:// enables TLS, \
                     #insecure skips verification). Put the password in the Password field.",
                ))
                .into_any_element(),
            EndpointKind::Tcp => h_flex()
                .gap_3()
                .items_start()
                .child(div().flex_1().child(Self::field("Host", &self.host)))
                .child(div().w(px(110.)).child(Self::field("Port", &self.port)))
                .into_any_element(),
            EndpointKind::File => v_flex()
                .gap_1p5()
                .child(Label::new("Database file").size(LabelSize::Small))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(self.path.clone()))
                        .child(
                            Button::new("cf-browse", "Browse…")
                                .style(ButtonStyle::Outlined)
                                .on_click(cx.listener(|this, _, _, cx| this.browse_for_file(cx))),
                        ),
                )
                .child(Self::hint(
                    "The file must already exist; FerrumData never creates databases.",
                ))
                .into_any_element(),
            EndpointKind::Uri => v_flex()
                .gap_1p5()
                .child(Self::field("Connection string", &self.uri))
                .child(Self::hint(
                    "TLS, replica set and authSource go in the connection string \
                     (mongodb+srv:// enables TLS). Put the password in the Password field.",
                ))
                .into_any_element(),
        }
    }

    /// Database (+ namespace for SurrealDB) and user. None for SQLite.
    fn database_fields(&self) -> Option<AnyElement> {
        let db_label = match self.engine {
            DatabaseEngine::Sqlite => return None,
            DatabaseEngine::MySql => "Database (optional)",
            DatabaseEngine::Redis => "DB index",
            _ => "Database",
        };
        let mut row = h_flex().gap_3().items_start();
        if self.engine == DatabaseEngine::SurrealDb {
            row = row.child(
                div()
                    .flex_1()
                    .child(Self::field("Namespace", &self.namespace)),
            );
        }
        Some(
            row.child(div().flex_1().child(Self::field(db_label, &self.database)))
                .child(div().flex_1().child(Self::field(
                    if self.engine == DatabaseEngine::Redis {
                        "ACL user (optional)"
                    } else {
                        "User"
                    },
                    &self.username,
                )))
                .into_any_element(),
        )
    }

    fn tls_hint(&self) -> Option<AnyElement> {
        match self.engine {
            DatabaseEngine::SurrealDb => Some(Self::hint(if self.tls == TlsMode::Disable {
                "Plain ws:// connection."
            } else {
                "wss:// with server certificate verification."
            })),
            DatabaseEngine::Redis if self.tls == TlsMode::Disable => {
                Some(Self::hint("Plain TCP (a rediss:// URL still enables TLS)."))
            }
            DatabaseEngine::Redis if self.tls.verifies_server() => Some(Self::hint(
                "TLS; the certificate chain and host name are verified.",
            )),
            _ if !self.tls.verifies_server() => Some(
                Label::new("Server certificate is not verified.")
                    .size(LabelSize::XSmall)
                    .color(Color::Warning)
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    fn read_only_label(&self) -> &'static str {
        match self.engine {
            DatabaseEngine::Postgres | DatabaseEngine::MySql => {
                "Read-only session (server-enforced default; use a read-only role for real protection)"
            }
            DatabaseEngine::Sqlite => "Open the file read-only (enforced by SQLite)",
            DatabaseEngine::SurrealDb | DatabaseEngine::MongoDb => {
                "Read-only: FerrumData refuses writing statements (use a read-only user for real protection)"
            }
            DatabaseEngine::Redis => {
                "Read-only: only commands on FerrumData's read allow-list run and key edits are \
                 disabled (use an ACL user without write permissions for real protection)"
            }
        }
    }
}

impl Render for ConnectionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = cx.theme().status().clone();
        let title = if self.is_new {
            "New connection".to_owned()
        } else {
            format!("Edit {} connection", self.engine.label())
        };
        let engine = Self::picker(
            "cf-engine",
            &DatabaseEngine::ALL,
            self.engine,
            DatabaseEngine::label,
            Self::set_engine,
            window,
            cx,
        );
        let tls = Self::picker(
            "cf-tls",
            &TlsMode::ALL,
            self.tls,
            TlsMode::label,
            |this, mode, _| this.tls = mode,
            window,
            cx,
        );
        let env = Self::picker(
            "cf-env",
            &EnvironmentTag::ALL,
            self.env,
            EnvironmentTag::label,
            |this, tag, _| this.env = tag,
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
        // TLS is part of the MongoDB connection string; SQLite is a local file.
        let shows_tls = !matches!(
            self.engine,
            DatabaseEngine::Sqlite | DatabaseEngine::MongoDb
        );
        let tls_column = shows_tls.then(|| {
            v_flex()
                .flex_1()
                .gap_1p5()
                .child(Label::new("TLS").size(LabelSize::Small))
                .child(tls)
                .children(self.tls_hint())
        });
        let password = self.engine.uses_password().then(|| {
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
                )
        });

        Modal::new(title)
            .width(px(560.))
            .on_dismiss(cx.listener(|_, _, _, cx| cx.emit(ConnectionFormEvent::Cancelled)))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().flex_1().child(Self::field("Name", &self.name)))
                    .child(
                        v_flex()
                            .w(px(170.))
                            .gap_1p5()
                            .child(Label::new("Engine").size(LabelSize::Small))
                            .child(engine),
                    ),
            )
            .child(self.endpoint_fields(cx))
            .children(self.database_fields())
            .children(password)
            .child(
                h_flex().gap_3().items_start().children(tls_column).child(
                    v_flex()
                        .flex_1()
                        .gap_1p5()
                        .child(Label::new("Environment").size(LabelSize::Small))
                        .child(env),
                ),
            )
            .child(
                ferrum_ui::ui::Checkbox::new("cf-read-only", toggle(self.read_only))
                    .label(self.read_only_label())
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
                        this.read_only = state.selected();
                        cx.notify();
                    })),
            )
            .children(
                self.errors
                    .iter()
                    .map(|e| div().text_sm().text_color(status.error).child(e.clone())),
            )
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
