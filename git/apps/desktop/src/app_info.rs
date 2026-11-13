//! What FerrumGIT tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs and where users migrate from.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, Migration, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumGIT", "ferrumgit")
        .tagline("A fast Git client: graph, diffs, branches, pull requests and stashes.")
        .icon(IconName::GitBranch, 0xf05133)
        .key("Quick open", "cmd-p")
        .key("Reload", "cmd-r")
        .key("Back", "cmd-[")
        .key("Forward", "cmd-]")
        .key("Switch repository", "cmd-alt-o")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::FullDiskAccess,
            "Lets FerrumGIT read repositories in Documents, Desktop and iCloud.",
        )
        .migration(Migration {
            from: &["GitKraken", "Fork", "Tower", "GitHub Desktop", "Sourcetree"],
            summary: "Your list of repositories comes over; accounts and SSH keys stay where they are.",
            task: "Every Ferrum app shares recent projects in ~/.ferrum/projects.json: {\"projects\": [{\"path\": \"/abs/path\", \"opened_at\": <unix seconds>, \"apps\": [\"ferrumgit\"]}]}, newest first, at most 50. Add each repository the other clients know (that still exists on disk and contains .git) with opened_at set to now and apps [\"ferrumgit\"]; keep existing entries and do not duplicate paths. Sources: GitHub Desktop ~/Library/Application Support/GitHub Desktop (repositories in its IndexedDB/Local Storage; if unreadable, ask me), Fork ~/Library/Application Support/com.DanPristupov.Fork, Tower ~/Library/Application Support/com.fournova.Tower3, Sourcetree ~/Library/Application Support/SourceTree/browser.plist, GitKraken ~/.gitkraken. Credentials and SSH keys are not migrated: Git keeps using ~/.ssh and the keychain.",
        })
}
