//! "Add a custom model" at the bottom of the settings sheet: an
//! OpenAI-compatible or Anthropic-compatible endpoint (or one of the known
//! ones), its model id, a display name and a key for the keychain; Test
//! sends it a tiny request before it is added.

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, TextInput};
use ferrumagent_core::custom_models::{CustomKind, CustomModel, PRESETS};
use gpui::{Context, Entity, Window};

use crate::view::ChatView;

/// A Test button's last result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Test {
    Idle,
    Running,
    Done(Result<String, String>),
}

impl Test {
    pub(crate) fn render(&self) -> AnyElement {
        let (text, color) = match self {
            Test::Idle => return div().into_any_element(),
            Test::Running => ("Testing…".to_string(), Color::Muted),
            Test::Done(Ok(text)) => (text.clone(), Color::Success),
            Test::Done(Err(text)) => (text.clone(), Color::Error),
        };
        div()
            .min_w_0()
            .flex_1()
            .child(Label::new(text).size(LabelSize::XSmall).color(color))
            .into_any_element()
    }
}

pub(crate) struct CustomForm {
    pub kind: CustomKind,
    pub name: Entity<TextInput>,
    pub base_url: Entity<TextInput>,
    pub model: Entity<TextInput>,
    pub key: Entity<TextInput>,
    pub test: Test,
}

impl CustomForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<ChatView>) -> Self {
        let input = |placeholder: &'static str,
                     mono: bool,
                     window: &mut Window,
                     cx: &mut Context<ChatView>| {
            cx.new(|cx| {
                let input = TextInput::new(window, cx).placeholder(placeholder);
                if mono { input.monospace() } else { input }
            })
        };
        Self {
            kind: CustomKind::OpenAi,
            name: input("Display name (e.g. DeepSeek Chat)", false, window, cx),
            base_url: input(CustomKind::OpenAi.placeholder_url(), true, window, cx),
            model: input("Model id (e.g. deepseek-chat)", true, window, cx),
            key: cx.new(|cx| {
                TextInput::new(window, cx)
                    .masked(true)
                    .monospace()
                    .placeholder("API key (stored in the keychain)")
            }),
            test: Test::Idle,
        }
    }

    fn model(&self, cx: &App) -> CustomModel {
        CustomModel {
            kind: self.kind,
            name: self.name.read(cx).text().to_string(),
            base_url: self.base_url.read(cx).text().to_string(),
            model: self.model.read(cx).text().to_string(),
        }
    }
}

impl ChatView {
    fn custom_form(&mut self) -> Option<&mut CustomForm> {
        self.settings.as_mut().map(|s| &mut s.custom)
    }

    fn set_custom_kind(&mut self, kind: CustomKind, url: Option<&str>, cx: &mut Context<Self>) {
        let Some(form) = self.custom_form() else {
            return;
        };
        form.kind = kind;
        form.test = Test::Idle;
        let input = form.base_url.clone();
        input.update(cx, |input, cx| {
            input.set_placeholder(kind.placeholder_url(), cx);
            if let Some(url) = url {
                input.set_text(url, cx);
            }
        });
        cx.notify();
    }

    /// Sends the model as typed one tiny request, with the typed key.
    pub(crate) fn test_custom(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref().map(|s| &s.custom) else {
            return;
        };
        let model = form.model(cx);
        let key = form.key.read(cx).text().to_string();
        let ping = self.models().ping_custom(&model, &key);
        if let Some(form) = self.custom_form() {
            form.test = Test::Running;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = ping.await;
            this.update(cx, |this, cx| {
                if let Some(form) = this.custom_form() {
                    form.test = Test::Done(result.map(|pong| pong.summary()));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Adds the model (its key to the keychain) and lists it above.
    pub(crate) fn add_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref().map(|s| &s.custom) else {
            return;
        };
        let model = form.model(cx);
        if let Err(e) = model.to_provider() {
            self.show_notice(Notice::error(e), cx);
            return;
        }
        let key = form.key.read(cx).text().to_string();
        let models = self.models();
        cx.spawn_in(window, async move |this, cx| {
            let added = cx
                .background_spawn({
                    let models = models.clone();
                    async move { models.add_custom(&model, &key) }
                })
                .await;
            this.update_in(cx, |this, window, cx| match added {
                Ok(id) => {
                    let settings = models.settings();
                    let Some(sheet) = this.settings.as_mut() else {
                        return;
                    };
                    if let Some(provider) = settings.provider(&id) {
                        let name = provider.name.clone();
                        sheet.settings.providers.push(provider.clone());
                        let form = Self::provider_form(provider, window, cx);
                        if let Some(sheet) = this.settings.as_mut() {
                            sheet.forms.push(form);
                            sheet.custom = CustomForm::new(window, cx);
                        }
                        this.load_key_status(cx);
                        this.show_notice(
                            Notice::info(format!("Added {name}: it is in the model menu now.")),
                            cx,
                        );
                    }
                }
                Err(e) => this.show_notice(Notice::error(e), cx),
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_custom_form(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let Some(form) = self.settings.as_ref().map(|s| &s.custom) else {
            return div().into_any_element();
        };
        let muted =
            |text: &'static str| Label::new(text).size(LabelSize::XSmall).color(Color::Muted);
        let current = form.kind;
        v_flex()
            .id("chat-custom-model")
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(colors.border_variant)
            .child(Label::new("Add a custom model").size(LabelSize::Small))
            .child(
                h_flex()
                    .gap_1()
                    .children(CustomKind::ALL.into_iter().map(|kind| {
                        Button::new(
                            SharedString::from(format!("chat-custom-kind-{kind:?}")),
                            kind.label(),
                        )
                        .style(ButtonStyle::Subtle)
                        .label_size(LabelSize::XSmall)
                        .toggle_state(kind == current)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.set_custom_kind(kind, None, cx)),
                        )
                    })),
            )
            .child(muted("Known endpoints"))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(PRESETS.into_iter().map(|(name, kind, url)| {
                        Button::new(
                            SharedString::from(format!("chat-custom-preset-{name}")),
                            name,
                        )
                        .style(ButtonStyle::Outlined)
                        .label_size(LabelSize::XSmall)
                        .tooltip(Tooltip::text(url))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_custom_kind(kind, Some(url), cx)
                        }))
                    })),
            )
            .child(muted("Base URL"))
            .child(form.base_url.clone())
            .child(muted("Model id"))
            .child(form.model.clone())
            .child(muted("Display name"))
            .child(form.name.clone())
            .child(muted("API key (leave empty if the endpoint needs none)"))
            .child(form.key.clone())
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        div().debug_selector(|| "chat-custom-test".into()).child(
                            Button::new("chat-custom-test", "Test")
                                .style(ButtonStyle::Outlined)
                                .label_size(LabelSize::XSmall)
                                .disabled(form.test == Test::Running)
                                .on_click(cx.listener(|this, _, _, cx| this.test_custom(cx))),
                        ),
                    )
                    .child(
                        div().debug_selector(|| "chat-custom-add".into()).child(
                            Button::new("chat-custom-add", "Add model")
                                .style(ButtonStyle::Filled)
                                .label_size(LabelSize::XSmall)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_custom(window, cx)),
                                ),
                        ),
                    )
                    .child(form.test.render()),
            )
            .into_any_element()
    }
}
