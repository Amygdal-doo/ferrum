//! Clone a repository with a chosen profile (its SSH key is used for the clone
//! and the repository keeps that profile afterwards).

use super::{DialogState, WIDE, modal, width};
use crate::app::FerrumGit;
use crate::components::widgets::{
    field, ghost_button, hint, primary_button, radio_row, small_button,
};
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumgit_application::CancellationToken;
use ferrumgit_domain::ProfileId;
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*};
use theme::ActiveTheme;
use ui::{Clickable, Color, Disableable, Label, LabelCommon, LabelSize, h_flex, v_flex};

pub struct CloneForm {
    pub url: Entity<TextInput>,
    pub parent: Entity<TextInput>,
    pub name: Entity<TextInput>,
    pub name_edited: bool,
    pub profile: Option<ProfileId>,
    pub running: Option<CancellationToken>,
}

/// `git@host:o/repo.git` → `repo`.
fn repo_name(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let last = u.rsplit(['/', ':']).next().unwrap_or(u);
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

impl CloneForm {
    pub fn new(app: &mut FerrumGit, window: &mut Window, cx: &mut Context<FerrumGit>) -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let url = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("git@github.com:owner/repo.git")
                .monospace()
        });
        let parent = cx.new(|cx| TextInput::new(window, cx).monospace().with_text(home));
        let name = app.input("Folder name", window, cx);
        app.dialog_subscriptions.push(cx.subscribe_in(
            &url,
            window,
            |app, input, event, window, cx| match event {
                TextInputEvent::Changed => {
                    let derived = repo_name(input.read(cx).text());
                    if let Some(DialogState::Clone(form)) = &app.dialog
                        && !form.name_edited
                    {
                        form.name.update(cx, |name, cx| name.set_text(derived, cx));
                    }
                    cx.notify();
                }
                TextInputEvent::Submit => app.submit_dialog(window, cx),
                TextInputEvent::Blurred => {}
            },
        ));
        app.dialog_subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |app, _, event, window, cx| match event {
                TextInputEvent::Changed => {
                    if let Some(DialogState::Clone(form)) = &mut app.dialog {
                        form.name_edited = true;
                    }
                    cx.notify();
                }
                TextInputEvent::Submit => app.submit_dialog(window, cx),
                TextInputEvent::Blurred => {}
            },
        ));
        app.watch_input(&parent, window, cx);
        Self {
            url,
            parent,
            name,
            name_edited: false,
            profile: app.state.profiles.first().map(|p| p.id),
            running: None,
        }
    }

    fn destination(&self, cx: &gpui::App) -> String {
        format!(
            "{}/{}",
            self.parent.read(cx).text().trim_end_matches('/'),
            self.name.read(cx).text().trim()
        )
    }

    fn valid(&self, cx: &gpui::App) -> bool {
        !self.url.read(cx).text().trim().is_empty()
            && !self.name.read(cx).text().trim().is_empty()
            && self.running.is_none()
    }
}

impl FerrumGit {
    pub fn start_clone(&mut self, cx: &mut Context<Self>) {
        let Some(DialogState::Clone(form)) = &mut self.dialog else {
            return;
        };
        if !form.valid(cx) {
            return;
        }
        let token = CancellationToken::new();
        form.running = Some(token.clone());
        let profile = form
            .profile
            .and_then(|id| self.state.profiles.iter().find(|p| p.id == id).cloned());
        let url = form.url.read(cx).text().to_owned();
        let dest = form.destination(cx);
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.clone_repo(&url, &dest, profile, Some(token)).await },
            |app, result, cx| {
                if let Some(DialogState::Clone(form)) = &mut app.dialog {
                    form.running = None;
                }
                match result {
                    Ok(entry) => {
                        app.dialog = None;
                        app.dialog_subscriptions.clear();
                        app.notify(format!("Cloned {}", entry.name), false, cx);
                        app.upsert_and_open(entry, cx);
                    }
                    Err(e) => app.notify(e, true, cx),
                }
            },
        );
        cx.notify();
    }
}

pub fn render(app: &FerrumGit, cx: &mut Context<FerrumGit>) -> AnyElement {
    let Some(DialogState::Clone(form)) = &app.dialog else {
        return div().into_any_element();
    };
    let dest = form.destination(cx);
    let valid = form.valid(cx);
    let selected = form.profile;
    let running = form.running.clone();
    let mut profiles: Vec<AnyElement> = vec![
        radio_row(
            "clone-profile-none",
            selected.is_none(),
            "System default",
            Some("ssh-agent / ~/.ssh/config".into()),
            cx.listener(|app, _, _, cx| {
                if let Some(DialogState::Clone(form)) = &mut app.dialog {
                    form.profile = None;
                }
                cx.notify();
            }),
            cx,
        )
        .into_any_element(),
    ];
    for p in &app.state.profiles {
        let pid = p.id;
        profiles.push(
            radio_row(
                SharedString::from(format!("clone-profile-{pid}")),
                selected == Some(pid),
                p.name.clone(),
                Some(p.ssh_label().into()),
                cx.listener(move |app, _, _, cx| {
                    if let Some(DialogState::Clone(form)) = &mut app.dialog {
                        form.profile = Some(pid);
                    }
                    cx.notify();
                }),
                cx,
            )
            .into_any_element(),
        );
    }
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut m = modal("Clone repository", cx)
        .width(width(WIDE))
        .child(field("Repository URL", form.url.clone()))
        .child(field(
            "Clone into",
            h_flex()
                .gap_2()
                .child(div().flex_1().child(form.parent.clone()))
                .child(
                    small_button("clone-browse", "Browse…").on_click(cx.listener(
                        |app, _, _, cx| {
                            app.pick_path(cx, "Clone into folder", true, |app, path, cx| {
                                if let Some(DialogState::Clone(form)) = &app.dialog {
                                    form.parent.update(cx, |input, cx| {
                                        input.set_text(path.display().to_string(), cx)
                                    });
                                }
                            });
                        },
                    )),
                ),
        ))
        .child(
            field("Folder name", form.name.clone()).child(
                Label::new(dest)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            ),
        )
        .child(field(
            "Profile (SSH key + identity for this repository)",
            v_flex()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .children(profiles),
        ));
    m = match running {
        Some(token) => m.footer(hint("Cloning…")).footer(
            ghost_button("clone-cancel", "Cancel clone").on_click(move |_, _, _| token.cancel()),
        ),
        None => m.footer(
            ghost_button("clone-close", "Cancel")
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        ),
    };
    m.footer(
        primary_button("clone-start", "Clone")
            .disabled(!valid)
            .on_click(cx.listener(|app, _, _, cx| app.start_clone(cx))),
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::repo_name;

    #[test]
    fn derives_folder_name() {
        assert_eq!(repo_name("git@github.com:o/ferrum.git"), "ferrum");
        assert_eq!(repo_name("https://gitlab.com/g/sub/app/"), "app");
        assert_eq!(repo_name("ssh://git@host:2222/x/y.git"), "y");
    }
}
