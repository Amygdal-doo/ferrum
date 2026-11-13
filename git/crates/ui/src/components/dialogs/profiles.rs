//! Profiles manager: identity + SSH key, passphrase in the OS keychain,
//! key generation and "Test connection".

use super::{DialogState, WIDEST, modal, width};
use crate::app::GitView;
use crate::commands::background;
use crate::components::widgets::{
    caption, card, danger_button, field, ghost_button, hint, primary_button, radio_row,
    small_button,
};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Disableable, Label, LabelCommon, LabelSize, StyledTypography,
    h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumgit_application::{AppServices, PassphraseChange};
use ferrumgit_domain::{Profile, ProfileId, Secret, SshAuth};
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*, px};

pub struct ProfilesForm {
    pub current: Option<ProfileId>,
    pub editor: Option<Editor>,
}

pub struct Editor {
    id: ProfileId,
    is_new: bool,
    stored: bool,
    name: Entity<TextInput>,
    author: Entity<TextInput>,
    email: Entity<TextInput>,
    use_key: bool,
    key_path: Entity<TextInput>,
    passphrase: Entity<TextInput>,
    passphrase_change: Option<PassphraseChange>,
    host: Entity<TextInput>,
    test: Option<Result<String, String>>,
    testing: bool,
    public: Option<String>,
    confirm_delete: bool,
    keys: Vec<String>,
}

fn editor_mut(app: &mut GitView) -> Option<&mut Editor> {
    match &mut app.dialog {
        Some(DialogState::Profiles(form)) => form.editor.as_mut(),
        _ => None,
    }
}

impl Editor {
    fn new(
        app: &mut GitView,
        profile: Profile,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<GitView>,
    ) -> Self {
        let text =
            |placeholder: &str, value: String, window: &mut Window, cx: &mut Context<GitView>| {
                let placeholder = placeholder.to_owned();
                cx.new(|cx| {
                    TextInput::new(window, cx)
                        .placeholder(placeholder)
                        .with_text(value)
                })
            };
        let (initial_key, stored) = match &profile.ssh {
            SshAuth::KeyFile {
                path,
                has_passphrase,
            } => (Some(path.clone()), *has_passphrase),
            SshAuth::System => (None, false),
        };
        let name = text(
            "Work, Personal, Client X…",
            profile.name.clone(),
            window,
            cx,
        );
        let author = text("Ada Lovelace", profile.author_name.clone(), window, cx);
        let email = text("ada@company.com", profile.author_email.clone(), window, cx);
        let key_path = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("~/.ssh/id_ed25519_work")
                .monospace()
                .with_text(initial_key.clone().unwrap_or_default())
        });
        let passphrase = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder(if stored {
                    "•••••••• (stored)"
                } else {
                    "Optional"
                })
        });
        let host = text(
            "github.com, gitlab.com, git@host…",
            "github.com".to_owned(),
            window,
            cx,
        );
        for input in [&name, &author, &email, &host] {
            app.watch_input(input, window, cx);
        }
        app.dialog_subscriptions
            .push(cx.subscribe(&key_path, |app, _, event, cx| {
                if *event == TextInputEvent::Changed {
                    if let Some(editor) = editor_mut(app) {
                        editor.public = None;
                    }
                    cx.notify();
                }
            }));
        app.dialog_subscriptions
            .push(cx.subscribe(&passphrase, |app, input, event, cx| {
                if *event == TextInputEvent::Changed {
                    let value = input.read(cx).text().to_owned();
                    if let Some(editor) = editor_mut(app) {
                        editor.passphrase_change = Some(PassphraseChange::Set(Secret::new(value)));
                    }
                    cx.notify();
                }
            }));

        let services = app.services.clone();
        app.spawn_run(
            cx,
            async move {
                Ok::<_, String>(
                    background(
                        &services.clone(),
                        async move { services.discover_keys().await },
                    )
                    .await,
                )
            },
            |app, result, _| {
                if let Ok(Ok(found)) = result
                    && let Some(editor) = editor_mut(app)
                {
                    editor.keys = found;
                }
            },
        );

        Self {
            id: profile.id,
            is_new,
            stored,
            name,
            author,
            email,
            use_key: initial_key.is_some(),
            key_path,
            passphrase,
            passphrase_change: None,
            host,
            test: None,
            testing: false,
            public: None,
            confirm_delete: false,
            keys: Vec::new(),
        }
    }

    fn build(&self, cx: &gpui::App) -> Profile {
        let path = self.key_path.read(cx).text().trim().to_owned();
        Profile {
            id: self.id,
            name: self.name.read(cx).text().trim().to_owned(),
            author_name: self.author.read(cx).text().trim().to_owned(),
            author_email: self.email.read(cx).text().trim().to_owned(),
            ssh: if self.use_key && !path.is_empty() {
                SshAuth::KeyFile {
                    path,
                    has_passphrase: self.stored,
                }
            } else {
                SshAuth::System
            },
        }
    }
}

impl ProfilesForm {
    pub fn new(
        app: &mut GitView,
        selected: Option<ProfileId>,
        window: &mut Window,
        cx: &mut Context<GitView>,
    ) -> Self {
        let current = selected.or_else(|| app.state.profiles.first().map(|p| p.id));
        let editor = current
            .and_then(|id| app.state.profiles.iter().find(|p| p.id == id).cloned())
            .map(|p| Editor::new(app, p, false, window, cx));
        Self { current, editor }
    }
}

impl GitView {
    fn select_profile(
        &mut self,
        profile: Option<Profile>,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = profile
            .clone()
            .map(|p| Editor::new(self, p, is_new, window, cx));
        if let Some(DialogState::Profiles(form)) = &mut self.dialog {
            form.current = if is_new { None } else { profile.map(|p| p.id) };
            form.editor = editor;
        }
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = editor_mut(self) else {
            return;
        };
        let change = editor
            .passphrase_change
            .take()
            .unwrap_or(PassphraseChange::Keep);
        let profile = editor.build(cx);
        self.save_profile(profile, change, cx, |app, saved, cx| {
            if let Some(DialogState::Profiles(form)) = &mut app.dialog {
                form.current = Some(saved.id);
                if let Some(editor) = form.editor.as_mut()
                    && editor.id == saved.id
                {
                    editor.is_new = false;
                    editor.stored = matches!(
                        saved.ssh,
                        SshAuth::KeyFile {
                            has_passphrase: true,
                            ..
                        }
                    );
                }
            }
            cx.notify();
        });
    }

    fn test_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = editor_mut(self) else {
            return;
        };
        editor.testing = true;
        editor.test = None;
        let profile = editor.build(cx);
        let host = editor.host.read(cx).text().to_owned();
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.test_profile(&profile, &host).await },
            |app, result, _| {
                if let Some(editor) = editor_mut(app) {
                    editor.testing = false;
                    editor.test = Some(result);
                }
            },
        );
    }

    fn show_public_key(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = editor_mut(self) else {
            return;
        };
        let key = editor.key_path.read(cx).text().to_owned();
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.public_key(&key).await },
            |app, result, cx| match result {
                Ok(public) => {
                    if let Some(editor) = editor_mut(app) {
                        editor.public = Some(public);
                    }
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    fn generate_key(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(editor) = editor_mut(self) else {
            return;
        };
        let comment = editor.email.read(cx).text().to_owned();
        let services = self.services.clone();
        let target = path.clone();
        self.spawn_run(
            cx,
            async move { services.generate_key(&target, &comment).await },
            move |app, result, cx| match result {
                Ok(public) => {
                    if let Some(editor) = editor_mut(app) {
                        editor
                            .key_path
                            .update(cx, |input, cx| input.set_text(path.clone(), cx));
                        editor.public = Some(public);
                    }
                    app.notify(
                        "Key generated. Add the public key to your Git host, then Save.",
                        false,
                        cx,
                    );
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    fn set_key_path(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(editor) = editor_mut(self) {
            editor
                .key_path
                .update(cx, |input, cx| input.set_text(path, cx));
            editor.public = None;
        }
        cx.notify();
    }
}

pub fn render(app: &GitView, cx: &mut Context<GitView>) -> AnyElement {
    let Some(DialogState::Profiles(form)) = &app.dialog else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let new_open = form.editor.as_ref().is_some_and(|e| e.is_new);
    let list: Vec<AnyElement> = app
        .state
        .profiles
        .iter()
        .map(|p| {
            let active = !new_open && form.current == Some(p.id);
            let profile = p.clone();
            v_flex()
                .id(SharedString::from(format!("profile-{}", p.id)))
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .when(active, |d| d.bg(colors.element_selected))
                .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
                .child(Label::new(p.name.clone()).truncate())
                .child(
                    Label::new(p.ssh_label())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.select_profile(Some(profile.clone()), false, window, cx);
                }))
                .into_any_element()
        })
        .collect();
    let no_profiles = list.is_empty() && !new_open;
    let editor = match &form.editor {
        Some(editor) => render_editor(editor, cx),
        None => div()
            .p_6()
            .child(Label::new("Create a profile to get started.").color(Color::Muted))
            .into_any_element(),
    };
    modal("Profiles", cx)
        .width(width(WIDEST))
        .child(hint(
            "A profile is a commit identity plus the SSH key used for network operations. Assign one per repository in its settings (toolbar → Profile).",
        ))
        .child(
            h_flex()
                .items_start()
                .gap_4()
                .child(
                    v_flex()
                        .flex_none()
                        .w(px(224.))
                        .gap_1()
                        .children(list)
                        .when(no_profiles, |d| d.child(div().px_3().py_2().child(hint("No profiles yet."))))
                        .child(div().mt_2().child(small_button("new-profile", "+ New profile").on_click(
                            cx.listener(|app, _, window, cx| {
                                app.select_profile(Some(Profile::new("New profile")), true, window, cx);
                            }),
                        ))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pl_4()
                        .border_l_1()
                        .border_color(colors.border)
                        .child(editor),
                ),
        )
        .into_any_element()
}

fn render_editor(editor: &Editor, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let profile = editor.build(cx);
    let preview = AppServices::ssh_command_preview(&profile);
    let pass_state = match (&editor.passphrase_change, editor.stored) {
        (Some(PassphraseChange::Set(_)), _) => "Will be saved to the keychain",
        (Some(PassphraseChange::Clear), _) => "Will be removed from the keychain",
        (_, true) => "Stored in the OS keychain",
        (_, false) => "None stored (fine for keys without a passphrase or keys in ssh-agent)",
    };
    let slug: String = editor
        .name
        .read(cx)
        .text()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let gen_path = format!("~/.ssh/ferrumgit_{slug}_ed25519");
    let key_empty = editor.key_path.read(cx).text().trim().is_empty();
    let name_empty = editor.name.read(cx).text().trim().is_empty();
    let id = editor.id;
    let use_key = editor.use_key;

    let ssh_mode = |use_key_choice: bool, title: &'static str, cx: &mut Context<GitView>| {
        radio_row(
            if use_key_choice {
                "ssh-key"
            } else {
                "ssh-system"
            },
            use_key == use_key_choice,
            title,
            None,
            cx.listener(move |app, _, _, cx| {
                if let Some(editor) = editor_mut(app) {
                    editor.use_key = use_key_choice;
                }
                cx.notify();
            }),
            cx,
        )
    };

    let key_section = use_key.then(|| {
        let keys: Vec<AnyElement> = editor
            .keys
            .iter()
            .map(|k| {
                let label = k.rsplit('/').next().unwrap_or(k).to_owned();
                let path = k.clone();
                ghost_button(SharedString::from(format!("key-{k}")), label)
                    .tooltip(ferrum_ui::components::Tooltip::text(k.clone()))
                    .on_click(cx.listener(move |app, _, _, cx| app.set_key_path(path.clone(), cx)))
                    .into_any_element()
            })
            .collect();
        let has_keys = !keys.is_empty();
        let gen_for_click = gen_path.clone();
        v_flex()
            .gap_2()
            .pl_6()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(editor.key_path.clone()))
                    .child(small_button("key-browse", "Browse…").on_click(cx.listener(
                        |app, _, _, cx| {
                            app.pick_path(cx, "Choose a private key", false, |app, path, cx| {
                                app.set_key_path(path.display().to_string(), cx);
                            });
                        },
                    ))),
            )
            .when(has_keys, |d| {
                d.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .child(hint("Found in ~/.ssh:"))
                        .children(keys),
                )
            })
            .child(field(
                "Key passphrase",
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(editor.passphrase.clone()))
                    .when(editor.stored, |d| {
                        d.child(ghost_button("pass-forget", "Forget").on_click(cx.listener(
                            |app, _, _, cx| {
                                if let Some(editor) = editor_mut(app) {
                                    editor.passphrase_change = Some(PassphraseChange::Clear);
                                }
                                cx.notify();
                            },
                        )))
                    }),
            ))
            .child(hint(pass_state))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        ghost_button("show-public", "Show public key")
                            .disabled(key_empty)
                            .on_click(cx.listener(|app, _, _, cx| app.show_public_key(cx))),
                    )
                    .child(
                        ghost_button("generate-key", "Generate new key")
                            .tooltip(ferrum_ui::components::Tooltip::text(format!(
                                "Creates {gen_path} (ed25519, no passphrase)"
                            )))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.generate_key(gen_for_click.clone(), cx)
                            })),
                    ),
            )
            .children(editor.public.clone().map(|pk| {
                let copy = pk.clone();
                h_flex()
                    .items_start()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(colors.element_background)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_buffer(cx)
                            .text_ui_xs(cx)
                            .child(pk),
                    )
                    .child(small_button("copy-public", "Copy").on_click(cx.listener(
                        move |app, _, _, cx| {
                            app.copy(&copy, cx);
                            app.notify(
                                "Public key copied — paste it into GitHub/GitLab → SSH keys",
                                false,
                                cx,
                            );
                        },
                    )))
            }))
    });

    let test_result = editor.test.clone().map(|r| match r {
        Ok(m) => Label::new(format!("✓ {m}"))
            .size(LabelSize::Small)
            .color(Color::Success),
        Err(e) => Label::new(format!("✕ {e}"))
            .size(LabelSize::Small)
            .color(Color::Error),
    });

    let delete_row: Option<AnyElement> = (!editor.is_new).then(|| {
        if editor.confirm_delete {
            h_flex()
                .gap_2()
                .child(
                    Label::new("Repositories using it fall back to System default.")
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
                .child(
                    danger_button("delete-profile", "Delete profile").on_click(cx.listener(
                        move |app, _, window, cx| {
                            app.delete_profile(id, cx);
                            app.select_profile(None, false, window, cx);
                        },
                    )),
                )
                .child(ghost_button("keep-profile", "Keep").on_click(cx.listener(
                    |app, _, _, cx| {
                        if let Some(editor) = editor_mut(app) {
                            editor.confirm_delete = false;
                        }
                        cx.notify();
                    },
                )))
                .into_any_element()
        } else {
            ghost_button("ask-delete", "Delete…")
                .color(Color::Error)
                .on_click(cx.listener(|app, _, _, cx| {
                    if let Some(editor) = editor_mut(app) {
                        editor.confirm_delete = true;
                    }
                    cx.notify();
                }))
                .into_any_element()
        }
    });

    v_flex()
        .gap_4()
        .child(
            v_flex()
                .gap_3()
                .child(field("Profile name", editor.name.clone()))
                .child(
                    h_flex()
                        .gap_3()
                        .child(div().flex_1().child(field("Author name", editor.author.clone())))
                        .child(div().flex_1().child(field("Author email", editor.email.clone()))),
                )
                .child(hint("Leave both empty to use your global git identity.")),
        )
        .child(
            card(cx)
                .child(caption("SSH"))
                .child(ssh_mode(false, "System SSH — ssh-agent and ~/.ssh/config decide", cx))
                .child(ssh_mode(true, "Use a specific private key", cx))
                .children(key_section)
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(colors.text_muted)
                        .child(format!("GIT_SSH_COMMAND: {preview}")),
                ),
        )
        .child(
            card(cx)
                .child(caption("Test connection"))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(editor.host.clone()))
                        .child(
                            small_button("test-connection", if editor.testing { "Testing…" } else { "Test" })
                                .disabled(editor.testing)
                                .on_click(cx.listener(|app, _, _, cx| app.test_editor(cx))),
                        ),
                )
                .child(hint(
                    "Runs ssh -T with this key (unsaved edits included). A host seen for the first time is trusted and added to known_hosts.",
                ))
                .children(test_result),
        )
        .child(
            h_flex()
                .gap_2()
                .children(delete_row)
                .child(div().flex_1())
                .child(ghost_button("profiles-close", "Close").on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))))
                .child(
                    primary_button("profiles-save", if editor.is_new { "Create profile" } else { "Save" })
                        .disabled(name_empty)
                        .on_click(cx.listener(|app, _, _, cx| app.save_editor(cx))),
                ),
        )
        .into_any_element()
}
