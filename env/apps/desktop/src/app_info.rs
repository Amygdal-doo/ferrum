//! What FerrumVariables tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs and where users migrate from.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, Migration, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumVariables", "ferrumenv")
        .tagline("A password-locked vault for your projects' environment variables.")
        .icon(IconName::Lock, 0xc792ea)
        .key("New variable", "cmd-n")
        .key("Lock vault", "cmd-l")
        .key("Go to a project or variable", "cmd-p")
        .key("Find in variable names and comments", "cmd-shift-f")
        .key("Switch project", "cmd-alt-o")
        .permission(
            PermissionKind::FilesAndFolders,
            "Lets FerrumVariables read and write .env files in Documents, Desktop and iCloud.",
        )
        .migration(Migration {
            from: &["loose .env files", "1Password", "Doppler", "Infisical"],
            summary: "Variables come over as .env files you import into the vault.",
            task: "FerrumVariables imports .env files (Import .env in the app) into its encrypted vault (~/.ferrum/env/vault.json), which must not be edited directly. Find .env, .env.local and .env.* files in my projects (the paths in ~/.ferrum/projects.json); list them grouped by project. For 1Password, Doppler or Infisical, use their CLIs (op, doppler, infisical) only if installed and signed in, and export each project's variables to ~/Downloads/ferrum-env-import/<project>.env with mode 600. Never print secret values in the chat. Finish by telling me which files to import for which project, and remind me to delete the export folder afterwards.",
        })
}
