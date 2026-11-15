//! The Ferrum apps that get a shell command: id (as in the launcher's
//! catalog), executable, display name, the word `ferrum <word>` takes and
//! the suggested short command.

/// One app of the suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// Catalog id (`editor`, `dock`, …); the key in `shell_commands`.
    pub id: &'static str,
    /// Executable, and the name of its CLI link in `~/.ferrum/bin`.
    pub binary: &'static str,
    pub name: &'static str,
    /// The word after `ferrum`: `ferrum ide .`.
    pub alias: &'static str,
    /// The suggested short command: "fe" plus the logo letter.
    pub default_command: &'static str,
    /// `<command> .` opens that folder (shown as usage).
    pub opens_folders: bool,
}

const fn app(
    id: &'static str,
    binary: &'static str,
    name: &'static str,
    alias: &'static str,
    default_command: &'static str,
    opens_folders: bool,
) -> App {
    App {
        id,
        binary,
        name,
        alias,
        default_command,
        opens_folders,
    }
}

/// Every app, in the order `ferrum list` shows them.
pub const APPS: [App; 12] = [
    app("editor", "ferrumeditor", "FerrumIDE", "ide", "fei", true),
    app(
        "vibe",
        "ferrumvibe",
        "FerrumEngineer",
        "engineer",
        "fen",
        true,
    ),
    app("git", "ferrumgit", "FerrumGIT", "git", "feg", true),
    app("term", "ferrumterm", "FerrumTerm", "term", "fet", true),
    app("api", "ferrumapi", "FerrumAPI", "api", "fea", false),
    app(
        "dock",
        "ferrumdock",
        "FerrumContainers",
        "containers",
        "fec",
        true,
    ),
    app("data", "ferrumdata", "FerrumData", "data", "fed", false),
    app(
        "env",
        "ferrumenv",
        "FerrumVariables",
        "variables",
        "fev",
        true,
    ),
    app("ports", "ferrumports", "FerrumPorts", "ports", "fep", false),
    app("kit", "ferrumkit", "FerrumKit", "kit", "fek", false),
    app(
        "system",
        "ferrumsystem",
        "FerrumSystem",
        "system",
        "fes",
        false,
    ),
    app(
        "launcher",
        "ferrum-launcher",
        "FerrumLauncher",
        "launcher",
        "fel",
        false,
    ),
];

/// The app with catalog id `id`.
#[must_use]
pub fn by_id(id: &str) -> Option<&'static App> {
    APPS.iter().find(|app| app.id == id)
}

/// The app whose executable is `binary` (what `ferrum-ui` knows).
#[must_use]
pub fn by_binary(binary: &str) -> Option<&'static App> {
    APPS.iter().find(|app| app.binary == binary)
}

impl App {
    /// What running `command` does, for Settings and notices.
    #[must_use]
    pub fn usage(&self, command: &str) -> String {
        match self.id {
            "term" => format!("`{command}` opens {} in the current folder", self.name),
            "dock" => format!(
                "`{command} .` opens {} on the current folder; `{command} start`, `stop`, `up`, \
                 `ps`… run the VM and docker",
                self.name
            ),
            _ if self.opens_folders => {
                format!("`{command} .` opens {} on the current folder", self.name)
            }
            _ => format!("`{command}` opens {}", self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_binaries_aliases_and_commands_are_unique() {
        for field in [
            |a: &App| a.id,
            |a: &App| a.binary,
            |a: &App| a.alias,
            |a: &App| a.default_command,
        ] {
            let mut seen = HashSet::new();
            for app in &APPS {
                assert!(seen.insert(field(app)), "{} twice", field(app));
            }
        }
        assert_eq!(by_binary("ferrumeditor").map(|a| a.id), Some("editor"));
        assert_eq!(by_id("dock").map(|a| a.alias), Some("containers"));
        assert!(by_id("agentic").is_none());
    }

    #[test]
    fn usage_says_what_the_command_does() {
        let ide = by_id("editor").expect("ide");
        assert_eq!(
            ide.usage("fei"),
            "`fei .` opens FerrumIDE on the current folder"
        );
        let api = by_id("api").expect("api");
        assert_eq!(api.usage("fea"), "`fea` opens FerrumAPI");
        assert!(
            by_id("term")
                .expect("term")
                .usage("fet")
                .contains("current folder")
        );
    }
}
