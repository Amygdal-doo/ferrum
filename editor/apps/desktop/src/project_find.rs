//! Find in files: a bottom panel with a query, options and a results list
//! grouped by file. The search runs on a background thread and can be
//! cancelled by starting another.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ferrum_ui::prelude::*;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumeditor_core::find::SearchQuery;
use ferrumeditor_core::project_search::{ProjectSearchResult, SearchLimits, search_project};
use gpui::{HighlightStyle, StyledText};
use ui::{IconButton, IconName, IconSize};

pub enum ProjectFindEvent {
    /// Open `path` at 0-based line / byte column.
    Open {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    Close,
}

/// One row of the flattened results list.
#[derive(Clone)]
enum Row {
    File {
        path: PathBuf,
        count: usize,
    },
    Match {
        path: PathBuf,
        line: usize,
        column: usize,
        text: SharedString,
        range: std::ops::Range<usize>,
    },
}

pub struct ProjectFind {
    pub input: Entity<TextInput>,
    root: Option<PathBuf>,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    rows: Arc<Vec<Row>>,
    status: SharedString,
    cancel: Arc<AtomicBool>,
    task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProjectFindEvent> for ProjectFind {}

impl ProjectFind {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Find in files (Enter to search)")
                .monospace()
        });
        let subscription = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if *event == TextInputEvent::Submit {
                this.search(cx);
            }
        });
        Self {
            input,
            root: None,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            rows: Arc::default(),
            status: "Type a query and press Enter".into(),
            cancel: Arc::default(),
            task: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn set_root(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.root = root;
        cx.notify();
    }

    pub fn focus(&mut self, prefill: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            if let Some(text) = prefill.filter(|t| !t.contains('\n') && !t.is_empty()) {
                input.set_text(text, cx);
            }
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            self.status = "Open a folder to search in files".into();
            cx.notify();
            return;
        };
        let query = SearchQuery {
            pattern: self.input.read(cx).text().to_string(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
        };
        self.cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        self.status = "Searching…".into();
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(
                    async move { search_project(&root, &query, &SearchLimits::default(), &cancel) },
                )
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(result) if !result.cancelled => this.show(result),
                    Ok(_) => {}
                    Err(error) => {
                        this.rows = Arc::default();
                        this.status = error.to_string().into();
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn show(&mut self, result: ProjectSearchResult) {
        let mut rows = Vec::new();
        for file in &result.files {
            rows.push(Row::File {
                path: file.path.clone(),
                count: file.matches.len(),
            });
            for m in &file.matches {
                rows.push(Row::Match {
                    path: file.path.clone(),
                    line: m.line,
                    column: m.byte_range.start + m.line_offset,
                    text: m.line_text.clone().into(),
                    range: m.byte_range.clone(),
                });
            }
        }
        self.rows = Arc::new(rows);
        self.status = format!(
            "{} result{} in {} file{}{}",
            result.total_matches,
            if result.total_matches == 1 { "" } else { "s" },
            result.files.len(),
            if result.files.len() == 1 { "" } else { "s" },
            if result.truncated {
                " (limit reached)"
            } else {
                ""
            }
        )
        .into();
    }

    fn toggle(&mut self, which: u8, cx: &mut Context<Self>) {
        match which {
            0 => self.case_sensitive = !self.case_sensitive,
            1 => self.whole_word = !self.whole_word,
            _ => self.regex = !self.regex,
        }
        cx.notify();
    }

    fn relative(&self, path: &Path) -> String {
        self.root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        match self.rows.get(ix).cloned() {
            Some(Row::File { path, count }) => h_flex()
                .id(("pf-file", ix))
                .px_2()
                .gap_2()
                .when_some(
                    crate::file_icons::icon_element(
                        &path,
                        ferrumeditor_core::icons::EntryKind::File,
                        cx,
                    ),
                    |this, icon| this.child(icon),
                )
                .child(Label::new(self.relative(&path)).size(LabelSize::Small))
                .child(
                    Label::new(count.to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element(),
            Some(Row::Match {
                path,
                line,
                column,
                text,
                range,
            }) => {
                let trimmed = text.trim_start();
                let cut = text.len() - trimmed.len();
                let shown: SharedString = trimmed.to_string().into();
                let hl =
                    range.start.saturating_sub(cut)..range.end.saturating_sub(cut).min(shown.len());
                let style = HighlightStyle {
                    background_color: Some(colors.search_match_background),
                    ..Default::default()
                };
                let highlights = if shown.is_char_boundary(hl.start)
                    && shown.is_char_boundary(hl.end)
                    && hl.start < hl.end
                {
                    vec![(hl, style)]
                } else {
                    Vec::new()
                };
                h_flex()
                    .id(("pf-match", ix))
                    .pl_6()
                    .pr_2()
                    .gap_2()
                    .rounded_sm()
                    .hover(|this| this.bg(colors.element_hover))
                    .child(
                        div().w(px(44.)).flex_none().child(
                            Label::new(format!("{}", line + 1))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_buffer(cx)
                            .text_ui_sm(cx)
                            .child(StyledText::new(shown).with_highlights(highlights)),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ProjectFindEvent::Open {
                            path: path.clone(),
                            line,
                            column,
                        });
                    }))
                    .into_any_element()
            }
            None => div().into_any_element(),
        }
    }
}

impl Render for ProjectFind {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = self.rows.len();
        let toggle = |id: &'static str,
                      icon: IconName,
                      on: bool,
                      tip: &'static str,
                      which: u8,
                      cx: &mut Context<Self>| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .toggle_state(on)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle(which, cx)))
        };
        v_flex()
            .key_context("FerrumProjectFind")
            .size_full()
            .bg(colors.panel_background)
            .on_action(cx.listener(|_, _: &menu::Cancel, _, cx| cx.emit(ProjectFindEvent::Close)))
            .child(
                h_flex()
                    .p_1p5()
                    .gap_1()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().flex_1().min_w_0().child(self.input.clone()))
                    .child(toggle(
                        "pf-case",
                        IconName::CaseSensitive,
                        self.case_sensitive,
                        "Match Case",
                        0,
                        cx,
                    ))
                    .child(toggle(
                        "pf-word",
                        IconName::WholeWord,
                        self.whole_word,
                        "Whole Word",
                        1,
                        cx,
                    ))
                    .child(toggle(
                        "pf-regex",
                        IconName::Regex,
                        self.regex,
                        "Regular Expression",
                        2,
                        cx,
                    ))
                    .child(
                        IconButton::new("pf-search", IconName::MagnifyingGlass)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Search"))
                            .on_click(cx.listener(|this, _, _, cx| this.search(cx))),
                    )
                    .child(
                        div().px_2().child(
                            Label::new(self.status.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(
                        IconButton::new("pf-close", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ProjectFindEvent::Close))),
                    ),
            )
            .child(
                div().flex_1().min_h_0().py_1().child(
                    uniform_list(
                        "project-find-results",
                        count,
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            range.map(|ix| this.render_row(ix, cx)).collect()
                        }),
                    )
                    .size_full(),
                ),
            )
    }
}

#[cfg(test)]
impl ProjectFind {
    pub fn result_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn status(&self) -> SharedString {
        self.status.clone()
    }

    /// Emits an open event for the first match row.
    pub fn open_first_match(&mut self, cx: &mut Context<Self>) {
        if let Some(Row::Match {
            path, line, column, ..
        }) = self
            .rows
            .iter()
            .find(|r| matches!(r, Row::Match { .. }))
            .cloned()
        {
            cx.emit(ProjectFindEvent::Open { path, line, column });
        }
    }
}
