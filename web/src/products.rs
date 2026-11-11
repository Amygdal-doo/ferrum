//! Product catalogue shared by the home page, nav, product pages and sitemap.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Installable through Ferrum Launcher, actively developed. Every app today.
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

#[derive(Debug, Clone, Copy)]
pub struct Product {
    pub slug: &'static str,
    pub name: &'static str,
    /// Element-style symbol from the logo, e.g. "Fd".
    pub symbol: &'static str,
    /// Logo file under `assets/img/brand/`, without extension (`data` → `data.svg`).
    pub logo: &'static str,
    /// CSS custom property holding the app's accent colour (defined in tokens.css).
    pub accent_var: &'static str,
    pub kind: &'static str,
    pub tagline: &'static str,
    pub status: Status,
}

impl Product {
    pub fn path(&self) -> String {
        format!("/{}", self.slug)
    }

    /// Embedded asset path of the logo, for `routes::assets::url`.
    pub fn logo_asset(&self) -> String {
        format!("img/brand/{}.svg", self.logo)
    }

    /// Inline style that exposes the accent as `--app-accent` to the component CSS.
    pub fn accent_style(&self) -> String {
        format!("--app-accent: var({})", self.accent_var)
    }
}

pub const FERRUMAPI: Product = Product {
    slug: "ferrumapi",
    name: "FerrumAPI",
    symbol: "Fa",
    logo: "api",
    accent_var: "--app-api",
    kind: "API client",
    tagline: "A native, local-first API client with the Postman basics, minus the bloat.",
    status: Status::Alpha,
};

pub const FERRUMGIT: Product = Product {
    slug: "ferrumgit",
    name: "FerrumGIT",
    symbol: "Fg",
    logo: "git",
    accent_var: "--app-git",
    kind: "Git client",
    tagline: "A native Git client with a commit graph, line staging and per-project SSH profiles.",
    status: Status::Alpha,
};

pub const FERRUMDATA: Product = Product {
    slug: "ferrumdata",
    name: "FerrumData",
    symbol: "Fd",
    logo: "data",
    accent_var: "--app-data",
    kind: "Database IDE",
    tagline: "A local-first, DataGrip-class database IDE. PostgreSQL today.",
    status: Status::Alpha,
};

pub const FERRUMTERM: Product = Product {
    slug: "ferrumterm",
    name: "FerrumTerm",
    symbol: "Ft",
    logo: "term",
    accent_var: "--app-term",
    kind: "Terminal",
    tagline: "A GPU terminal whose sessions live in a server, with the fterm CLI for humans and agents.",
    status: Status::Alpha,
};

pub const FERRUMKIT: Product = Product {
    slug: "ferrumkit",
    name: "FerrumKit",
    symbol: "Fk",
    logo: "kit",
    accent_var: "--app-kit",
    kind: "Developer utilities",
    tagline: "JSON, JWT, Base64, regex, hashes and timestamps in one native window. Nothing leaves your Mac.",
    status: Status::Alpha,
};

pub const FERRUMDOCK: Product = Product {
    slug: "ferrumdock",
    name: "FerrumDock",
    symbol: "Fc",
    logo: "dock",
    accent_var: "--app-dock",
    kind: "Container manager",
    tagline: "A fast, native window on your containers, for Docker Desktop, OrbStack, Colima and Podman.",
    status: Status::Alpha,
};

pub const FERRUM_EDITOR: Product = Product {
    slug: "ferrum-editor",
    name: "Ferrum Editor",
    symbol: "Fe",
    logo: "editor",
    accent_var: "--app-editor",
    kind: "Code editor",
    tagline: "A small, hackable code editor in the spirit of Atom, written from scratch in Rust and \
              configured in plain TOML.",
    status: Status::Alpha,
};

pub const FERRUM_LAUNCHER: Product = Product {
    slug: "ferrum-launcher",
    name: "Ferrum Launcher",
    symbol: "Fx",
    logo: "launcher",
    accent_var: "--app-launcher",
    kind: "App manager",
    tagline: "Installs, updates, rolls back and launches every Ferrum app from one window.",
    status: Status::Alpha,
};

/// Site order: Ferrum Editor, the suite's code editor, leads.
pub const ALL: [Product; 8] = [
    FERRUM_EDITOR,
    FERRUMAPI,
    FERRUMGIT,
    FERRUMDATA,
    FERRUMTERM,
    FERRUMKIT,
    FERRUMDOCK,
    FERRUM_LAUNCHER,
];

/// The apps Ferrum Launcher manages, i.e. everything except the launcher itself.
pub fn apps() -> impl Iterator<Item = &'static Product> {
    ALL.iter().filter(|p| p.slug != FERRUM_LAUNCHER.slug)
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
                assert_ne!(a.symbol, b.symbol);
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
        }
    }

    #[test]
    fn apps_excludes_the_launcher() {
        assert_eq!(apps().count(), ALL.len() - 1);
        assert!(apps().all(|p| p.slug != FERRUM_LAUNCHER.slug));
    }
}
