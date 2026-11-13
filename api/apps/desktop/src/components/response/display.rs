//! What the Pretty / Raw body view shows: the display text (cached per
//! response and mode, so JSON is not re-parsed every frame), JSON syntax
//! colours and find-in-body matches, pushed into the read-only field as
//! highlights.

use crate::app::FerrumApi;
use crate::state::BodyMode;
use ferrum_ui::TextInput;
use ferrum_ui::components::ActiveTheme as _;
use ferrumapi_domain::highlight::{JsonToken, find_matches, json_tokens, layer_spans, line_of};
use ferrumapi_domain::protocol::trpc::{TrpcOutcome, unwrap_response};
use ferrumapi_domain::text::pretty_json;
use ferrumapi_domain::xml::{XmlToken, looks_like_xml, pretty_xml, xml_tokens};
use ferrumapi_domain::{HttpResponse, TabId};
use gpui::{App, Context, Entity, EntityId, HighlightStyle, Window, point, px};
use std::ops::Range;

/// Large bodies are cut for display (Copy / Save still use the full body).
const MAX_RENDER_CHARS: usize = 400_000;
/// Matches beyond this are neither highlighted nor counted.
pub const MAX_MATCHES: usize = 10_000;
/// Padding of the body scroll container (`p_3`).
const BODY_PADDING: f32 = 12.;

type ResponseKey = (TabId, u16, u64, usize, BodyMode);
type SearchKey = Option<(String, bool)>;

pub struct BodyDisplay {
    key: ResponseKey,
    pub text: String,
    pub clipped: bool,
    /// Syntax spans as indexes into [`token_styles`].
    tokens: Vec<(Range<usize>, usize)>,
    search: SearchKey,
    pub matches: Vec<Range<usize>>,
    /// Highlights last pushed: (field entity, search, current match).
    applied: Option<(EntityId, SearchKey, usize)>,
}

fn clip(mut text: String) -> (String, bool) {
    if text.len() <= MAX_RENDER_CHARS {
        return (text, false);
    }
    let mut end = MAX_RENDER_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Plain,
    Json,
    Xml,
}

/// Display text and how to colour it.
fn display_text(resp: &HttpResponse, mode: BodyMode) -> (String, Syntax) {
    let text = resp.body_text();
    let looks_json = resp.is_json() || text.trim_start().starts_with(['{', '[']);
    match mode {
        BodyMode::Data => match unwrap_response(&resp.body) {
            Some(TrpcOutcome::Data(v)) => (
                serde_json::to_string_pretty(&v).unwrap_or_default(),
                Syntax::Json,
            ),
            Some(TrpcOutcome::Error(e)) => (
                serde_json::to_string_pretty(&serde_json::json!({ "error": e }))
                    .unwrap_or_default(),
                Syntax::Json,
            ),
            None => (
                "Not a tRPC response (no result.data or error). See Pretty or Raw.".to_owned(),
                Syntax::Plain,
            ),
        },
        BodyMode::Pretty if looks_json => match pretty_json(&text) {
            Some(pretty) => (pretty, Syntax::Json),
            None => (text, Syntax::Plain),
        },
        BodyMode::Pretty if looks_like_xml(resp.content_type(), &text) => match pretty_xml(&text) {
            Some(pretty) => (pretty, Syntax::Xml),
            None => (text, Syntax::Xml),
        },
        _ => (text, Syntax::Plain),
    }
}

fn token_styles(cx: &App) -> [HighlightStyle; 10] {
    let syntax = cx.theme().syntax();
    let fallback = cx.theme().colors().text;
    let style = |name: &str| HighlightStyle {
        color: Some(
            syntax
                .style_for_name(name)
                .and_then(|s| s.color)
                .unwrap_or(fallback),
        ),
        ..HighlightStyle::default()
    };
    [
        style("property"),
        style("string"),
        style("number"),
        style("boolean"),
        style("constant"),
        style("tag"),
        style("attribute"),
        style("string"),
        style("comment"),
        style("string.special"),
    ]
}

const fn token_index(kind: JsonToken) -> usize {
    match kind {
        JsonToken::Key => 0,
        JsonToken::String => 1,
        JsonToken::Number => 2,
        JsonToken::Bool => 3,
        JsonToken::Null => 4,
    }
}

const fn xml_index(kind: XmlToken) -> usize {
    match kind {
        XmlToken::Tag => 5,
        XmlToken::AttrName => 6,
        XmlToken::AttrValue => 7,
        XmlToken::Comment => 8,
        XmlToken::Cdata => 9,
    }
}

impl FerrumApi {
    /// Refreshes `self.body_display` for this response, mode and search.
    pub(super) fn update_body_display(
        &mut self,
        tab_id: TabId,
        response: &HttpResponse,
        mode: BodyMode,
    ) {
        let key = (
            tab_id,
            response.status,
            response.time_ms,
            response.body.len(),
            mode,
        );
        if self.body_display.as_ref().is_none_or(|d| d.key != key) {
            let (text, syntax) = display_text(response, mode);
            // The field normalizes CRLF; offsets must match what it holds.
            let (text, clipped) = clip(text.replace("\r\n", "\n"));
            let tokens = match syntax {
                Syntax::Json => json_tokens(&text)
                    .into_iter()
                    .map(|(r, k)| (r, token_index(k)))
                    .collect(),
                Syntax::Xml => xml_tokens(&text)
                    .into_iter()
                    .map(|(r, k)| (r, xml_index(k)))
                    .collect(),
                Syntax::Plain => Vec::new(),
            };
            self.body_display = Some(BodyDisplay {
                key,
                text,
                clipped,
                tokens,
                search: None,
                matches: Vec::new(),
                applied: None,
            });
        }
        let search: SearchKey = self
            .state
            .response_search
            .as_ref()
            .filter(|s| !s.query.is_empty())
            .map(|s| (s.query.clone(), s.case_sensitive));
        let Some(display) = self.body_display.as_mut() else {
            return;
        };
        if display.search != search {
            display.matches = search.as_ref().map_or_else(Vec::new, |(query, case)| {
                find_matches(&display.text, query, *case, MAX_MATCHES)
            });
            display.search = search;
        }
        if let Some(s) = self.state.response_search.as_mut() {
            s.current = s.current.min(display.matches.len().saturating_sub(1));
        }
    }

    /// Pushes syntax + match highlights into `input` when they changed, and
    /// scrolls the current match into view when asked to.
    pub(super) fn apply_body_highlights(
        &mut self,
        input: &Entity<TextInput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.state.response_search.as_ref().map_or(0, |s| s.current);
        let Some(display) = self.body_display.as_mut() else {
            return;
        };
        let applied = Some((input.entity_id(), display.search.clone(), current));
        if display.applied != applied {
            display.applied = applied;
            let styles = token_styles(cx);
            let syntax: Vec<_> = display
                .tokens
                .iter()
                .map(|(range, index)| (range.clone(), styles[*index]))
                .collect();
            let colors = cx.theme().colors();
            let found: Vec<_> = display
                .matches
                .iter()
                .enumerate()
                .map(|(i, range)| {
                    let background = if i == current {
                        colors.search_active_match_background
                    } else {
                        colors.search_match_background
                    };
                    let style = HighlightStyle {
                        background_color: Some(background),
                        ..HighlightStyle::default()
                    };
                    (range.clone(), style)
                })
                .collect();
            let spans = layer_spans(&syntax, &found, |base, top| base.highlight(*top));
            input.update(cx, |input, cx| input.set_highlights(spans, cx));
        }

        let reveal = self
            .state
            .response_search
            .as_mut()
            .is_some_and(|s| std::mem::take(&mut s.reveal));
        if let Some(range) = display.matches.get(current).filter(|_| reveal) {
            let line = line_of(&display.text, range.start);
            let range = range.clone();
            // Selecting the match also scrolls the field horizontally to it.
            input.update(cx, |input, cx| input.select_range(range, cx));
            let line_height = input
                .read(cx)
                .line_height()
                .unwrap_or_else(|| window.line_height());
            let viewport = self.response_scroll.bounds().size.height;
            let top = line_height * line as f32 + px(BODY_PADDING);
            let y = (top - viewport / 3.).max(px(0.));
            self.response_scroll.set_offset(point(px(0.), -y));
        }
    }
}
