//! Screens shown before runs: no repository yet, and sign-in.

use crate::ci::{CiEvent, CiView, Screen};
use ferrum_ui::prelude::*;
use ferrumci_core::auth::env_names;

/// No GitHub repository open: the message plus recent GitHub projects.
pub fn render_empty(
    title: String,
    detail: String,
    app: &CiView,
    cx: &mut Context<CiView>,
) -> AnyElement {
    let home = ferrum_ui::project::home_dir();
    let projects: Vec<_> = app
        .github_projects
        .iter()
        .take(12)
        .enumerate()
        .map(|(i, (path, repo))| {
            let path = path.clone();
            ListItem::new(("recent", i))
                .rounded()
                .child(
                    h_flex().gap_2().child(Label::new(repo.path())).child(
                        Label::new(ferrum_ui::project::display_path(&path, home.as_deref()))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                )
                .on_click(
                    cx.listener(move |_, _, _, cx| cx.emit(CiEvent::OpenProject(path.clone()))),
                )
        })
        .collect();
    let has_projects = !projects.is_empty();
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .child(Headline::new(title).size(HeadlineSize::Small))
        .child(Label::new(detail).color(Color::Muted))
        .when(has_projects, |el| {
            el.child(
                v_flex()
                    .mt_4()
                    .w(px(520.))
                    .child(section_header("Recent GitHub projects"))
                    .children(projects),
            )
        })
        .into_any_element()
}

pub fn render_sign_in(app: &CiView, cx: &mut Context<CiView>) -> AnyElement {
    let Screen::SignIn { repo, error } = &app.screen else {
        return div().into_any_element();
    };
    let host = repo.host.clone();
    let help = ferrumgit_forge::auth::token_help(repo);
    let create_url = help.create_url.replace("FerrumGIT", "Ferrum%20Editor%20CI");
    let vars = env_names(&host).join(" or ");
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .child(
            v_flex()
                .w(px(560.))
                .gap_3()
                .child(Headline::new(format!("Sign in to {host}")).size(HeadlineSize::Small))
                .child(Label::new(format!(
                    "The CI page needs a GitHub token to read Actions for {}. It looks in this order:",
                    repo.path()
                )))
                .child(Label::new("1. The token FerrumGIT saved in the macOS keychain").color(Color::Muted))
                .child(
                    Label::new(format!("2. The GitHub CLI login (`gh auth login --hostname {host}`)"))
                        .color(Color::Muted),
                )
                .child(Label::new(format!("3. The {vars} environment variable")).color(Color::Muted))
                .child(
                    Label::new(
                        "Classic tokens need the `repo` scope; fine-grained tokens need Actions (read and write), Contents, Commit statuses and Pull requests (read).",
                    )
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                )
                .child(section_header("Paste a token (saved in the keychain, shared with FerrumGIT)"))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(app.token_input.clone()))
                        .child(
                            Button::new("save-token", "Save token")
                                .style(ButtonStyle::Filled)
                                .on_click(cx.listener(|this, _, _, cx| this.save_token(cx))),
                        ),
                )
                .when_some(error.clone(), |el, e| {
                    el.child(Label::new(e).size(LabelSize::Small).color(Color::Error))
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("create-token", "Create a token on GitHub")
                                .style(ButtonStyle::Outlined)
                                .on_click(move |_, _, cx| cx.open_url(&create_url)),
                        )
                        .child(
                            Button::new("retry-token", "I ran `gh auth login` — try again")
                                .style(ButtonStyle::Subtle)
                                .on_click(cx.listener(|this, _, _, cx| this.retry_sign_in(cx))),
                        ),
                ),
        )
        .into_any_element()
}
