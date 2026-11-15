//! `ferrum <app> [path]`: one command for every app, so nobody has to
//! remember the short ones. `ferrum ide .` does what `fei .` does;
//! `ferrum list` shows the apps and their short commands, `ferrum help`
//! the usage. A script in `~/.ferrum/bin` like the per-app ones, rewritten
//! whenever a short command changes (its list shows them).

use std::path::Path;

use crate::apps::{APPS, App};
use crate::prefs::SharedPrefs;
use crate::script::{MARKER, launch, missing_check, quote};

/// The command's name.
pub const NAME: &str = "ferrum";

/// The words `ferrum <word>` takes for `app`: its alias (`ide`), its
/// catalog id (`editor`) and its suggested short command (`fei`).
#[must_use]
pub fn words(app: &App) -> Vec<&'static str> {
    let mut words = vec![app.alias, app.id, app.default_command];
    words.dedup();
    words.sort_unstable();
    words.dedup();
    words
}

/// The app `ferrum <word>` opens.
#[must_use]
pub fn lookup(word: &str) -> Option<&'static App> {
    APPS.iter().find(|app| words(app).contains(&word))
}

/// The `ferrum help` text.
#[must_use]
pub fn help() -> String {
    let mut text = String::from(
        "Usage: ferrum <app> [folder or arguments]\n\
         \n\
         Opens a Ferrum app, like its short command:\n",
    );
    for app in &APPS {
        text.push_str(&format!("  ferrum {:<11} {}\n", app.alias, app.name));
    }
    text.push_str(
        "\n\
         Examples:\n  ferrum ide .         open this folder in FerrumIDE\n  \
         ferrum term          FerrumTerm in this folder\n  \
         ferrum git ~/code/x  FerrumGIT on a repository\n\
         \n  ferrum list          the apps, their short commands and what is installed\n  \
         ferrum help          this text\n\
         \n\
         Short commands and this one are set in each app's Settings › System.\n",
    );
    text
}

/// The `ferrum list` lines: alias, app name, short command ("-" while off).
#[must_use]
pub fn list_rows(prefs: &SharedPrefs) -> Vec<(&'static str, &'static str, String)> {
    APPS.iter()
        .map(|app| {
            let short = prefs
                .enabled_command(app.id)
                .unwrap_or_else(|| "-".to_owned());
            (app.alias, app.name, short)
        })
        .collect()
}

/// The script. `bin_dir` holds the apps' CLI links.
#[must_use]
pub fn script(bin_dir: &Path, prefs: &SharedPrefs) -> String {
    let mut text = format!(
        "#!/bin/sh\n{MARKER}: `ferrum <app> [path]` for every Ferrum app (turn it off in any app's Settings › System).\n"
    );
    text.push_str(&format!("help() {{\ncat <<'EOF'\n{}EOF\n}}\n", help()));
    text.push_str("list() {\n  printf '%-12s %-18s %-8s %s\\n' APP NAME SHORT STATUS\n");
    for (alias, name, short) in list_rows(prefs) {
        let app = APPS
            .iter()
            .find(|app| app.alias == alias)
            .map_or("", |app| app.binary);
        let entry = quote(&bin_dir.join(app));
        text.push_str(&format!(
            "  if [ -x {entry} ]; then s=installed; else s='not installed'; fi\n  \
             printf '%-12s %-18s %-8s %s\\n' {alias} {name} {short} \"$s\"\n"
        ));
    }
    text.push_str("}\n");
    text.push_str(
        "case \"$1\" in\n  \"\"|help|-h|--help) help; exit 0 ;;\n  list|ls) list; exit 0 ;;\nesac\n\
         app=$1\nshift\ncase \"$app\" in\n",
    );
    for app in &APPS {
        let target = quote(&bin_dir.join(app.binary));
        let command = format!("ferrum {}", app.alias);
        let body = format!(
            "{}{}",
            missing_check(app, &command, &target),
            launch(app, &command, &target)
        );
        let body: String = body.lines().map(|line| format!("    {line}\n")).collect();
        text.push_str(&format!("  {})\n{body}    ;;\n", words(app).join("|")));
    }
    text.push_str(
        "  *)\n    echo \"ferrum: no app called '$app'. Try: ferrum list\" >&2\n    exit 2\n    ;;\nesac\n",
    );
    text
}
