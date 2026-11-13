//! The Debug tab of the right dock: the toolbar (continue F5, step over
//! F10, into F11, out ⇧F11, restart, stop ⇧F5), threads and the call stack
//! (click a frame), variables (expand lazily), watch expressions, the
//! breakpoints list and the debug console with an evaluate input.

use std::path::PathBuf;

use ferrum_ui::components::{Button, ButtonStyle, IconButton, IconName, IconSize};
use ferrum_ui::prelude::*;
use ferrum_ui::{TextInput, TextInputEvent};
use gpui::{FontWeight, ScrollHandle};

use super::Debugger;
use crate::actions::run;
use crate::settings::Settings;
use crate::theme_apply::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugPanelEvent {
    /// Open a file at a 1-based line.
    Open { path: PathBuf, line: usize },
}

pub struct DebugPanel {
    debugger: Entity<Debugger>,
    repl: Entity<TextInput>,
    watch: Entity<TextInput>,
    scroll: ScrollHandle,
    console_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DebugPanelEvent> for DebugPanel {}

impl DebugPanel {
    pub fn new(debugger: Entity<Debugger>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repl = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Evaluate in the paused frame…")
                .monospace()
        });
        let watch = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Add a watch expression…")
                .monospace()
        });
        let subscriptions = vec![
            cx.observe(&debugger, |this, _, cx| {
                this.console_scroll.scroll_to_bottom();
                cx.notify();
            }),
            cx.subscribe_in(&repl, window, |this, input, event, _, cx| {
                if *event == TextInputEvent::Submit {
                    let text = input.read(cx).text().to_string();
                    input.update(cx, |i, cx| i.set_text("", cx));
                    this.debugger.update(cx, |d, cx| d.evaluate(text, cx));
                }
            }),
            cx.subscribe_in(&watch, window, |this, input, event, _, cx| {
                if *event == TextInputEvent::Submit {
                    let text = input.read(cx).text().to_string();
                    input.update(cx, |i, cx| i.set_text("", cx));
                    this.debugger.update(cx, |d, cx| d.add_watch(text, cx));
                }
            }),
        ];
        Self {
            debugger,
            repl,
            watch,
            scroll: ScrollHandle::new(),
            console_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    pub fn watch_input(&self) -> Entity<TextInput> {
        self.watch.clone()
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let debugger = self.debugger.read(cx);
        let running = debugger.session.is_some();
        let stopped = debugger.is_stopped();
        let label = debugger
            .status_label()
            .unwrap_or_else(|| "Not debugging".to_string());
        let button = |id: &'static str,
                      icon: IconName,
                      tip: &'static str,
                      command: &'static str,
                      enabled: bool| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .disabled(!enabled)
                .tooltip(Tooltip::text(tip))
                .on_click(move |_, window, cx| window.dispatch_action(Box::new(run(command)), cx))
        };
        v_flex()
            .flex_none()
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .h(px(30.))
                    .px_1()
                    .gap_0p5()
                    .child(if running && !stopped {
                        button(
                            "debug-pause",
                            IconName::DebugPause,
                            "Pause (F6)",
                            "debug:pause",
                            true,
                        )
                    } else {
                        button(
                            "debug-continue",
                            IconName::DebugContinue,
                            "Start / Continue (F5)",
                            "debug:continue",
                            true,
                        )
                    })
                    .child(button(
                        "debug-step-over",
                        IconName::DebugStepOver,
                        "Step Over (F10)",
                        "debug:step-over",
                        stopped,
                    ))
                    .child(button(
                        "debug-step-into",
                        IconName::DebugStepInto,
                        "Step Into (F11)",
                        "debug:step-into",
                        stopped,
                    ))
                    .child(button(
                        "debug-step-out",
                        IconName::DebugStepOut,
                        "Step Out (⇧F11)",
                        "debug:step-out",
                        stopped,
                    ))
                    .child(button(
                        "debug-restart",
                        IconName::RotateCw,
                        "Restart (⇧⌘F5)",
                        "debug:restart",
                        running,
                    ))
                    .child(button(
                        "debug-stop",
                        IconName::Stop,
                        "Stop (⇧F5)",
                        "debug:stop",
                        running,
                    )),
            )
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
    }

    fn section(title: &str) -> impl IntoElement {
        div()
            .px_2()
            .pt_2()
            .pb_0p5()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.to_uppercase())
    }

    fn stack(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors().clone();
        let debugger = self.debugger.read(cx);
        let Some(session) = &debugger.session else {
            return vec![];
        };
        let state = &session.state;
        let mut rows: Vec<AnyElement> = Vec::new();
        if state.threads.len() > 1 {
            for thread in &state.threads {
                let current = state.thread_id == Some(thread.id);
                rows.push(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(if current {
                            colors.text
                        } else {
                            colors.text_muted
                        })
                        .child(format!(
                            "{} {}",
                            if current { "▸" } else { " " },
                            thread.name
                        ))
                        .into_any_element(),
                );
            }
        }
        for (i, frame) in state.frames.iter().enumerate() {
            let selected = i == state.selected_frame;
            let place = frame.path.as_ref().map_or_else(String::new, |p| {
                format!(
                    "{}:{}",
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    frame.line
                )
            });
            rows.push(
                h_flex()
                    .id(("debug-frame", i))
                    .px_2()
                    .gap_2()
                    .cursor_pointer()
                    .text_sm()
                    .when(selected, |d| d.bg(colors.element_selected))
                    .hover(|d| d.bg(colors.element_hover))
                    .text_color(if frame.subtle {
                        colors.text_muted
                    } else {
                        colors.text
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.debugger.update(cx, |d, cx| d.select_frame(i, cx))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(frame.name.clone()),
                    )
                    .child(div().text_xs().text_color(colors.text_muted).child(place))
                    .into_any_element(),
            );
        }
        rows
    }

    fn variables(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut flat = Vec::new();
        {
            let debugger = self.debugger.read(cx);
            if let Some(session) = &debugger.session {
                for scope in &session.state.scopes {
                    let reference = scope.variables_reference;
                    flat.push((
                        0,
                        scope.name.clone(),
                        None,
                        reference,
                        debugger.expanded.contains(&reference),
                    ));
                    Self::children(debugger, reference, 1, &mut flat);
                }
            }
        }
        flat.into_iter()
            .map(|(depth, name, value, reference, open)| {
                self.tree_row(depth, &name, value.as_deref(), reference, open, cx)
            })
            .collect()
    }

    /// (depth, name, value, reference, expanded) rows under `reference`.
    #[allow(clippy::type_complexity)]
    fn children(
        debugger: &Debugger,
        reference: i64,
        depth: usize,
        rows: &mut Vec<(usize, String, Option<String>, i64, bool)>,
    ) {
        if depth > 12 || !debugger.expanded.contains(&reference) {
            return;
        }
        let Some(vars) = debugger
            .session
            .as_ref()
            .and_then(|s| s.state.variables.get(&reference))
        else {
            return;
        };
        for var in vars {
            let open = debugger.expanded.contains(&var.variables_reference);
            rows.push((
                depth,
                var.name.clone(),
                Some(var.value.clone()),
                var.variables_reference,
                open,
            ));
            if var.variables_reference > 0 {
                Self::children(debugger, var.variables_reference, depth + 1, rows);
            }
        }
    }

    fn tree_row(
        &self,
        depth: usize,
        name: &str,
        value: Option<&str>,
        reference: i64,
        open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let theme = cx.global::<Settings>().theme.clone();
        let arrow = match (reference > 0, open) {
            (false, _) => " ",
            (true, true) => "▾",
            (true, false) => "▸",
        };
        h_flex()
            .id(SharedString::from(format!(
                "var-{depth}-{name}-{reference}"
            )))
            .pl(px(8. + depth as f32 * 12.))
            .pr_2()
            .gap_1()
            .text_sm()
            .when(reference > 0, |d| {
                d.cursor_pointer()
                    .hover(|d| d.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.debugger
                            .update(cx, |d, cx| d.toggle_variable(reference, cx))
                    }))
            })
            .child(div().w(px(10.)).text_color(colors.text_muted).child(arrow))
            .child(
                div()
                    .text_color(ui(&theme, "accent"))
                    .child(name.to_string()),
            )
            .when_some(value, |d, value| {
                d.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(format!("= {value}")),
                )
            })
            .into_any_element()
    }

    fn watches(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors().clone();
        let watches = self.debugger.read(cx).watches.clone();
        let mut rows: Vec<AnyElement> = watches
            .into_iter()
            .enumerate()
            .map(|(i, watch)| {
                let value = match &watch.value {
                    Some(Ok(value)) => value.clone(),
                    Some(Err(error)) => format!("⚠ {error}"),
                    None => "—".into(),
                };
                h_flex()
                    .px_2()
                    .gap_1()
                    .text_sm()
                    .child(div().child(watch.expression.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(colors.text_muted)
                            .child(format!("= {value}")),
                    )
                    .child(
                        IconButton::new(("watch-remove", i), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.debugger.update(cx, |d, cx| d.remove_watch(i, cx))
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        rows.push(
            div()
                .px_2()
                .py_0p5()
                .child(self.watch.clone())
                .into_any_element(),
        );
        rows
    }

    fn breakpoints(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors().clone();
        let theme = cx.global::<Settings>().theme.clone();
        let debugger = self.debugger.read(cx);
        let mut list: Vec<(PathBuf, usize, Option<bool>)> = Vec::new();
        for (path, lines) in debugger.breakpoints.files() {
            let (marks, _) = debugger.marks_for(path);
            for (line, (_, verified)) in lines.iter().zip(marks) {
                list.push((path.clone(), *line, verified));
            }
        }
        if list.is_empty() {
            return vec![
                div()
                    .px_2()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child("Click the gutter or press F9 to add one.")
                    .into_any_element(),
            ];
        }
        list.into_iter()
            .enumerate()
            .map(|(i, (path, line, verified))| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let color = ui(&theme, "error");
                let (open_path, remove_path) = (path.clone(), path.clone());
                h_flex()
                    .id(("debug-breakpoint", i))
                    .px_2()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .hover(|d| d.bg(colors.element_hover))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(DebugPanelEvent::Open {
                            path: open_path.clone(),
                            line,
                        })
                    }))
                    .child(
                        div()
                            .text_color(if verified == Some(false) {
                                color.opacity(0.45)
                            } else {
                                color
                            })
                            .child("●"),
                    )
                    .child(div().flex_1().child(format!("{name}:{line}")))
                    .child(
                        IconButton::new(("breakpoint-remove", i), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let path = remove_path.clone();
                                this.debugger
                                    .update(cx, |d, cx| d.toggle_breakpoint(&path, line, cx))
                            })),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn console(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let theme = cx.global::<Settings>().theme.clone();
        let font = SharedString::from(cx.global::<Settings>().config.font_family.clone());
        let lines: Vec<AnyElement> = self
            .debugger
            .read(cx)
            .console
            .iter()
            .map(|line| {
                let color = match line.category.as_str() {
                    "stderr" => ui(&theme, "error"),
                    "important" => ui(&theme, "warning"),
                    "repl" => ui(&theme, "text_muted"),
                    "result" => ui(&theme, "accent"),
                    _ => ui(&theme, "text"),
                };
                div()
                    .px_2()
                    .text_color(color)
                    .child(line.text.trim_end_matches('\n').to_string())
                    .into_any_element()
            })
            .collect();
        v_flex()
            .flex_none()
            .h(px(200.))
            .border_t_1()
            .border_color(colors.border)
            .child(Self::section("Debug Console"))
            .child(
                v_flex()
                    .id("debug-console")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.console_scroll)
                    .font_family(font)
                    .text_xs()
                    .children(lines),
            )
            .child(div().p_1().child(self.repl.clone()))
    }
}

impl Render for DebugPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let has_session = self.debugger.read(cx).session.is_some();
        let stack = self.stack(cx);
        let variables = self.variables(cx);
        let watches = self.watches(cx);
        let breakpoints = self.breakpoints(cx);
        v_flex()
            .size_full()
            .bg(colors.panel_background)
            .child(self.toolbar(cx))
            .child(
                v_flex()
                    .id("debug-sections")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .when(!has_session, |d| {
                        d.child(
                            v_flex()
                                .p_2()
                                .gap_1()
                                .text_sm()
                                .text_color(colors.text_muted)
                                .child("F5 starts debugging with .vscode/launch.json or a default for the project (Rust, Go, JS/TS).")
                                .child(
                                    Button::new("debug-start", "Start Debugging…")
                                        .style(ButtonStyle::Filled)
                                        .on_click(|_, window, cx| {
                                            window.dispatch_action(Box::new(run("debug:start")), cx)
                                        }),
                                ),
                        )
                    })
                    .when(has_session, |d| {
                        d.child(Self::section("Call Stack"))
                            .children(stack)
                            .child(Self::section("Variables"))
                            .children(variables)
                    })
                    .child(Self::section("Watch"))
                    .children(watches)
                    .child(Self::section("Breakpoints"))
                    .children(breakpoints),
            )
            .child(self.console(cx))
    }
}
