//! Headless tests of loadable themes and file icons: the theme selector's
//! live preview / revert / apply-and-save, bad theme files, the icon theme
//! selector, and icons resolved for tree view rows.

use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::settings::Settings;
use crate::tests::{init_with_user_dir, primary};
use crate::workspace::Workspace;

fn theme_name(cx: &mut VisualTestContext) -> String {
    cx.read(|cx| cx.global::<Settings>().theme_name())
}

fn icon_theme_name(cx: &mut VisualTestContext) -> String {
    cx.read(|cx| cx.global::<Settings>().icon_theme_name())
}

fn config_text(home: &Path) -> String {
    std::fs::read_to_string(home.join("editor/config.toml")).expect("config.toml")
}

fn run(workspace: &Entity<Workspace>, name: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |w, window, cx| w.run_command(name, window, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn theme_selector_previews_reverts_and_applies(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    let themes = home.path().join("editor/themes");
    std::fs::create_dir_all(&themes).expect("mkdir");
    std::fs::write(
        themes.join("ocean.toml"),
        "appearance = \"dark\"\n[ui]\naccent = \"#00aaff\"\n[terminal]\nred = \"#ff0011\"\n",
    )
    .expect("write");
    std::fs::write(
        themes.join("paper.json"),
        r##"{ "name": "Paper", "type": "light", // VS Code theme
             "colors": { "editor.background": "#fafafa", }, }"##,
    )
    .expect("write");
    init_with_user_dir(cx, home.path());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    cx.run_until_parked();
    assert_eq!(theme_name(cx), "ferrum-dark");

    // Moving the selection previews; Escape restores and saves nothing.
    cx.simulate_keystrokes(&primary("cmd-k cmd-t"));
    assert!(workspace.read_with(cx, |w, _| w.picker_open()));
    cx.simulate_keystrokes("down");
    assert_eq!(theme_name(cx), "ferrum-light");
    // Shorter names list first (the bundled Material themes are longer).
    cx.simulate_keystrokes("up up up");
    assert_eq!(theme_name(cx), "ocean");
    cx.simulate_keystrokes("escape");
    assert!(!workspace.read_with(cx, |w, _| w.picker_open()));
    assert_eq!(theme_name(cx), "ferrum-dark");
    assert!(config_text(home.path()).contains("theme = \"ferrum-dark\""));

    // Filtering previews the match; Enter applies and saves it.
    run(&workspace, "theme:select", cx);
    cx.simulate_input("paper");
    assert_eq!(theme_name(cx), "Paper");
    cx.simulate_keystrokes("enter");
    assert_eq!(theme_name(cx), "Paper");
    let light = cx.read(|cx| !cx.global::<Settings>().theme.dark);
    assert!(light);
    assert!(config_text(home.path()).contains("theme = \"Paper\""));
    let (config, _) =
        ferrumeditor_core::config::Config::parse(&config_text(home.path())).expect("valid");
    assert_eq!(config.theme, "Paper");

    // A broken theme file is a notice, not a crash; the rest still load.
    std::fs::write(themes.join("broken.json"), "{ nope").expect("write");
    workspace.update(cx, |w, cx| w.reload_settings(cx));
    let problems = cx.read(|cx| cx.global::<Settings>().problems.clone());
    assert!(problems.iter().any(|p| p.starts_with("themes/broken.json")));
    assert_eq!(theme_name(cx), "Paper");

    // Installing from a file copies it in and switches to it.
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let file = elsewhere.path().join("forest.toml");
    std::fs::write(&file, "name = \"Forest\"\n[ui]\naccent = \"#228b22\"\n").expect("write");
    workspace.update(cx, |w, cx| w.install_theme_file(&file, cx));
    assert!(themes.join("forest.toml").exists());
    assert_eq!(theme_name(cx), "Forest");
    assert!(config_text(home.path()).contains("theme = \"Forest\""));
}

#[gpui::test]
fn icon_theme_selector_previews_and_reverts(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    init_with_user_dir(cx, home.path());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    cx.run_until_parked();
    assert_eq!(icon_theme_name(cx), "ferrum");
    // The example icon theme was written on first run.
    run(&workspace, "icon-theme:select", cx);
    cx.simulate_keystrokes("down");
    assert_eq!(icon_theme_name(cx), "example");
    cx.simulate_keystrokes("escape");
    assert_eq!(icon_theme_name(cx), "ferrum");
    run(&workspace, "icon-theme:select", cx);
    cx.simulate_input("example");
    cx.simulate_keystrokes("enter");
    assert_eq!(icon_theme_name(cx), "example");
    assert!(config_text(home.path()).contains("icon_theme = \"example\""));
}

#[gpui::test]
fn tree_view_rows_resolve_file_icons(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    let icons = home.path().join("editor/icon-themes/mono");
    std::fs::create_dir_all(&icons).expect("mkdir");
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"/>"#;
    for name in ["r.svg", "f.svg"] {
        std::fs::write(icons.join(name), svg).expect("write");
    }
    std::fs::write(
        icons.join("theme.toml"),
        "[defaults]\nfile = \"f.svg\"\n[extensions]\nrs = \"r.svg\"\n",
    )
    .expect("write");
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(project.path().join("src")).expect("mkdir");
    for (name, text) in [
        ("Cargo.toml", "[package]\n"),
        ("README.md", "# hi\n"),
        ("notes.xyz", ""),
        ("src/main.rs", "fn main() {}\n"),
    ] {
        std::fs::write(project.path().join(name), text).expect("write");
    }
    init_with_user_dir(cx, home.path());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    let root = project.path().to_path_buf();
    workspace.update_in(cx, |w, window, cx| w.set_root(root.clone(), window, cx));
    cx.run_until_parked();
    let tree = workspace.read_with(cx, |w, _| w.tree()).expect("tree view");
    let icon = |name: &str, cx: &mut VisualTestContext| {
        tree.read_with(cx, |t, cx| t.visible_icon_ids(cx))
            .into_iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, id)| id)
    };
    assert_eq!(icon("Cargo.toml", cx).as_deref(), Some("cargo"));
    assert_eq!(icon("README.md", cx).as_deref(), Some("readme"));
    assert_eq!(icon("notes.xyz", cx).as_deref(), Some("file"));
    assert_eq!(icon("src", cx).as_deref(), Some("folder"));
    let src = root.canonicalize().expect("canonical").join("src");
    tree.update(cx, |t, cx| t.toggle_path(&src, cx));
    assert_eq!(icon("src", cx).as_deref(), Some("folder-open"));
    assert_eq!(icon("main.rs", cx).as_deref(), Some("rust"));

    // A user icon theme: its own mappings and default win; what it lacks
    // (folders) comes from the built-in set.
    cx.update(|_, cx| {
        assert!(cx.global_mut::<Settings>().set_icon_theme("mono"));
    });
    assert_eq!(icon("main.rs", cx).as_deref(), Some("r.svg"));
    assert_eq!(icon("Cargo.toml", cx).as_deref(), Some("f.svg"));
    assert_eq!(icon("src", cx).as_deref(), Some("folder-open"));

    // Icons can be turned off.
    cx.update(|_, cx| cx.global_mut::<Settings>().config.show_file_icons = false);
    assert_eq!(icon("main.rs", cx), None);
}

#[gpui::test]
fn material_themes_and_vscode_import(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    init_with_user_dir(cx, home.path());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    cx.run_until_parked();

    // The bundled Material themes are in the selector.
    run(&workspace, "theme:select", cx);
    cx.simulate_input("palenight");
    cx.simulate_keystrokes("enter");
    assert_eq!(theme_name(cx), "Material Palenight");
    assert!(config_text(home.path()).contains("theme = \"Material Palenight\""));
    // …and the Material icons.
    run(&workspace, "icon-theme:select", cx);
    cx.simulate_input("material");
    cx.simulate_keystrokes("enter");
    assert_eq!(icon_theme_name(cx), "material");

    // A VS Code theme with an `include` is converted into themes/.
    let elsewhere = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        elsewhere.path().join("base.json"),
        r##"{ "colors": { "sideBar.background": "#e8e8e8" }, }"##,
    )
    .expect("write");
    let theme = elsewhere.path().join("fixture-color-theme.json");
    std::fs::write(
        &theme,
        r##"{ "name": "Fixture", "type": "light", "include": "./base.json",
             // comments and trailing commas are fine
             "colors": { "editor.background": "#fcfcfc", },
             "tokenColors": [{ "scope": "keyword", "settings": { "foreground": "#0000ff" } }], }"##,
    )
    .expect("write");
    workspace.update(cx, |w, cx| w.import_vscode_theme(&theme, cx));
    assert_eq!(theme_name(cx), "Fixture");
    assert!(home.path().join("editor/themes/fixture.toml").is_file());
    assert!(config_text(home.path()).contains("theme = \"Fixture\""));
    cx.read(|cx| {
        let theme = &cx.global::<Settings>().theme;
        assert!(!theme.dark);
        assert_eq!(theme.ui("editor").to_hex(), "#fcfcfc");
        assert_eq!(theme.ui("background").to_hex(), "#e8e8e8");
        assert_eq!(
            theme.syntax("keyword").map(|c| c.to_hex()).as_deref(),
            Some("#0000ff")
        );
    });
    // Not a theme: a notice, nothing changes.
    let junk = elsewhere.path().join("junk.json");
    std::fs::write(&junk, "[1, 2]").expect("write");
    workspace.update(cx, |w, cx| w.import_vscode_theme(&junk, cx));
    assert_eq!(theme_name(cx), "Fixture");
}

/// Every bundled token theme keeps the chrome readable once bridged to the
/// UI theme: text, muted, accent and status text at 4.5:1 on the window
/// background and on the status tints, filled-chip text on status fills.
#[gpui::test]
fn every_bundled_theme_keeps_text_readable(cx: &mut TestAppContext) {
    use ferrum_ui::theme::ActiveTheme;
    use ferrum_ui::theme::contrast::{MIN_TEXT_CONTRAST, contrast_over};
    use ferrumeditor_core::theme::registry::ThemeRegistry;

    let registry = ThemeRegistry::builtins();
    let mut failures = Vec::new();
    for entry in registry.entries() {
        let theme = registry.get(&entry.name).expect("listed theme");
        cx.update(|cx| {
            crate::theme_apply::apply(&theme, cx);
            let ui = cx.theme().clone();
            let c = ui.colors();
            let base = c.background;
            let mut check = |what: &str, fg, bg| {
                let ratio = contrast_over(fg, bg, base);
                if ratio < MIN_TEXT_CONTRAST {
                    failures.push(format!("{}: {what} {ratio:.2}", entry.name));
                }
            };
            check("text", c.text, base);
            check("muted", c.text_muted, base);
            check("accent", c.text_accent, base);
            check(
                "accent fill",
                ui.readable_text_on(c.text_accent),
                c.text_accent,
            );
            for (name, fg, bg) in ui.status().pairs() {
                check(name, fg, base);
                check(&format!("{name} tint"), fg, bg);
                check(&format!("text on {name} tint"), c.text, bg);
            }
        });
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
