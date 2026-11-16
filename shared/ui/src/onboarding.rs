//! The first-launch walkthrough every Ferrum app shows once (and again from
//! Settings › Interface or the app menu): welcome, look and feel,
//! integrations & permissions, config migration, and the keys worth
//! knowing. Completed apps are listed in `~/.ferrum/onboarding.json`.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, Window, div, prelude::*, px,
};
use serde::{Deserialize, Serialize};

use crate::app_info::{AppInfo, app_info};
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, Headline, HeadlineSize, Icon,
    IconButton, IconName, Label, LabelCommon, LabelSize, format_keystroke, h_flex, v_flex,
};
use crate::integrations::{self, Agent, PermissionStatus};
use crate::settings::interface_page::{density_pickers, scheme_pickers};
use crate::settings::themes_page::theme_row;
use crate::settings::widgets::{ChoiceCard, icon_tile, key_caps, primary_button};
use crate::theme::ActiveTheme;
use crate::{appearance, prefs};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct OnboardingState {
    /// Binaries that finished or skipped the walkthrough.
    completed: BTreeSet<String>,
}

fn state_file() -> Option<std::path::PathBuf> {
    prefs::path("onboarding.json")
}

/// Whether `binary` still has to show the walkthrough. `FERRUM_ONBOARDING=1`
/// forces it.
pub fn pending(binary: &str) -> bool {
    if std::env::var_os("FERRUM_ONBOARDING").is_some_and(|v| v == "1") {
        return true;
    }
    let Some(file) = state_file() else {
        return false;
    };
    !prefs::read::<OnboardingState>(&file)
        .completed
        .contains(binary)
}

/// Records that `binary` finished the walkthrough.
pub fn mark_done(binary: &str) {
    let Some(file) = state_file() else {
        return;
    };
    let mut state: OnboardingState = prefs::read(&file);
    if state.completed.insert(binary.to_owned())
        && let Err(error) = prefs::write(&file, &state)
    {
        eprintln!("ferrum: could not save {}: {error}", file.display());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome,
    Look,
    Integrations,
    Migrate,
    Done,
}

impl Step {
    fn all(info: &AppInfo) -> Vec<Self> {
        let mut steps = vec![Self::Welcome, Self::Look, Self::Integrations];
        if info.migration.is_some() {
            steps.push(Self::Migrate);
        }
        steps.push(Self::Done);
        steps
    }
}

pub enum OnboardingEvent {
    Finished,
}

pub struct Onboarding {
    info: Rc<AppInfo>,
    steps: Vec<Step>,
    index: usize,
    agents: Vec<Agent>,
    agent: Option<Agent>,
    migration_started: bool,
    message: Option<String>,
    focus: FocusHandle,
}

impl EventEmitter<OnboardingEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Onboarding {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let info = app_info(cx);
        let agents = integrations::installed_agents();
        let steps = Step::all(&info);
        // Lets screenshots and tests open a later step directly.
        let index = std::env::var("FERRUM_ONBOARDING_STEP")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(0, |i| i.min(steps.len() - 1));
        Self {
            steps,
            info,
            index,
            agent: agents.first().copied(),
            agents,
            migration_started: false,
            message: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn step(&self) -> Step {
        self.steps[self.index]
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        if self.index + 1 < self.steps.len() {
            self.index += 1;
            self.message = None;
            cx.notify();
        } else {
            self.finish(cx);
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.index > 0 {
            self.index -= 1;
            self.message = None;
            cx.notify();
        }
    }

    fn go_to(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Some(index) = self.steps.iter().position(|s| *s == step) {
            self.index = index;
            cx.notify();
        }
    }

    pub fn finish(&mut self, cx: &mut Context<Self>) {
        mark_done(self.info.binary);
        cx.emit(OnboardingEvent::Finished);
    }

    fn nav(&self, primary: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .justify_center()
            .when(self.index > 0, |this| {
                this.child(
                    IconButton::new("onboarding-back", IconName::ArrowLeft)
                        .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                )
            })
            .child(primary_button(
                "onboarding-continue",
                primary,
                Some(IconName::ArrowRight),
                cx.listener(|this, _, _, cx| this.next(cx)),
                cx,
            ))
    }

    fn header(&self, title: String, detail: String, cx: &App) -> impl IntoElement {
        v_flex()
            .items_center()
            .gap_2()
            .child(Headline::new(title).size(HeadlineSize::Large))
            .child(
                div()
                    .max_w(px(520.))
                    .text_center()
                    .text_color(cx.theme().colors().text_muted)
                    .child(detail),
            )
    }

    fn card(cx: &App) -> gpui::Div {
        let colors = cx.theme().colors();
        v_flex()
            .w(px(760.))
            .max_w_full()
            .p_4()
            .gap_3()
            .rounded_xl()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.elevated_surface_background)
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let name = self.info.name;
        let detail = if self.info.tagline.is_empty() {
            format!(
                "Take a moment to customize {name}'s appearance, set up permissions and \
                 integrations, and optionally migrate your own configuration."
            )
        } else {
            format!(
                "{} Take a moment to set up its look, permissions and integrations.",
                self.info.tagline
            )
        };
        v_flex()
            .items_center()
            .gap_6()
            .child(self.header(format!("Get Started with {name}"), detail, cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(self.nav("Continue", cx))
                    .child(
                        Button::new("onboarding-skip", "Skip for now")
                            .style(ButtonStyle::Transparent)
                            .color(Color::Accent)
                            .on_click(cx.listener(|this, _, _, cx| this.finish(cx))),
                    ),
            )
            .into_any_element()
    }

    fn render_look(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let active = appearance::effective(cx);
        v_flex()
            .items_center()
            .gap_5()
            .child(
                self.header(
                    format!("Customize the look and feel of {}", self.info.name),
                    "Choose a color scheme, toolbar style and theme. Every Ferrum app follows it."
                        .into(),
                    cx,
                ),
            )
            .child(
                Self::card(cx)
                    .child(Label::new("Color scheme").weight(gpui::FontWeight::SEMIBOLD))
                    .child(scheme_pickers(cx))
                    .child(Label::new("Density").weight(gpui::FontWeight::SEMIBOLD))
                    .child(density_pickers(cx))
                    .child(Label::new("Interface style").weight(gpui::FontWeight::SEMIBOLD))
                    .child(crate::settings::interface_page::style_pickers(cx))
                    .child(
                        Label::new(format!(
                            "Built-in themes ({} mode)",
                            if active.is_light() { "light" } else { "dark" }
                        ))
                        .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(theme_row(active, cx)),
            )
            .into_any_element()
    }

    fn render_integrations(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let skill = integrations::skill_installed();
        let found = self
            .agents
            .iter()
            .map(|a| a.name())
            .collect::<Vec<_>>()
            .join(", ");
        let colors = cx.theme().colors().clone();
        let skill_card = v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_2()
            .p_3()
            .child(icon_tile(
                IconName::Blocks,
                gpui::rgb(0xe8743b).into(),
                px(40.),
            ))
            .child(Label::new("Ferrum Agent Skill").weight(gpui::FontWeight::SEMIBOLD))
            .child(crate::badge(
                if skill { "Installed" } else { "Recommended" },
                colors.text_muted,
                cx,
            ))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_sm()
                    .text_color(colors.text_muted)
                    .child(if found.is_empty() {
                        "Teach coding agents to open and drive the Ferrum apps.".to_owned()
                    } else {
                        format!("Teach {found} to open and drive the Ferrum apps.")
                    }),
            )
            .child(
                Button::new("install-skill", if skill { "Reinstall" } else { "Install" })
                    .style(ButtonStyle::Subtle)
                    .color(Color::Accent)
                    .on_click(cx.listener(|this, _, _, cx| {
                        // The badge turns to "Installed"; only failures need words.
                        this.message = integrations::install_skill()
                            .err()
                            .map(|error| format!("Could not install the skill: {error}"));
                        cx.notify();
                    })),
            );

        let summary = integrations::permissions_summary(&self.info);
        let permissions = self.info.permissions.clone();
        let permission_card = v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_2()
            .p_3()
            .child(icon_tile(
                IconName::Lock,
                gpui::rgb(0x4c8bf5).into(),
                px(40.),
            ))
            .child(Label::new("System Permissions").weight(gpui::FontWeight::SEMIBOLD))
            .child(crate::badge(summary, colors.text_muted, cx))
            .children(if permissions.is_empty() {
                vec![
                    div()
                        .w_full()
                        .text_center()
                        .text_sm()
                        .text_color(colors.text_muted)
                        .child(format!("{} needs no special permissions.", self.info.name))
                        .into_any_element(),
                ]
            } else {
                permissions
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let kind = p.kind;
                        let granted = kind.status() == PermissionStatus::Granted;
                        h_flex()
                            .w_full()
                            .gap_2()
                            .justify_between()
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(Label::new(kind.title()).size(LabelSize::Small))
                                    .child(
                                        Label::new(p.reason)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(if granted {
                                Label::new("Granted")
                                    .size(LabelSize::Small)
                                    .color(Color::Success)
                                    .into_any_element()
                            } else {
                                Button::new(("grant", i), "Grant…")
                                    .style(ButtonStyle::Subtle)
                                    .color(Color::Accent)
                                    .on_click(move |_, _, cx| crate::integrations::grant(kind, cx))
                                    .into_any_element()
                            })
                            .into_any_element()
                    })
                    .collect()
            });

        v_flex()
            .items_center()
            .gap_5()
            .child(self.header(
                "Integrations & Permissions".into(),
                format!(
                    "Install the Ferrum skill and grant permissions to let agents and {} work \
                     smoothly.",
                    self.info.name
                ),
                cx,
            ))
            .child(
                Self::card(cx).child(
                    h_flex()
                        .items_start()
                        .child(skill_card)
                        .child(div().w(px(1.)).self_stretch().bg(colors.border_variant))
                        .child(permission_card),
                ),
            )
            .children(
                self.message
                    .clone()
                    .map(|m| Label::new(m).size(LabelSize::Small).color(Color::Muted)),
            )
            .into_any_element()
    }

    fn render_migrate(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(migration) = self.info.migration.clone() else {
            return div().into_any_element();
        };
        let colors = cx.theme().colors().clone();
        let tiles = h_flex()
            .gap_3()
            .justify_center()
            .children(Agent::ALL.map(|agent| {
                let present = self.agents.contains(&agent);
                ChoiceCard::new(
                    ("agent", agent as usize),
                    agent.name(),
                    div()
                        .h(px(64.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(!present, |this| this.opacity(0.4))
                        .child(crate::agent_logo(agent.logo(), px(44.), cx)),
                )
                .selected(self.agent == Some(agent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.agents.contains(&agent) {
                        this.agent = Some(agent);
                    } else {
                        this.message = Some(format!(
                            "{} isn't installed. Install `{}` first, or copy the prompt.",
                            agent.name(),
                            agent.command()
                        ));
                    }
                    cx.notify();
                }))
            }))
            .child(div().w(px(1.)).h(px(64.)).bg(colors.border_variant))
            .child(
                ChoiceCard::new(
                    "skip-migration",
                    "Skip migration",
                    div()
                        .h(px(64.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::XCircle).color(Color::Muted)),
                )
                .selected(self.agent.is_none())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.agent = None;
                    cx.notify();
                })),
            );

        let info = self.info.clone();
        let actions = h_flex()
            .gap_2()
            .justify_center()
            .child(
                Button::new("copy-migration-prompt", "Copy Prompt")
                    .style(ButtonStyle::Subtle)
                    .start_icon(Icon::new(IconName::Copy))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(prompt) = integrations::migration_prompt(&info) {
                            cx.write_to_clipboard(ClipboardItem::new_string(prompt));
                            this.message = Some("Prompt copied. Paste it into your agent.".into());
                            cx.notify();
                        }
                    })),
            )
            .when_some(self.agent, |this, agent| {
                let info = self.info.clone();
                this.child(
                    Button::new("start-migration", format!("Start with {}", agent.name()))
                        .style(ButtonStyle::Subtle)
                        .color(Color::Accent)
                        .start_icon(Icon::new(IconName::Terminal))
                        .disabled(self.migration_started)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.message = Some(match integrations::run_migration(agent, &info) {
                                Ok(_) => {
                                    this.migration_started = true;
                                    format!(
                                        "{} opened in a terminal. Answer its questions there, \
                                         then continue.",
                                        agent.name()
                                    )
                                }
                                Err(error) => error.to_string(),
                            });
                            cx.notify();
                        })),
                )
            });

        v_flex()
            .items_center()
            .gap_5()
            .child(self.header(
                format!("Migrate your config to {}", self.info.name),
                "Choose your agent to find and migrate your config.".into(),
                cx,
            ))
            .child(
                Self::card(cx)
                    .items_center()
                    .child(tiles)
                    .child(
                        div()
                            .max_w(px(520.))
                            .text_center()
                            .text_sm()
                            .text_color(colors.text_muted)
                            .child(format!(
                                "The selected agent will look for your {} configuration. {}",
                                migration.from.join(", "),
                                migration.summary
                            )),
                    )
                    .child(actions),
            )
            .children(
                self.message
                    .clone()
                    .map(|m| Label::new(m).size(LabelSize::Small).color(Color::Muted)),
            )
            .into_any_element()
    }

    fn render_done(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = cx.theme().colors().clone();
        let keys = self.info.keys.clone();
        let count = keys.len();
        let has_migration = self.info.migration.is_some();
        v_flex()
            .items_center()
            .gap_5()
            .child(self.header(
                "You're all set".into(),
                "A few keys worth knowing.".into(),
                cx,
            ))
            .when(count > 0, |this| {
                this.child(
                    v_flex()
                        .w(px(440.))
                        .px_3()
                        .rounded_xl()
                        .border_1()
                        .border_color(colors.border_variant)
                        .bg(colors.elevated_surface_background)
                        .children(keys.into_iter().enumerate().map(|(i, hint)| {
                            h_flex()
                                .py_2()
                                .justify_between()
                                .when(i + 1 < count, |this| {
                                    this.border_b_1().border_color(colors.border_variant)
                                })
                                .child(Label::new(hint.label))
                                .child(key_caps(&display_keys(hint.keys), cx))
                        })),
                )
            })
            .child(primary_button(
                "start-using",
                format!("Start Using {}", self.info.name),
                None,
                cx.listener(|this, _, _, cx| this.finish(cx)),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .when(has_migration, |this| {
                        this.child(
                            Button::new("return-to-migration", "← Return to Migration")
                                .style(ButtonStyle::Transparent)
                                .color(Color::Accent)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.go_to(Step::Migrate, cx)),
                                ),
                        )
                    })
                    .child(
                        Button::new("open-settings-from-onboarding", "All Settings…")
                            .style(ButtonStyle::Transparent)
                            .color(Color::Accent)
                            .on_click(|_, _, cx| crate::settings::open_settings(None, cx)),
                    ),
            )
            .into_any_element()
    }
}

/// `"cmd-shift-p"` → `"⇧ ⌘ P"`; chords are separated by wider gaps.
pub fn display_keys(keys: &str) -> String {
    keys.split_whitespace()
        .filter_map(|k| gpui::Keystroke::parse(k).ok())
        .map(|k| {
            let text = format_keystroke(&k);
            let mut parts: Vec<String> = Vec::new();
            let mut rest = text.as_str();
            while let Some(c) = rest.chars().next().filter(|c| "⌃⌥⇧⌘".contains(*c)) {
                parts.push(c.to_string());
                rest = &rest[c.len_utf8()..];
            }
            parts.push(rest.trim_start_matches(['-', '+']).to_owned());
            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let body = match self.step() {
            Step::Welcome => self.render_welcome(cx),
            Step::Look => self.render_look(cx),
            Step::Integrations => self.render_integrations(cx),
            Step::Migrate => self.render_migrate(cx),
            Step::Done => self.render_done(cx),
        };
        let total = self.steps.len();
        let index = self.index;
        v_flex()
            .id("ferrum-onboarding")
            .track_focus(&self.focus)
            .key_context("Onboarding")
            .on_action(cx.listener(|this, _: &crate::menu::Cancel, _, cx| this.finish(cx)))
            .on_action(cx.listener(|this, _: &crate::menu::Confirm, _, cx| this.next(cx)))
            .absolute()
            .inset_0()
            .occlude()
            .child(crate::surface::backdrop(true, cx).absolute().inset_0())
            .text_color(colors.text)
            .child(
                // A title row so the traffic lights get their own room
                // instead of sitting on top of the title.
                crate::window_chrome::title_row("onboarding-title", true, window)
                    .pr_4()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        h_flex().gap_2().pl_2().child(
                            Label::new(format!("Get Started with {}", self.info.name))
                                .size(LabelSize::Small),
                        ),
                    )
                    .child(
                        IconButton::new("close-onboarding", IconName::Close)
                            .on_click(cx.listener(|this, _, _, cx| this.finish(cx))),
                    ),
            )
            .child(
                v_flex()
                    .id("ferrum-onboarding-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .min_h_full()
                            .w_full()
                            .py_8()
                            .px_6()
                            .items_center()
                            .justify_center()
                            .child(body),
                    ),
            )
            .child(
                // Footer: Back, one pill per step (the current one wide and in
                // the accent colour) and Continue, always on screen.
                h_flex()
                    .flex_none()
                    .h(px(56.))
                    .px_4()
                    .justify_center()
                    .gap_2()
                    .relative()
                    .when(index > 0, |bar| {
                        bar.child(
                            div().absolute().left_4().child(
                                Button::new("onboarding-footer-back", "Back")
                                    .style(ButtonStyle::Subtle)
                                    .start_icon(Icon::new(IconName::ArrowLeft))
                                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                            ),
                        )
                    })
                    .when(index > 0 && index + 1 < total, |bar| {
                        bar.child(div().absolute().right_4().child(primary_button(
                            "onboarding-footer-continue",
                            "Continue",
                            Some(IconName::ArrowRight),
                            cx.listener(|this, _, _, cx| this.next(cx)),
                            cx,
                        )))
                    })
                    .border_t_1()
                    .border_color(colors.border_variant)
                    .children((0..total).map(|i| {
                        let current = i == index;
                        div()
                            .id(("onboarding-step", i))
                            .h(px(8.))
                            .w(if current { px(24.) } else { px(8.) })
                            .rounded_full()
                            .cursor_pointer()
                            .bg(if current {
                                colors.text_accent
                            } else if i < index {
                                colors.text_muted
                            } else {
                                colors.text_muted.opacity(0.45)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.index = i;
                                this.message = None;
                                cx.notify();
                            }))
                    }))
                    .child(
                        Label::new(format!("{} of {total}", index + 1))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_info::Migration;

    #[test]
    fn migration_step_only_when_the_app_has_one() {
        let plain = AppInfo::new("X", "x");
        assert_eq!(
            Step::all(&plain),
            [Step::Welcome, Step::Look, Step::Integrations, Step::Done]
        );
        let with = plain.migration(Migration {
            from: &["A"],
            summary: "",
            task: "",
        });
        assert!(Step::all(&with).contains(&Step::Migrate));
    }

    #[test]
    fn keys_display_as_caps() {
        if cfg!(target_os = "macos") {
            assert_eq!(display_keys("cmd-shift-p"), "⇧ ⌘ P");
            assert_eq!(display_keys("cmd-k right"), "⌘ K  →");
        }
    }

    #[test]
    fn completion_is_remembered() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("onboarding.json");
        let mut state = OnboardingState::default();
        state.completed.insert("ferrumterm".into());
        prefs::write(&file, &state).expect("write");
        let read: OnboardingState = prefs::read(&file);
        assert!(read.completed.contains("ferrumterm"));
    }
}
