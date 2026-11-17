//! The find bar (⌘F) over the focused pane: query, match case, regex, the
//! match count, older / newer (↩ / ⇧↩, ⌘G / ⌘⇧G) and close (Esc). The
//! server searches the pane's screen and scrollback (`Request::Find`); the
//! matches are highlighted by `paint` and the view scrolls to the current
//! one. While output arrives the pane is searched again, at most every
//! [`RESEARCH`].

use super::TermView;
use crate::app::UiEvent;
use crate::find::{Search, Step};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, IconButton, IconName, IconSize, Label, LabelCommon, LabelSize,
    Toggleable, Tooltip, h_flex,
};
use ferrum_ui::gpui::{
    self, AppContext as _, Context, Entity, Focusable as _, IntoElement, Subscription, Task,
    Window, div, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumterm_protocol::{BlockId, Request, Response};
use std::time::{Duration, Instant};

/// Least time between searches while output arrives.
const RESEARCH: Duration = Duration::from_millis(300);
/// The first server revision that searches.
const FIND_REVISION: u32 = 3;
const BAR_WIDTH: f32 = 380.;

pub(crate) struct FindBar {
    pub(crate) input: Entity<TextInput>,
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
    pub(crate) search: Search,
    /// An invalid regex, or why this server can't search.
    pub(crate) error: Option<String>,
    /// A search is on its way; `again` runs another after it.
    pending: bool,
    again: bool,
    /// Scroll to the current match when the results arrive (the query changed).
    jump: bool,
    /// Numbers searches, so a late answer to an older one is dropped.
    generation: u64,
    last: Option<Instant>,
    _refresh: Option<Task<()>>,
    _events: Subscription,
}

impl FindBar {
    /// Whether its field has keyboard focus.
    pub(crate) fn focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.input.read(cx).focus_handle(cx).is_focused(window)
    }
}

impl TermView {
    /// ⌘F: opens the bar on the focused pane, or selects its query.
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bar) = &self.find {
            let input = bar.input.clone();
            input.update(cx, |input, cx| {
                let len = input.text().len();
                input.select_range(0..len, cx);
                input.focus(window, cx);
            });
            return;
        }
        let input = cx.new(|cx| TextInput::new(window, cx).placeholder("Find").monospace());
        let events = cx.subscribe(&input, |view, _, event, cx| match event {
            TextInputEvent::Changed => view.search_now(true, cx),
            TextInputEvent::Submit => view.find_step(Step::Older, cx),
            TextInputEvent::Blurred => {}
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.find = Some(FindBar {
            input,
            case_sensitive: false,
            regex: false,
            search: Search::default(),
            error: None,
            pending: false,
            again: false,
            jump: false,
            generation: 0,
            last: None,
            _refresh: None,
            _events: events,
        });
        cx.notify();
    }

    /// Esc: closes the bar and gives the terminal its focus back.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// The query, when there is one to search for.
    fn find_query(&self, cx: &gpui::App) -> Option<String> {
        let bar = self.find.as_ref()?;
        let query = bar.input.read(cx).text().to_owned();
        (!query.is_empty()).then_some(query)
    }

    /// Searches the focused pane (`jump`: then shows the current match).
    pub(crate) fn search_now(&mut self, jump: bool, cx: &mut Context<Self>) {
        let query = self.find_query(cx);
        let block = self.app.focused();
        let client = self.app.client.clone();
        let Some(bar) = self.find.as_mut() else {
            return;
        };
        bar.jump |= jump;
        bar._refresh = None;
        let (Some(query), Some(block), Some(client)) = (query, block, client) else {
            bar.search = Search::default();
            bar.error = None;
            cx.notify();
            return;
        };
        if client.server_revision() < FIND_REVISION {
            bar.error = Some("Restart the FerrumTerm server to search".into());
            cx.notify();
            return;
        }
        if bar.pending {
            bar.again = true;
            return;
        }
        bar.pending = true;
        bar.generation += 1;
        bar.last = Some(Instant::now());
        let generation = bar.generation;
        let request = Request::Find {
            target: Some(block.to_string()),
            query,
            case_sensitive: bar.case_sensitive,
            regex: bar.regex,
        };
        let proxy = self.app.proxy.clone();
        self.app.rt.spawn(async move {
            let found = match client.request(request).await {
                Ok(Response::Found { matches, truncated }) => Ok((matches, truncated)),
                Ok(_) => Err("unexpected reply".to_owned()),
                Err(e) => Err(e.to_string()),
            };
            let _ = proxy.unbounded_send(UiEvent::Found(generation, block, found));
        });
    }

    /// The answer to a search.
    pub(crate) fn on_found(
        &mut self,
        generation: u64,
        block: BlockId,
        found: Result<(Vec<ferrumterm_protocol::FindMatch>, bool), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(bar) = self.find.as_mut().filter(|b| b.generation == generation) else {
            return;
        };
        bar.pending = false;
        if bar.search.block != Some(block) {
            bar.search = Search {
                block: Some(block),
                ..Search::default()
            };
        }
        match found {
            Ok((matches, truncated)) => {
                bar.error = None;
                bar.search.update(matches, truncated);
            }
            Err(error) => {
                bar.error = Some(error);
                bar.search.update(Vec::new(), false);
            }
        }
        if std::mem::take(&mut bar.jump) {
            self.scroll_to_match();
        }
        if self
            .find
            .as_mut()
            .is_some_and(|b| std::mem::take(&mut b.again))
        {
            self.search_now(false, cx);
        }
        cx.notify();
    }

    /// ↩ / ⇧↩: the next match up or down, scrolled into view.
    pub(crate) fn find_step(&mut self, step: Step, cx: &mut Context<Self>) {
        let Some(bar) = self.find.as_mut() else {
            return;
        };
        bar.search.step(step);
        self.scroll_to_match();
        cx.notify();
    }

    fn scroll_to_match(&mut self) {
        let Some(search) = self.find.as_ref().map(|b| &b.search) else {
            return;
        };
        let Some(block) = search.block else { return };
        let Some(lines) = self
            .app
            .frames
            .get(&block)
            .and_then(|f| search.scroll_to_current(f))
        else {
            return;
        };
        self.app.send(Request::Scroll {
            target: Some(block.to_string()),
            lines: Some(lines),
        });
    }

    /// A frame of `block` arrived: search again when it is the searched
    /// pane (soon, if it searched just now).
    pub(crate) fn find_on_frame(&mut self, block: BlockId, cx: &mut Context<Self>) {
        let Some(bar) = self.find.as_mut() else {
            return;
        };
        if bar.search.block != Some(block) || bar.pending || bar._refresh.is_some() {
            return;
        }
        let wait = RESEARCH.saturating_sub(bar.last.map_or(RESEARCH, |t| t.elapsed()));
        if wait.is_zero() {
            self.search_now(false, cx);
            return;
        }
        bar._refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |view, cx| {
                if let Some(bar) = view.find.as_mut() {
                    bar._refresh = None;
                }
                view.search_now(false, cx);
            });
        }));
    }

    pub(crate) fn toggle_find_option(&mut self, regex: bool, cx: &mut Context<Self>) {
        if let Some(bar) = self.find.as_mut() {
            if regex {
                bar.regex = !bar.regex;
            } else {
                bar.case_sensitive = !bar.case_sensitive;
            }
        }
        self.search_now(true, cx);
        cx.notify();
    }

    /// The bar, over the top-right corner of the focused pane.
    pub(crate) fn render_find_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let bar = self.find.as_ref()?;
        let colors = cx.theme().colors().clone();
        let (left, top) = self.focused_pane_corner(BAR_WIDTH);
        let status = match (&bar.error, self.find_query(cx)) {
            (Some(error), _) => Label::new(error.clone()).color(Color::Error),
            (None, None) => Label::new(""),
            (None, Some(_)) => Label::new(bar.search.label()).color(Color::Muted),
        };
        let button = |id: &'static str, icon: IconName, tip: &'static str| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(tip))
        };
        Some(
            h_flex()
                .id("find-bar")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(BAR_WIDTH))
                .px_1p5()
                .py_1()
                .gap_1()
                .occlude()
                .rounded_md()
                .border_1()
                .border_color(colors.border_variant)
                .bg(colors.elevated_surface_background)
                .shadow_md()
                .text_size(px(12.))
                .child(div().flex_1().min_w_0().child(bar.input.clone()))
                .child(
                    div()
                        .flex_none()
                        .max_w(px(120.))
                        .truncate()
                        .child(status.size(LabelSize::Small)),
                )
                .child(
                    button("find-case", IconName::CaseSensitive, "Match case")
                        .toggle_state(bar.case_sensitive)
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_find_option(false, cx))),
                )
                .child(
                    button("find-regex", IconName::Regex, "Regular expression")
                        .toggle_state(bar.regex)
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_find_option(true, cx))),
                )
                .child(
                    button(
                        "find-older",
                        IconName::ArrowUp,
                        "Previous match, up (↩, ⌘G)",
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.find_step(Step::Older, cx))),
                )
                .child(
                    button(
                        "find-newer",
                        IconName::ArrowDown,
                        "Next match, down (⇧↩, ⌘⇧G)",
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.find_step(Step::Newer, cx))),
                )
                .child(
                    button("find-close", IconName::Close, "Close (Esc)")
                        .on_click(cx.listener(|view, _, window, cx| view.close_find(window, cx))),
                ),
        )
    }

    /// Where a `width`-wide overlay goes at the focused pane's top-right
    /// corner, relative to the terminal area (from the last paint).
    pub(crate) fn focused_pane_corner(&self, width: f32) -> (f32, f32) {
        let (origin_x, origin_y) = self.canvas_origin;
        let pane = self
            .app
            .focused()
            .and_then(|b| self.app.panes.iter().find(|p| p.0 == b));
        match pane {
            Some(&(_, x, y, w, _)) => (
                (x + w - width - 4. - origin_x).max(x - origin_x),
                y - origin_y + 2.,
            ),
            None => (8., 6.),
        }
    }
}
