//! One view per tool. Each view owns its inputs, so switching tools keeps
//! what was typed for the rest of the session; history and saved inputs
//! (see [`ferrumkit_core::store`]) keep them across launches.

mod base64;
mod case;
mod color;
mod cron;
mod diff;
mod hash;
mod ids;
mod json;
mod json_schema;
mod jwt;
mod lines;
mod number_base;
mod qr;
mod regex;
mod sql;
mod timestamp;
mod url;

use std::rc::Rc;

use crate::app::FerrumKit;
use crate::widgets::KitEvent;
use ferrum_ui::gpui::AnyView;
use ferrum_ui::prelude::*;
use ferrumkit_core::Tool;

/// What the root needs from a tool view besides drawing it.
pub trait KitTool: Render + EventEmitter<KitEvent> {
    /// The fields holding the tool's input, in a fixed order: history and
    /// saved inputs store their text.
    fn inputs(&self) -> Vec<Entity<TextInput>>;

    /// Result fields (soft wrap applies to them as well).
    fn outputs(&self) -> Vec<Entity<TextInput>> {
        Vec::new()
    }

    /// Recomputes the result after the inputs were replaced.
    fn rerun(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}

/// A tool view with its fields, independent of its type.
#[derive(Clone)]
pub struct ToolView {
    pub view: AnyView,
    inputs: Vec<Entity<TextInput>>,
    outputs: Vec<Entity<TextInput>>,
    rerun: Rc<dyn Fn(&mut App)>,
}

impl ToolView {
    /// The text of the input fields.
    pub fn values(&self, cx: &App) -> Vec<String> {
        self.inputs
            .iter()
            .map(|input| input.read(cx).text().to_string())
            .collect()
    }

    /// Puts `values` into the input fields (missing ones are cleared) and
    /// recomputes the result.
    pub fn restore(&self, values: &[String], cx: &mut App) {
        for (i, input) in self.inputs.iter().enumerate() {
            let text = values.get(i).cloned().unwrap_or_default();
            input.update(cx, |input, cx| input.set_text(text, cx));
        }
        (self.rerun)(cx);
    }

    pub fn inputs(&self) -> &[Entity<TextInput>] {
        &self.inputs
    }

    /// Soft wrap on or off in every multi-line field.
    pub fn set_wrap(&self, wrap: bool, cx: &mut App) {
        for field in self.inputs.iter().chain(&self.outputs) {
            field.update(cx, |field, cx| field.set_soft_wrap(wrap, cx));
        }
    }

    #[cfg(test)]
    pub fn fields(&self) -> impl Iterator<Item = &Entity<TextInput>> {
        self.inputs.iter().chain(&self.outputs)
    }
}

/// Creates the view for `tool` and forwards its events to the root.
pub fn build(
    tool: Tool,
    window: &mut Window,
    cx: &mut Context<FerrumKit>,
) -> (ToolView, Subscription) {
    match tool {
        Tool::Json => wire(cx.new(|cx| json::JsonTool::new(window, cx)), cx),
        Tool::JsonSchema => wire(
            cx.new(|cx| json_schema::JsonSchemaTool::new(window, cx)),
            cx,
        ),
        Tool::Sql => wire(cx.new(|cx| sql::SqlTool::new(window, cx)), cx),
        Tool::Base64 => wire(cx.new(|cx| base64::Base64Tool::new(window, cx)), cx),
        Tool::Jwt => wire(cx.new(|cx| jwt::JwtTool::new(window, cx)), cx),
        Tool::Url => wire(cx.new(|cx| url::UrlTool::new(window, cx)), cx),
        Tool::Timestamp => wire(cx.new(|cx| timestamp::TimestampTool::new(window, cx)), cx),
        Tool::Cron => wire(cx.new(|cx| cron::CronTool::new(window, cx)), cx),
        Tool::Ids => wire(cx.new(|cx| ids::IdsTool::new(window, cx)), cx),
        Tool::Qr => wire(cx.new(|cx| qr::QrTool::new(window, cx)), cx),
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

fn wire<T: KitTool>(entity: Entity<T>, cx: &mut Context<FerrumKit>) -> (ToolView, Subscription) {
    let subscription = cx.subscribe(&entity, |this, _, event, cx| this.handle_event(event, cx));
    let (inputs, outputs) = {
        let tool = entity.read(cx);
        (tool.inputs(), tool.outputs())
    };
    let target = entity.clone();
    let rerun: Rc<dyn Fn(&mut App)> = Rc::new(move |cx: &mut App| {
        target.update(cx, |tool, cx| tool.rerun(cx));
    });
    let view = ToolView {
        view: entity.into(),
        inputs,
        outputs,
        rerun,
    };
    (view, subscription)
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
