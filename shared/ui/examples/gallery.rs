//! Smoke test for the shared widgets: `cargo run -p ferrum-ui --example gallery`.

use ferrum_ui::prelude::*;
use gpui::Axis;

struct Gallery {
    single: Entity<TextInput>,
    secret: Entity<TextInput>,
    multi: Entity<TextInput>,
    sidebar_width: Pixels,
    notice: Option<Notice>,
    show_modal: bool,
    _subscriptions: Vec<Subscription>,
}

impl Gallery {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let single = cx.new(|cx| TextInput::new(window, cx).placeholder("https://example.com"));
        let secret = cx.new(|cx| TextInput::new(window, cx).masked(true).with_text("hunter2"));
        let multi = cx.new(|cx| {
            TextInput::new(window, cx)
                .multiline(6)
                .monospace()
                .with_text("SELECT *\nFROM users\nWHERE id = 1;")
        });
        let subscription = cx.subscribe(&single, |this: &mut Self, input, event, cx| {
            if *event == TextInputEvent::Submit {
                this.notice = Some(Notice::info(format!("Submitted: {}", input.read(cx).text())));
                cx.notify();
            }
        });
        Self {
            single,
            secret,
            multi,
            sidebar_width: px(220.),
            notice: None,
            show_modal: false,
            _subscriptions: vec![subscription],
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .child(toolbar(cx).child(Label::new("Ferrum UI gallery")))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .on_drag_move(cx.listener(|this, event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                        this.sidebar_width = (event.event.position.x - event.bounds.left()).max(px(120.));
                        cx.notify();
                    }))
                    .child(
                        v_flex()
                            .w(self.sidebar_width)
                            .h_full()
                            .bg(colors.panel_background)
                            .child(section_header("Sidebar"))
                            .child(ListItem::new("one").child(Label::new("Item one")))
                            .child(ListItem::new("two").child(Label::new("Item two"))),
                    )
                    .child(splitter("sidebar", Axis::Horizontal, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .p_4()
                            .gap_3()
                            .child(h_flex().gap_2().child(badge("GET", cx.theme().status().success, cx)).child(self.single.clone()))
                            .child(self.secret.clone())
                            .child(self.multi.clone())
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Button::new("notice", "Show notice").on_click(cx.listener(|this, _, _, cx| {
                                        this.notice = Some(Notice::error("Something failed"));
                                        cx.notify();
                                    })))
                                    .child(Button::new("modal", "Open dialog").style(ButtonStyle::Filled).on_click(cx.listener(|this, _, _, cx| {
                                        this.show_modal = true;
                                        cx.notify();
                                    })))
                                    .child(kbd("⌘↵", cx)),
                            ),
                    ),
            )
            .child(status_bar(cx).child("Ready").child("GPUI"))
            .when_some(self.notice.as_ref(), |this, notice| {
                this.child(render_notice(notice, cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                }), cx))
            })
            .when(self.show_modal, |this| {
                this.child(
                    Modal::new("Example dialog")
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.show_modal = false;
                            cx.notify();
                        }))
                        .child(Label::new("Dialogs render over a dimmed backdrop."))
                        .footer(Button::new("ok", "OK").style(ButtonStyle::Filled).on_click(cx.listener(|this, _, _, cx| {
                            this.show_modal = false;
                            cx.notify();
                        }))),
                )
            })
    }
}

fn main() {
    ferrum_ui::run(AppConfig::new("Ferrum UI Gallery").size(900., 600.), |window, cx| {
        cx.new(|cx| Gallery::new(window, cx))
    });
}
