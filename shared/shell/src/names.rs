//! Which names a command may have.

use std::path::Path;

use crate::apps::{APPS, by_id};
use crate::prefs::SharedPrefs;

/// Shortest and longest command name.
pub const NAME_LEN: std::ops::RangeInclusive<usize> = 2..=20;

/// Checks the form of a command name: 2–20 lowercase letters, digits and
/// `-`, starting with a letter.
pub fn validate(name: &str) -> Result<(), String> {
    let valid = NAME_LEN.contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{name}` can't be a command: use 2–20 lowercase letters, digits or -, starting with a letter"
        ))
    }
}

/// Names that belong to something else in `~/.ferrum/bin`: the `ferrum`
/// command and the apps' own CLI links.
fn reserved(name: &str) -> bool {
    name == crate::umbrella::NAME
        || APPS.iter().any(|app| app.binary == name)
        || matches!(name, "fterm" | "ferrumdock-cli" | "ferrum-toolbox")
}

/// Checks `name` as `app_id`'s command: its form, that no other Ferrum app
/// has it (another app's chosen name, or its suggested one while it has
/// none) and that `bin_dir` holds no file of that name Ferrum didn't write.
pub fn check(app_id: &str, name: &str, prefs: &SharedPrefs, bin_dir: &Path) -> Result<(), String> {
    validate(name)?;
    if reserved(name) {
        return Err(format!("`{name}` is taken by Ferrum itself"));
    }
    if let Some(other) = APPS
        .iter()
        .filter(|other| other.id != app_id)
        .find(|other| prefs.command(other.id).name == name)
    {
        return Err(format!("`{name}` is {}'s command", other.name));
    }
    let file = bin_dir.join(name);
    if (file.exists() || file.is_symlink()) && !crate::script::is_ours(&file) {
        return Err(format!(
            "{} already exists and wasn't made by Ferrum",
            file.display()
        ));
    }
    if by_id(app_id).is_none() {
        return Err(format!("`{app_id}` has no shell command"));
    }
    Ok(())
}
