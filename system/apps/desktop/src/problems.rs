//! The Problems tab: what [`ferrumsystem_core::lint`] finds in the design
//! (no health check, one replica on a critical path, a database reachable
//! from a public layer, a connection without auth…). Clicking a problem
//! selects its component on the diagram; the toolbar shows how many there
//! are.

use ferrum_ui::prelude::*;
use ferrumsystem_core::lint::{self, Severity};

use crate::app::{FerrumSystem, Tab};
use crate::panel::para;

impl FerrumSystem {
    /// Selects `id` and keeps the Problems tab open, to go on down the list.
    pub(crate) fn select_problem(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if let Some(id) = id {
            self.select(Some(id), cx);
            self.tab = Tab::Problems;
        }
        cx.notify();
    }

    /// Opens the panel on the Problems tab.
    pub(crate) fn show_problems(&mut self, cx: &mut Context<Self>) {
        self.set_panel_open(true, cx);
        self.tab = Tab::Problems;
        cx.notify();
    }

    pub(crate) fn problems_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let problems = lint::check(&self.system);
        if problems.is_empty() {
            return v_flex()
                .pt_3()
                .child(para("No problems found in this design.", cx))
                .into_any_element();
        }
        let colors = cx.theme().colors().clone();
        v_flex()
            .pt_3()
            .gap_1()
            .children(problems.into_iter().enumerate().map(|(i, p)| {
                let selected = p.node.is_some() && p.node == self.selected;
                let (icon, color) = match p.rule.severity() {
                    Severity::Warning => (IconName::Warning, Color::Warning),
                    Severity::Info => (IconName::Info, Color::Muted),
                };
                let node = p.node.clone();
                h_flex()
                    .id(("problem", i))
                    .items_start()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .when(selected, |row| row.bg(colors.element_selected))
                    .when(p.node.is_some(), |row| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(colors.element_hover))
                    })
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_problem(node.clone(), cx)),
                    )
                    .child(
                        div()
                            .pt(px(2.))
                            .child(Icon::new(icon).size(IconSize::Small).color(color)),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(p.rule.label()),
                            )
                            .child(para(p.message, cx)),
                    )
            }))
            .into_any_element()
    }

    /// The toolbar's count of warnings, opening the Problems tab.
    pub(crate) fn problems_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let warnings = lint::check(&self.system)
            .iter()
            .filter(|p| p.rule.severity() == Severity::Warning)
            .count();
        (warnings > 0).then(|| {
            Button::new("problems", warnings.to_string())
                .size(ButtonSize::Compact)
                .style(ButtonStyle::Subtle)
                .start_icon(
                    Icon::new(IconName::Warning)
                        .size(IconSize::Small)
                        .color(Color::Warning),
                )
                .tooltip(Tooltip::text("Problems in the design"))
                .on_click(cx.listener(|this, _, _, cx| this.show_problems(cx)))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::app::Tab;
    use crate::tests::{Fixture, draw};

    #[gpui::test]
    fn clicking_a_problem_selects_its_component(cx: &mut gpui::TestAppContext) {
        let f = Fixture::new();
        let project = f.project("shop", true);
        let (app, cx) = f.open(cx, Some(project));
        app.update(cx, |app, cx| {
            // The starter's API pods have no health check.
            let problems = ferrumsystem_core::lint::check(&app.system);
            let api = problems
                .iter()
                .find(|p| p.node.as_deref() == Some("api"))
                .expect("a problem on the API");
            app.show_problems(cx);
            app.select_problem(api.node.clone(), cx);
            assert_eq!(app.selected.as_deref(), Some("api"));
            assert_eq!(app.tab, Tab::Problems);
            assert!(app.problems_button(cx).is_some());
        });
        draw(cx);
        // Fixing the health check clears that problem.
        app.update(cx, |app, cx| {
            app.edit(cx, |s| {
                for n in &mut s.nodes {
                    if let Some(d) = n.deploy.as_mut() {
                        d.health = Some("GET /healthz".into());
                    }
                }
            });
            let left = ferrumsystem_core::lint::check(&app.system);
            assert!(
                !left
                    .iter()
                    .any(|p| p.rule == ferrumsystem_core::lint::Rule::NoHealthCheck),
                "{left:?}"
            );
        });
    }
}
