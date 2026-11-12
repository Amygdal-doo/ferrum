use crate::Store;
use crate::test_util::{TempDb, exec, mem};
use ferrumapi_domain::{
    AppSettings, EnvironmentId, HttpMethod, HttpRequest, RequestTab, Theme, WorkspaceState,
};

fn workspace() -> WorkspaceState {
    let tab = RequestTab::new("Users", HttpRequest::new(HttpMethod::Get, "{{base}}/users"));
    WorkspaceState {
        active_tab: Some(tab.id),
        tabs: vec![tab, RequestTab::new("Blank", HttpRequest::default())],
        active_environment: Some(EnvironmentId::new()),
        project: Some("/code/app".into()),
    }
}

#[tokio::test]
async fn defaults_then_round_trip() {
    let store = mem().await;
    assert_eq!(
        store.load_workspace().await.expect("load"),
        WorkspaceState::default()
    );
    assert_eq!(
        store.load_settings().await.expect("load"),
        AppSettings::default()
    );

    let ws = workspace();
    let s = AppSettings {
        theme: Theme::Dark,
        sidebar_width: 280.5,
        history_limit: 42,
    };
    store.save_workspace(&ws).await.expect("save");
    store.save_workspace(&ws).await.expect("save twice upserts");
    store.save_settings(&s).await.expect("save");
    assert_eq!(store.load_workspace().await.expect("load"), ws);
    assert_eq!(store.load_settings().await.expect("load"), s);
}

#[tokio::test]
async fn unreadable_values_default() {
    let store = mem().await;
    exec(
        &store,
        "INSERT INTO settings (key, value, updated_at) VALUES ('app', '{\"theme\": 5}', ''),
         ('workspace', 'garbage', '')",
    )
    .await;
    assert_eq!(
        store.load_settings().await.expect("load"),
        AppSettings::default()
    );
    assert_eq!(
        store.load_workspace().await.expect("load"),
        WorkspaceState::default()
    );
}

#[tokio::test]
async fn file_backed_reopen_keeps_everything() {
    let db = TempDb::new();
    let ws = workspace();
    let s = AppSettings {
        theme: Theme::Light,
        ..AppSettings::default()
    };
    {
        let store = Store::open(&db.path).await.expect("open");
        store.save_workspace(&ws).await.expect("save");
        store.save_settings(&s).await.expect("save");
    }
    let store = Store::open(&db.path).await.expect("reopen");
    assert_eq!(store.load_workspace().await.expect("load"), ws);
    assert_eq!(store.load_settings().await.expect("load"), s);
}
