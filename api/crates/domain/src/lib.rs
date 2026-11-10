//! Pure FerrumAPI domain types. No UI, no network, no database.
//!
//! Every persisted type is serde (JSON) with `#[serde(default)]` on structs so
//! older records keep loading when fields are added.

mod auth;
mod body;
mod collection;
mod cookie_jar;
mod environment;
mod error;
pub mod highlight;
mod history;
mod ids;
pub mod load;
pub mod mock;
mod request;
mod response;
pub mod text;
mod tree;
pub mod urlparams;
pub mod vars;
pub mod webhook;
mod workspace;

pub use auth::{ApiKeyLocation, Auth};
pub use body::{FormField, FormFieldKind, RawLanguage, RequestBody};
pub use collection::{Collection, Folder, Item, RequestOrigin, SavedRequest};
pub use cookie_jar::{
    CookieJar, StoredCookie, default_path, domain_matches, parse_cookie_date, path_matches,
};
pub use environment::{Environment, Variable};
pub use error::ExecError;
pub use history::HistoryEntry;
pub use ids::{CollectionId, EnvironmentId, HistoryId, ItemId, MockId, TabId};
pub use request::{HttpMethod, HttpRequest, KeyValue, RequestSettings};
pub use response::{HttpResponse, ResponseCookie};
pub use vars::VariableSet;
pub use workspace::{AppSettings, RequestTab, Theme, WorkspaceState};
