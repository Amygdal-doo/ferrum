//! Results panel: status line, error/empty states and the virtual grid.

mod grid;
mod header;
mod synthetic;

pub use grid::bind_grid_keys;

use crate::app::FerrumData;
use crate::state::ResultsPane;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{EditorTabId, QueryState};
use ferrumdata_result_grid_core::ExportFormat;

impl FerrumData {
    pub fn render_results(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(tab_id) = self.state.active_tab else {
            return div().into_any_element();
        };
        let pane = self.state.results_pane;
        let warning = cx.theme().status().warning;
        let run = self
            .state
            .runs
            .get(&tab_id)
            .filter(|_| pane == ResultsPane::Results);
        let status_line = run.map(|run| {
            h_flex()
                .gap_3()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .flex_none()
                        .text_color(colors.text_muted)
                        .child(run.status_label()),
                )
                .when_some(run.note.clone(), |this, note| {
                    this.child(div().truncate().text_color(warning).child(note))
                })
                .when(run.buffer.is_truncated(), |this| {
                    this.child(div().truncate().text_color(warning).child(format!(
                        "Showing the first {} rows (client fetch limit). The SQL was not modified.",
                        run.buffer.len()
                    )))
                })
        });
        let has_rows = run.is_some_and(|r| !r.buffer.columns().is_empty() && !r.buffer.is_empty());
        let has_selection = run.is_some_and(|r| r.selection.is_some());
        let explainable = self.can_explain();
        let body = match pane {
            ResultsPane::History => self.render_history(cx),
            ResultsPane::Plan => self.render_plan(tab_id, cx),
            ResultsPane::Results if self.state.runs.contains_key(&tab_id) => {
                self.render_run(tab_id, cx)
            }
            ResultsPane::Results => self.render_empty(tab_id, cx),
        };
        let pane_button = |id: &'static str, label: &'static str, target: ResultsPane| {
            ferrum_ui::header_tab(id, pane == target, cx)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.show_pane(target, cx)))
        };

        v_flex()
            .size_full()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .h(px(36.))
                    .gap_3()
                    .px_2()
                    .text_xs()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_0p5()
                            .child(pane_button("pane-results", "Results", ResultsPane::Results))
                            .child(pane_button("pane-history", "History", ResultsPane::History))
                            .when(explainable || self.state.plans.contains_key(&tab_id), |this| {
                                this.child(pane_button("pane-plan", "Plan", ResultsPane::Plan))
                            }),
                    )
                    .children(status_line)
                    .child(div().flex_1())
                    .when(explainable, |this| {
                        this.child(
                            Button::new("explain", "Explain")
                                .style(ButtonStyle::Subtle)
                                .size(ButtonSize::Compact)
                                .tooltip(Tooltip::text(
                                    "Show the plan of the selection or the statement under the caret",
                                ))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.explain_from_editor(false, cx);
                                })),
                        )
                        .child(
                            Button::new("explain-analyze", "Analyze…")
                                .style(ButtonStyle::Subtle)
                                .size(ButtonSize::Compact)
                                .tooltip(Tooltip::text(
                                    "EXPLAIN ANALYZE: runs the statement and shows actual rows and times",
                                ))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.explain_from_editor(true, cx);
                                })),
                        )
                    })
                    .when(has_rows, |this| {
                        this.child(self.render_copy_menu(has_selection, window, cx))
                            .child(self.render_export_menu(window, cx))
                    }),
            )
            .child(div().relative().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// "Copy as" CSV / JSON / Markdown: the selection, or the whole result.
    fn render_copy_menu(
        &mut self,
        selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let scope = if selection { "selection" } else { "all rows" };
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for format in [
                ExportFormat::Csv,
                ExportFormat::Json,
                ExportFormat::Markdown,
            ] {
                let this = this.clone();
                menu = menu.entry(
                    format!("Copy {scope} as {}", format.label()),
                    None,
                    move |_, cx| {
                        if let Err(e) = this.update(cx, |this, cx| this.copy_results(format, cx)) {
                            tracing::debug!(error = %e, "results panel dropped");
                        }
                    },
                );
            }
            menu
        });
        DropdownMenu::new("copy-as", "Copy as", menu).aria_label("Copy results as")
    }

    /// Save the full result set (every loaded row, in display order) to a file.
    fn render_export_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for format in [ExportFormat::Csv, ExportFormat::Json] {
                let this = this.clone();
                menu = menu.entry(
                    format!("Export {}…", format.label()),
                    None,
                    move |_, cx| {
                        if let Err(e) = this.update(cx, |this, cx| this.export_results(format, cx))
                        {
                            tracing::debug!(error = %e, "results panel dropped");
                        }
                    },
                );
            }
            menu
        });
        DropdownMenu::new("export", "Export", menu).aria_label("Export results to a file")
    }

    fn render_run(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(run) = self.state.runs.get(&tab_id) else {
            return div().into_any_element();
        };
        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        if let Some(err) = &run.error {
            let line = |text: String| div().mt_1().child(text);
            return v_flex()
                .id("run-error")
                .size_full()
                .overflow_y_scroll()
                .p_4()
                .text_sm()
                .child(
                    div()
                        .font_weight(ferrum_ui::gpui::FontWeight::MEDIUM)
                        .text_color(cx.theme().status().error)
                        .child(err.message.clone()),
                )
                .when_some(err.code.clone(), |this, code| {
                    this.child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(format!("SQLSTATE {code}")),
                    )
                })
                .when_some(err.detail.clone(), |this, d| {
                    this.child(line(format!("Detail: {d}")))
                })
                .when_some(err.hint.clone(), |this, h| {
                    this.child(line(format!("Hint: {h}")))
                })
                .when_some(err.suggestion(), |this, s| {
                    this.child(
                        div()
                            .mt_2()
                            .text_color(cx.theme().colors().text_muted)
                            .child(s),
                    )
                })
                .into_any_element();
        }
        match run.state {
            QueryState::Created | QueryState::Running if run.buffer.columns().is_empty() => {
                message("Running…".into())
            }
            QueryState::Cancelled if run.buffer.is_empty() => message("Query cancelled.".into()),
            QueryState::TimedOut if run.buffer.is_empty() => message("Query timed out.".into()),
            QueryState::Completed if run.buffer.columns().is_empty() => div()
                .p_4()
                .text_sm()
                .child(run.status_label())
                .into_any_element(),
            _ => self.render_grid(tab_id, cx),
        }
    }

    fn render_empty(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Label::new("Run a statement to see results here.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Button::new("synthetic", "Load 100k synthetic rows (grid benchmark)")
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text(
                        "Grid spike (ADR-004): renders 100,000 generated rows without a database",
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state
                            .runs
                            .insert(tab_id, synthetic::synthetic_run(100_000));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
