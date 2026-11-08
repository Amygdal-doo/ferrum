//! First-launch "Sign in to GitHub" panel, shown when no token is found.
//! The repo is private, so releases can't be listed without one.

use ferrum_ui::prelude::*;

pub enum OnboardingEvent {
    UseGhCli,
    SaveToken(String),
    Skip,
}

pub struct Onboarding {
    repo: SharedString,
    token: Entity<TextInput>,
    busy: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OnboardingEvent> for Onboarding {}

impl Onboarding {
    pub fn new(repo: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let token = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder("github_pat_… or ghp_…")
        });
        let submit = cx.subscribe(&token, |this: &mut Self, _, event, cx| {
            if *event == TextInputEvent::Submit {
                this.save(cx);
            }
        });
        Self {
            repo,
            token,
            busy: false,
            error: None,
            _subscriptions: vec![submit],
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.token.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Shows why the last attempt failed and re-enables the buttons.
    pub fn fail(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = Some(message.into());
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let token = self.token.read(cx).text().trim().to_string();
        if token.is_empty() {
            self.fail("Paste a token first.", cx);
            return;
        }
        self.busy = true;
        self.error = None;
        cx.emit(OnboardingEvent::SaveToken(token));
        cx.notify();
    }

    fn use_gh(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        cx.emit(OnboardingEvent::UseGhCli);
        cx.notify();
    }
}

impl Render for Onboarding {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = self.repo.split('/').next().unwrap_or_default().to_string();
        let busy = self.busy;
        Modal::new("Sign in to GitHub")
            .width(px(520.))
            .child(Label::new(format!(
                "Ferrum releases are published in the private repo {}. \
                 FerrumToolbox needs read access to list and download them.",
                self.repo
            )))
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Option 1: GitHub CLI").size(LabelSize::Small))
                    .child(
                        Label::new("Run `gh auth login` in Terminal once, then:")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        h_flex().child(
                            Button::new("use-gh", "Use gh CLI")
                                .start_icon(Icon::new(IconName::Terminal))
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.use_gh(cx))),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Option 2: Access token").size(LabelSize::Small))
                    .child(
                        Label::new(
                            "Create a fine-grained token with Contents: Read on the repo. \
                             It is stored in the system keychain (macOS Keychain, Windows Credential \
                             Manager or Secret Service), never on disk.",
                        )
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(self.token.clone())
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("create-token", "Create a token on GitHub")
                                    .style(ButtonStyle::Subtle)
                                    .start_icon(Icon::new(IconName::ArrowUpRight))
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&ferrumtoolbox_core::auth::new_token_url(
                                            &owner,
                                        ))
                                    }),
                            )
                            .child(
                                Button::new("save-token", "Save token")
                                    .style(ButtonStyle::Filled)
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                            ),
                    ),
            )
            .when(busy, |this| {
                this.child(
                    Label::new("Checking…")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            })
            .footer(
                Button::new("onboarding-skip", "Skip (public repos only)")
                    .style(ButtonStyle::Subtle)
                    .disabled(busy)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(OnboardingEvent::Skip))),
            )
    }
}
