//! The Ferrum apps the launcher manages: a built-in table for the apps this
//! build knows, completed and extended by `catalog.json` (fetched from the
//! release repo, with an embedded fallback). An app published later only
//! needs a catalog entry with full metadata to be installable.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

/// Stable id used in release tags (`<id>-v<semver>`), asset names and paths:
/// 1–32 characters of `a-z`, `0-9` and `_`, starting with a letter (no `-`,
/// so `<id>-v<semver>` splits unambiguously).
///
/// Ids are interned, so the type stays `Copy`. Only ids that appear in the
/// catalog, in release tags or in `state.json` are created, so the interned
/// set stays small.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppId(&'static str);

impl AppId {
    pub const API: AppId = AppId("api");
    pub const GIT: AppId = AppId("git");
    pub const DATA: AppId = AppId("data");
    pub const TERM: AppId = AppId("term");
    pub const KIT: AppId = AppId("kit");
    pub const DOCK: AppId = AppId("dock");
    pub const EDITOR: AppId = AppId("editor");
    pub const PORTS: AppId = AppId("ports");
    pub const ENV: AppId = AppId("env");
    pub const VIBE: AppId = AppId("vibe");
    pub const SYSTEM: AppId = AppId("system");
    /// FerrumVibe's id while it was FerrumAgentic (see `State::retire_agentic`).
    pub const LEGACY_VIBE: AppId = AppId("agentic");
    pub const LAUNCHER: AppId = AppId("launcher");

    /// The launcher's id before it was renamed from Ferrum Toolbox. Tags
    /// (`toolbox-v*`), state keys and catalog entries with it read as
    /// [`AppId::LAUNCHER`].
    pub const LEGACY_LAUNCHER: &'static str = "toolbox";

    /// Apps this build has metadata for, in overview order.
    pub const BUILTIN: [AppId; 12] = [
        AppId::EDITOR,
        AppId::VIBE,
        AppId::SYSTEM,
        AppId::API,
        AppId::GIT,
        AppId::DATA,
        AppId::TERM,
        AppId::KIT,
        AppId::DOCK,
        AppId::PORTS,
        AppId::ENV,
        AppId::LAUNCHER,
    ];

    /// The apps Ferrum Toolbox 0.5.0 and older listed. A state file from
    /// those versions has no seen-apps record; these count as seen so the
    /// apps added since are announced as new.
    pub const PRE_LAUNCHER: [AppId; 5] = [
        AppId("ide"),
        AppId::API,
        AppId::GIT,
        AppId::DATA,
        AppId::TERM,
    ];

    /// Apps the suite no longer ships (Ferrum IDE). The launcher still
    /// knows enough about them to launch and uninstall a copy installed
    /// earlier, but never lists, offers or updates them, even if a catalog
    /// still has an entry.
    pub const RETIRED: [AppId; 1] = [AppId("ide")];

    /// Validates (and interns) `id`. `toolbox` maps to [`AppId::LAUNCHER`].
    #[must_use]
    pub fn new(id: &str) -> Option<AppId> {
        if id == Self::LEGACY_LAUNCHER {
            return Some(Self::LAUNCHER);
        }
        if !is_valid_id(id) {
            return None;
        }
        if let Some(known) = Self::BUILTIN
            .iter()
            .chain(&Self::RETIRED)
            .find(|app| app.0 == id)
        {
            return Some(*known);
        }
        Some(AppId(intern(id)))
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }

    #[must_use]
    pub fn is_builtin(self) -> bool {
        Self::BUILTIN.contains(&self)
    }

    /// Whether the suite dropped this app (see [`AppId::RETIRED`]).
    #[must_use]
    pub fn is_retired(self) -> bool {
        Self::RETIRED.contains(&self)
    }
}

fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn intern(id: &str) -> &'static str {
    static IDS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut ids = IDS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = ids.get(id) {
        return existing;
    }
    let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
    ids.insert(leaked);
    leaked
}

impl fmt::Debug for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AppId({})", self.0)
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl FromStr for AppId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AppId::new(s).ok_or(())
    }
}

impl Serialize for AppId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for AppId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        AppId::new(&id).ok_or_else(|| serde::de::Error::custom(format!("invalid app id `{id}`")))
    }
}

/// Everything the launcher needs to install, link and show an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub id: AppId,
    pub name: String,
    pub description: String,
    /// Main executable: at the root of a plain archive, or in
    /// `<bundle_name>.app/Contents/MacOS/` of a macOS bundle archive.
    pub binary: String,
    /// Other executables shipped next to `binary` that get CLI links too.
    pub extra_binaries: Vec<String>,
    /// macOS bundle name without `.app`.
    pub bundle_name: String,
    /// Names the app was published under before a rename (FerrumDock for
    /// FerrumContainers). A copy published as `<former>.app` in the
    /// Applications folder, or a `<former>.lnk` Start Menu shortcut, is
    /// replaced by the current name's (see `State::retire_former_names`).
    pub former_names: Vec<String>,
    /// Name of a `ferrum_ui::components::IconName` variant (e.g. `ToolHammer`); the UI falls
    /// back to a generic icon for names it doesn't know.
    pub icon: String,
    /// URL of a PNG logo, downloaded once and cached; built-in apps also have
    /// an embedded logo.
    pub logo: Option<String>,
    /// Position on the overview, ascending (ties sort by name).
    pub order: i32,
}

impl AppInfo {
    /// `binary` followed by `extra_binaries`.
    pub fn binaries(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.binary.as_str()).chain(self.extra_binaries.iter().map(String::as_str))
    }

    /// The built-in metadata for `app`, if this build knows it.
    #[must_use]
    pub fn builtin(app: AppId) -> Option<AppInfo> {
        BUILTIN.iter().find(|b| b.id == app).map(Builtin::info)
    }

    /// What a copy of a retired app installed earlier is called and where
    /// its files are, so it can still be launched and uninstalled.
    #[must_use]
    pub fn retired(app: AppId) -> Option<AppInfo> {
        RETIRED.iter().find(|b| b.id == app).map(Builtin::info)
    }

    /// Stand-in for an id nothing describes (e.g. an installed app whose
    /// catalog entry was removed): every name is the id itself.
    #[must_use]
    pub fn generic(app: AppId) -> AppInfo {
        AppInfo {
            id: app,
            name: app.to_string(),
            description: String::new(),
            binary: app.to_string(),
            extra_binaries: Vec::new(),
            bundle_name: app.to_string(),
            former_names: Vec::new(),
            icon: String::new(),
            logo: None,
            order: DEFAULT_ORDER,
        }
    }
}

/// Where apps without an `order` go: after the built-in ones.
const DEFAULT_ORDER: i32 = 1000;

struct Builtin {
    id: AppId,
    name: &'static str,
    description: &'static str,
    binary: &'static str,
    extra_binaries: &'static [&'static str],
    bundle_name: &'static str,
    former_names: &'static [&'static str],
    icon: &'static str,
    order: i32,
}

impl Builtin {
    fn info(&self) -> AppInfo {
        AppInfo {
            id: self.id,
            name: self.name.into(),
            description: self.description.into(),
            binary: self.binary.into(),
            extra_binaries: self.extra_binaries.iter().map(|s| (*s).into()).collect(),
            bundle_name: self.bundle_name.into(),
            former_names: self.former_names.iter().map(|s| (*s).into()).collect(),
            icon: self.icon.into(),
            logo: None,
            order: self.order,
        }
    }
}

/// Install layout of the apps in [`AppId::RETIRED`], as the releases that
/// were published for them ship it.
static RETIRED: [Builtin; 1] = [Builtin {
    id: AppId("ide"),
    name: "Ferrum IDE",
    description: "No longer offered: Ferrum Editor is the suite's code editor.",
    binary: "zed",
    extra_binaries: &[],
    bundle_name: "Ferrum IDE",
    former_names: &[],
    icon: "Code",
    order: 10,
}];

static BUILTIN: [Builtin; 12] = [
    Builtin {
        id: AppId::EDITOR,
        name: "Ferrum Editor",
        description: "Lightweight, hackable text editor: tree view, tabs, fuzzy finder and command palette.",
        binary: "ferrumeditor",
        extra_binaries: &[],
        bundle_name: "Ferrum Editor",
        former_names: &[],
        icon: "CursorIBeam",
        order: 10,
    },
    Builtin {
        id: AppId::VIBE,
        name: "FerrumVibe",
        description: "Vibe coding window: FerrumChat, FerrumGIT and FerrumTerm side by side.",
        binary: "ferrumvibe",
        extra_binaries: &[],
        bundle_name: "FerrumVibe",
        former_names: &["FerrumAgentic"],
        icon: "Sparkle",
        order: 15,
    },
    Builtin {
        id: AppId::SYSTEM,
        name: "Ferrum System",
        description: "System architecture diagrams: layers, services on any cloud, data, flows and live pods.",
        binary: "ferrumsystem",
        extra_binaries: &[],
        bundle_name: "Ferrum System",
        former_names: &[],
        icon: "Blocks",
        order: 17,
    },
    Builtin {
        id: AppId::API,
        name: "FerrumAPI",
        description: "HTTP/API client: collections, environments, history and code generation.",
        binary: "ferrumapi",
        extra_binaries: &[],
        bundle_name: "FerrumAPI",
        former_names: &[],
        icon: "Public",
        order: 20,
    },
    Builtin {
        id: AppId::GIT,
        name: "FerrumGIT",
        description: "Git client with commit graph, staging, diffs and profiles.",
        binary: "ferrumgit",
        extra_binaries: &[],
        bundle_name: "FerrumGIT",
        former_names: &[],
        icon: "GitBranch",
        order: 30,
    },
    Builtin {
        id: AppId::DATA,
        name: "FerrumData",
        description: "Database IDE: connections, SQL editor and result grid.",
        binary: "ferrumdata",
        extra_binaries: &[],
        bundle_name: "FerrumData",
        former_names: &[],
        icon: "DatabaseZap",
        order: 40,
    },
    Builtin {
        id: AppId::TERM,
        name: "FerrumTerm",
        description: "GPU terminal with sessions, windows and panes.",
        binary: "ferrumterm",
        extra_binaries: &["fterm"],
        bundle_name: "FerrumTerm",
        former_names: &[],
        icon: "Terminal",
        order: 50,
    },
    Builtin {
        id: AppId::KIT,
        name: "FerrumKit",
        description: "Developer utilities: JSON, Base64, JWT, regex, timestamps, hashes and more.",
        binary: "ferrumkit",
        extra_binaries: &[],
        bundle_name: "FerrumKit",
        former_names: &[],
        icon: "ToolHammer",
        order: 60,
    },
    Builtin {
        id: AppId::DOCK,
        name: "FerrumContainers",
        description: "Container manager for Docker, OrbStack, Colima and Podman engines.",
        // The binary, the id and the data stay `dock`/`ferrumdock`, so
        // FerrumDock installs update in place.
        binary: "ferrumdock",
        extra_binaries: &[],
        bundle_name: "FerrumContainers",
        former_names: &["FerrumDock"],
        icon: "Blocks",
        order: 70,
    },
    Builtin {
        id: AppId::PORTS,
        name: "FerrumPorts",
        description: "Listening ports and the processes behind them: open, inspect or stop them.",
        binary: "ferrumports",
        extra_binaries: &[],
        bundle_name: "FerrumPorts",
        former_names: &[],
        icon: "Server",
        order: 80,
    },
    Builtin {
        id: AppId::ENV,
        name: "FerrumEnv",
        description: "Password-locked vault for project environment variables, shared encrypted between developers.",
        binary: "ferrumenv",
        extra_binaries: &[],
        bundle_name: "FerrumEnv",
        former_names: &[],
        icon: "Lock",
        order: 90,
    },
    Builtin {
        id: AppId::LAUNCHER,
        name: "Ferrum Launcher",
        description: "Installs, updates and launches the Ferrum apps.",
        binary: "ferrum-launcher",
        extra_binaries: &[],
        bundle_name: "Ferrum Launcher",
        former_names: &[],
        icon: "Box",
        order: 1100,
    },
];

/// Where an app is in its life cycle, as set in `catalog.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppStatus {
    Planned,
    InDevelopment,
    Alpha,
    Beta,
    Stable,
    Deprecated,
}

impl AppStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AppStatus::Planned => "Planned",
            AppStatus::InDevelopment => "In development",
            AppStatus::Alpha => "Alpha",
            AppStatus::Beta => "Beta",
            AppStatus::Stable => "Stable",
            AppStatus::Deprecated => "Deprecated",
        }
    }
}

/// One app in `catalog.json`. The metadata fields are optional and override
/// the built-in table; an app this build doesn't know needs at least `name`,
/// `binary` and `bundle_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    pub id: AppId,
    pub status: AppStatus,
    pub summary: String,
    pub highlights: Vec<String>,
    pub roadmap: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_binaries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,
    /// Ignored by Launcher 0.8.1 and older (they leave the old copy in place).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub former_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

impl CatalogEntry {
    /// Reads one entry. `id`, `status` and `summary` are required; a
    /// malformed optional field is dropped (logged) rather than the entry,
    /// and an unknown app without complete metadata is rejected.
    fn from_value(value: &Value) -> Result<Self, String> {
        let id_text = value.get("id").and_then(Value::as_str).unwrap_or_default();
        let id = AppId::new(id_text).ok_or_else(|| format!("invalid id `{id_text}`"))?;
        let status = value
            .get("status")
            .cloned()
            .ok_or("missing status")
            .and_then(|status| {
                serde_json::from_value::<AppStatus>(status).map_err(|_| "unknown status")
            })
            .map_err(|e| format!("{id}: {e}"))?;
        let summary = value
            .get("summary")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{id}: missing summary"))?
            .to_string();
        let field = |name: &str| optional_field(value, name);
        let entry = CatalogEntry {
            id,
            status,
            summary,
            highlights: string_list(id, value, "highlights").unwrap_or_default(),
            roadmap: string_list(id, value, "roadmap").unwrap_or_default(),
            homepage: field("homepage").and_then(|s| as_string(id, "homepage", s)),
            name: field("name")
                .and_then(|s| as_string(id, "name", s))
                .filter(|s| !s.trim().is_empty()),
            description: field("description").and_then(|s| as_string(id, "description", s)),
            binary: field("binary")
                .and_then(|s| as_string(id, "binary", s))
                .filter(|s| validated(id, "binary", is_safe_file_name(s))),
            extra_binaries: string_list(id, value, "extra_binaries").filter(|list| {
                validated(
                    id,
                    "extra_binaries",
                    list.iter().all(|s| is_safe_file_name(s)),
                )
            }),
            bundle_name: field("bundle_name")
                .and_then(|s| as_string(id, "bundle_name", s))
                .filter(|s| validated(id, "bundle_name", is_safe_file_name(s))),
            former_names: string_list(id, value, "former_names").filter(|list| {
                validated(
                    id,
                    "former_names",
                    list.iter().all(|s| is_safe_file_name(s)),
                )
            }),
            icon: field("icon")
                .and_then(|s| as_string(id, "icon", s))
                .filter(|s| {
                    validated(
                        id,
                        "icon",
                        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    )
                }),
            logo: field("logo")
                .and_then(|s| as_string(id, "logo", s))
                .filter(|s| validated(id, "logo", s.starts_with("https://"))),
            order: field("order").and_then(|order| match order.as_i64() {
                Some(order) => i32::try_from(order).ok(),
                None => {
                    tracing::warn!(app = %id, "catalog: ignoring non-integer `order`");
                    None
                }
            }),
        };
        if !id.is_builtin() && !entry.is_complete() {
            return Err(format!(
                "{id}: an app this launcher doesn't know needs `name`, `binary` and `bundle_name`"
            ));
        }
        Ok(entry)
    }

    /// Whether the entry alone describes the app well enough to install it.
    fn is_complete(&self) -> bool {
        self.name.is_some() && self.binary.is_some() && self.bundle_name.is_some()
    }
}

fn optional_field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.get(name).filter(|v| !v.is_null())
}

fn as_string(id: AppId, name: &str, value: &Value) -> Option<String> {
    match value.as_str() {
        Some(text) => Some(text.to_string()),
        None => {
            tracing::warn!(app = %id, field = name, "catalog: ignoring non-string field");
            None
        }
    }
}

fn string_list(id: AppId, value: &Value, name: &str) -> Option<Vec<String>> {
    let list = optional_field(value, name)?;
    let items = list.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
    });
    if items.is_none() {
        tracing::warn!(app = %id, field = name, "catalog: ignoring field that isn't a list of strings");
    }
    items
}

fn validated(id: AppId, field: &str, ok: bool) -> bool {
    if !ok {
        tracing::warn!(app = %id, field, "catalog: ignoring invalid value");
    }
    ok
}

/// A plain file name: no separators, no leading dot, nothing a path could
/// climb out of the install directory with.
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
}

/// Parsed `catalog.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Catalog {
    pub apps: Vec<CatalogEntry>,
}

/// The cached copy in `state.json` is read as leniently as a fetched one.
impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Catalog::from_json_value(&value))
    }
}

/// The copy of the repo's `catalog.json` this build was compiled with.
const EMBEDDED_CATALOG: &str = include_str!("../../../../catalog.json");

impl Catalog {
    /// Parses `catalog.json`. Bad entries (invalid id, unknown status, an
    /// unknown app without complete metadata) are skipped and logged,
    /// entries for retired apps are dropped, and unknown fields are ignored,
    /// so neither an older nor a newer catalog breaks the launcher.
    pub fn parse(json: &str) -> crate::Result<Self> {
        let value: Value = serde_json::from_str(json)?;
        if !value.get("apps").is_some_and(Value::is_array) {
            return Err(crate::LauncherError::Invalid(
                "catalog.json has no `apps` list".into(),
            ));
        }
        Ok(Self::from_json_value(&value))
    }

    fn from_json_value(value: &Value) -> Self {
        let mut seen = BTreeSet::new();
        let apps = value
            .get("apps")
            .and_then(Value::as_array)
            .map(|apps| {
                apps.iter()
                    .filter_map(|app| match CatalogEntry::from_value(app) {
                        // Older catalogs (and copies cached from them) still
                        // list apps the suite dropped; they are not offered.
                        Ok(entry) if entry.id.is_retired() => None,
                        Ok(entry) if seen.insert(entry.id) => Some(entry),
                        Ok(entry) => {
                            tracing::warn!(app = %entry.id, "skipping duplicate catalog entry");
                            None
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "skipping catalog entry");
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { apps }
    }

    #[must_use]
    pub fn embedded() -> &'static Self {
        static EMBEDDED: OnceLock<Catalog> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            Self::parse(EMBEDDED_CATALOG).unwrap_or_else(|e| {
                tracing::error!(error = %e, "embedded catalog.json is invalid");
                Self::default()
            })
        })
    }

    #[must_use]
    pub fn entry(&self, app: AppId) -> Option<&CatalogEntry> {
        self.apps.iter().find(|entry| entry.id == app)
    }

    /// `self`, with apps it lacks filled in from `fallback`.
    #[must_use]
    pub fn or_fill_from(mut self, fallback: &Catalog) -> Self {
        for entry in &fallback.apps {
            if self.entry(entry.id).is_none() {
                self.apps.push(entry.clone());
            }
        }
        self
    }

    /// The app's metadata: the built-in table overridden by this catalog's
    /// entry. `None` for an id neither knows.
    #[must_use]
    pub fn info(&self, app: AppId) -> Option<AppInfo> {
        let entry = self.entry(app);
        let mut info = match (AppInfo::builtin(app), entry) {
            (Some(info), _) => info,
            (None, Some(entry)) if entry.is_complete() => AppInfo {
                description: entry.summary.clone(),
                ..AppInfo::generic(app)
            },
            (None, _) => return None,
        };
        if let Some(entry) = entry {
            let set = |target: &mut String, value: &Option<String>| {
                if let Some(value) = value {
                    target.clone_from(value);
                }
            };
            set(&mut info.name, &entry.name);
            set(&mut info.description, &entry.description);
            set(&mut info.binary, &entry.binary);
            set(&mut info.bundle_name, &entry.bundle_name);
            set(&mut info.icon, &entry.icon);
            if let Some(extra) = &entry.extra_binaries {
                info.extra_binaries.clone_from(extra);
            }
            // Added to the built-in list, so a catalog can't make this
            // launcher forget a rename it knows about.
            for former in entry.former_names.iter().flatten() {
                if !info.former_names.contains(former) {
                    info.former_names.push(former.clone());
                }
            }
            if entry.logo.is_some() {
                info.logo.clone_from(&entry.logo);
            }
            if let Some(order) = entry.order {
                info.order = order;
            }
        }
        Some(info)
    }

    /// Every app the catalog or the built-in table describes, in overview
    /// order.
    #[must_use]
    pub fn apps(&self) -> Vec<AppInfo> {
        let ids: BTreeSet<AppId> = AppId::BUILTIN
            .into_iter()
            .chain(self.apps.iter().map(|entry| entry.id))
            .collect();
        let mut apps: Vec<AppInfo> = ids.into_iter().filter_map(|id| self.info(id)).collect();
        apps.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
        apps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_covers_every_builtin_app_with_metadata() {
        let catalog = Catalog::embedded();
        for app in AppId::BUILTIN {
            let entry = catalog.entry(app).expect("catalog entry");
            assert!(!entry.summary.is_empty(), "{app} has no summary");
            assert!(entry.is_complete(), "{app} lacks name/binary/bundle_name");
            let logo = entry.logo.as_deref().expect("logo");
            assert_eq!(
                logo,
                format!(
                    "https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/logos/{app}.png"
                )
            );
            // The catalog agrees with the built-in table, so old and new
            // launchers install the same files.
            let builtin = AppInfo::builtin(app).expect("builtin");
            let merged = catalog.info(app).expect("info");
            assert_eq!(merged.binary, builtin.binary, "{app}");
            assert_eq!(merged.bundle_name, builtin.bundle_name, "{app}");
            assert_eq!(merged.extra_binaries, builtin.extra_binaries, "{app}");
            assert_eq!(merged.name, builtin.name, "{app}");
            assert_eq!(merged.former_names, builtin.former_names, "{app}");
        }
        assert!(
            catalog
                .entry(AppId::new("toolbox").expect("alias"))
                .is_some()
        );
    }

    #[test]
    fn parses_catalog_and_skips_bad_entries() {
        let json = r#"{"version":1,"apps":[
          {"id":"data","status":"beta","summary":"DB IDE","highlights":["PG"],"homepage":"https://x"},
          {"id":"api","status":"in-development","summary":"API"},
          {"id":"rocket","status":"stable","summary":"Unknown app without metadata"},
          {"id":"git","status":"sideways","summary":"Unknown status"},
          {"id":"Bad-Id","status":"stable","summary":"Invalid id"},
          {"id":"term","status":"alpha"}
        ]}"#;
        let catalog = Catalog::parse(json).expect("parse");
        assert_eq!(catalog.apps.len(), 2);
        let data = catalog.entry(AppId::DATA).expect("data");
        assert_eq!(data.status, AppStatus::Beta);
        assert_eq!(data.highlights, ["PG"]);
        assert_eq!(data.homepage.as_deref(), Some("https://x"));
        let api = catalog.entry(AppId::API).expect("api");
        assert_eq!(api.status, AppStatus::InDevelopment);
        assert!(api.roadmap.is_empty());
        assert!(catalog.entry(AppId::GIT).is_none());
        assert!(Catalog::parse("not json").is_err());
        assert!(Catalog::parse(r#"{"apps":"none"}"#).is_err());
    }

    #[test]
    fn unknown_app_with_complete_metadata_is_installable() {
        let json = r#"{"apps":[{
          "id":"forge","status":"alpha","summary":"Build runner.",
          "name":"FerrumForge","binary":"ferrumforge","extra_binaries":["fforge"],
          "bundle_name":"FerrumForge","icon":"Hammer",
          "logo":"https://raw.githubusercontent.com/o/r/main/logos/forge.png","order":65,
          "future_field":{"nested":true}
        }]}"#;
        let catalog = Catalog::parse(json).expect("parse");
        let forge = AppId::new("forge").expect("id");
        let info = catalog.info(forge).expect("info");
        assert_eq!(info.name, "FerrumForge");
        assert_eq!(info.description, "Build runner.");
        assert_eq!(
            info.binaries().collect::<Vec<_>>(),
            ["ferrumforge", "fforge"]
        );
        assert_eq!(info.bundle_name, "FerrumForge");
        assert_eq!(info.icon, "Hammer");
        assert_eq!(info.order, 65);
        // Placed between the built-in apps by `order`.
        let order: Vec<_> = catalog.apps().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            order,
            [
                "editor", "vibe", "system", "api", "git", "data", "term", "kit", "forge", "dock",
                "ports", "env", "launcher"
            ]
        );
    }

    #[test]
    fn catalog_values_override_builtins_and_bad_values_are_dropped() {
        let json = r#"{"apps":[
          {"id":"data","status":"alpha","summary":"S","name":"Ferrum Data","order":"first",
           "binary":"../../evil","bundle_name":".hidden","logo":"http://insecure/x.png","icon":"Not An Icon!"},
          {"id":"kit","status":"alpha","summary":"S","description":"Kit desc","extra_binaries":["fkit"],"order":1}
        ]}"#;
        let catalog = Catalog::parse(json).expect("parse");
        let data = catalog.info(AppId::DATA).expect("data");
        assert_eq!(data.name, "Ferrum Data");
        assert_eq!(data.binary, "ferrumdata", "unsafe binary falls back");
        assert_eq!(data.bundle_name, "FerrumData");
        assert_eq!(data.logo, None);
        assert_eq!(data.icon, "DatabaseZap");
        assert_eq!(data.order, 40);
        let kit = catalog.info(AppId::KIT).expect("kit");
        assert_eq!(kit.description, "Kit desc");
        assert_eq!(kit.extra_binaries, ["fkit"]);
        assert_eq!(catalog.apps()[0].id, AppId::KIT);
        // An unknown app whose required field is invalid is skipped.
        let bad = Catalog::parse(
            r#"{"apps":[{"id":"x","status":"alpha","summary":"S","name":"X","binary":"a/b","bundle_name":"X"}]}"#,
        )
        .expect("parse");
        assert!(bad.apps.is_empty());
    }

    #[test]
    fn former_names_are_read_validated_and_added_to_the_builtin_ones() {
        // The catalog that renames FerrumDock, as every launcher reads it.
        let renamed = Catalog::parse(
            r#"{"apps":[{"id":"dock","status":"alpha","summary":"S","name":"FerrumContainers",
                "binary":"ferrumdock","bundle_name":"FerrumContainers","former_names":["FerrumDock"]}]}"#,
        )
        .expect("parse");
        let dock = renamed.info(AppId::DOCK).expect("dock");
        assert_eq!(dock.name, "FerrumContainers");
        assert_eq!(dock.bundle_name, "FerrumContainers");
        assert_eq!(dock.binary, "ferrumdock");
        assert_eq!(dock.former_names, ["FerrumDock"], "not listed twice");
        // A catalog may add names; unsafe ones drop the whole list.
        let catalog = Catalog::parse(
            r#"{"apps":[
              {"id":"kit","status":"alpha","summary":"S","bundle_name":"FerrumTools","former_names":["FerrumKit"]},
              {"id":"api","status":"alpha","summary":"S","former_names":["../Evil", "FerrumHTTP"]},
              {"id":"git","status":"alpha","summary":"S","former_names":"FerrumGit"}
            ]}"#,
        )
        .expect("parse");
        assert_eq!(
            catalog.info(AppId::KIT).expect("kit").former_names,
            ["FerrumKit"]
        );
        assert!(
            catalog
                .info(AppId::API)
                .expect("api")
                .former_names
                .is_empty()
        );
        assert!(
            catalog
                .info(AppId::GIT)
                .expect("git")
                .former_names
                .is_empty()
        );
        // A catalog without the field (the one Launcher 0.8.1 shipped with)
        // keeps the built-in rename.
        let old = Catalog::parse(
            r#"{"apps":[{"id":"dock","status":"alpha","summary":"S","name":"FerrumDock","bundle_name":"FerrumDock"}]}"#,
        )
        .expect("parse");
        let dock = old.info(AppId::DOCK).expect("dock");
        assert_eq!(dock.bundle_name, "FerrumDock");
        assert_eq!(dock.former_names, ["FerrumDock"]);
        // Round-trips through the cache in state.json.
        let json = serde_json::to_string(&renamed).expect("serialize");
        assert!(json.contains(r#""former_names":["FerrumDock"]"#), "{json}");
        let back: Catalog = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, renamed);
    }

    #[test]
    fn remote_catalog_falls_back_per_app() {
        let remote =
            Catalog::parse(r#"{"apps":[{"id":"data","status":"stable","summary":"Released"}]}"#)
                .expect("parse");
        let merged = remote.or_fill_from(Catalog::embedded());
        assert_eq!(
            merged.entry(AppId::DATA).map(|e| e.status),
            Some(AppStatus::Stable)
        );
        assert!(
            AppId::BUILTIN
                .iter()
                .all(|app| merged.entry(*app).is_some())
        );
        assert_eq!(AppStatus::InDevelopment.label(), "In development");
    }

    #[test]
    fn cached_catalog_round_trips_through_serde() {
        let catalog = Catalog::embedded().clone();
        let json = serde_json::to_string(&catalog).expect("serialize");
        let back: Catalog = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, catalog);
        // A cached catalog from Toolbox 0.5.0 (enum ids, no metadata) still reads.
        let old: Catalog = serde_json::from_str(
            r#"{"apps":[{"id":"toolbox","status":"alpha","summary":"Old","highlights":[],"roadmap":[],"homepage":null}]}"#,
        )
        .expect("old cache");
        assert_eq!(old.apps[0].id, AppId::LAUNCHER);
        assert_eq!(
            old.info(AppId::LAUNCHER).map(|i| i.bundle_name),
            Some("Ferrum Launcher".into())
        );
    }

    #[test]
    fn retired_apps_are_never_listed_but_keep_their_install_layout() {
        let ide = AppId::new("ide").expect("id");
        assert!(ide.is_retired());
        assert!(!ide.is_builtin());
        assert!(AppInfo::builtin(ide).is_none());
        // A catalog published before Ferrum IDE was dropped still lists it.
        let old = Catalog::parse(
            r#"{"apps":[{"id":"ide","status":"alpha","summary":"Editor","name":"FerrumIDE",
                "binary":"zed","bundle_name":"Ferrum IDE"},
              {"id":"api","status":"alpha","summary":"API"}]}"#,
        )
        .expect("parse");
        assert!(old.entry(ide).is_none());
        assert!(old.info(ide).is_none());
        assert!(old.apps().iter().all(|info| info.id != ide));
        assert!(Catalog::embedded().entry(ide).is_none());
        let info = AppInfo::retired(ide).expect("retired");
        assert_eq!(info.name, "Ferrum IDE");
        assert_eq!(info.binary, "zed");
        assert_eq!(info.bundle_name, "Ferrum IDE");
        assert!(AppInfo::retired(AppId::EDITOR).is_none());
    }

    #[test]
    fn ids_validate_intern_and_alias() {
        for id in AppId::BUILTIN {
            assert_eq!(id.as_str().parse::<AppId>(), Ok(id));
            assert_eq!(AppInfo::builtin(id).map(|info| info.id), Some(id));
        }
        assert_eq!(AppId::new("toolbox"), Some(AppId::LAUNCHER));
        let a = AppId::new("newapp2").expect("valid");
        let b = AppId::new(&String::from("newapp2")).expect("valid");
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()), "interned once");
        for bad in ["", "Zed", "1app", "my-app", "a/b", "../x", &"a".repeat(33)] {
            assert!(AppId::new(bad).is_none(), "{bad:?}");
        }
        assert_eq!(
            serde_json::to_string(&AppId::TERM).expect("json"),
            r#""term""#
        );
        assert!(serde_json::from_str::<AppId>(r#""no-pe""#).is_err());
        assert_eq!(
            AppInfo::builtin(AppId::LAUNCHER).map(|i| i.bundle_name),
            Some("Ferrum Launcher".into())
        );
        assert_eq!(
            AppInfo::builtin(AppId::TERM).map(|i| i.extra_binaries),
            Some(vec!["fterm".to_string()])
        );
        assert!(AppInfo::builtin(AppId::new("unknownapp").expect("id")).is_none());
    }
}
