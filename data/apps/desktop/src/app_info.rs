//! What FerrumData tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs and where users migrate from.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, Migration, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumData", "ferrumdata")
        .tagline("A database IDE for Postgres, MySQL, SQLite, MongoDB, Redis and SurrealDB.")
        .icon(IconName::DatabaseZap, 0x3fb8a0)
        .key("Run query", "cmd-enter")
        .key("Copy selection", "cmd-c")
        .key("Select all cells", "cmd-a")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::LocalNetwork,
            "Needed for databases on other machines in your network.",
        )
        .permission(
            PermissionKind::FilesAndFolders,
            "Lets FerrumData open SQLite files in Documents, Desktop and Downloads.",
        )
        .migration(Migration {
            from: &["TablePlus", "DBeaver", "DataGrip", "Sequel Ace", "Postico"],
            summary: "Connections come over as a checklist to add; passwords stay in your keychain.",
            task: "FerrumData keeps connections in its own database, which must not be edited directly. Read the other tools' connection lists (TablePlus ~/Library/Application Support/com.tinyapp.TablePlus/Data/Connections.plist, DBeaver ~/Library/DBeaverData/workspace6/General/.dbeaver/data-sources.json, DataGrip/JetBrains dataSources.xml in project .idea folders, Sequel Ace ~/Library/Containers/com.sequel-ace.sequel-ace/Data/Library/Application Support/Sequel Ace/Data/Favorites.plist, Postico ~/Library/Containers/at.eggerapps.Postico/Data/Library/Application Support/Postico). Write ~/Downloads/ferrum-data-connections.md: one entry per connection with name, engine, host, port, user, database and SSL/SSH-tunnel settings, as a checklist I follow in FerrumData's New Connection dialog. Never copy passwords.",
        })
        .event(ferrumagent_ui::NEEDS_YOU)
        .event(ferrumagent_ui::DONE)
        .event(ferrumagent_ui::FAILED)
}
