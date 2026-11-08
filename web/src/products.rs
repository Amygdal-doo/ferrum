//! Product catalogue shared by the home page, nav, product pages and sitemap.

use crate::site;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Working foundation, actively developed (FerrumData).
    Alpha,
    /// Early, being built now (FerrumAPI, Ferrum IDE).
    Preview,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Alpha => "Alpha",
            Status::Preview => "Early preview",
        }
    }
    /// CSS modifier used by `.badge--{class}`.
    pub fn css_class(self) -> &'static str {
        match self {
            Status::Alpha => "alpha",
            Status::Preview => "preview",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Product {
    pub slug: &'static str,
    pub name: &'static str,
    /// Chemistry-style mark shown on cards, e.g. "Fd".
    pub symbol: &'static str,
    pub kind: &'static str,
    pub tagline: &'static str,
    pub status: Status,
    pub github_url: &'static str,
}

impl Product {
    pub fn path(&self) -> String {
        format!("/{}", self.slug)
    }
}

pub const FERRUMDATA: Product = Product {
    slug: "ferrumdata",
    name: "FerrumData",
    symbol: "Fd",
    kind: "Database IDE",
    tagline: "A local-first, DataGrip-class database IDE. PostgreSQL today.",
    status: Status::Alpha,
    github_url: site::GITHUB_FERRUMDATA_URL,
};

pub const FERRUMAPI: Product = Product {
    slug: "ferrumapi",
    name: "FerrumAPI",
    symbol: "Fa",
    kind: "API client",
    tagline: "A native desktop API client with the Postman basics, minus the bloat.",
    status: Status::Preview,
    github_url: site::GITHUB_FERRUMAPI_URL,
};

pub const FERRUM_IDE: Product = Product {
    slug: "ferrum-ide",
    name: "Ferrum IDE",
    symbol: "Fi",
    kind: "Code editor",
    tagline: "A GPU-accelerated code editor based on the open-source Zed editor.",
    status: Status::Preview,
    github_url: site::GITHUB_FERRUM_IDE_URL,
};

pub const ALL: [Product; 3] = [FERRUMDATA, FERRUMAPI, FERRUM_IDE];
