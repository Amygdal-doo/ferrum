//! End-to-end through AppServices: prepare → send (real HTTP to a local mock)
//! → history, and Postman file import/export via the store.

use ferrumapi_application::domain::{
    Auth, Collection, Environment, HttpMethod, HttpRequest, Item, RequestOrigin, SavedRequest,
    Variable,
};
use ferrumapi_application::{AppServices, CancellationToken, prepare};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn services() -> (AppServices, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-e2e-{}", uuid::Uuid::new_v4()));
    (AppServices::start(&dir).expect("start"), dir)
}

#[test]
fn send_with_inherited_auth_and_env_records_history() {
    let (svc, dir) = services();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(header("authorization", "Bearer s3cret"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .mount(&server)
            .await;

        let mut coll = Collection::new("API");
        coll.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        let saved = SavedRequest::new("Users", HttpRequest::new(HttpMethod::Get, "{{base}}/users"));
        let origin = RequestOrigin {
            collection: coll.id,
            item: saved.id,
        };
        let authored = saved.request.clone();
        coll.items.push(Item::Request(saved));
        let mut env = Environment::new("local");
        env.variables = vec![
            Variable::new("base", server.uri()),
            Variable::new("token", "s3cret"),
        ];

        let prepared = prepare(
            &authored,
            Some(origin),
            &[coll],
            &Environment::globals(),
            Some(&env),
        );
        let s = svc.clone();
        let res = svc
            .spawn(async move {
                s.send(authored, prepared, Some(origin), CancellationToken::new())
                    .await
            })
            .await
            .expect("join")
            .expect("response");
        assert_eq!(res.status, 200);
        assert_eq!(res.body_text(), r#"{"ok":true}"#);

        let store = svc.store.clone();
        let history = svc
            .spawn(async move { store.list_history(10).await })
            .await
            .expect("join")
            .expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, Some(200));
        assert_eq!(
            history[0].request.url, "{{base}}/users",
            "history keeps authored request"
        );
        assert_eq!(history[0].origin, Some(origin));
    });
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn postman_file_import_saves_and_exports() {
    let (svc, dir) = services();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../interop/tests/fixtures/postman_collection_v21.json");
    let out = dir.join("export.json");
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    rt.block_on(async {
        let s = svc.clone();
        let out2 = out.clone();
        let (imported, listed) = svc
            .spawn(async move {
                let c = s.import_postman_file(fixture).await.expect("import");
                s.export_postman_file(c.clone(), out2)
                    .await
                    .expect("export");
                (c, s.store.list_collections().await.expect("list"))
            })
            .await
            .expect("join");
        assert!(!imported.requests().is_empty());
        assert_eq!(listed, vec![imported]);
    });
    let json = std::fs::read_to_string(&out).expect("exported file");
    assert!(json.contains("collection/v2.1.0/collection.json"));
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}
