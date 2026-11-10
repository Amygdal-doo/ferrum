//! One view per tool. Each view owns its inputs, so switching tools keeps
//! what was typed for the rest of the session.

mod base64;
mod case;
mod color;
mod diff;
mod hash;
mod ids;
mod json;
mod jwt;
mod lines;
mod number_base;
mod regex;
mod timestamp;
mod url;

use crate::app::FerrumKit;
use crate::widgets::KitEvent;
use ferrum_ui::gpui::AnyView;
use ferrum_ui::prelude::*;
use ferrumkit_core::Tool;

/// Creates the view for `tool` and forwards its events to the root.
pub fn build(
    tool: Tool,
    window: &mut Window,
    cx: &mut Context<FerrumKit>,
) -> (AnyView, Subscription) {
    match tool {
        Tool::Json => wire(cx.new(|cx| json::JsonTool::new(window, cx)), cx),
        Tool::Base64 => wire(cx.new(|cx| base64::Base64Tool::new(window, cx)), cx),
        Tool::Jwt => wire(cx.new(|cx| jwt::JwtTool::new(window, cx)), cx),
        Tool::Url => wire(cx.new(|cx| url::UrlTool::new(window, cx)), cx),
        Tool::Timestamp => wire(cx.new(|cx| timestamp::TimestampTool::new(window, cx)), cx),
        Tool::Ids => wire(cx.new(|cx| ids::IdsTool::new(window, cx)), cx),
        Tool::Hash => wire(cx.new(|cx| hash::HashTool::new(window, cx)), cx),
        Tool::Regex => wire(cx.new(|cx| regex::RegexTool::new(window, cx)), cx),
        Tool::Diff => wire(cx.new(|cx| diff::DiffTool::new(window, cx)), cx),
        Tool::NumberBase => wire(
            cx.new(|cx| number_base::NumberBaseTool::new(window, cx)),
            cx,
        ),
        Tool::Color => wire(cx.new(|cx| color::ColorTool::new(window, cx)), cx),
        Tool::Case => wire(cx.new(|cx| case::CaseTool::new(window, cx)), cx),
        Tool::Lines => wire(cx.new(|cx| lines::LinesTool::new(window, cx)), cx),
    }
}

fn wire<T: Render + EventEmitter<KitEvent>>(
    entity: Entity<T>,
    cx: &mut Context<FerrumKit>,
) -> (AnyView, Subscription) {
    let subscription = cx.subscribe(&entity, |this, _, event, cx| this.handle_event(event, cx));
    (entity.into(), subscription)
}

/// Two panes side by side (input left, output right).
pub fn split(left: impl IntoElement, right: impl IntoElement) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_h_0()
        .items_stretch()
        .gap_3()
        .child(left)
        .child(right)
}

/// The standard tool layout: controls on top, then the body.
pub fn layout(controls: impl IntoElement, body: impl IntoElement) -> Div {
    v_flex().size_full().gap_2().child(controls).child(body)
}
