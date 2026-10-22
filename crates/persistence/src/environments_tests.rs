use crate::test_util::{exec, mem};
use ferrumapi_domain::{Environment, EnvironmentId, Variable};

#[tokio::test]
async fn crud_ordered_and_globals_excluded() {
    let store = mem().await;
    let mut dev = Environment::new("dev");
    dev.variables.push(Variable::new("host", "localhost"));
    let prod = Environment::new("Prod");
    let alpha = Environment::new("alpha");
    for e in [&prod, &dev, &alpha] {
        store.save_environment(e).await.expect("save");
    }
    let mut g = Environment::globals();
    g.variables.push(Variable::new("token", "x"));
    store.save_globals(&g).await.expect("save globals");

    let list = store.list_environments().await.expect("list");
    assert_eq!(list, vec![alpha.clone(), dev.clone(), prod.clone()]);
    assert!(list.iter().all(|e| !e.id.is_globals()));

    dev.name = "Zeta".into();
    store.save_environment(&dev).await.expect("update");
    store.delete_environment(alpha.id).await.expect("delete");
    let list = store.list_environments().await.expect("list");
    assert_eq!(list, vec![prod, dev]);
}

#[tokio::test]
async fn globals_default_and_round_trip() {
    let store = mem().await;
    assert_eq!(
        store.load_globals().await.expect("load"),
        Environment::globals()
    );

    let mut g = Environment::globals();
    let mut secret = Variable::new("api_key", "s3cr3t");
    secret.secret = true;
    g.variables.push(secret);
    store.save_globals(&g).await.expect("save");
    store.save_globals(&g).await.expect("save twice upserts");
    assert_eq!(store.load_globals().await.expect("load"), g);

    // A wrong id passed in is forced to the globals id.
    let mut other = g.clone();
    other.id = EnvironmentId::new();
    other.variables.clear();
    store.save_globals(&other).await.expect("save");
    let loaded = store.load_globals().await.expect("load");
    assert_eq!(loaded.id, EnvironmentId::GLOBALS);
    assert!(loaded.variables.is_empty());
    assert!(store.list_environments().await.expect("list").is_empty());
}

#[tokio::test]
async fn corrupt_rows_skipped_or_defaulted() {
    let store = mem().await;
    let ok = Environment::new("ok");
    store.save_environment(&ok).await.expect("save");
    exec(
        &store,
        "INSERT INTO environments (id, name, data, created_at, updated_at)
         VALUES ('bad', 'a', '[1,2', '', '')",
    )
    .await;
    exec(
        &store,
        "INSERT INTO environments (id, name, data, created_at, updated_at)
         VALUES ('00000000-0000-0000-0000-000000000000', 'Globals', '{\"variables\": 7}', '', '')",
    )
    .await;
    assert_eq!(store.list_environments().await.expect("list"), vec![ok]);
    assert_eq!(
        store.load_globals().await.expect("load"),
        Environment::globals()
    );
}
