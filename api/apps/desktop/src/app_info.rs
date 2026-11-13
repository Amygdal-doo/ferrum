//! What FerrumAPI tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs and where users migrate from.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, Migration, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumAPI", "ferrumapi")
        .tagline("An HTTP, gRPC, WebSocket and SOAP client that keeps collections next to your code.")
        .icon(IconName::ArrowRightLeft, 0x4c8bf5)
        .key("Send request", "cmd-enter")
        .key("Save request", "cmd-s")
        .key("Find in response", "cmd-f")
        .key("Switch project", "cmd-alt-o")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::LocalNetwork,
            "Needed for requests to machines on your network and .local hosts.",
        )
        .migration(Migration {
            from: &["Postman", "Insomnia", "Bruno", "Hoppscotch"],
            summary: "Collections, environments and requests come over as Postman files you import in one step.",
            task: "FerrumAPI imports Postman v2.1 collection and environment files (File › Import Collection / Import Environment) and curl commands; it has no Insomnia or Bruno importer. Postman: export from the app or use files the user points to. Insomnia (~/Library/Application Support/Insomnia), Bruno (.bru files in the user's collection folders) and Hoppscotch exports: convert each collection to a Postman v2.1 collection JSON and each environment to a Postman environment JSON. Write them to ~/Downloads/ferrum-api-import/ only, never into FerrumAPI's database, and finish by telling me which files to import. Leave secret values empty and list the variables that need one.",
        })
}
