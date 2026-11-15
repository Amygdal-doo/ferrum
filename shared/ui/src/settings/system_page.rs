//! Settings › System: how the app is reached outside its window — its
//! shell command (`fei .`), the `ferrum <app>` command, PATH and the Dock
//! icon. The choices live in `~/.ferrum/preferences.json`, shared with
//! FerrumLauncher and every other app (see [`ferrum_shell`]); changes
//! write or remove the scripts in `~/.ferrum/bin` at once.

use std::cell::RefCell;

use ferrum_shell::ops::{self, Env};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, px,
};

use super::widgets::{Row, Section, Switch};
use super::{SettingsPage, tint};
use crate::app_info::app_info;
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, IconName, Label, LabelCommon,
    LabelSize, h_flex,
};
use crate::text_input::{TextInput, TextInputEvent};

#[derive(Default)]
pub(super) struct SystemPage {
    view: RefCell<Option<Entity<SystemView>>>,
}

impl SettingsPage for SystemPage {
    fn id(&self) -> &'static str {
        "system"
    }
    fn title(&self) -> &'static str {
        "System"
    }
    fn icon(&self) -> IconName {
        IconName::Terminal
    }
    fn tint(&self) -> Hsla {
        tint(0x4c8bf5)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "shell",
            "command",
            "terminal",
            "cli",
            "path",
            "ferrum",
            "dock",
            "menu bar",
            "integration",
        ]
    }

    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let view = self
            .view
            .borrow_mut()
            .get_or_insert_with(|| {
                let env = crate::prefs::ferrum_dir().map(Env::from_process);
                cx.new(|cx| SystemView::new(env, window, cx))
            })
            .clone();
        view.into_any_element()
    }
}

/// The page's state: the name field and the last result.
pub(crate) struct SystemView {
    env: Option<Env>,
    input: Entity<TextInput>,
    /// The last change's message, and whether it failed.
    message: Option<(SharedString, bool)>,
    _subscription: Subscription,
}

impl SystemView {
    /// `env`: where `~/.ferrum` is and the shell (`None`: no home folder).
    pub(crate) fn new(env: Option<Env>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = Self::app(cx)
            .zip(env.as_ref())
            .map(|(app, env)| {
                ops::load(&env.prefs_path(), &env.bin_dir())
                    .command(app.id)
                    .name
            })
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_text(name)
                .monospace()
                .placeholder("command")
        });
        let subscription = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if *event == TextInputEvent::Submit {
                this.rename(cx);
            }
        });
        Self {
            env,
            input,
            message: None,
            _subscription: subscription,
        }
    }

    /// The running app's entry in the suite, if it has a shell command.
    fn app(cx: &App) -> Option<&'static ferrum_shell::App> {
        ferrum_shell::apps::by_binary(app_info(cx).binary)
    }

    fn typed(&self, cx: &App) -> String {
        self.input.read(cx).text().trim().to_owned()
    }

    fn report(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        self.message = Some(match result {
            Ok(message) => (message.into(), false),
            Err(error) => (error.into(), true),
        });
        cx.notify();
    }

    /// Turns the command on (with the typed name) or off.
    pub(crate) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let (Some(app), Some(env)) = (Self::app(cx), self.env.clone()) else {
            return;
        };
        let typed = if enabled {
            self.typed(cx)
        } else {
            String::new()
        };
        let result = ops::set_command(&env, app.id, &typed, enabled);
        self.report(result, cx);
    }

    /// Gives the command the typed name (on or off as it is).
    pub(crate) fn rename(&mut self, cx: &mut Context<Self>) {
        let (Some(app), Some(env)) = (Self::app(cx), self.env.clone()) else {
            return;
        };
        let enabled = ops::load(&env.prefs_path(), &env.bin_dir())
            .command(app.id)
            .enabled;
        let typed = self.typed(cx);
        let result = ops::set_command(&env, app.id, &typed, enabled).map(|message| {
            if enabled {
                message
            } else {
                format!("Renamed to `{typed}` (off)")
            }
        });
        self.report(result, cx);
    }

    pub(crate) fn set_umbrella(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(env) = self.env.clone() {
            let result = ops::set_umbrella(&env, enabled);
            self.report(result, cx);
        }
    }

    fn add_to_path(&mut self, cx: &mut Context<Self>) {
        if let Some(env) = self.env.clone() {
            let result = ops::add_bin_to_path(&env);
            self.report(result, cx);
        }
    }

    fn shell_section(&self, env: &Env, cx: &mut Context<Self>) -> Section {
        let Some(app) = Self::app(cx) else {
            return Section::new("Shell Command").row(
                Row::new("no-command", "No shell command")
                    .detail("This app isn't opened from a terminal."),
            );
        };
        let prefs = ops::load(&env.prefs_path(), &env.bin_dir());
        let command = prefs.command(app.id);
        let typed = self.typed(cx);
        let this = cx.entity().downgrade();
        let rename = cx.entity().downgrade();
        let mut section = Section::new("Shell Command")
            .detail("Open the app from any terminal.")
            .row(
                Row::new("shell-command", format!("`{}` command", command.name))
                    .detail(app.usage(&command.name))
                    .control(
                        Switch::new("shell-command-switch", command.enabled).on_change(
                            move |on, _, cx| {
                                this.update(cx, |view, cx| view.set_enabled(on, cx)).ok();
                            },
                        ),
                    ),
            )
            .row(
                Row::new("shell-command-name", "Name")
                    .detail("2–20 lowercase letters, digits or -. Enter renames.")
                    .control(
                        h_flex()
                            .gap_2()
                            .child(div().w(px(140.)).child(self.input.clone()))
                            .child(
                                Button::new("shell-command-rename", "Rename")
                                    .style(ButtonStyle::Subtle)
                                    .disabled(typed.is_empty() || typed == command.name)
                                    .on_click(move |_, _, cx| {
                                        rename.update(cx, |view, cx| view.rename(cx)).ok();
                                    }),
                            ),
                    ),
            );
        if let Some((message, error)) = &self.message {
            section = section.row(
                div().py_2().child(
                    Label::new(message.clone())
                        .size(LabelSize::Small)
                        .color(if *error { Color::Error } else { Color::Success }),
                ),
            );
        }
        section.footnote(
            "The command is a small script in ~/.ferrum/bin. FerrumLauncher's app pages show \
             the same choice.",
        )
    }

    fn ferrum_section(&self, env: &Env, cx: &mut Context<Self>) -> Section {
        let prefs = ops::load(&env.prefs_path(), &env.bin_dir());
        let on_path = env.on_path();
        let this = cx.entity().downgrade();
        let path_button = cx.entity().downgrade();
        Section::new("Ferrum Command")
            .row(
                Row::new("ferrum-command", "`ferrum <app> [path]`")
                    .detail(
                        "One command for every app: `ferrum ide .`, `ferrum git`, `ferrum term`, \
                         `ferrum list`, `ferrum help`.",
                    )
                    .control(Switch::new("ferrum-command-switch", prefs.umbrella_enabled()).on_change(
                        move |on, _, cx| {
                            this.update(cx, |view, cx| view.set_umbrella(on, cx)).ok();
                        },
                    )),
            )
            .row(
                Row::new("ferrum-path", "PATH")
                    .detail(if on_path {
                        "~/.ferrum/bin is on your PATH. Open a new terminal window after a change."
                    } else {
                        "~/.ferrum/bin isn't on your PATH yet, so terminals won't find the commands."
                    })
                    .control(
                        Button::new("ferrum-path-add", if on_path { "Added" } else { "Add to PATH" })
                            .style(ButtonStyle::Subtle)
                            .disabled(on_path)
                            .on_click(move |_, _, cx| {
                                path_button.update(cx, |view, cx| view.add_to_path(cx)).ok();
                            }),
                    ),
            )
    }

    fn dock_section(cx: &App) -> Section {
        let in_dock = !crate::dock::hidden_by_preference();
        let info = app_info(cx);
        Section::new("Dock").row(
            Row::new("dock-icon", "Show in Dock")
                .detail(
                    "Off: no Dock icon or ⌘-Tab entry; the app's menus move to an icon in the \
                     menu bar.",
                )
                .control(
                    Switch::new("dock-icon-switch", in_dock).on_change(move |on, _, cx| {
                        if let Some(on_dock) = &info.on_dock {
                            on_dock(on, cx);
                        } else {
                            crate::dock::set_hidden(info.binary, !on);
                            crate::dock::set_dock_icon_visible(on);
                        }
                        cx.refresh_windows();
                    }),
                ),
        )
    }
}

impl Render for SystemView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut page = div().flex().flex_col().gap_6();
        match self.env.clone() {
            Some(env) => {
                page = page
                    .child(self.shell_section(&env, cx))
                    .child(self.ferrum_section(&env, cx));
            }
            None => {
                page = page.child(Section::new("Shell Command").row(
                    Row::new("no-home", "Unavailable").detail("No home folder to install into."),
                ));
            }
        }
        if cfg!(target_os = "macos") {
            page = page.child(Self::dock_section(cx));
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gpui::TestAppContext;

    use super::*;
    use crate::app_info::{AppInfo, AppInfoGlobal};

    fn env(dir: &std::path::Path) -> Env {
        Env {
            ferrum_dir: dir.join(".ferrum"),
            home: Some(dir.to_path_buf()),
            shell: ferrum_shell::Shell::Zsh,
            path_var: String::new(),
        }
    }

    /// The page renders, and its switches and name field write the scripts
    /// (into a temporary `~/.ferrum`, never the real one).
    #[cfg(unix)]
    #[gpui::test]
    fn renders_and_changes_take_effect(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("tmp");
        let env = env(dir.path());
        let bin = env.bin_dir();
        cx.update(|cx| {
            crate::init_for_tests(cx);
            cx.set_global(AppInfoGlobal(Rc::new(AppInfo::new(
                "FerrumIDE",
                "ferrumeditor",
            ))));
        });
        let page_env = env.clone();
        let (view, cx) =
            cx.add_window_view(move |window, cx| SystemView::new(Some(page_env), window, cx));
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |v, cx| v.typed(cx)), "fei");

        view.update(cx, |v, cx| v.set_enabled(true, cx));
        cx.run_until_parked();
        assert!(bin.join("fei").is_file());
        assert!(bin.join("ferrum").is_file(), "the umbrella comes along");
        assert!(view.read_with(cx, |v, _| v.message.as_ref().is_some_and(|m| !m.1)));

        view.update_in(cx, |v, _, cx| {
            v.input.update(cx, |input, cx| input.set_text("my-ide", cx));
            v.rename(cx);
        });
        cx.run_until_parked();
        assert!(!bin.join("fei").exists());
        assert!(bin.join("my-ide").is_file());

        // A bad name is refused and shown.
        view.update(cx, |v, cx| {
            v.input.update(cx, |input, cx| input.set_text("My IDE", cx));
            v.rename(cx);
        });
        cx.run_until_parked();
        assert!(view.read_with(cx, |v, _| v.message.as_ref().is_some_and(|m| m.1)));
        assert!(bin.join("my-ide").is_file());

        view.update(cx, |v, cx| v.set_umbrella(false, cx));
        view.update(cx, |v, cx| v.set_enabled(false, cx));
        cx.run_until_parked();
        assert!(!bin.join("ferrum").exists());
        assert!(!bin.join("my-ide").exists());
    }

    /// Apps outside the suite (previews, the gallery) say so.
    #[gpui::test]
    fn an_app_without_a_command_renders(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("tmp");
        let env = env(dir.path());
        cx.update(|cx| {
            crate::init_for_tests(cx);
            cx.set_global(AppInfoGlobal(Rc::new(AppInfo::new("Preview", "preview"))));
        });
        let (view, cx) =
            cx.add_window_view(move |window, cx| SystemView::new(Some(env), window, cx));
        cx.run_until_parked();
        view.update(cx, |v, cx| v.set_enabled(true, cx));
        assert!(!dir.path().join(".ferrum/bin").exists());
    }
}
