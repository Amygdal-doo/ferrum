//! The Scripts section: pre-request and test scripts (the sandboxed
//! statement language of `ferrumapi_domain::script`), checked as you type.

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::hint;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color,
    Label, LabelCommon as _, LabelSize, StyledTypography as _, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::script::{Scripts, check};
use ferrumapi_domain::{HttpRequest, TabId};
use gpui::{AnyElement, Context, Window, div, prelude::*};

// Placeholders are one line (the text field shapes them as a single line).
const PRE_PLACEHOLDER: &str = "set var.request_id = uuid()";
const TESTS_PLACEHOLDER: &str = "test \"status is 200\": status == 200";

const HELP: &str = "One statement per line: set env.x = … / global.x / var.x, unset, \
test \"name\": expr, assert expr, log expr. Read status, time, size, body, json.a.b[0], \
header(\"Name\"), env.x. Operators: == != < > contains matches exists is number|string|array|object. \
No file or network access.";

impl FerrumApi {
    pub(super) fn render_scripts_editor(
        &mut self,
        tab_id: TabId,
        scripts: &Scripts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pre = self.script_editor(
            tab_id,
            "Pre-request",
            "script-pre",
            &scripts.pre_request,
            PRE_PLACEHOLDER,
            |r, v| r.scripts.pre_request = v,
            window,
            cx,
        );
        let tests = self.script_editor(
            tab_id,
            "Tests",
            "script-tests",
            &scripts.tests,
            TESTS_PLACEHOLDER,
            |r, v| r.scripts.tests = v,
            window,
            cx,
        );
        let draft = Button::new("draft-tests", "Draft tests with agent…")
            .style(ButtonStyle::Outlined)
            .size(ButtonSize::Compact)
            .tooltip(Tooltip::text(
                "Uses your FerrumChat models to draft checks from the request, its OpenAPI description and last response. You review them before saving.",
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.draft_tests_for_tab(tab_id, cx);
                cx.notify();
            }));
        v_flex()
            .gap_4()
            .child(hint(HELP))
            .child(pre)
            .child(tests)
            .child(h_flex().child(draft))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn script_editor(
        &mut self,
        tab_id: TabId,
        title: &'static str,
        key: &'static str,
        value: &str,
        placeholder: &'static str,
        set: fn(&mut HttpRequest, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = self.field(
            FieldSpec::new(format!("{key}:{tab_id}"))
                .value(value)
                .placeholder(placeholder)
                .multiline(5)
                .mono(),
            on_change(move |this, text, _| {
                this.state.edit_request(tab_id, |r| set(r, text));
            }),
            window,
            cx,
        );
        let problems = check(value);
        let error = cx.theme().status().error;
        v_flex()
            .gap_1()
            .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
            .child(div().min_h_24().child(editor))
            .children(problems.into_iter().take(5).map(|(line, msg)| {
                div()
                    .text_xs()
                    .font_buffer(cx)
                    .text_color(error)
                    .child(format!("line {line}: {msg}"))
            }))
            .into_any_element()
    }
}
