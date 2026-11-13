//! Start screen: open / clone / init, and recent repositories with their profiles.

use crate::app::FerrumGit;
use crate::components::now_secs;
use crate::components::widgets::small_button;
use crate::state::Dialog;
use ferrum_ui::components::{
    Clickable, Color, Headline, HeadlineSize, Label, LabelCommon, LabelSize, Tooltip, h_flex,
    v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::text::relative_time;
use gpui::{AnyElement, ClickEvent, Context, ElementId, Window, div, prelude::*, px};

fn home_action(
    id: impl Into<ElementId>,
    title: &'static str,
    hint: &'static str,
    on_click: impl Fn(&mut FerrumGit, &ClickEvent, &mut Window, &mut Context<FerrumGit>) + 'static,
    cx: &mut Context<FerrumGit>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    v_flex()
        .id(id)
        .flex_1()
        .gap_1()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .cursor_pointer()
        .hover(|s| s.bg(colors.element_hover))
        .child(Label::new(title).weight(gpui::FontWeight::SEMIBOLD))
        .child(Label::new(hint).size(LabelSize::Small).color(Color::Muted))
        .on_click(cx.listener(on_click))
}

pub fn render(app: &FerrumGit, _: &mut Window, cx: &mut Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let needle = app.inputs.home_filter.read(cx).text().to_lowercase();
    let repos: Vec<_> = app
        .state
        .repos
        .iter()
        .filter(|r| {
            needle.is_empty()
                || r.name.to_lowercase().contains(&needle)
                || r.path.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    let none = app.state.repos.is_empty();
    let no_profiles = app.state.profiles.is_empty();
    let now = now_secs();

    let rows = repos
        .into_iter()
        .map(|r| {
            let id = r.id;
            let profile = app
                .state
                .profile_of(&r)
                .map_or_else(|| "System default".to_owned(), |p| p.name);
            let when = if r.last_opened_ms > 0 {
                relative_time(r.last_opened_ms / 1000, now)
            } else {
                String::new()
            };
            h_flex()
                .id(ElementId::Name(format!("repo-{id}").into()))
                .group("repo-row")
                .gap_3()
                .px_4()
                .py_2p5()
                .cursor_pointer()
                .border_b_1()
                .border_color(colors.border_variant)
                .hover(|s| s.bg(colors.element_hover))
                .on_click(cx.listener(move |app, _, _, cx| app.activate(Some(id), cx)))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(Label::new(r.name.clone()).truncate())
                        .child(
                            Label::new(r.path.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .truncate(),
                        ),
                )
                .child(
                    div()
                        .px_2()
                        .rounded_md()
                        .bg(colors.element_background)
                        .child(
                            Label::new(profile)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    div()
                        .w(px(80.))
                        .flex()
                        .justify_end()
                        .child(Label::new(when).size(LabelSize::XSmall).color(Color::Muted)),
                )
                .child(
                    div()
                        .id(ElementId::Name(format!("forget-{id}").into()))
                        .px_1p5()
                        .rounded_sm()
                        .invisible()
                        .group_hover("repo-row", |s| s.visible())
                        .hover(|s| s.bg(colors.element_active))
                        .tooltip(Tooltip::text("Remove from list (files are kept)"))
                        .child(Label::new("×").color(Color::Muted))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.forget(id, cx);
                        })),
                )
        })
        .collect::<Vec<_>>();

    div()
        .id("home")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .justify_center()
        .p_10()
        .child(
            v_flex()
                .w_full()
                .max_w(px(768.))
                .gap_8()
                .child(
                    v_flex()
                        .gap_1()
                        .child(Headline::new("Repositories").size(HeadlineSize::Large))
                        .child(
                            Label::new(
                                "Each repository can use its own profile — a commit identity and an SSH key — for commit, fetch, pull and push.",
                            )
                            .color(Color::Muted),
                        ),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .child(home_action(
                            "home-open",
                            "Open",
                            "An existing local repository",
                            |app, _, _, cx| app.pick_and_open(cx),
                            cx,
                        ))
                        .child(home_action(
                            "home-clone",
                            "Clone",
                            "From a URL, with a chosen SSH profile",
                            |app, _, window, cx| app.open_dialog(Dialog::Clone, window, cx),
                            cx,
                        ))
                        .child(home_action(
                            "home-init",
                            "Init",
                            "Start a new repository in a folder",
                            |app, _, _, cx| app.pick_and_init(cx),
                            cx,
                        )),
                )
                .when(no_profiles, |d| {
                    d.child(
                        h_flex()
                            .gap_3()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_dashed()
                            .border_color(colors.border)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .child(Label::new("Set up a profile"))
                                    .child(
                                        Label::new("Add your work and personal identities with their SSH keys, then pick one per repository.")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(small_button("home-add-profile", "Add profile").on_click(
                                cx.listener(|app, _, window, cx| {
                                    app.open_dialog(Dialog::Profiles(None), window, cx);
                                }),
                            )),
                    )
                })
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div().flex_1().child(
                                        Label::new("RECENT")
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                                )
                                .when(!none, |d| {
                                    d.child(div().w(px(224.)).child(app.inputs.home_filter.clone()))
                                }),
                        )
                        .when(none, |d| {
                            d.child(
                                div()
                                    .p_6()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(colors.border)
                                    .flex()
                                    .justify_center()
                                    .child(
                                        Label::new("No repositories yet. Open, clone or init one above.")
                                            .color(Color::Muted),
                                    ),
                            )
                        })
                        .when(!none, |d| {
                            d.child(
                                v_flex()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(colors.border)
                                    .overflow_hidden()
                                    .children(rows),
                            )
                        }),
                ),
        )
        .into_any_element()
}
