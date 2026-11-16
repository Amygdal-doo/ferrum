use super::*;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumapi_domain::{HttpMethod, KeyValue};

fn input() -> DraftInput {
    let mut request = HttpRequest::new(HttpMethod::Post, "{{baseUrl}}/pets");
    request.headers = vec![KeyValue::new("Authorization", "Bearer sk-live-123")];
    request.auth = ferrumapi_domain::Auth::Bearer {
        token: "tok-secret".into(),
    };
    request.body = RequestBody::Raw {
        language: ferrumapi_domain::RawLanguage::Json,
        text: "{\"name\":\"Rex\"}".into(),
    };
    DraftInput {
        name: "Create pet".into(),
        request,
        description:
            "Expected responses (OpenAPI):\n- 201: Created\n  application/json: {\"id\":7}".into(),
        response: None,
    }
}

#[test]
fn prompt_has_the_spec_but_no_secrets() {
    let p = prompt(&input());
    assert!(p.contains("POST {{baseUrl}}/pets"));
    assert!(p.contains("Header names: Authorization"));
    assert!(p.contains("- 201: Created"));
    assert!(p.contains("{\"name\":\"Rex\"}"));
    assert!(!p.contains("sk-live") && !p.contains("tok-secret"), "{p}");
}

#[test]
fn scripts_are_taken_from_the_first_code_block() {
    assert_eq!(
        extract_script("Here:\n```ferrum\nassert status == 201\n```\nDone"),
        "assert status == 201"
    );
    assert_eq!(
        extract_script("assert status == 200"),
        "assert status == 200"
    );
    assert_eq!(extract_script("```\nassert x\n"), "assert x");
}

#[test]
fn drafts_through_the_agent_core_and_flags_bad_lines() {
    let mock = MockProvider::new();
    mock.push_text(
        "```ferrum\ntest \"created\": status == 201\nassert json.id is number\npm.expect(1)\n```",
    );
    let drafter = TestDrafter::with_provider(Arc::new(mock.clone()));
    let models = drafter.models();
    assert_eq!(models[0].label, "FerrumAI");
    assert!(!models[0].enabled, "FerrumAI is listed but disabled");
    let model = drafter.default_model().expect("a usable model");
    assert!(model.enabled && model.provider == "anthropic", "{model:?}");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let draft = rt.block_on(drafter.draft(&model, &input())).expect("draft");
    assert!(draft.script.starts_with("test \"created\": status == 201"));
    assert_eq!(draft.problems.len(), 1);
    assert_eq!(draft.problems[0].0, 3);
    let sent = &mock.requests()[0];
    assert_eq!(sent.model, model.model);
    assert!(sent.system.contains("FerrumAPI script language"));
    assert!(sent.tools.is_empty());

    let err = rt
        .block_on(drafter.draft(&models[0], &input()))
        .expect_err("FerrumAI");
    assert!(err.contains("not available"));
}
