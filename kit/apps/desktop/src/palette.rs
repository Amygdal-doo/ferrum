//! The command palette (⌘K): every tool, fuzzy-searched; ↑/↓ choose,
//! Enter opens, Esc closes.

use ferrum_ui::gpui::{Action, HighlightStyle, StyledText};
use ferrum_ui::prelude::*;
use ferrumkit_core::{Tool, ToolMatch, fuzzy_tools};

pub enum PaletteEvent {
    Open(Tool),
    Dismissed,
}

pub struct Palette {
    input: Entity<TextInput>,
    matches: Vec<ToolMatch>,
    selected: usize,
    up: Option<Box<dyn Action>>,
    down: Option<Box<dyn Action>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Palette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(window, cx).placeholder("Go to a tool…"));
        let subscription = cx.subscribe(&input, |this: &mut Self, _, event, cx| match event {
            TextInputEvent::Changed => this.refilter(cx),
            TextInputEvent::Submit => this.confirm(cx),
            TextInputEvent::Blurred => {}
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut this = Self {
            input,
            matches: Vec::new(),
            selected: 0,
            // A single-line field passes ↑/↓ on to its parent.
            up: cx.build_action("text_input::Up", None).ok(),
            down: cx.build_action("text_input::Down", None).ok(),
            _subscriptions: vec![subscription],
        };
        this.refilter(cx);
        this
    }

    #[cfg(test)]
    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        self.matches = fuzzy_tools(&query);
        self.selected = 0;
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.matches.get(self.selected) {
            Some(found) => cx.emit(PaletteEvent::Open(found.tool)),
            None => cx.emit(PaletteEvent::Dismissed),
        }
    }

    fn render_row(&self, ix: usize, found: &ToolMatch, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let name = found.tool.name();
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = name
            .char_indices()
            .enumerate()
            .filter(|(char_ix, _)| found.name_chars.contains(char_ix))
            .map(|(_, (byte, c))| {
                (
                    byte..byte + c.len_utf8(),
                    HighlightStyle {
                        color: Some(colors.text_accent),
                        font_weight: Some(gpui::FontWeight::BOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let tool = found.tool;
        h_flex()
            .id(("palette-row", ix))
            .w_full()
            .px_3()
            .py_1()
            .gap_3()
            .rounded_sm()
            .cursor_pointer()
            .when(ix == self.selected, |row| row.bg(colors.element_selected))
            .hover(|row| row.bg(colors.element_hover))
            .child(
                div()
                    .flex_none()
                    .w(px(170.))
                    .truncate()
                    .child(StyledText::new(name).with_highlights(highlights)),
            )
            .child(
                div().flex_1().min_w_0().truncate().child(
                    Label::new(tool.description())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(PaletteEvent::Open(tool))))
            .into_any_element()
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let rows: Vec<AnyElement> = self
            .matches
            .clone()
            .iter()
            .enumerate()
            .map(|(ix, found)| self.render_row(ix, found, cx))
            .collect();
        let mut card =
            v_flex()
                .key_context("FerrumKitPalette")
                .w(px(560.))
                .max_h(px(420.))
                .p_1()
                .gap_1()
                .rounded_lg()
                .border_1()
                .border_color(colors.border)
                .bg(colors.elevated_surface_background)
                .shadow_lg()
                .on_action(cx.listener(|_, _: &ferrum_ui::menu::Cancel, _, cx| {
                    cx.emit(PaletteEvent::Dismissed)
                }))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.input.clone());
        if let Some(up) = &self.up {
            card = card.on_boxed_action(
                up.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(-1, cx)),
            );
        }
        if let Some(down) = &self.down {
            card = card.on_boxed_action(
                down.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(1, cx)),
            );
        }
        card.when(rows.is_empty(), |card| {
            card.child(
                div().px_3().py_2().child(
                    Label::new("No tools match")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
        })
        .child(
            v_flex()
                .id("palette-rows")
                .min_h_0()
                .overflow_y_scroll()
                .children(rows),
        )
    }
}
