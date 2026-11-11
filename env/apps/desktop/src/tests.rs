//! Headless GPUI tests (no window is shown): first run, lock / unlock /
//! auto-lock, the clipboard clear, sharing a bundle between two vaults,
//! `.env` import / export and changing the master password. Argon2 runs
//! with cheap test parameters.

use std::path::Path;
use std::time::{Duration, Instant};

use ferrum_ui::theme::{self, ThemeSettingsProvider, UiDensity};
use ferrum_ui::{TextInput, ToggleProjectSwitcher};
use ferrumenv_core::Variable;
use ferrumenv_core::crypto::KdfParams;
use ferrumenv_core::diff::ChangeKind;
use gpui::{App, Entity, Font, Pixels, TestAppContext, VisualTestContext, font, px};

use crate::app::{EnvApp, EnvConfig, Screen};
use crate::dialogs::Dialog;
use crate::unlocked::Unlocked;

struct TestFonts {
    ui: Font,
    buffer: Font,
}

impl ThemeSettingsProvider for TestFonts {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui
    }
    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.buffer
    }
    fn ui_font_size(&self, _: &App) -> Pixels {
        px(14.)
    }
    fn buffer_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }
    fn ui_density(&self, _: &App) -> UiDensity {
        UiDensity::Default
    }
}

/// The real app gets the text input bindings from `ferrum_ui::run`; bind
/// the ones these tests type with, by action name (the types are private).
fn bind_text_input_keys(cx: &mut App) {
    let predicate = gpui::KeyBindingContextPredicate::parse("FerrumTextInput").expect("predicate");
    let predicate = std::rc::Rc::new(predicate);
    let mut bindings = Vec::new();
    for (keys, action) in [
        ("enter", "text_input::Enter"),
        ("backspace", "text_input::Backspace"),
    ] {
        let action = cx.build_action(action, None).expect("text input action");
        bindings.push(
            gpui::KeyBinding::load(
                keys,
                action,
                Some(predicate.clone()),
                false,
                None,
                &gpui::DummyKeyboardMapper,
            )
            .expect("binding"),
        );
    }
    cx.bind_keys(bindings);
    cx.bind_keys([gpui::KeyBinding::new("escape", menu::Cancel, None)]);
}

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        theme::set_theme_settings_provider(
            Box::new(TestFonts {
                ui: font("Helvetica"),
                buffer: font("Menlo"),
            }),
            cx,
        );
        theme::init(theme::LoadThemes::JustBase, cx);
        bind_text_input_keys(cx);
        crate::app::bind_keys(cx);
    });
}

/// Opens an app window on `vault` with its own test context, so several
/// windows (two developers) can live in one test.
fn open(cx: &mut TestAppContext, vault: &Path) -> (Entity<EnvApp>, VisualTestContext) {
    let config = EnvConfig {
        vault_path: vault.to_path_buf(),
        kdf: KdfParams::INSECURE_FAST,
        record_recent: false,
    };
    let window = cx.add_window(|window, cx| EnvApp::new(config, window, cx));
    let app = window.root(cx).expect("root view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn set(input: &Entity<TextInput>, text: &str, cx: &mut VisualTestContext) {
    input.update(cx, |input, cx| input.set_text(text, cx));
}

/// Types the password(s) on the lock screen and submits.
fn submit_password(
    app: &Entity<EnvApp>,
    password: &str,
    confirm: &str,
    cx: &mut VisualTestContext,
) {
    let (pw, repeat) = app.read_with(cx, |a, _| (a.form.password.clone(), a.form.confirm.clone()));
    set(&pw, password, cx);
    set(&repeat, confirm, cx);
    app.update_in(cx, |a, window, cx| a.submit_lock_form(window, cx));
    cx.run_until_parked();
}

fn screen_name(app: &Entity<EnvApp>, cx: &mut VisualTestContext) -> &'static str {
    app.read_with(cx, |a, _| match a.screen {
        Screen::Setup => "setup",
        Screen::Locked => "locked",
        Screen::Unlocked(_) => "unlocked",
    })
}

fn form_error(app: &Entity<EnvApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |a, _| {
        a.form
            .error
            .clone()
            .map(|e| e.to_string())
            .unwrap_or_default()
    })
}

fn dialog_error(app: &Entity<EnvApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |a, _| {
        a.vault()
            .and_then(|u| u.dialog_error.clone())
            .map(|e| e.to_string())
            .unwrap_or_default()
    })
}

fn with_vault<R>(
    app: &Entity<EnvApp>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&Unlocked) -> R,
) -> R {
    app.read_with(cx, |a, _| f(a.vault().expect("unlocked")))
}

/// Adds `project` with `vars` in its `env` environment, selected.
fn add_project(
    app: &Entity<EnvApp>,
    project: &str,
    env: &str,
    vars: &[(&str, &str)],
    cx: &mut VisualTestContext,
) {
    let vars: Vec<Variable> = vars.iter().map(|(k, v)| Variable::new(*k, *v)).collect();
    let project = project.to_string();
    let env = env.to_string();
    app.update_in(cx, |a, window, cx| {
        let index = a
            .change(cx, |u| {
                let i = u
                    .vault
                    .add_project(&project, None)
                    .map_err(|e| e.to_string())?;
                let e = u.vault.projects[i].environment_mut(&env);
                for var in vars {
                    e.upsert(var);
                }
                Ok(i)
            })
            .expect("project added");
        a.select_project(index, window, cx);
        let env_index = a
            .vault()
            .and_then(|u| u.project())
            .and_then(|p| p.environment_index(&env))
            .expect("env");
        a.select_environment(env_index, cx);
    });
}

const PASSWORD: &str = "correct horse battery";

#[gpui::test]
fn first_run_lock_and_unlock(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("env/vault.json");
    let (app, mut vcx) = open(cx, &vault);
    let cx = &mut vcx;
    assert_eq!(screen_name(&app, cx), "setup");

    // Too short, then mismatched: refused with a message.
    submit_password(&app, "short", "short", cx);
    assert!(form_error(&app, cx).contains("at least"));
    submit_password(&app, PASSWORD, "correct horse batterY", cx);
    assert!(form_error(&app, cx).contains("match"));
    assert_eq!(screen_name(&app, cx), "setup");
    submit_password(&app, PASSWORD, PASSWORD, cx);
    assert_eq!(screen_name(&app, cx), "unlocked");
    assert!(vault.exists());

    // New project through its dialog, typed and submitted with Enter.
    app.update_in(cx, |a, window, cx| a.open_new_project(window, cx));
    cx.simulate_input("shop");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        with_vault(&app, cx, |u| u.project().map(|p| p.name.clone())).as_deref(),
        Some("shop")
    );

    // ⌘N opens the variable dialog for the current environment.
    cx.simulate_keystrokes("cmd-n");
    let (key, value) = with_vault(&app, cx, |u| match &u.dialog {
        Some(Dialog::Variable(d)) => (d.key.clone(), d.value.clone()),
        _ => panic!("variable dialog not open"),
    });
    set(&key, "STRIPE_KEY", cx);
    set(&value, "sk_live_123", cx);
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    let var = with_vault(&app, cx, |u| {
        u.environment().and_then(|e| e.get("STRIPE_KEY")).cloned()
    });
    let var = var.expect("variable saved");
    assert_eq!(var.value, "sk_live_123");
    assert!(var.secret, "values are masked by default");
    assert!(with_vault(&app, cx, |u| u.dialog.is_none()));
    // A duplicate key is refused inside the dialog.
    cx.simulate_keystrokes("cmd-n");
    let key = with_vault(&app, cx, |u| match &u.dialog {
        Some(Dialog::Variable(d)) => d.key.clone(),
        _ => panic!("variable dialog not open"),
    });
    set(&key, "STRIPE_KEY", cx);
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    assert!(dialog_error(&app, cx).contains("already exists"));
    cx.simulate_keystrokes("escape");
    assert!(with_vault(&app, cx, |u| u.dialog.is_none()));

    // The project switcher action is forwarded to the strip without fuss.
    cx.dispatch_action(ToggleProjectSwitcher);

    let on_disk = std::fs::read_to_string(&vault).expect("vault");
    assert!(!on_disk.contains("sk_live_123") && !on_disk.contains("STRIPE_KEY"));

    // ⌘L locks: everything decrypted is gone.
    cx.simulate_keystrokes("cmd-l");
    assert_eq!(screen_name(&app, cx), "locked");
    assert!(app.read_with(cx, |a, _| a.vault().is_none()));

    submit_password(&app, "wrong horse battery", "", cx);
    assert_eq!(screen_name(&app, cx), "locked");
    assert!(form_error(&app, cx).contains("Wrong master password"));
    assert!(app.read_with(cx, |a, cx| a.form.password.read(cx).text().is_empty()));

    submit_password(&app, PASSWORD, "", cx);
    assert_eq!(screen_name(&app, cx), "unlocked");
    let value = with_vault(&app, cx, |u| {
        u.project()
            .and_then(|p| p.environments.iter().find_map(|e| e.get("STRIPE_KEY")))
            .map(|v| v.value.clone())
    });
    assert_eq!(value.as_deref(), Some("sk_live_123"));

    // Restarting the app finds the vault and asks for the password.
    let (app, mut restarted) = open(&mut cx.cx, &vault);
    assert_eq!(screen_name(&app, &mut restarted), "locked");
}

#[gpui::test]
fn auto_lock_after_inactivity(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let (app, mut vcx) = open(cx, &dir.path().join("vault.json"));
    let cx = &mut vcx;
    submit_password(&app, PASSWORD, PASSWORD, cx);
    assert_eq!(
        with_vault(&app, cx, |u| u.auto_lock.label(Instant::now())),
        "Locks in 29 min"
    );

    // Activity before the deadline keeps it open.
    app.update_in(cx, |a, window, cx| {
        a.check_auto_lock(Instant::now() + Duration::from_secs(29 * 60), window, cx);
    });
    assert_eq!(screen_name(&app, cx), "unlocked");

    app.update(cx, |a, cx| a.set_auto_lock(5, cx));
    assert_eq!(
        with_vault(&app, cx, |u| u.vault.settings.auto_lock_minutes),
        5
    );
    app.update_in(cx, |a, window, cx| {
        a.check_auto_lock(Instant::now() + Duration::from_secs(6 * 60), window, cx);
    });
    assert_eq!(screen_name(&app, cx), "locked");
    assert!(form_error(&app, cx).contains("inactivity"));

    // The setting was saved in the vault.
    submit_password(&app, PASSWORD, "", cx);
    assert_eq!(
        with_vault(&app, cx, |u| u.vault.settings.auto_lock_minutes),
        5
    );
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui::test]
fn copied_values_are_cleared_from_the_clipboard(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let (app, mut vcx) = open(cx, &dir.path().join("vault.json"));
    let cx = &mut vcx;
    submit_password(&app, PASSWORD, PASSWORD, cx);
    add_project(&app, "shop", "development", &[("TOKEN", "t0k3n")], cx);

    app.update(cx, |a, cx| a.copy_variable(0, cx));
    assert_eq!(clipboard(cx).as_deref(), Some("t0k3n"));
    cx.executor().advance_clock(Duration::from_secs(29));
    cx.run_until_parked();
    assert_eq!(clipboard(cx).as_deref(), Some("t0k3n"));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(clipboard(cx).unwrap_or_default(), "");

    // Something else copied meanwhile is left alone.
    app.update(cx, |a, cx| a.copy_variable(0, cx));
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("mine".into())));
    cx.executor().advance_clock(Duration::from_secs(31));
    cx.run_until_parked();
    assert_eq!(clipboard(cx).as_deref(), Some("mine"));

    // Locking clears a copied secret at once.
    app.update(cx, |a, cx| a.copy_variable(0, cx));
    assert!(app.read_with(cx, |a, _| a.clipboard.is_armed()));
    cx.simulate_keystrokes("cmd-l");
    assert_eq!(clipboard(cx).unwrap_or_default(), "");
}

fn share_key_text(app: &Entity<EnvApp>, cx: &mut VisualTestContext) -> String {
    with_vault(app, cx, |u| {
        u.project()
            .and_then(|p| p.share_key.as_ref())
            .map(|k| k.encode().to_string())
            .expect("share key")
    })
}

/// Creates a bundle of the current project from the Share dialog.
fn make_bundle(app: &Entity<EnvApp>, cx: &mut VisualTestContext) -> String {
    app.update_in(cx, |a, window, cx| a.open_share(window, cx));
    app.update(cx, |a, cx| a.create_bundle(cx));
    with_vault(app, cx, |u| match &u.dialog {
        Some(Dialog::Share(d)) => d.output.clone().expect("bundle"),
        _ => panic!("share dialog not open"),
    })
}

fn import_inputs(
    app: &Entity<EnvApp>,
    cx: &mut VisualTestContext,
) -> (Entity<TextInput>, Entity<TextInput>) {
    with_vault(app, cx, |u| match &u.dialog {
        Some(Dialog::ImportBundle(d)) => (d.text.clone(), d.key.clone()),
        _ => panic!("import dialog not open"),
    })
}

#[gpui::test]
fn bundles_move_environments_between_vaults(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let (alice, mut acx) = open(cx, &dir.path().join("alice.json"));
    let (bob, mut bcx) = open(cx, &dir.path().join("bob.json"));
    let cx = &mut acx;
    submit_password(&alice, PASSWORD, PASSWORD, cx);
    add_project(
        &alice,
        "shop",
        "staging",
        &[("STRIPE_KEY", "sk_1"), ("PORT", "80"), ("OLD", "x")],
        cx,
    );
    alice.update_in(cx, |a, window, cx| a.generate_share_key(window, cx));
    let key = share_key_text(&alice, cx);
    let armored = make_bundle(&alice, cx);
    assert!(armored.contains("BEGIN FERRUMENV BUNDLE"));
    assert!(!armored.contains("sk_1") && !armored.contains(&key));

    let alice_cx = cx;
    let cx = &mut bcx;
    submit_password(&bob, "another long password", "another long password", cx);
    bob.update_in(cx, |a, window, cx| a.open_import_bundle(window, cx));
    let (text, key_input) = import_inputs(&bob, cx);
    set(&text, &armored, cx);
    bob.update(cx, |a, cx| a.preview_bundle(cx));
    assert!(dialog_error(&bob, cx).contains("Enter the share key"));

    // A typo in the key is caught by its checksum.
    let typo = format!("{}x", &key[..key.len() - 1]);
    set(&key_input, &typo, cx);
    bob.update(cx, |a, cx| a.preview_bundle(cx));
    assert!(dialog_error(&bob, cx).contains("Invalid share key"));

    set(&key_input, &key, cx);
    bob.update(cx, |a, cx| a.preview_bundle(cx));
    assert_eq!(dialog_error(&bob, cx), "");
    let added = with_vault(&bob, cx, |u| match &u.dialog {
        Some(Dialog::ImportBundle(d)) => d.preview.as_ref().map(|p| {
            p.plans
                .iter()
                .map(|plan| plan.count(ChangeKind::Added))
                .sum::<usize>()
        }),
        _ => None,
    });
    assert_eq!(added, Some(3));
    bob.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    let (value, bob_key) = with_vault(&bob, cx, |u| {
        let p = u.project().expect("imported project");
        let env = &p.environments[p.environment_index("staging").expect("staging")];
        (
            env.get("STRIPE_KEY").map(|v| v.value.clone()),
            p.share_key.as_ref().map(|k| k.encode().to_string()),
        )
    });
    assert_eq!(value.as_deref(), Some("sk_1"));
    assert_eq!(bob_key.as_deref(), Some(key.as_str()));

    // Alice updates; Bob's stored key opens the next bundle, and the
    // preview shows what changes.
    alice.update(alice_cx, |a, cx| {
        a.change(cx, |u| {
            let env = u.environment_mut().ok_or("env")?;
            env.upsert(Variable::new("STRIPE_KEY", "sk_2"));
            env.upsert(Variable::new("SENTRY_DSN", "https://x"));
            env.variables.retain(|v| v.key != "OLD");
            Ok(())
        })
        .expect("changed");
    });
    let armored = make_bundle(&alice, alice_cx);
    bob.update_in(cx, |a, window, cx| a.open_import_bundle(window, cx));
    let (text, _) = import_inputs(&bob, cx);
    set(&text, &armored, cx);
    bob.update(cx, |a, cx| a.preview_bundle(cx));
    let counts = with_vault(&bob, cx, |u| match &u.dialog {
        Some(Dialog::ImportBundle(d)) => d.preview.as_ref().map(|p| {
            let plan = p
                .plans
                .iter()
                .find(|plan| plan.name == "staging")
                .expect("staging");
            (
                plan.count(ChangeKind::Added),
                plan.count(ChangeKind::Changed),
                plan.count(ChangeKind::Removed),
            )
        }),
        _ => None,
    });
    assert_eq!(counts, Some((1, 1, 1)));
    bob.update(cx, |a, cx| {
        if let Some(Dialog::ImportBundle(d)) = a.dialog_mut() {
            d.remove_missing = true;
        }
        cx.notify();
    });
    bob.update_in(cx, |a, window, cx| a.import_bundle(window, cx));
    let keys = with_vault(&bob, cx, |u| {
        let p = u.project().expect("project");
        let env = &p.environments[p.environment_index("staging").expect("staging")];
        env.variables
            .iter()
            .map(|v| v.key.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(keys, ["STRIPE_KEY", "PORT", "SENTRY_DSN"]);

    // After Alice rotates the key, Bob's old key is reported as different.
    alice.update_in(alice_cx, |a, window, cx| a.generate_share_key(window, cx));
    let armored = make_bundle(&alice, alice_cx);
    bob.update_in(cx, |a, window, cx| a.open_import_bundle(window, cx));
    let (text, _) = import_inputs(&bob, cx);
    set(&text, &armored, cx);
    bob.update(cx, |a, cx| a.preview_bundle(cx));
    assert!(dialog_error(&bob, cx).contains("different share key"));
}

#[gpui::test]
fn dotenv_files_import_and_export(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("web-app");
    std::fs::create_dir_all(&folder).expect("mkdir");
    std::fs::write(
        folder.join(".env"),
        "# local db\nDATABASE_URL=postgres://localhost/app\nexport DEBUG=1\n",
    )
    .expect("write");
    std::fs::write(
        folder.join(".env.production"),
        "DATABASE_URL=\"postgres://prod/app\"\n",
    )
    .expect("write");
    std::fs::write(folder.join(".gitignore"), "node_modules\n.env\n").expect("write");

    let (app, mut vcx) = open(cx, &dir.path().join("vault.json"));
    let cx = &mut vcx;
    submit_password(&app, PASSWORD, PASSWORD, cx);
    // A folder picked in the project strip becomes (or selects) a project.
    app.update_in(cx, |a, window, cx| a.open_folder(&folder, window, cx));
    app.update_in(cx, |a, window, cx| a.open_folder(&folder, window, cx));
    let (name, count) = with_vault(&app, cx, |u| {
        (u.project().map(|p| p.name.clone()), u.vault.projects.len())
    });
    assert_eq!((name.as_deref(), count), (Some("web-app"), 1));

    app.update_in(cx, |a, window, cx| a.open_import_dotenv(window, cx));
    let found = with_vault(&app, cx, |u| match &u.dialog {
        Some(Dialog::ImportDotenv(d)) => d
            .files
            .iter()
            .map(|f| f.environment.clone())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    });
    assert_eq!(found, ["development", "production"]);
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    let (dev, prod) = with_vault(&app, cx, |u| {
        let p = u.project().expect("project");
        let get = |env: &str, key: &str| {
            p.environments[p.environment_index(env).expect("env")]
                .get(key)
                .map(|v| (v.value.clone(), v.comment.clone()))
        };
        (
            get("development", "DATABASE_URL"),
            get("production", "DATABASE_URL"),
        )
    });
    assert_eq!(
        dev,
        Some(("postgres://localhost/app".into(), "local db".into()))
    );
    assert_eq!(prod.map(|(v, _)| v).as_deref(), Some("postgres://prod/app"));

    // Write production back: the file exists, so the first try asks.
    let prod_index = with_vault(&app, cx, |u| {
        u.project().and_then(|p| p.environment_index("production"))
    });
    app.update(cx, |a, cx| {
        a.select_environment(prod_index.expect("production"), cx)
    });
    app.update(cx, |a, cx| {
        a.change(cx, |u| {
            u.environment_mut()
                .ok_or("env")?
                .upsert(Variable::new("API_SECRET", "it's \"q\""));
            Ok(())
        });
    });
    app.update_in(cx, |a, window, cx| a.open_export_dotenv(window, cx));
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    assert!(dialog_error(&app, cx).contains("already exists"));
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    assert!(with_vault(&app, cx, |u| u.dialog.is_none()));
    let written = std::fs::read_to_string(folder.join(".env.production")).expect("written");
    let parsed = ferrumenv_core::dotenv::parse(&written);
    let expected = with_vault(&app, cx, |u| u.environment().map(|e| e.variables.clone()));
    assert_eq!(Some(parsed.variables), expected);
    let ignore = std::fs::read_to_string(folder.join(".gitignore")).expect("gitignore");
    assert!(ignore.lines().any(|l| l == ".env.production"));
}

#[gpui::test]
fn changing_the_master_password(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.json");
    let (app, mut vcx) = open(cx, &vault);
    let cx = &mut vcx;
    submit_password(&app, PASSWORD, PASSWORD, cx);
    add_project(&app, "shop", "development", &[("A", "1")], cx);
    app.update_in(cx, |a, window, cx| a.open_settings(window, cx));
    let inputs = with_vault(&app, cx, |u| match &u.dialog {
        Some(Dialog::Settings(d)) => (d.current.clone(), d.new.clone(), d.confirm.clone()),
        _ => panic!("settings not open"),
    });
    set(&inputs.0, "not the password", cx);
    set(&inputs.1, "a brand new password", cx);
    set(&inputs.2, "a brand new password", cx);
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    cx.run_until_parked();
    assert!(dialog_error(&app, cx).contains("Wrong master password"));

    set(&inputs.0, PASSWORD, cx);
    app.update_in(cx, |a, window, cx| a.submit_dialog(window, cx));
    cx.run_until_parked();
    assert_eq!(dialog_error(&app, cx), "");

    cx.simulate_keystrokes("cmd-l");
    submit_password(&app, PASSWORD, "", cx);
    assert_eq!(screen_name(&app, cx), "locked");
    submit_password(&app, "a brand new password", "", cx);
    assert_eq!(screen_name(&app, cx), "unlocked");
    assert_eq!(with_vault(&app, cx, |u| u.vault.projects.len()), 1);
}
