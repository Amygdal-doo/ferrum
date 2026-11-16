//! Models and approvals, headless with the scripted mock provider: both
//! pickers work before the session opens (the choice reaches it), FerrumAI
//! can't be chosen, and the settings sheet tests, adds and removes a custom
//! model whose key goes to the (fake) keychain, never to the file.

use std::sync::Arc;
use std::time::Duration;

use ferrumagent_core::permissions::Approval;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::secrets::{MemoryStore, SecretStore};
use ferrumagent_core::session::{AgentChoice, SessionConfig};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

use crate::custom_form::Test;
use crate::view::{ChatOptions, ChatView};

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn read<R>(
    view: &Entity<ChatView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&ChatView) -> R,
) -> R {
    view.read_with(cx, |v, _| f(v))
}

fn type_into(input: &Entity<ferrum_ui::TextInput>, text: &str, cx: &mut VisualTestContext) {
    input.update(cx, |input, cx| input.set_text(text, cx));
}

#[gpui::test]
fn pickers_before_the_session_and_custom_models_in_settings(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    let mock = MockProvider::new();
    // The turn, then three tests (custom form, a provider, the menu's).
    mock.push_text("hi")
        .push_text("pong")
        .push_text("pong")
        .push_text("pong");
    let keychain = MemoryStore::default();
    let mut config = SessionConfig::isolated(&root, home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    config.secrets = Arc::new(keychain.clone());
    let options = ChatOptions {
        config: Some(config),
        ..Default::default()
    };
    let opus = AgentChoice::Native {
        provider: "anthropic".into(),
        model: "claude-opus-5-5".into(),
    };
    // Before the session opens: both pickers take a choice, FerrumAI not.
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = ChatView::build(root.clone(), options, window, cx);
        assert!(v.session().is_none(), "still opening");
        v.set_approval(Approval::FullAccess, cx);
        v.set_agent(opus.clone(), cx);
        v.set_agent(
            AgentChoice::Native {
                provider: "ferrum".into(),
                model: "ferrum-ai".into(),
            },
            cx,
        );
        v
    });
    assert_eq!(read(&view, cx, |v| v.approval()), Approval::FullAccess);
    let saved = std::fs::read_to_string(home.path().join("settings.json")).expect("saved");
    assert!(saved.contains("\"approval\": \"full_access\""), "{saved}");
    assert_eq!(read(&view, cx, |v| v.shown_agent()), opus.key());
    let options = read(&view, cx, |v| v.models().agent_options());
    assert_eq!(options[0].label, "FerrumAI");
    assert!(!options[0].enabled, "FerrumAI is listed but disabled");

    cx.simulate_resize(size(px(420.), px(900.)));

    // The choices reach the session with the first message.
    wait_until(cx, "the session", |cx| {
        read(&view, cx, |v| v.session().is_some())
    });
    assert_eq!(read(&view, cx, |v| v.shown_agent()), opus.key());
    let composer = read(&view, cx, |v| v.composer.clone());
    type_into(&composer, "hello", cx);
    view.update_in(cx, |v, window, cx| v.send(&crate::Send, window, cx));
    wait_until(cx, "the answer", |cx| {
        read(&view, cx, |v| {
            !v.is_busy() && v.thread().messages.len() == 2
        })
    });
    let session = read(&view, cx, |v| v.session().cloned().expect("session"));
    assert_eq!(session.agent(), opus);
    assert_eq!(session.approval(), Approval::FullAccess);
    assert_eq!(mock.requests()[0].model, "claude-opus-5-5");

    // Settings: test a custom model as typed, add it, and it is listed.
    view.update_in(cx, |v, window, cx| v.open_settings(window, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("chat-settings-ferrum-ai").is_some());
    let form = read(&view, cx, |v| {
        let f = &v.settings.as_ref().expect("sheet").custom;
        (
            f.base_url.clone(),
            f.model.clone(),
            f.name.clone(),
            f.key.clone(),
        )
    });
    type_into(&form.0, "https://gw.example.com/v1", cx);
    type_into(&form.1, "gw-large", cx);
    type_into(&form.2, "Gateway Large", cx);
    type_into(&form.3, "sk-gw-0123456789abcdef", cx);
    view.update(cx, |v, cx| v.test_custom(cx));
    wait_until(cx, "the custom test", |cx| {
        read(&view, cx, |v| {
            matches!(
                v.settings.as_ref().map(|s| &s.custom.test),
                Some(Test::Done(_))
            )
        })
    });
    let test = read(&view, cx, |v| {
        v.settings.as_ref().map(|s| s.custom.test.clone())
    });
    assert!(
        matches!(test, Some(Test::Done(Ok(ref t))) if t.starts_with("Answered in")),
        "{test:?}"
    );
    assert_eq!(mock.requests()[1].model, "gw-large");

    view.update_in(cx, |v, window, cx| v.add_custom(window, cx));
    wait_until(cx, "the custom model", |cx| {
        read(&view, cx, |v| {
            v.settings
                .as_ref()
                .is_some_and(|s| s.forms.iter().any(|f| f.id == "custom-gateway-large"))
        })
    });
    let file = std::fs::read_to_string(home.path().join("settings.json")).expect("settings");
    assert!(
        file.contains("gw-large") && !file.contains("sk-gw-"),
        "{file}"
    );
    assert_eq!(
        keychain
            .get("custom-gateway-large")
            .expect("get")
            .map(|k| k.expose().to_string()),
        Some("sk-gw-0123456789abcdef".to_string())
    );
    let listed = session
        .agent_options()
        .into_iter()
        .find(|o| o.choice.key() == "custom-gateway-large/gw-large")
        .expect("in the session's menu");
    assert_eq!(listed.label, "Gateway Large");

    // Test a saved provider from its card, and the chosen model from the menu.
    let index = read(&view, cx, |v| {
        v.settings
            .as_ref()
            .expect("sheet")
            .forms
            .iter()
            .position(|f| f.id == "anthropic")
    })
    .expect("anthropic");
    view.update(cx, |v, cx| v.test_provider(index, cx));
    wait_until(cx, "the provider test", |cx| {
        read(&view, cx, |v| {
            matches!(
                v.settings.as_ref().map(|s| &s.forms[index].test),
                Some(Test::Done(Ok(_)))
            )
        })
    });
    view.update(cx, |v, cx| v.ping_current(cx));
    wait_until(cx, "the menu's test", |cx| {
        read(&view, cx, |v| {
            v.notice
                .as_ref()
                .is_some_and(|n| n.message.contains("Answered in"))
        })
    });
    assert_eq!(mock.remaining(), 0);

    // Removing the custom model takes its key.
    let index = read(&view, cx, |v| {
        v.settings
            .as_ref()
            .expect("sheet")
            .forms
            .iter()
            .position(|f| f.custom)
    })
    .expect("custom form");
    view.update(cx, |v, cx| v.remove_custom(index, cx));
    wait_until(cx, "the removal", |cx| {
        read(&view, cx, |v| {
            v.settings
                .as_ref()
                .is_some_and(|s| s.forms.iter().all(|f| !f.custom))
        })
    });
    assert!(keychain.get("custom-gateway-large").expect("get").is_none());
}
