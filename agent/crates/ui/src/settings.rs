//! The settings sheet: per provider, the API key (typed by the user,
//! stored in the OS keychain, never shown again — only where it comes
//! from and a short hint), the models offered in the selector and the base
//! URL (custom OpenAI-compatible endpoints).

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, TextInput};
use ferrumagent_core::secrets::KeySource;
use ferrumagent_core::settings::{AgentSettings, ProviderKind};
use gpui::{Context, Entity, Window};

use crate::view::ChatView;

pub(crate) struct ProviderForm {
    pub id: String,
    pub name: String,
    pub needs_key: bool,
    pub key: Entity<TextInput>,
    pub models: Entity<TextInput>,
    pub base_url: Entity<TextInput>,
    pub status: String,
}

pub(crate) struct SettingsSheet {
    pub settings: AgentSettings,
    pub forms: Vec<ProviderForm>,
}

fn kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "Anthropic Messages API",
        ProviderKind::OpenAiChat => "OpenAI-compatible Chat Completions",
        ProviderKind::OpenAiResponses => "OpenAI Responses API",
        ProviderKind::Ollama => "Ollama (local, no key)",
    }
}

impl ChatView {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            self.show_notice(Notice::error("The chat is still starting."), cx);
            return;
        };
        let settings = session.settings();
        let forms = settings
            .providers
            .iter()
            .map(|p| ProviderForm {
                id: p.id.clone(),
                name: p.name.clone(),
                needs_key: p.needs_key(),
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
            })
            .collect();
        self.settings = Some(SettingsSheet { settings, forms });
        self.refocus(window, cx);
        self.load_key_status(cx);
        cx.notify();
    }

    fn load_key_status(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(sheet)) = (self.session.clone(), self.settings.as_ref()) else {
            return;
        };
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
                            if !needs {
                                return "No key needed".to_string();
                            }
                            match session.key_status(&id) {
                                Some((KeySource::Keychain, hint)) => {
                                    format!("Key in keychain ({hint})")
                                }
                                Some((KeySource::Env, hint)) => {
                                    format!("Key from environment ({hint})")
                                }
                                None => "No key".to_string(),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(sheet) = this.settings.as_mut() {
                    for (form, status) in sheet.forms.iter_mut().zip(status) {
                        form.status = status;
                    }
                }
                cx.notify();
            })
            .ok();
        });
        task.detach();
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
        self.run_blocking(cx, move |s| s.set_api_key(&id, &key));
        self.load_key_status(cx);
        self.show_notice(
            Notice::info(if clear {
                "Key removed."
            } else {
                "Key saved in the keychain."
            }),
            cx,
        );
    }

    /// Asks the provider for its models and puts them in the form.
    fn fetch_models(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(session), Some(form)) = (
            self.session.clone(),
            self.settings.as_ref().and_then(|s| s.forms.get(index)),
        ) else {
            return;
        };
        let models = form.models.clone();
        let fetch = session.fetch_models(&form.id);
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

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(sheet)) = (self.session.clone(), self.settings.as_ref()) else {
            return;
        };
        let mut settings = sheet.settings.clone();
        for form in &sheet.forms {
            let Some(provider) = settings.providers.iter_mut().find(|p| p.id == form.id) else {
                continue;
            };
            let models: Vec<String> = form
                .models
                .read(cx)
                .text()
                .split([',', '\n'])
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .collect();
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
        }
        match session.update_settings(settings) {
            Ok(()) => {
                self.settings = None;
                self.show_notice(Notice::info("Settings saved."), cx);
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
    }

    pub(crate) fn render_settings(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let Some(sheet) = self.settings.as_ref() else {
            return div().into_any_element();
        };
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
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.settings = None;
                                this.refocus(window, cx);
                                cx.notify();
                            })),
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
                                    .child(Label::new("Models (comma separated)").size(LabelSize::XSmall).color(Color::Muted))
                                    .child(
                                        Button::new(("chat-fetch-models", i), "Fetch models")
                                            .style(ButtonStyle::Transparent)
                                            .label_size(LabelSize::XSmall)
                                            .on_click(cx.listener(move |this, _, _, cx| this.fetch_models(i, cx))),
                                    ),
                            )
                            .child(form.models.clone())
                            .child(Label::new("Base URL").size(LabelSize::XSmall).color(Color::Muted))
                            .child(form.base_url.clone())
                    }))
                    .child(
                        Label::new(
                            "Keys are stored in the OS keychain and only sent to their provider. \
                             External agents (Claude Code, Codex, Gemini CLI) sign in on their own; \
                             configure them in ~/.ferrum/agent/agents.json and MCP servers in ~/.ferrum/agent/mcp.json.",
                        )
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    ),
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
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.settings = None;
                                this.refocus(window, cx);
                                cx.notify();
                            })),
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
