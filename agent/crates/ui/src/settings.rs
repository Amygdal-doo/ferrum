//! The settings sheet: per provider, the API key (typed by the user,
//! stored in the OS keychain, never shown again — only where it comes
//! from and a short hint), the models offered in the selector, the base
//! URL and a Test (one tiny request); custom models can be removed, and
//! added below ([`crate::custom_form`]). It works before the session has
//! opened: it edits `settings.json`, which every Ferrum app with AI reads.

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, TextInput};
use ferrumagent_core::custom_models::{COMING_SOON, FERRUM_AI_LABEL};
use ferrumagent_core::models::ModelStore;
use ferrumagent_core::secrets::KeySource;
use ferrumagent_core::settings::{AgentSettings, ProviderKind, ProviderSettings};
use gpui::{Context, Entity, Window};

use crate::custom_form::{CustomForm, Test};
use crate::view::ChatView;

pub(crate) struct ProviderForm {
    pub id: String,
    pub name: String,
    pub needs_key: bool,
    pub custom: bool,
    pub key: Entity<TextInput>,
    pub models: Entity<TextInput>,
    pub base_url: Entity<TextInput>,
    pub status: String,
    pub test: Test,
}

pub(crate) struct SettingsSheet {
    pub settings: AgentSettings,
    pub forms: Vec<ProviderForm>,
    pub custom: CustomForm,
}

fn kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "Anthropic Messages API",
        ProviderKind::OpenAiChat => "OpenAI-compatible Chat Completions",
        ProviderKind::OpenAiResponses => "OpenAI Responses API",
        ProviderKind::Ollama => "Ollama (local, no key)",
    }
}

/// A comma-separated model list as typed.
fn parse_models(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect()
}

impl ChatView {
    pub(crate) fn provider_form(
        p: &ProviderSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ProviderForm {
        ProviderForm {
            id: p.id.clone(),
            name: p.name.clone(),
            needs_key: p.needs_key(),
            custom: p.custom,
            key: cx.new(|cx| {
                TextInput::new(window, cx)
                    .masked(true)
                    .monospace()
                    .placeholder(match &p.env_key {
                        Some(var) => format!("Paste a key (or set {var})"),
                        None => "Paste a key".into(),
                    })
            }),
            models: cx.new(|cx| TextInput::new(window, cx).with_text(p.models.join(", "))),
            base_url: cx.new(|cx| {
                TextInput::new(window, cx)
                    .monospace()
                    .with_text(p.base_url.clone())
            }),
            status: "Checking…".into(),
            test: Test::Idle,
        }
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.models().settings();
        let forms = settings
            .providers
            .iter()
            .map(|p| Self::provider_form(p, window, cx))
            .collect();
        let custom = CustomForm::new(window, cx);
        self.settings = Some(SettingsSheet {
            settings,
            forms,
            custom,
        });
        self.refocus(window, cx);
        self.load_key_status(cx);
        cx.notify();
    }

    pub(crate) fn load_key_status(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.settings.as_ref() else {
            return;
        };
        let models = self.models();
        let ids: Vec<(String, bool)> = sheet
            .forms
            .iter()
            .map(|f| (f.id.clone(), f.needs_key))
            .collect();
        let task = cx.spawn(async move |this, cx| {
            let status = cx
                .background_spawn(async move {
                    ids.into_iter()
                        .map(|(id, needs)| {
                            let status = if !needs {
                                "No key needed".to_string()
                            } else {
                                match models.key_status(&id) {
                                    Some((KeySource::Keychain, hint)) => {
                                        format!("Key in keychain ({hint})")
                                    }
                                    Some((KeySource::Env, hint)) => {
                                        format!("Key from environment ({hint})")
                                    }
                                    None => "No key".to_string(),
                                }
                            };
                            (id, status)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(sheet) = this.settings.as_mut() {
                    for (id, status) in status {
                        if let Some(form) = sheet.forms.iter_mut().find(|f| f.id == id) {
                            form.status = status;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    /// Runs a blocking keychain or settings call off the UI thread.
    pub(crate) fn run_models_blocking(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&ModelStore) -> Result<(), String> + std::marker::Send + 'static,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let models = self.models();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { f(&models) }).await;
            this.update(cx, |this, cx| match result {
                Ok(()) => done(this, cx),
                Err(e) => this.show_notice(Notice::error(e), cx),
            })
            .ok();
        })
        .detach();
    }

    fn save_key(&mut self, index: usize, clear: bool, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref().and_then(|s| s.forms.get(index)) else {
            return;
        };
        let id = form.id.clone();
        let key = if clear {
            String::new()
        } else {
            form.key.read(cx).text().to_string()
        };
        if !clear && key.trim().is_empty() {
            return;
        }
        form.key.update(cx, |input, cx| input.set_text("", cx));
        self.run_models_blocking(
            cx,
            move |models| models.set_api_key(&id, &key),
            move |this, cx| {
                this.load_key_status(cx);
                this.show_notice(
                    Notice::info(if clear {
                        "Key removed."
                    } else {
                        "Key saved in the keychain."
                    }),
                    cx,
                );
            },
        );
    }

    /// Asks the provider for its models and puts them in the form.
    fn fetch_models(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref().and_then(|s| s.forms.get(index)) else {
            return;
        };
        let models = form.models.clone();
        let fetch = self.models().fetch_models(&form.id);
        cx.spawn(async move |this, cx| {
            let result = fetch.await;
            this.update(cx, |this, cx| match result {
                Ok(list) if !list.is_empty() => {
                    let ids: Vec<String> = list.into_iter().map(|m| m.id).collect();
                    models.update(cx, |input, cx| input.set_text(ids.join(", "), cx));
                    this.show_notice(
                        Notice::info(format!("{} models found; edit, then Save.", ids.len())),
                        cx,
                    );
                }
                Ok(_) => this.show_notice(Notice::info("The provider listed no models."), cx),
                Err(e) => this.show_notice(Notice::error(e), cx),
            })
            .ok();
        })
        .detach();
    }

    /// The provider as edited in its form (base URL and models).
    fn edited_provider(&self, index: usize, cx: &App) -> Option<ProviderSettings> {
        let sheet = self.settings.as_ref()?;
        let form = sheet.forms.get(index)?;
        let mut provider = sheet.settings.provider(&form.id)?.clone();
        let models = parse_models(form.models.read(cx).text());
        if !models.is_empty() {
            if !models.contains(&provider.default_model) {
                provider.default_model = models[0].clone();
            }
            provider.models = models;
        }
        let url = form.base_url.read(cx).text().trim().to_string();
        if !url.is_empty() {
            provider.base_url = url;
        }
        Some(provider)
    }

    /// Sends the provider's default model one tiny request.
    pub(crate) fn test_provider(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(provider) = self.edited_provider(index, cx) else {
            return;
        };
        let model = provider.default_model.clone();
        let ping = self.models().ping_provider(provider, &model);
        if let Some(form) = self.settings.as_mut().and_then(|s| s.forms.get_mut(index)) {
            form.test = Test::Running;
        }
        cx.notify();
        let id = self.settings.as_ref().map(|s| s.forms[index].id.clone());
        cx.spawn(async move |this, cx| {
            let result = ping.await;
            this.update(cx, |this, cx| {
                let form = this
                    .settings
                    .as_mut()
                    .and_then(|s| s.forms.iter_mut().find(|f| Some(&f.id) == id.as_ref()));
                if let Some(form) = form {
                    form.test =
                        Test::Done(result.map(|pong| format!("{model}: {}", pong.summary())));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn remove_custom(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref().and_then(|s| s.forms.get(index)) else {
            return;
        };
        let (id, name) = (form.id.clone(), form.name.clone());
        let removed = id.clone();
        self.run_models_blocking(
            cx,
            move |models| models.remove_custom(&id),
            move |this, cx| {
                if let Some(sheet) = this.settings.as_mut() {
                    sheet.forms.retain(|f| f.id != removed);
                    sheet.settings.providers.retain(|p| p.id != removed);
                }
                this.show_notice(Notice::info(format!("Removed {name}.")), cx);
            },
        );
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.settings.as_ref() else {
            return;
        };
        let mut settings = sheet.settings.clone();
        for index in 0..sheet.forms.len() {
            if let Some(edited) = self.edited_provider(index, cx)
                && let Some(provider) = settings.providers.iter_mut().find(|p| p.id == edited.id)
            {
                *provider = edited;
            }
        }
        match self.models().save_providers(&settings) {
            Ok(()) => {
                self.settings = None;
                self.show_notice(Notice::info("Settings saved."), cx);
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = None;
        self.refocus(window, cx);
        cx.notify();
    }

    pub(crate) fn render_settings(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let custom = self.render_custom_form(cx).into_any_element();
        let Some(sheet) = self.settings.as_ref() else {
            return div().into_any_element();
        };
        let muted =
            |text: &'static str| Label::new(text).size(LabelSize::XSmall).color(Color::Muted);
        v_flex()
            .id("chat-settings")
            .absolute()
            .inset_0()
            .occlude()
            .bg(colors.elevated_surface_background)
            .child(
                h_flex()
                    .flex_none()
                    .h(px(36.))
                    .px_2()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().flex_1().child(Label::new("Models and API keys").size(LabelSize::Small)))
                    .child(
                        IconButton::new("chat-settings-close", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| this.close_settings(window, cx))),
                    ),
            )
            .child(
                v_flex()
                    .id("chat-settings-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .gap_3()
                    // Ferrum's own models: listed, not usable yet.
                    .child(
                        h_flex()
                            .id("chat-settings-ferrum-ai")
                            .debug_selector(|| "chat-settings-ferrum-ai".into())
                            .gap_1p5()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border_variant)
                            .opacity(0.6)
                            .child(Label::new(FERRUM_AI_LABEL).size(LabelSize::Small))
                            .child(div().flex_1())
                            .child(muted(COMING_SOON)),
                    )
                    .children(sheet.forms.iter().enumerate().map(|(i, form)| {
                        let kind = sheet
                            .settings
                            .provider(&form.id)
                            .map_or("", |p| kind_label(p.kind));
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border_variant)
                            .child(
                                h_flex()
                                    .justify_between()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .gap_1p5()
                                            .min_w_0()
                                            .when_some(provider_logo(&form.id, &form.name), |this, logo| {
                                                this.child(ferrum_ui::agent_logo(logo, px(16.), cx))
                                            })
                                            .child(Label::new(form.name.clone()).size(LabelSize::Small)),
                                    )
                                    .child(Label::new(kind).size(LabelSize::XSmall).color(Color::Muted).single_line().truncate()),
                            )
                            .when(form.needs_key, |this| {
                                this.child(Label::new(form.status.clone()).size(LabelSize::XSmall).color(Color::Muted))
                                    .child(form.key.clone())
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .child(
                                                Button::new(("chat-key-save", i), "Save key")
                                                    .style(ButtonStyle::Filled)
                                                    .label_size(LabelSize::XSmall)
                                                    .on_click(cx.listener(move |this, _, _, cx| this.save_key(i, false, cx))),
                                            )
                                            .child(
                                                Button::new(("chat-key-clear", i), "Remove key")
                                                    .style(ButtonStyle::Subtle)
                                                    .label_size(LabelSize::XSmall)
                                                    .on_click(cx.listener(move |this, _, _, cx| this.save_key(i, true, cx))),
                                            ),
                                    )
                            })
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(muted("Models (comma separated)"))
                                    .child(
                                        Button::new(("chat-fetch-models", i), "Fetch models")
                                            .style(ButtonStyle::Transparent)
                                            .label_size(LabelSize::XSmall)
                                            .on_click(cx.listener(move |this, _, _, cx| this.fetch_models(i, cx))),
                                    ),
                            )
                            .child(form.models.clone())
                            .child(muted("Base URL"))
                            .child(form.base_url.clone())
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        div().debug_selector(move || format!("chat-test-{i}")).child(
                                            Button::new(("chat-test", i), "Test")
                                                .style(ButtonStyle::Outlined)
                                                .label_size(LabelSize::XSmall)
                                                .disabled(form.test == Test::Running)
                                                .tooltip(Tooltip::text("Send the first model a tiny request"))
                                                .on_click(cx.listener(move |this, _, _, cx| this.test_provider(i, cx))),
                                        ),
                                    )
                                    .when(form.custom, |this| {
                                        this.child(
                                            Button::new(("chat-remove-custom", i), "Remove model")
                                                .style(ButtonStyle::Subtle)
                                                .label_size(LabelSize::XSmall)
                                                .on_click(cx.listener(move |this, _, _, cx| this.remove_custom(i, cx))),
                                        )
                                    })
                                    .child(form.test.render()),
                            )
                    }))
                    .child(custom)
                    .child(muted(
                        "Keys are stored in the OS keychain and only sent to their provider. \
                         Models added here appear in every Ferrum app with AI. \
                         External agents (Claude Code, Codex, Gemini CLI) sign in on their own; \
                         configure them in ~/.ferrum/agent/agents.json and MCP servers in ~/.ferrum/agent/mcp.json.",
                    )),
            )
            .child(
                h_flex()
                    .flex_none()
                    .p_2()
                    .gap_1()
                    .justify_end()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(
                        Button::new("chat-settings-cancel", "Cancel")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| this.close_settings(window, cx))),
                    )
                    .child(
                        Button::new("chat-settings-save", "Save")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
            .into_any_element()
    }
}

/// A provider's logo, by id (`anthropic`, `ollama`) or else its name.
fn provider_logo(id: &str, name: &str) -> Option<ferrum_ui::AgentLogo> {
    ferrum_ui::AgentLogo::for_provider(id).or_else(|| ferrum_ui::AgentLogo::for_provider(name))
}
