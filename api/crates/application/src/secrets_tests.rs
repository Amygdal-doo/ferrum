use super::*;
use ferrumapi_domain::{DigestAuth, EnvironmentId, Folder, HttpRequest, OAuth1Auth, SavedRequest};

fn services() -> (AppServices, MemoryStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-secrets-{}", uuid::Uuid::new_v4()));
    let store = MemoryStore::default();
    let svc = AppServices::start_with_secrets(&dir, Arc::new(store.clone())).expect("start");
    (svc, store, dir)
}

fn digest(password: &str) -> Auth {
    Auth::Digest(Box::new(DigestAuth {
        username: "u".into(),
        password: password.into(),
        ..DigestAuth::default()
    }))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt")
}

#[test]
fn stash_then_hydrate_round_trips_through_the_store_only() {
    let (svc, store, dir) = services();
    rt().block_on(async {
        let auth = digest("hunter2");
        let account = auth.secret_ref().expect("ref").to_owned();
        svc.stash_secrets(vec![auth.clone()]).await.expect("stash");
        assert_eq!(
            store.dump().get(&account).map(String::as_str),
            Some(r#"{"password":"hunter2"}"#)
        );
        // What collections, tabs and history keep:
        let json = serde_json::to_string(&auth).expect("ser");
        assert!(!json.contains("hunter2"));
        let mut loaded: Auth = serde_json::from_str(&json).expect("de");
        assert!(loaded.secrets_empty());
        svc.hydrate_secrets(vec![&mut loaded]).await;
        assert_eq!(loaded, auth);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stash_merges_and_only_replace_clears() {
    let (svc, store, dir) = services();
    rt().block_on(async {
        let mut aws = Auth::AwsSigV4(Box::default());
        let account = aws.secret_ref().expect("ref").to_owned();
        store
            .set(
                &account,
                r#"{"secret_access_key":"old","session_token":"tok"}"#,
            )
            .expect("seed");
        // An auth read back from disk (nothing loaded) changes nothing.
        svc.stash_secrets(vec![aws.clone()]).await.expect("stash");
        assert!(store.dump()[&account].contains("tok"));
        // A new key is merged in; the token is kept.
        aws.set_secret_field("secret_access_key", "new".into());
        svc.stash_secrets(vec![aws.clone()]).await.expect("stash");
        assert_eq!(
            store.dump()[&account],
            r#"{"secret_access_key":"new","session_token":"tok"}"#
        );
        store.set(&account, "tampered").expect("set");
        // Unchanged since the last write: no write.
        svc.stash_secrets(vec![aws.clone()]).await.expect("stash");
        assert_eq!(store.dump()[&account], "tampered");
        // The user's edits replace: the token goes, then everything.
        svc.replace_secrets(vec![aws.clone()])
            .await
            .expect("replace");
        assert_eq!(store.dump()[&account], r#"{"secret_access_key":"new"}"#);
        aws.set_secret_field("secret_access_key", String::new());
        svc.replace_secrets(vec![aws]).await.expect("replace");
        assert!(!store.dump().contains_key(&account));
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn collections_hydrate_every_level() {
    let (svc, store, dir) = services();
    rt().block_on(async {
        let mut c = Collection::new("C");
        c.auth = digest("coll");
        let mut folder = Folder::new("F");
        folder.auth = Auth::OAuth1(Box::new(OAuth1Auth {
            consumer_secret: "cs".into(),
            token_secret: "ts".into(),
            ..OAuth1Auth::default()
        }));
        let req = HttpRequest {
            auth: digest("req"),
            ..HttpRequest::default()
        };
        folder
            .items
            .push(Item::Request(SavedRequest::new("R", req)));
        c.items.push(Item::Folder(folder));
        svc.stash_collection_secrets(&c).await.expect("stash");
        assert_eq!(store.dump().len(), 3);
        let json = serde_json::to_string(&c).expect("ser");
        assert!(
            !json.contains("\"coll\"") && !json.contains("\"cs\"") && !json.contains("\"req\"")
        );
        let mut loaded: Vec<Collection> = vec![serde_json::from_str(&json).expect("de")];
        svc.hydrate_collections(&mut loaded).await;
        assert_eq!(loaded[0], c);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tokens_are_cached_per_environment() {
    let (svc, store, dir) = services();
    let dir2 = std::env::temp_dir().join(format!("ferrumapi-secrets-{}", uuid::Uuid::new_v4()));
    let svc2 = AppServices::start_with_secrets(&dir2, Arc::new(store.clone())).expect("start");
    rt().block_on(async {
        let env = Some(EnvironmentId::new());
        let token = OAuth2Token {
            access_token: "tok".into(),
            ..OAuth2Token::default()
        };
        svc.store_token("ref", env, Some(token.clone()))
            .await
            .expect("store");
        assert_eq!(svc.cached_token("ref", env).await, Some(token.clone()));
        assert_eq!(svc.cached_token("ref", None).await, None);
        assert!(store.dump().contains_key(&token_account("ref", env)));
        // A fresh process reads it back from the vault.
        assert_eq!(svc2.cached_token("ref", env).await, Some(token));
        svc.store_token("ref", env, None).await.expect("forget");
        assert_eq!(svc.cached_token("ref", env).await, None);
    });
    drop(svc2);
    let _ = std::fs::remove_dir_all(dir2);
    let _ = std::fs::remove_dir_all(dir);
}
