//! Detail panel: one container (logs, inspect, stats) or a compose project
//! (combined logs, `compose ps`).

use crate::app::FerrumDock;
use crate::state::DetailTab;
use crate::views::containers::state_color;
use crate::views::widgets::{icon_button, muted, segmented, state_dot};
use ferrumdock_engine::compose::ComposeAction;
use ferrumdock_engine::shell::Shell;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{IconName, Label, LabelCommon, LabelSize, Tab, TabBar, Toggleable, h_flex, v_flex};

pub fn render(
    app: &mut FerrumDock,
    window: &mut Window,
    cx: &mut Context<FerrumDock>,
) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let container = app.state.container(&detail.id).cloned();
    let project = detail.project.clone();
    let title = match (&project, &container) {
        (Some(p), _) => format!("{} · all services", p.name),
        (None, Some(c)) => c.name.clone(),
        (None, None) => detail.id.clone(),
    };
    let shell_picker = container.as_ref().filter(|c| c.state.is_up()).map(|_| {
        let options = Shell::ALL.map(|shell| {
            let available = detail
                .shells
                .as_ref()
                .is_none_or(|known| shell == Shell::Auto || known.contains(&shell));
            (shell.label(), detail.shell == shell, !available, shell)
        });
        segmented("shell", &options, cx, |app, shell, cx| {
            app.set_shell(shell, cx)
        })
    });
    let compose_buttons = project.clone().map(|p| {
        let busy = app.state.busy.contains(&format!("compose:{}", p.name));
        h_flex().gap_0p5().children(
            [
                ("Up", ComposeAction::Up),
                ("Restart", ComposeAction::Restart),
                ("Down", ComposeAction::Down),
            ]
            .map(|(label, action)| {
                let p = p.clone();
                crate::views::widgets::button(
                    SharedString::from(format!("detail-compose-{label}")),
                    label,
                    busy,
                    cx,
                    move |app, cx| app.compose(p.clone(), action, cx),
                )
            }),
        )
    });
    let header = h_flex()
        .flex_none()
        .h(px(40.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(colors.border)
        .when_some(container.as_ref(), |d, c| {
            d.child(state_dot(state_color(c.state, cx)))
        })
        .child(Label::new(title).weight(FontWeight::SEMIBOLD).truncate())
        .when_some(container.as_ref(), |d, c| d.child(muted(c.status.clone())))
        .when(project.is_some(), |d| {
            d.child(muted(format!("{} containers", detail.sources.len())))
        })
        .child(div().flex_1())
        .children(compose_buttons)
        .children(shell_picker)
        .when_some(container.clone(), |d, c| {
            let up = c.state.is_up();
            d.child(icon_button(
                "detail-shell",
                IconName::Terminal,
                "Open shell",
                !up,
                cx,
                move |app, cx| {
                    app.open_shell(&c, cx);
                },
            ))
        })
        .child(icon_button(
            "detail-close",
            IconName::Close,
            "Close (Esc)",
            false,
            cx,
            |app, cx| app.close_detail(cx),
        ));

    let tab = detail.tab;
    let tabs = if project.is_some() {
        TabBar::new("detail-tabs")
            .child(tab_button("Logs", DetailTab::Logs, tab, cx))
            .child(tab_button("Services", DetailTab::Services, tab, cx))
    } else {
        TabBar::new("detail-tabs")
            .child(tab_button("Logs", DetailTab::Logs, tab, cx))
            .child(tab_button("Inspect", DetailTab::Inspect, tab, cx))
            .child(tab_button("Stats", DetailTab::Stats, tab, cx))
    };
    let content = match tab {
        DetailTab::Logs => super::logs::render(app, window, cx),
        DetailTab::Inspect => super::inspect::render(app, cx),
        DetailTab::Stats => super::stats::render(app, cx),
        DetailTab::Services => super::services::render(app, cx),
    };
    v_flex()
        .size_full()
        .bg(colors.panel_background)
        .child(header)
        .child(tabs)
        .child(content)
        .into_any_element()
}

fn tab_button(
    label: &'static str,
    tab: DetailTab,
    current: DetailTab,
    cx: &mut Context<FerrumDock>,
) -> Tab {
    Tab::new(label)
        .toggle_state(tab == current)
        .child(Label::new(label).size(LabelSize::Small))
        .on_click(cx.listener(move |app, _, _, cx| app.set_detail_tab(tab, cx)))
}
