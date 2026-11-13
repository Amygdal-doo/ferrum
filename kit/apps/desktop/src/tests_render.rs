//! Headless GPUI smoke test: the shell renders every tool. No window is shown.

use ferrumkit_core::Tool;
use gpui::TestAppContext;

use crate::app::FerrumKit;

#[gpui::test]
fn every_tool_renders(cx: &mut TestAppContext) {
    cx.update(ferrum_ui::init_for_tests);
    let (app, cx) = cx.add_window_view(FerrumKit::new);
    cx.run_until_parked();
    for tool in Tool::ALL {
        app.update(cx, |app, cx| {
            app.select(tool);
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(app.read_with(cx, |app, _| app.selected()), tool);
    }
}
