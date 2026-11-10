//! Per-repository settings: which profile (identity + SSH key) it uses, remotes.

use super::{WIDE, modal, width};
use crate::app::FerrumGit;
use crate::commands::workspace::reveal;
use crate::components::widgets::{
    caption, ghost_button, hint, primary_button, radio_row, small_button,
};
use crate::state::Dialog;
use ferrumgit_domain::{PullMode, RepoId, RepoOp};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, Checkbox, Clickable, Color, Label, LabelCommon, LabelSize, StyledTypography,
    ToggleState, Toggleable, h_flex, v_flex,
};

pub fn render(
    app: &FerrumGit,
    id: RepoId,
    _: &mut Window,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some(entry) = app.state.entry(id) else {
        return div().into_any_element();
    };
    let current = entry.profile;
    let remotes = app
        .state
        .view(id)
        .data
        .map(|d| d.snapshot.remotes.clone())
        .unwrap_or_default();
    let ssh_remote = remotes.iter().any(ferrumgit_domain::Remote::is_ssh);
    let path = entry.path.clone();
    let settings = app.state.settings;
    let (pull_mode, prune, show_ws) = (
        settings.pull_mode,
        settings.prune_on_fetch,
        !settings.hide_invisibles,
    );

    let mut choices: Vec<AnyElement> = vec![
        radio_row(
            "repo-profile-none",
            current.is_none(),
            "System default",
            Some("Global git identity · ssh-agent / ~/.ssh/config".into()),
            cx.listener(move |app, _, _, cx| app.assign_profile(id, None, cx)),
            cx,
        )
        .into_any_element(),
    ];
    for p in &app.state.profiles {
        let pid = p.id;
        let who = p.identity().map_or_else(
            || "git config identity".to_owned(),
            |(n, e)| format!("{n} <{e}>"),
        );
        choices.push(
            radio_row(
                SharedString::from(format!("repo-profile-{pid}")),
                current == Some(pid),
                p.name.clone(),
                Some(format!("{who} · 🔑 {}", p.ssh_label()).into()),
                cx.listener(move |app, _, _, cx| app.assign_profile(id, Some(pid), cx)),
                cx,
            )
            .into_any_element(),
        );
    }

    let remote_rows: Vec<AnyElement> = remotes
        .iter()
        .map(|r| {
            let name = r.name.clone();
            h_flex()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .child(Label::new(r.name.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(colors.text_muted)
                        .child(r.fetch_url.clone()),
                )
                .child(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .bg(colors.element_background)
                        .child(
                            Label::new(if r.is_ssh() { "SSH" } else { "HTTPS/local" })
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    ghost_button(
                        SharedString::from(format!("remove-remote-{}", r.name)),
                        "Remove",
                    )
                    .color(Color::Error)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_dialog(
                            Dialog::Confirm {
                                title: format!("Remove remote {name}?"),
                                body: "Its remote-tracking branches are removed too.".into(),
                                confirm: "Remove".into(),
                                destructive: true,
                                op: RepoOp::RemoveRemote(name.clone()),
                            },
                            window,
                            cx,
                        );
                    })),
                )
                .into_any_element()
        })
        .collect();

    let pull_buttons: Vec<AnyElement> = PullMode::ALL
        .into_iter()
        .map(|m| {
            small_button(SharedString::from(format!("pull-{}", m.label())), m.label())
                .toggle_state(m == pull_mode)
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.state.settings.pull_mode = m;
                    app.save_settings(cx);
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    let section_head = |title: &str, action: Option<AnyElement>| {
        h_flex()
            .child(div().flex_1().child(caption(title)))
            .children(action)
    };

    modal(format!("Repository settings — {}", entry.name), cx)
        .width(width(WIDE))
        .child(
            v_flex()
                .gap_2()
                .child(section_head(
                    "Profile for this repository",
                    Some(
                        ghost_button("manage-profiles", "Manage profiles…")
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_dialog(Dialog::Profiles(current), window, cx);
                            }))
                            .into_any_element(),
                    ),
                ))
                .child(hint(
                    "Commits use the profile's name and email; fetch, pull, push and clone use its SSH key (only that key is offered to the server).",
                ))
                .child(
                    v_flex()
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border)
                        .children(choices),
                )
                .when(!ssh_remote && !remotes.is_empty(), |d| {
                    d.child(
                        Label::new("This repository's remotes use HTTPS, so the SSH key is not used; git's credential helper handles authentication.")
                            .size(LabelSize::Small)
                            .color(Color::Warning),
                    )
                }),
        )
        .child(
            v_flex()
                .gap_2()
                .child(section_head(
                    "Remotes",
                    Some(
                        ghost_button("add-remote-btn", "Add remote…")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.open_dialog(Dialog::AddRemote, window, cx);
                            }))
                            .into_any_element(),
                    ),
                ))
                .when(remotes.is_empty(), |d| d.child(hint("No remotes.")))
                .children(remote_rows),
        )
        .child(
            v_flex()
                .gap_2()
                .child(section_head(
                    "Pull requests",
                    Some(
                        ghost_button("forge-settings", "Token & forge…")
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_dialog(Dialog::ForgeSettings(id), window, cx);
                            }))
                            .into_any_element(),
                    ),
                ))
                .child(hint(
                    "GitHub and GitLab (including self-hosted) are detected from the remote. Tokens are kept in the OS keychain.",
                )),
        )
        .child(
            v_flex()
                .gap_2()
                .child(caption("Preferences (all repositories)"))
                .child(
                    h_flex()
                        .gap_3()
                        .child(div().w(px(112.)).child(Label::new("Pull").color(Color::Muted)))
                        .child(h_flex().gap_1().children(pull_buttons)),
                )
                .child(
                    Checkbox::new("pref-prune", ToggleState::from(prune))
                        .label("Prune deleted remote branches when fetching")
                        .on_click(cx.listener(|app, state: &ToggleState, _, cx| {
                            app.state.settings.prune_on_fetch = state.selected();
                            app.save_settings(cx);
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("pref-whitespace", ToggleState::from(show_ws))
                        .label("Show whitespace (· and ¬) in diffs")
                        .on_click(cx.listener(|app, state: &ToggleState, _, cx| {
                            app.state.settings.hide_invisibles = !state.selected();
                            app.save_settings(cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
            v_flex()
                .gap_2()
                .child(caption("Location"))
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(colors.border)
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .child(entry.path.clone()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(small_button("open-folder", "Open folder").on_click(move |_, _, _| reveal(&path)))
                        .child(
                            ghost_button("forget-repo", "Remove from FerrumGIT")
                                .color(Color::Error)
                                .tooltip(ui::Tooltip::text("Removes it from FerrumGIT; files are kept"))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.close_dialog(window, cx);
                                    app.forget(id, cx);
                                })),
                        ),
                ),
        )
        .footer(
            primary_button("repo-settings-done", "Done")
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        )
        .into_any_element()
}
