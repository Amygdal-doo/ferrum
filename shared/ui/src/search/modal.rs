//! The search modal: a query field, tabs (Search Everywhere), options (Find
//! in files) and one keyboard-navigable list of items and text hits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{
    Action, AnyElement, App, Context, Entity, EventEmitter, HighlightStyle, MouseButton, OwnedMenu,
    OwnedMenuItem, ScrollStrategy, SharedString, StyledText, Subscription, Task,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};

use super::{
    CONTEXT, FuzzyMatch, NextTab, PreviousTab, SearchCategory, SearchEvent, SearchItem, SearchMode,
    TextHit, TextQuery, TextSearcher, fuzzy_filter,
};
use crate::components::{
    ButtonCommon, Clickable, Color, Icon, IconButton, IconName, IconSize, Label, LabelCommon,
    LabelSize, StyledTypography, Toggleable, Tooltip, format_keystroke, h_flex, v_flex,
};
use crate::theme::ActiveTheme;
use crate::{TextInput, TextInputEvent, kbd};

/// Rows of one category in the All tab.
const PER_SECTION: usize = 6;
/// Rows of a single tab.
const MAX_ROWS: usize = 500;
/// Text hits asked for in the All tab / the Text tab and Find in files.
const HITS_IN_ALL: usize = 20;
const HITS_MAX: usize = 2000;
/// Typing pause before a text search starts.
const DEBOUNCE: Duration = Duration::from_millis(180);
const ROW_H: f32 = 30.;

#[derive(Clone)]
enum Row {
    Header(SharedString),
    Item(usize, FuzzyMatch),
    Hit(usize),
}

impl Row {
    fn selectable(&self) -> bool {
        !matches!(self, Row::Header(_))
    }
}

pub struct SearchModal {
    mode: SearchMode,
    items_label: SharedString,
    /// `None` is the All tab.
    tabs: Vec<Option<SearchCategory>>,
    tab: usize,
    input: Entity<TextInput>,
    mask: Entity<TextInput>,
    items: Arc<Vec<SearchItem>>,
    /// Menu bar actions listed under Actions (ids `MENU_PREFIX` + index).
    menu_actions: Vec<Box<dyn Action>>,
    searcher: Option<TextSearcher>,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    hits: Arc<Vec<TextHit>>,
    /// Query the hits are for.
    hits_for: Option<TextQuery>,
    text_status: SharedString,
    rows: Vec<Row>,
    selected: usize,
    scroll: UniformListScrollHandle,
    cancel: Arc<AtomicBool>,
    task: Option<Task<()>>,
    up: Option<Box<dyn Action>>,
    down: Option<Box<dyn Action>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SearchEvent> for SearchModal {}

impl SearchModal {
    /// `items_label` names [`SearchCategory::Items`] ("Files", "Requests").
    /// `searcher` enables the Text tab and Find in files.
    pub fn new(
        mode: SearchMode,
        items_label: impl Into<SharedString>,
        items: Vec<SearchItem>,
        searcher: Option<TextSearcher>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let items_label = items_label.into();
        let placeholder = match mode {
            SearchMode::Everywhere => "Search everywhere (Tab: next tab)".to_string(),
            SearchMode::GoTo => format!("Go to {}…", items_label.to_lowercase()),
            SearchMode::FindInFiles => "Find in files".to_string(),
        };
        let input = cx.new(|cx| {
            let input = TextInput::new(window, cx).placeholder(placeholder);
            if mode == SearchMode::FindInFiles {
                input.monospace()
            } else {
                input
            }
        });
        let mask = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Mask: *.rs, !*.test.ts")
                .monospace()
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &input,
                window,
                |this: &mut Self, _, event, window, cx| match event {
                    TextInputEvent::Changed => this.query_changed(cx),
                    TextInputEvent::Submit => this.confirm(window, cx),
                    TextInputEvent::Blurred => {}
                },
            ),
            cx.subscribe_in(
                &mask,
                window,
                |this: &mut Self, _, event, window, cx| match event {
                    TextInputEvent::Changed => this.query_changed(cx),
                    TextInputEvent::Submit => this.confirm(window, cx),
                    TextInputEvent::Blurred => {}
                },
            ),
        ];
        input.update(cx, |input, cx| input.focus(window, cx));
        let (mut items, mut menu_actions) = (items, Vec::new());
        if mode == SearchMode::Everywhere {
            items.extend(super::settings_items(cx));
            // Apps with a command list of their own (FerrumIDE) list it instead.
            if !items.iter().any(|i| i.category == SearchCategory::Actions) {
                let (actions, boxed) = menu_items(window, cx);
                items.extend(actions);
                menu_actions = boxed;
            }
        }
        let mut this = Self {
            mode,
            items_label,
            tabs: Vec::new(),
            tab: 0,
            input,
            mask,
            items: Arc::new(items),
            menu_actions,
            searcher,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            hits: Arc::default(),
            hits_for: None,
            text_status: SharedString::default(),
            rows: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            cancel: Arc::default(),
            task: None,
            up: cx.build_action("text_input::Up", None).ok(),
            down: cx.build_action("text_input::Down", None).ok(),
            _subscriptions: subscriptions,
        };
        this.rebuild_tabs();
        this.refilter(cx);
        this
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    /// Replaces the items (a background file walk finished), keeping the
    /// query and tab.
    pub fn set_items(&mut self, items: Vec<SearchItem>, cx: &mut Context<Self>) {
        let current = self.tabs.get(self.tab).copied().flatten();
        self.items = Arc::new(items);
        self.rebuild_tabs();
        if let Some(ix) = self.tabs.iter().position(|t| *t == current) {
            self.tab = ix;
        }
        self.refilter(cx);
    }

    /// Fills the query (the selection when the modal opens) and searches.
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_text(query, cx);
            input.select_range(0..query.len(), cx);
            input.focus(window, cx);
        });
        self.query_changed(cx);
    }

    /// Opens on the tab of `category` (Search Everywhere).
    pub fn select_tab(&mut self, category: SearchCategory, cx: &mut Context<Self>) {
        if let Some(ix) = self.tabs.iter().position(|t| *t == Some(category)) {
            self.tab = ix;
            self.query_changed(cx);
        }
    }

    fn rebuild_tabs(&mut self) {
        self.tabs = match self.mode {
            SearchMode::GoTo => vec![Some(SearchCategory::Items)],
            SearchMode::FindInFiles => vec![Some(SearchCategory::Text)],
            SearchMode::Everywhere => {
                let mut tabs = vec![None];
                for category in SearchCategory::ALL {
                    let present = if category == SearchCategory::Text {
                        self.searcher.is_some()
                    } else {
                        // Items always: it is the app's main tab even while loading.
                        category == SearchCategory::Items
                            || self.items.iter().any(|i| i.category == category)
                    };
                    if present {
                        tabs.push(Some(category));
                    }
                }
                tabs
            }
        };
        self.tab = self.tab.min(self.tabs.len() - 1);
    }

    fn tab_label(&self, tab: Option<SearchCategory>) -> SharedString {
        match tab {
            None => "All".into(),
            Some(SearchCategory::Items) => self.items_label.clone(),
            Some(c) => c.default_label().into(),
        }
    }

    fn query(&self, cx: &App) -> TextQuery {
        TextQuery {
            text: self.input.read(cx).text().to_string(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
            mask: if self.mode == SearchMode::FindInFiles {
                self.mask.read(cx).text().to_string()
            } else {
                String::new()
            },
        }
    }

    fn wants_text(&self) -> bool {
        self.searcher.is_some()
            && matches!(
                self.tabs.get(self.tab),
                Some(None) | Some(Some(SearchCategory::Text))
            )
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        self.refilter(cx);
        if self.wants_text() {
            self.search_text(cx);
        }
    }

    fn search_text(&mut self, cx: &mut Context<Self>) {
        let Some(searcher) = self.searcher.clone() else {
            return;
        };
        let query = self.query(cx);
        if self.hits_for.as_ref() == Some(&query) {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        // The All tab needs a couple of characters before text is worth it.
        let min = if self.mode == SearchMode::Everywhere {
            2
        } else {
            1
        };
        if query.text.chars().count() < min {
            self.hits = Arc::default();
            self.hits_for = None;
            self.text_status = SharedString::default();
            self.task = None;
            self.refilter(cx);
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        let limit = if self.tabs.get(self.tab) == Some(&None) {
            HITS_IN_ALL
        } else {
            HITS_MAX
        };
        self.text_status = "Searching…".into();
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let q = query.clone();
            let flag = cancel.clone();
            let result = cx
                .background_executor()
                .spawn(async move { searcher(&q, &flag, limit) })
                .await;
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            this.update(cx, |this, cx| {
                match result {
                    Ok(hits) => {
                        this.text_status = match hits.len() {
                            0 => "No matches".into(),
                            n if n >= limit => format!("{n}+ matches").into(),
                            1 => "1 match".into(),
                            n => format!("{n} matches").into(),
                        };
                        this.hits = Arc::new(hits);
                    }
                    Err(error) => {
                        this.hits = Arc::default();
                        this.text_status = format!("{error:#}").into();
                    }
                }
                this.hits_for = Some(query);
                this.refilter(cx);
            })
            .ok();
        }));
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        let tab = self.tabs.get(self.tab).copied().flatten();
        let all = self.tabs.get(self.tab) == Some(&None);
        let mut rows = Vec::new();
        let items = self.items.clone();
        let filter = |category: SearchCategory, limit: usize| -> Vec<Row> {
            let candidates: Vec<(usize, &SearchItem)> = items
                .iter()
                .enumerate()
                .filter(|(_, i)| i.category == category)
                .collect();
            fuzzy_filter(&query, &candidates, |(_, i)| i.title.as_ref(), limit)
                .into_iter()
                .map(|(pos, m)| Row::Item(candidates[pos].0, m))
                .collect()
        };
        if all {
            for category in SearchCategory::ALL {
                if category == SearchCategory::Text {
                    if !self.hits.is_empty() {
                        rows.push(Row::Header(self.tab_label(Some(category))));
                        rows.extend((0..self.hits.len().min(PER_SECTION)).map(Row::Hit));
                    }
                    continue;
                }
                // With no query the All tab lists the app's items first only.
                if query.is_empty() && category != SearchCategory::Items {
                    continue;
                }
                let found = filter(category, PER_SECTION);
                if !found.is_empty() {
                    rows.push(Row::Header(self.tab_label(Some(category))));
                    rows.extend(found);
                }
            }
        } else if tab == Some(SearchCategory::Text) {
            rows.extend((0..self.hits.len()).map(Row::Hit));
        } else if let Some(category) = tab {
            rows = filter(category, MAX_ROWS);
        }
        self.rows = rows;
        self.selected = self.rows.iter().position(Row::selectable).unwrap_or(0);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn switch_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            return;
        }
        let len = self.tabs.len() as isize;
        self.tab = (self.tab as isize + delta).rem_euclid(len) as usize;
        self.hits_for = None;
        self.query_changed(cx);
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !self.rows.iter().any(Row::selectable) {
            return;
        }
        let len = self.rows.len() as isize;
        let mut ix = self.selected as isize;
        loop {
            ix = (ix + delta).rem_euclid(len);
            if self.rows[ix as usize].selectable() {
                break;
            }
        }
        self.selected = ix as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.rows.get(self.selected) {
            Some(Row::Item(ix, _)) => {
                let Some(item) = self.items.get(*ix).cloned() else {
                    return;
                };
                // Menu actions and Settings pages are handled here, the same
                // in every app; the rest goes to the app.
                if let Some(action) = item
                    .id
                    .strip_prefix(MENU_PREFIX)
                    .and_then(|ix| ix.parse::<usize>().ok())
                    .and_then(|ix| self.menu_actions.get(ix))
                {
                    let action = action.boxed_clone();
                    cx.emit(SearchEvent::Dismissed);
                    window.dispatch_action(action, cx);
                } else if super::open_common(&item, cx) {
                    cx.emit(SearchEvent::Dismissed);
                } else {
                    cx.emit(SearchEvent::Open(item));
                }
            }
            Some(Row::Hit(ix)) => {
                if let Some(hit) = self.hits.get(*ix) {
                    cx.emit(SearchEvent::OpenHit(hit.clone()));
                }
            }
            // Enter in Find in files with nothing listed yet searches now.
            _ if self.mode == SearchMode::FindInFiles => {
                self.hits_for = None;
                self.search_text(cx);
            }
            _ => {}
        }
    }

    fn toggle(&mut self, which: u8, cx: &mut Context<Self>) {
        match which {
            0 => self.case_sensitive = !self.case_sensitive,
            1 => self.whole_word = !self.whole_word,
            _ => self.regex = !self.regex,
        }
        self.query_changed(cx);
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let accent = HighlightStyle {
            color: Some(colors.text_accent),
            font_weight: Some(gpui::FontWeight::BOLD),
            ..Default::default()
        };
        let selected = ix == self.selected;
        let row = match self.rows.get(ix) {
            Some(Row::Header(label)) => {
                return div()
                    .id(("search-row", ix))
                    .h(px(ROW_H))
                    .px_3()
                    .flex()
                    .items_end()
                    .pb_1()
                    .child(
                        Label::new(label.clone())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .into_any_element();
            }
            Some(Row::Item(item_ix, matched)) => {
                let item = &self.items[*item_ix];
                let title: &str = item.title.as_ref();
                let highlights: Vec<_> = title
                    .char_indices()
                    .enumerate()
                    .filter(|(char_ix, _)| matched.indices.contains(char_ix))
                    .map(|(_, (byte, c))| (byte..byte + c.len_utf8(), accent))
                    .collect();
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_2()
                            .when_some(item.icon, |this, icon| {
                                this.child(
                                    Icon::new(icon).size(IconSize::Small).color(Color::Muted),
                                )
                            })
                            .child(div().min_w_0().overflow_hidden().text_ellipsis().child(
                                StyledText::new(item.title.clone()).with_highlights(highlights),
                            )),
                    )
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_2()
                            .when(!item.detail.is_empty(), |this| {
                                this.child(
                                    Label::new(item.detail.clone())
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                            })
                            .when_some(item.keys.clone(), |this, keys| this.child(kbd(keys, cx))),
                    )
            }
            Some(Row::Hit(hit_ix)) => {
                let hit = &self.hits[*hit_ix];
                let range = hit.range.clone();
                let valid = range.end <= hit.preview.len()
                    && hit.preview.is_char_boundary(range.start)
                    && hit.preview.is_char_boundary(range.end);
                let highlights = if valid {
                    vec![(range, accent)]
                } else {
                    Vec::new()
                };
                h_flex()
                    .gap_3()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_buffer(cx)
                            .child(
                                StyledText::new(hit.preview.clone()).with_highlights(highlights),
                            ),
                    )
                    .child(
                        Label::new(format!("{}:{}", hit.source_label, hit.line + 1))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
            }
            None => return div().into_any_element(),
        };
        row.id(("search-row", ix))
            .h(px(ROW_H))
            .w_full()
            .px_3()
            .rounded_sm()
            .when(selected, |this| this.bg(colors.element_selected))
            .hover(|this| this.bg(colors.element_hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected = ix;
                this.confirm(window, cx);
            }))
            .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .px_2()
            .gap_1()
            .children(self.tabs.iter().enumerate().map(|(ix, tab)| {
                let active = ix == self.tab;
                div()
                    .id(("search-tab", ix))
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(active, |this| this.bg(colors.element_selected))
                    .hover(|this| this.bg(colors.element_hover))
                    .child(
                        Label::new(self.tab_label(*tab))
                            .size(LabelSize::Small)
                            .color(if active { Color::Default } else { Color::Muted }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = ix;
                        this.hits_for = None;
                        this.query_changed(cx);
                    }))
            }))
            .child(div().flex_1())
            .child(
                Label::new("Tab to switch")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }

    fn render_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggle = |id: &'static str, icon: IconName, on: bool, tip: &'static str, which: u8| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .toggle_state(on)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle(which, cx)))
        };
        h_flex()
            .px_2()
            .gap_1()
            .child(toggle(
                "search-case",
                IconName::CaseSensitive,
                self.case_sensitive,
                "Match case",
                0,
            ))
            .child(toggle(
                "search-word",
                IconName::WholeWord,
                self.whole_word,
                "Whole word",
                1,
            ))
            .child(toggle(
                "search-regex",
                IconName::Regex,
                self.regex,
                "Regular expression",
                2,
            ))
            .when(self.mode == SearchMode::FindInFiles, |this| {
                this.child(div().w(px(220.)).child(self.mask.clone()))
            })
            .child(div().flex_1())
            .child(
                Label::new(self.text_status.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }
}

impl Render for SearchModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = self.rows.len();
        let text_tab = self.tabs.get(self.tab) == Some(&Some(SearchCategory::Text));
        let hint: Option<SharedString> = if count > 0 {
            None
        } else if self.mode == SearchMode::FindInFiles || text_tab {
            Some(if self.input.read(cx).is_empty() {
                "Type to search text".into()
            } else if self.text_status.is_empty() {
                "No matches".into()
            } else {
                self.text_status.clone()
            })
        } else {
            Some("No matches".into())
        };
        let width = if self.mode == SearchMode::FindInFiles {
            820.
        } else {
            680.
        };
        let mut root = v_flex()
            .key_context(CONTEXT)
            .w(px(width))
            .max_h(px(560.))
            .p_1()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .shadow_lg()
            .on_action(
                cx.listener(|_, _: &crate::menu::Cancel, _, cx| cx.emit(SearchEvent::Dismissed)),
            )
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.switch_tab(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| this.switch_tab(-1, cx)))
            .child(self.input.clone());
        if let Some(up) = &self.up {
            root = root.on_boxed_action(
                up.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(-1, cx)),
            );
        }
        if let Some(down) = &self.down {
            root = root.on_boxed_action(
                down.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(1, cx)),
            );
        }
        root.when(self.mode == SearchMode::Everywhere, |this| {
            this.child(self.render_tabs(cx))
        })
        .when(self.mode == SearchMode::FindInFiles || text_tab, |this| {
            this.child(self.render_options(cx))
        })
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .px_3()
                    .py_2()
                    .child(Label::new(hint).size(LabelSize::Small).color(Color::Muted)),
            )
        })
        .when(count > 0, |this| {
            this.child(
                uniform_list(
                    "search-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|ix| this.render_row(ix, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .h(px((count.min(14) as f32) * ROW_H)),
            )
        })
    }
}

const MENU_PREFIX: &str = "menu-action:";

/// The menu bar's commands (where the platform reports them: macOS) as
/// Actions, "Save" with "File" as detail and its shortcut.
fn menu_items(window: &Window, cx: &App) -> (Vec<SearchItem>, Vec<Box<dyn Action>>) {
    fn walk(
        menu: &OwnedMenu,
        path: &str,
        window: &Window,
        items: &mut Vec<SearchItem>,
        actions: &mut Vec<Box<dyn Action>>,
    ) {
        for entry in &menu.items {
            match entry {
                OwnedMenuItem::Submenu(sub) => walk(
                    sub,
                    &format!("{path} › {}", sub.name),
                    window,
                    items,
                    actions,
                ),
                OwnedMenuItem::Action {
                    name,
                    action,
                    disabled: false,
                    ..
                } => {
                    let mut item = SearchItem::new(
                        SearchCategory::Actions,
                        name.trim_end_matches('…').to_string(),
                        format!("{MENU_PREFIX}{}", actions.len()),
                    )
                    .detail(path.to_string());
                    if let Some(binding) =
                        window.highest_precedence_binding_for_action(action.as_ref())
                    {
                        let keys: Vec<String> = binding
                            .keystrokes()
                            .iter()
                            .map(|k| format_keystroke(k.inner()))
                            .collect();
                        item = item.keys(keys.join(" "));
                    }
                    items.push(item);
                    actions.push(action.boxed_clone());
                }
                _ => {}
            }
        }
    }
    let (mut items, mut actions) = (Vec::new(), Vec::new());
    for menu in cx.get_menus().unwrap_or_default() {
        walk(&menu, &menu.name, window, &mut items, &mut actions);
    }
    (items, actions)
}

/// The modal over a light backdrop, near the top of the window; a click on
/// the backdrop dismisses it. Render it as the root view's last child (the
/// root must be `relative`).
pub fn search_overlay(modal: &Entity<SearchModal>) -> impl IntoElement {
    let dismiss = modal.downgrade();
    div()
        .id("ferrum-search-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .pt(px(72.))
        .bg(gpui::hsla(0., 0., 0., 0.18))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            dismiss
                .update(cx, |_, cx| cx.emit(SearchEvent::Dismissed))
                .ok();
        })
        .child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(modal.clone()),
        )
}
