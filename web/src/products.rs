//! Product catalogue shared by the home page, nav, product pages and sitemap.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Installable through FerrumLauncher, actively developed. Every app today.
    Alpha,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Alpha => "Alpha",
        }
    }
    /// CSS modifier used by `.badge--{class}`.
    pub fn css_class(self) -> &'static str {
        match self {
            Status::Alpha => "alpha",
        }
    }
}

/// Which list an app is shown in. The launcher has no group: it is listed
/// on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The apps a working day happens in: editor, agents, Git, terminal, API,
    /// containers and data.
    Apps,
    /// Smaller utilities: variables, ports, conversions and architecture.
    Tools,
}

impl Group {
    pub const ALL: [Group; 2] = [Group::Apps, Group::Tools];

    pub fn label(self) -> &'static str {
        match self {
            Group::Apps => "Apps",
            Group::Tools => "Tools",
        }
    }

    /// Anchor id of the group's section.
    pub fn id(self) -> &'static str {
        match self {
            Group::Apps => "apps",
            Group::Tools => "tools",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Product {
    pub slug: &'static str,
    pub name: &'static str,
    /// Logo file under `assets/img/brand/`, without extension (`data` → `data.svg`).
    pub logo: &'static str,
    /// CSS custom property holding the app's accent colour (defined in tokens.css).
    pub accent_var: &'static str,
    pub kind: &'static str,
    pub tagline: &'static str,
    pub status: Status,
    /// `None` only for the launcher.
    pub group: Option<Group>,
}

impl Product {
    pub fn path(&self) -> String {
        format!("/{}", self.slug)
    }

    /// Release tags are `<release id>-v<version>` (see [`by_release_id`]).
    pub fn release_id(&self) -> &'static str {
        self.logo
    }

    /// `/download/<slug>/<installer>`: redirects to the newest installer.
    pub fn installer_path(&self, installer: crate::releases::Installer) -> String {
        format!("/download/{}/{}", self.slug, installer.slug())
    }

    /// Embedded asset path of the logo, for `routes::assets::url`.
    pub fn logo_asset(&self) -> String {
        format!("img/brand/{}.svg", self.logo)
    }

    /// Embedded 1200×630 social image (`assets/img/og/<slug>.png`).
    pub fn og_image_asset(&self) -> String {
        format!("img/og/{}.png", self.slug)
    }

    /// Inline style that exposes the accent as `--app-accent` to the component CSS.
    pub fn accent_style(&self) -> String {
        format!("--app-accent: var({})", self.accent_var)
    }
}

pub const FERRUMENGINEER: Product = Product {
    slug: "ferrumengineer",
    name: "FerrumEngineer",
    logo: "vibe",
    accent_var: "--app-vibe",
    kind: "Agentic engineering",
    tagline: "FerrumChat, FerrumGIT and FerrumTerm side by side on one project, in one window.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUMAPI: Product = Product {
    slug: "ferrumapi",
    name: "FerrumAPI",
    logo: "api",
    accent_var: "--app-api",
    kind: "API client",
    tagline: "A native, local-first API client for REST, GraphQL, gRPC and more, minus \
              the bloat.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUMGIT: Product = Product {
    slug: "ferrumgit",
    name: "FerrumGIT",
    logo: "git",
    accent_var: "--app-git",
    kind: "Git client",
    tagline: "A native Git client with a commit graph, line staging and per-project SSH profiles.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUMDATA: Product = Product {
    slug: "ferrumdata",
    name: "FerrumData",
    logo: "data",
    accent_var: "--app-data",
    kind: "Database IDE",
    tagline: "A local-first, DataGrip-class database IDE for PostgreSQL, MySQL, SQLite, \
              SurrealDB, MongoDB and Redis.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUMTERM: Product = Product {
    slug: "ferrumterm",
    name: "FerrumTerm",
    logo: "term",
    accent_var: "--app-term",
    kind: "Terminal",
    tagline: "A GPU terminal whose sessions live in a server, with the fterm CLI for humans and agents.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUMKIT: Product = Product {
    slug: "ferrumkit",
    name: "FerrumKit",
    logo: "kit",
    accent_var: "--app-kit",
    kind: "Developer utilities",
    tagline: "JSON, JWT, Base64, regex, hashes and timestamps in one native window. Nothing leaves your Mac.",
    status: Status::Alpha,
    group: Some(Group::Tools),
};

pub const FERRUMCONTAINERS: Product = Product {
    slug: "ferrumcontainers",
    name: "FerrumContainers",
    logo: "dock",
    accent_var: "--app-dock",
    kind: "Container manager",
    tagline: "A fast, native window on your containers, for Docker Desktop, OrbStack, Colima and Podman.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUM_SYSTEM: Product = Product {
    slug: "ferrum-system",
    name: "FerrumSystem",
    logo: "system",
    accent_var: "--app-system",
    kind: "Architecture diagrams",
    tagline: "Design a system's architecture in its own repo: layers, services on any cloud, \
              data, flows and the pods that run it.",
    status: Status::Alpha,
    group: Some(Group::Tools),
};

pub const FERRUMPORTS: Product = Product {
    slug: "ferrumports",
    name: "FerrumPorts",
    logo: "ports",
    accent_var: "--app-ports",
    kind: "Port inspector",
    tagline: "Every listening port with its process and project, and a confirmed way to stop it.",
    status: Status::Alpha,
    group: Some(Group::Tools),
};

pub const FERRUMVARIABLES: Product = Product {
    slug: "ferrumvariables",
    name: "FerrumVariables",
    logo: "env",
    accent_var: "--app-env",
    kind: "Environment vault",
    tagline: "A password-locked vault for your projects' environment variables, with encrypted \
              sharing between developers.",
    status: Status::Alpha,
    group: Some(Group::Tools),
};

pub const FERRUMIDE: Product = Product {
    slug: "ferrumide",
    name: "FerrumIDE",
    logo: "editor",
    accent_var: "--app-editor",
    kind: "Code editor",
    tagline: "A small, hackable code editor in the spirit of Atom, written from scratch in Rust and \
              configured in plain TOML.",
    status: Status::Alpha,
    group: Some(Group::Apps),
};

pub const FERRUM_LAUNCHER: Product = Product {
    slug: "ferrum-launcher",
    name: "FerrumLauncher",
    logo: "launcher",
    accent_var: "--app-launcher",
    kind: "App manager",
    tagline: "Installs, updates, rolls back and launches every Ferrum app from one window.",
    status: Status::Alpha,
    group: None,
};

/// Site order: the apps (FerrumIDE first), then the tools, then the launcher.
pub const ALL: [Product; 12] = [
    FERRUMIDE,
    FERRUMENGINEER,
    FERRUMGIT,
    FERRUMTERM,
    FERRUMAPI,
    FERRUMCONTAINERS,
    FERRUMDATA,
    FERRUMVARIABLES,
    FERRUMPORTS,
    FERRUMKIT,
    FERRUM_SYSTEM,
    FERRUM_LAUNCHER,
];

/// The product whose release tags start with `id` (`api-v…`). Release ids
/// are the logo names (`editor`, `api`, …, `launcher`).
pub fn by_release_id(id: &str) -> Option<&'static Product> {
    ALL.iter().find(|p| p.logo == id)
}

/// The product at `/<slug>`.
pub fn by_slug(slug: &str) -> Option<&'static Product> {
    ALL.iter().find(|p| p.slug == slug)
}

/// The apps FerrumLauncher manages, i.e. everything except the launcher itself.
pub fn apps() -> impl Iterator<Item = &'static Product> {
    ALL.iter().filter(|p| p.slug != FERRUM_LAUNCHER.slug)
}

/// The products of one group, in site order.
pub fn in_group(group: Group) -> impl Iterator<Item = &'static Product> {
    ALL.iter().filter(move |p| p.group == Some(group))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_logos_are_unique() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.slug, b.slug);
                assert_ne!(a.logo, b.logo);
            }
        }
    }

    #[test]
    fn every_logo_is_embedded() {
        for p in ALL {
            assert!(
                crate::routes::assets::version(&p.logo_asset()).is_some(),
                "missing {}",
                p.logo_asset()
            );
            assert!(
                crate::routes::assets::version(&p.og_image_asset()).is_some(),
                "missing {}",
                p.og_image_asset()
            );
        }
    }

    #[test]
    fn release_ids_find_products() {
        assert_eq!(by_release_id("api").map(|p| p.slug), Some("ferrumapi"));
        assert_eq!(
            by_release_id("launcher").map(|p| p.slug),
            Some("ferrum-launcher")
        );
        assert!(by_release_id("ide").is_none());
    }

    #[test]
    fn groups_split_the_apps_in_order() {
        let names = |g| in_group(g).map(|p| p.name).collect::<Vec<_>>();
        assert_eq!(
            names(Group::Apps),
            [
                "FerrumIDE",
                "FerrumEngineer",
                "FerrumGIT",
                "FerrumTerm",
                "FerrumAPI",
                "FerrumContainers",
                "FerrumData",
            ]
        );
        assert_eq!(
            names(Group::Tools),
            [
                "FerrumVariables",
                "FerrumPorts",
                "FerrumKit",
                "FerrumSystem"
            ]
        );
        // Every app but the launcher has a group, and ALL lists groups in order.
        assert!(apps().all(|p| p.group.is_some()));
        assert!(FERRUM_LAUNCHER.group.is_none());
        let grouped: Vec<_> = Group::ALL.iter().flat_map(|g| in_group(*g)).collect();
        assert_eq!(grouped.len(), apps().count());
        assert!(grouped.iter().zip(apps()).all(|(a, b)| a.slug == b.slug));
    }

    #[test]
    fn apps_excludes_the_launcher() {
        assert_eq!(apps().count(), ALL.len() - 1);
        assert!(apps().all(|p| p.slug != FERRUM_LAUNCHER.slug));
    }
}
