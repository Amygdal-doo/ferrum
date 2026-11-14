//! Automations: saved prompts with a mode and a schedule. "Run now" starts
//! one in its project's chat; while the app is open, a timer starts the
//! due ones (in the background for other projects: their chats keep
//! running).

use std::time::Duration;

use chrono::{Local, Utc};
use ferrum_ui::components::{Checkbox, IconSize};
use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::core::Mode;

use super::{field, frame, placeholder, row, section, two_lines};
use crate::app::{ChatAction, FerrumVibe};
use crate::automations::{Automation, Schedule};
use crate::nav::Page;
use crate::threads;

/// How often the schedules are checked.
const TICK: Duration = Duration::from_secs(30);

/// Checks the schedules every [`TICK`] while the window lives.
pub fn start_timer(window: &mut Window, cx: &mut Context<FerrumVibe>) -> Task<()> {
    cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            if this
                .update_in(cx, |this, window, cx| this.run_due(window, cx))
                .is_err()
            {
                return;
            }
        }
    })
}

impl FerrumVibe {
    fn save_automations(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = self.pages.automations_file()
            && let Err(e) = crate::automations::save(&file, &self.pages.automations)
        {
            self.show_notice(
                Notice::error(format!("Could not save automations: {e}")),
                cx,
            );
        }
        cx.notify();
    }

    /// Starts every automation whose time has come.
    pub fn run_due(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Another window (or the user) may have changed the file.
        if let Some(file) = self.pages.automations_file() {
            self.pages.automations = crate::automations::load(&file);
        }
        let now = Local::now();
        let due: Vec<String> = self
            .pages
            .automations
            .iter()
            .filter(|a| a.is_due(&now) && a.project.is_dir())
            .map(|a| a.id.clone())
            .collect();
        for id in due {
            self.run_automation(&id, false, window, cx);
        }
    }

    /// Runs automation `id` in its project's chat. `show`: switch to that
    /// project and show the chat ("Run now").
    pub fn run_automation(
        &mut self,
        id: &str,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(automation) = self.pages.automations.iter_mut().find(|a| a.id == id) else {
            return;
        };
        automation.last_run = Some(Utc::now());
        let automation = automation.clone();
        self.save_automations(cx);
        let action = ChatAction::StartTask(automation.mode, Some(automation.prompt.clone()));
        if show {
            self.set_project(automation.project.clone(), window, cx);
            self.history.visit(crate::nav::Location::Page(Page::Chat));
            self.chat_action(&automation.project, action, window, cx);
            self.focus_chat(window, cx);
        } else {
            self.chat_action(&automation.project, action, window, cx);
            self.show_notice(
                Notice::info(format!(
                    "Automation “{}” started in {}",
                    automation.name,
                    project::project_name(&automation.project)
                )),
                cx,
            );
        }
        self.refresh_index(cx);
    }

    fn add_automation(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let text = |input: &Entity<TextInput>, cx: &App| input.read(cx).text().to_string();
        let (name, prompt) = (
            text(&self.pages.auto_name, cx),
            text(&self.pages.auto_prompt, cx),
        );
        let schedule_text = text(&self.pages.auto_schedule, cx);
        let Some(schedule) = Schedule::parse(&schedule_text) else {
            self.show_notice(
                Notice::error("Schedule: manual, every 4h or daily 09:00."),
                cx,
            );
            return;
        };
        if name.trim().is_empty() || prompt.trim().is_empty() {
            self.show_notice(
                Notice::error("An automation needs a name and a prompt."),
                cx,
            );
            return;
        }
        let automation = Automation::new(&name, &project, &prompt, self.pages.auto_mode, schedule);
        self.pages.automations.push(automation);
        for input in [
            self.pages.auto_name.clone(),
            self.pages.auto_prompt.clone(),
            self.pages.auto_schedule.clone(),
        ] {
            input.update(cx, |input, cx| input.set_text("", cx));
        }
        self.save_automations(cx);
    }

    fn edit_automation(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Vec<Automation>, usize),
    ) {
        if let Some(i) = self.pages.automations.iter().position(|a| a.id == id) {
            f(&mut self.pages.automations, i);
            self.save_automations(cx);
        }
    }

    fn mode_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut picker = h_flex().gap_0p5();
        for mode in Mode::ALL {
            picker = picker.child(
                Button::new(
                    SharedString::from(format!("auto-mode-{}", mode.label())),
                    mode.label(),
                )
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .toggle_state(self.pages.auto_mode == mode)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pages.auto_mode = mode;
                    cx.notify();
                })),
            );
        }
        picker
    }

    pub(crate) fn automations_page(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut page = frame(
            Page::Automations,
            "Prompts that start a task in their project, by hand or on a schedule while FerrumVibe is open.",
            cx,
        );
        if self.project.is_some() {
            page = page.child(
                v_flex()
                    .gap_2()
                    .max_w(px(720.))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(field(&self.pages.auto_name, cx))
                            .child(field(&self.pages.auto_schedule, cx))
                            .child(self.mode_picker(cx)),
                    )
                    .child(field(&self.pages.auto_prompt, cx))
                    .child(
                        h_flex().child(
                            Button::new("add-automation", "Add Automation")
                                .style(ButtonStyle::Filled)
                                .size(ButtonSize::Compact)
                                .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                                .on_click(cx.listener(|this, _, _, cx| this.add_automation(cx))),
                        ),
                    ),
            );
        }
        page = page.child(section("Saved", cx));
        if self.pages.automations.is_empty() {
            page = page.child(placeholder("No automations yet.", cx));
        }
        let now = Utc::now();
        for (i, a) in self.pages.automations.clone().into_iter().enumerate() {
            let last = a.last_run.map_or_else(
                || "never run".to_string(),
                |t| format!("ran {} ago", threads::ago(t, now)),
            );
            let next = a
                .next_run(&Local)
                .map(|t| format!(" · next {}", t.format("%a %H:%M")))
                .unwrap_or_default();
            let detail = format!(
                "{} · {} · {} · {last}{next}",
                project::project_name(&a.project),
                a.mode.label(),
                a.schedule.label()
            );
            let (toggle_id, run_id, remove_id) = (a.id.clone(), a.id.clone(), a.id.clone());
            let enabled = if a.enabled {
                ToggleState::Selected
            } else {
                ToggleState::Unselected
            };
            page =
                page.child(
                    row(("automation", i), cx)
                        .child(Checkbox::new(("automation-enabled", i), enabled).on_click(
                            cx.listener(move |this, state: &ToggleState, _, cx| {
                                let on = *state == ToggleState::Selected;
                                this.edit_automation(&toggle_id, cx, |list, i| {
                                    list[i].enabled = on
                                });
                            }),
                        ))
                        .child(two_lines(a.name.clone(), detail))
                        .child(
                            Button::new(("run-automation", i), "Run now")
                                .style(ButtonStyle::Subtle)
                                .size(ButtonSize::Compact)
                                .start_icon(Icon::new(IconName::PlayFilled).size(IconSize::XSmall))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_automation(&run_id, true, window, cx);
                                })),
                        )
                        .child(
                            IconButton::new(("remove-automation", i), IconName::Trash)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Muted)
                                .tooltip(Tooltip::text("Delete"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_automation(&remove_id, cx, |list, i| {
                                        list.remove(i);
                                    });
                                })),
                        ),
                );
        }
        page.into_any_element()
    }
}
