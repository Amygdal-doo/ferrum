//! Settings dialog: how often to check, how many versions to keep and the
//! menu bar item. Where releases come from is not a setting.

use ferrum_ui::components::Checkbox;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::Settings;

pub enum SettingsFormEvent {
    Saved(Settings),
    Cancelled,
}

pub struct SettingsForm {
    interval: Entity<TextInput>,
    keep: Entity<TextInput>,
    /// Show the launcher in the menu bar (macOS).
    menu_bar: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsFormEvent> for SettingsForm {}

impl SettingsForm {
    pub fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let subscriptions = vec![submit(cx, &interval), submit(cx, &keep)];
        let menu_bar = settings.menu_bar;
        Self {
            menu_bar,
            interval,
            keep,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus_first(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.interval
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.collect(cx) {
            Ok(settings) => cx.emit(SettingsFormEvent::Saved(settings)),
            Err(message) => {
                self.error = Some(message.into());
                cx.notify();
            }
        }
    }

    fn collect(&self, cx: &App) -> Result<Settings, String> {
        let text = |input: &Entity<TextInput>| input.read(cx).text().trim().to_string();
        let check_interval_hours = text(&self.interval)
            .parse::<u32>()
            .ok()
            .filter(|hours| (1..=168).contains(hours))
            .ok_or("Check interval must be 1–168 hours.")?;
        let keep_versions = text(&self.keep)
            .parse::<u32>()
            .map_err(|_| "Versions to keep must be a number (0 keeps all).")?;
        Ok(Settings {
            check_interval_hours,
            keep_versions,
            menu_bar: self.menu_bar,
            ..Settings::default()
        })
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
        Modal::new("Settings")
            .width(px(520.))
            .on_dismiss(cx.listener(|_, _, _, cx| cx.emit(SettingsFormEvent::Cancelled)))
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
            .when(cfg!(target_os = "macos"), |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            Checkbox::new(
                                "settings-menu-bar",
                                if self.menu_bar {
                                    ToggleState::Selected
                                } else {
                                    ToggleState::Unselected
                                },
                            )
                            .label("Show Ferrum Launcher in the menu bar")
                            .on_click(cx.listener(
                                |this: &mut Self, state: &ToggleState, _, cx| {
                                    this.menu_bar = state.selected();
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            Label::new(
                                "Open any app from the menu bar. Closing the window keeps the launcher running there, so app shortcuts keep working.",
                            )
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                        ),
                )
            })
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
