//! SQL dump tab: pick tables, schema and/or data, then Export…; the CSV
//! import tab lives in `import.rs`.

mod import;

use crate::app::FerrumData;
use crate::state::{SnapshotLoad, TaskStatus};
use ferrum_ui::prelude::*;
use ferrumdata_domain::{EditorTabId, TableKind};

pub(crate) fn toggle(on: bool) -> ToggleState {
    if on {
        ToggleState::Selected
    } else {
        ToggleState::Unselected
    }
}

/// One line for a tool's progress or outcome.
pub(crate) fn status_line(status: &TaskStatus, running: &str, cx: &App) -> Option<AnyElement> {
    let (text, color) = match status {
        TaskStatus::Idle => return None,
        TaskStatus::Running => (running.to_owned(), cx.theme().colors().text_muted),
        TaskStatus::Done(text) => (text.clone(), cx.theme().status().success),
        TaskStatus::Failed(text) => (text.clone(), cx.theme().status().error),
    };
    Some(
        div()
            .text_sm()
            .text_color(color)
            .child(text)
            .into_any_element(),
    )
}

impl FerrumData {
    pub fn render_dump_view(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(view) = self.state.dumps.get(&tab_id).cloned() else {
            return div().into_any_element();
        };
        let profile_id = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id);
        let snapshot = match &view.load {
            SnapshotLoad::Loading => {
                return div()
                    .p_4()
                    .child(Label::new("Reading the schema…").color(Color::Muted))
                    .into_any_element();
            }
            SnapshotLoad::Failed(e) => {
                return div()
                    .p_4()
                    .child(Label::new(e.clone()).color(Color::Error))
                    .into_any_element();
            }
            SnapshotLoad::Loaded(snapshot) => snapshot.clone(),
        };
        let tables: Vec<_> = snapshot
            .tables
            .iter()
            .filter(|t| t.info.kind != TableKind::View)
            .map(|t| t.info.table.clone())
            .collect();
        let all = tables.len();
        let chosen = view.selected.len();
        let running = view.status == TaskStatus::Running;
        let rows = tables.into_iter().enumerate().map(|(i, table)| {
            let label = match &table.schema {
                Some(s)
                    if snapshot
                        .tables
                        .iter()
                        .any(|t| t.info.table.schema.as_deref() != Some(s)) =>
                {
                    format!("{s}.{}", table.name)
                }
                _ => table.name.clone(),
            };
            let checked = view.selected.contains(&table);
            ferrum_ui::components::Checkbox::new(
                SharedString::from(format!("dump-{i}")),
                toggle(checked),
            )
            .label(label)
            .label_size(LabelSize::Small)
            .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                    if state.selected() {
                        view.selected.insert(table.clone());
                    } else {
                        view.selected.remove(&table);
                    }
                    cx.notify();
                }
            }))
        });

        v_flex()
            .flex_1()
            .min_h_0()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(Label::new(format!("{chosen} of {all} tables")).size(LabelSize::Small))
                    .child(
                        Button::new("dump-all", "All")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(view) = this.state.dumps.get_mut(&tab_id)
                                    && let SnapshotLoad::Loaded(s) = &view.load
                                {
                                    view.selected = s
                                        .tables
                                        .iter()
                                        .filter(|t| t.info.kind != TableKind::View)
                                        .map(|t| t.info.table.clone())
                                        .collect();
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("dump-none", "None")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                                    view.selected.clear();
                                    cx.notify();
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        ferrum_ui::components::Checkbox::new("dump-schema", toggle(view.schema))
                            .label("Schema")
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
                                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                                    view.schema = state.selected();
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        ferrum_ui::components::Checkbox::new("dump-data", toggle(view.data))
                            .label("Data")
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
                                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                                    view.data = state.selected();
                                    cx.notify();
                                }
                            })),
                    )
                    .when_some(profile_id, |this, profile_id| {
                        this.child(
                            Button::new("dump-reload", "Reload")
                                .style(ButtonStyle::Outlined)
                                .disabled(running)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.load_dump_tables(tab_id, profile_id, cx);
                                })),
                        )
                    })
                    .child(
                        Button::new("dump-run", "Export…")
                            .style(ButtonStyle::Filled)
                            .disabled(running || chosen == 0 || !(view.schema || view.data))
                            .on_click(cx.listener(move |this, _, _, cx| this.run_dump(tab_id, cx))),
                    ),
            )
            .children(status_line(&view.status, "Writing the dump…", cx))
            .child(
                div()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(
                        "Tables in dependency order, indexes, then INSERTs (100 rows each); foreign keys are \
                         added after the data. Views, triggers and CHECK constraints are not included.",
                    ),
            )
            .child(
                v_flex()
                    .id("dump-tables")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .children(rows),
            )
            .into_any_element()
    }
}
