//! Importing files and pasted text through AppServices: OpenAPI YAML from a
//! file, an Insomnia export pasted (its OAuth secret moves to the keychain
//! store, not SQLite) and a pasted cURL command (not saved).

use ferrumapi_application::AppServices;
use ferrumapi_application::domain::Auth;
use ferrumapi_application::interop::detect::Imported;
use ferrumapi_application::secrets::MemoryStore;
use std::sync::Arc;

const OPENAPI: &str = "openapi: 3.0.0\ninfo: {title: Tiny}\nservers: [{url: 'http://localhost:1'}]\n\
paths:\n  /ping:\n    get:\n      summary: Ping\n      responses: {'200': {description: pong}}\n";

const INSOMNIA: &str = r#"{"_type": "export", "__export_format": 4, "resources": [
  {"_id": "wrk", "_type": "workspace", "name": "Ins"},
  {"_id": "r1", "_type": "request", "parentId": "wrk", "name": "Token", "method": "GET",
   "url": "https://x.test", "authentication": {"type": "oauth2", "grantType": "client_credentials",
   "accessTokenUrl": "https://id.test/token", "clientId": "cid", "clientSecret": "very-secret"}}]}"#;

#[test]
fn files_and_pasted_text_are_imported() {
    let dir = std::env::temp_dir().join(format!("ferrumapi-imports-{}", uuid::Uuid::new_v4()));
    let keychain = MemoryStore::default();
    let svc = AppServices::start_with_secrets(&dir, Arc::new(keychain.clone())).expect("start");
    let file = dir.join("tiny.yaml");
    std::fs::write(&file, OPENAPI).expect("write");
    let s = svc.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async move {
        let (c, format) = s.import_collection_file(file).await.expect("openapi");
        assert_eq!((c.name.as_str(), format), ("Tiny", "OpenAPI"));

        let Imported::Collection(ins, "Insomnia") = s
            .import_pasted(INSOMNIA.to_owned())
            .await
            .expect("insomnia")
        else {
            panic!("an Insomnia collection");
        };
        let saved = s.store.list_collections().await.expect("list");
        assert_eq!(saved.len(), 2);
        let json = serde_json::to_string(&saved).expect("json");
        assert!(!json.contains("very-secret"), "secret written to SQLite");
        assert!(
            keychain.dump().values().any(|v| v.contains("very-secret")),
            "secret in the keychain store"
        );
        let mut reloaded = saved;
        s.hydrate_collections(&mut reloaded).await;
        let r = reloaded.iter().find(|c| c.id == ins.id).expect("saved");
        let ferrumapi_application::domain::Item::Request(req) = &r.items[0] else {
            panic!("request");
        };
        assert!(matches!(&req.request.auth, Auth::OAuth2(o) if o.client_secret == "very-secret"));

        let curl = s
            .import_pasted("curl https://x.test/a -H 'X: 1'".into())
            .await
            .expect("curl");
        assert!(matches!(curl, Imported::Request(r) if r.headers[0].key == "X"));
        assert_eq!(s.store.list_collections().await.expect("list").len(), 2);
    });
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}
