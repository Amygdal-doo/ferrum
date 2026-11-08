//! Collections: a tree of folders and saved requests. Tree operations live in
//! `tree.rs`.

use crate::auth::Auth;
use crate::environment::Variable;
use crate::ids::{CollectionId, ItemId};
use crate::request::HttpRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub description: String,
    /// Root auth. `Inherit` at the root behaves as no auth.
    pub auth: Auth,
    /// Collection variables (between globals and the active environment).
    pub variables: Vec<Variable>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    Folder(Folder),
    Request(SavedRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub auth: Auth,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedRequest {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub request: HttpRequest,
}

/// Where a tab or history entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestOrigin {
    pub collection: CollectionId,
    pub item: ItemId,
}

impl Collection {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

impl Folder {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

impl SavedRequest {
    #[must_use]
    pub fn new(name: impl Into<String>, request: HttpRequest) -> Self {
        Self {
            id: ItemId::new(),
            name: name.into(),
            description: String::new(),
            request,
        }
    }
}

impl Item {
    #[must_use]
    pub const fn id(&self) -> ItemId {
        match self {
            Self::Folder(f) => f.id,
            Self::Request(r) => r.id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Folder(f) => &f.name,
            Self::Request(r) => &r.name,
        }
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        match self {
            Self::Folder(f) => f.name = name.into(),
            Self::Request(r) => r.name = name.into(),
        }
    }

    /// Deep copy with fresh ids for this item and every descendant.
    #[must_use]
    pub fn with_fresh_ids(&self) -> Self {
        match self {
            Self::Request(r) => Self::Request(SavedRequest {
                id: ItemId::new(),
                ..r.clone()
            }),
            Self::Folder(f) => Self::Folder(Folder {
                id: ItemId::new(),
                items: f.items.iter().map(Self::with_fresh_ids).collect(),
                ..f.clone()
            }),
        }
    }
}
