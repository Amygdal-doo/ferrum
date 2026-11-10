//! Strongly typed UUID identifiers.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(
    /// A collection (root of a request tree).
    CollectionId
);
id_type!(
    /// A folder or saved request inside a collection.
    ItemId
);
id_type!(
    /// An environment. The globals environment uses [`EnvironmentId::GLOBALS`].
    EnvironmentId
);
id_type!(
    /// A history entry.
    HistoryId
);
id_type!(
    /// An open request tab.
    TabId
);
id_type!(
    /// A mock response attached to a saved request.
    MockId
);

impl EnvironmentId {
    /// Fixed id of the single globals environment.
    pub const GLOBALS: Self = Self(Uuid::nil());

    #[must_use]
    pub fn is_globals(self) -> bool {
        self == Self::GLOBALS
    }
}
