//! Settings dialog. The token is only passed to the keychain, never stored
//! in `state.json` or logged.

use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumlauncher_core::{Settings, TokenSource};

#[derive(Clone)]
pub enum TokenChange {
    Keep,
    Set(String),
    Clear,
}

pub enum SettingsFormEvent {
    Saved {
        settings: Settings,
        token: TokenChange,
    },
    Cancelled,
}

pub struct SettingsForm {
    owner: Entity<TextInput>,
    repo: Entity<TextInput>,
    token: Entity<TextInput>,
    interval: Entity<TextInput>,
    keep: Entity<TextInput>,
    clear_token: bool,
    token_source: Option<TokenSource>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsFormEvent> for SettingsForm {}

impl SettingsForm {
    pub fn new(
        settings: Settings,
        token_source: Option<TokenSource>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Amygdal-doo")
                .with_text(settings.repo_owner.clone())
        });
        let repo = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("ferrum")
                .with_text(settings.repo_name.clone())
        });
        let token = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder("Leave empty to keep the current token")
        });
        let interval = cx.new(|cx| {
            TextInput::new(window, cx).with_text(settings.check_interval_hours.to_string())
        });
        let keep =
            cx.new(|cx| TextInput::new(window, cx).with_text(settings.keep_versions.to_string()));

        let submit = |cx: &mut Context<Self>, input: &Entity<TextInput>| {
            cx.subscribe(input, |this: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    this.save(cx);
                }
            })
        };
        let subscriptions = vec![
            submit(cx, &owner),
            submit(cx, &repo),
            submit(cx, &token),
            submit(cx, &interval),
            submit(cx, &keep),
        ];
        Self {
            owner,
            repo,
            token,
            interval,
            keep,
            clear_token: false,
            token_source,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus_first(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.owner.update(cx, |input, cx| input.focus(window, cx));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.collect(cx) {
            Ok((settings, token)) => cx.emit(SettingsFormEvent::Saved { settings, token }),
            Err(message) => {
                self.error = Some(message.into());
                cx.notify();
            }
        }
    }

    fn collect(&self, cx: &App) -> Result<(Settings, TokenChange), String> {
        let text = |input: &Entity<TextInput>| input.read(cx).text().trim().to_string();
        let owner = text(&self.owner);
        let repo = text(&self.repo);
        if owner.is_empty() || repo.is_empty() {
            return Err("Repository owner and name are required.".into());
        }
        let check_interval_hours = text(&self.interval)
            .parse::<u32>()
            .ok()
            .filter(|hours| (1..=168).contains(hours))
            .ok_or("Check interval must be 1–168 hours.")?;
        let keep_versions = text(&self.keep)
            .parse::<u32>()
            .map_err(|_| "Versions to keep must be a number (0 keeps all).")?;
        let token_text = self.token.read(cx).text().trim().to_string();
        let token = if self.clear_token {
            TokenChange::Clear
        } else if token_text.is_empty() {
            TokenChange::Keep
        } else {
            TokenChange::Set(token_text)
        };
        Ok((
            Settings {
                repo_owner: owner,
                repo_name: repo,
                check_interval_hours,
                keep_versions,
                ..Settings::default()
            },
            token,
        ))
    }
}

fn field(
    label: &'static str,
    hint: Option<&'static str>,
    input: Entity<TextInput>,
) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(Label::new(label).size(LabelSize::Small))
        .child(input)
        .when_some(hint, |this, hint| {
            this.child(Label::new(hint).size(LabelSize::XSmall).color(Color::Muted))
        })
}

impl Render for SettingsForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let token_status = match self.token_source {
            Some(source) => format!("Currently using a token from: {source}."),
            None => "No token found (keychain, GITHUB_TOKEN or gh CLI).".to_string(),
        };
        Modal::new("Settings")
            .width(px(520.))
            .on_dismiss(cx.listener(|_, _, _, cx| cx.emit(SettingsFormEvent::Cancelled)))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(field(
                        "Repository owner",
                        None,
                        self.owner.clone(),
                    )))
                    .child(
                        div()
                            .flex_1()
                            .child(field("Repository name", None, self.repo.clone())),
                    ),
            )
            .child(field(
                "GitHub token",
                Some("Needed for private repos. Stored in the OS keychain."),
                self.token.clone(),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(token_status)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Checkbox::new(
                            "clear-token",
                            if self.clear_token {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .label("Remove saved token")
                        .on_click(cx.listener(
                            |this: &mut Self, state: &ToggleState, _, cx| {
                                this.clear_token = state.selected();
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(field(
                        "Check every (hours)",
                        None,
                        self.interval.clone(),
                    )))
                    .child(div().flex_1().child(field(
                        "Versions to keep",
                        Some("Includes the active one; 0 keeps all."),
                        self.keep.clone(),
                    ))),
            )
            .child(
                Label::new("Update channels (Stable, Beta or Alpha) are set per app on its page.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            })
            .footer(
                Button::new("settings-cancel", "Cancel")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsFormEvent::Cancelled))),
            )
            .footer(
                Button::new("settings-save", "Save")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
    }
}
