//! Pure FerrumAPI domain types. No UI, no network, no database.
//!
//! Every persisted type is serde (JSON) with `#[serde(default)]` on structs so
//! older records keep loading when fields are added.

mod auth;
mod body;
mod collection;
mod environment;
mod error;
mod history;
mod ids;
mod request;
mod response;
pub mod text;
mod tree;
pub mod urlparams;
pub mod vars;
mod workspace;

pub use auth::{ApiKeyLocation, Auth};
pub use body::{FormField, FormFieldKind, RawLanguage, RequestBody};
pub use collection::{Collection, Folder, Item, RequestOrigin, SavedRequest};
pub use environment::{Environment, Variable};
pub use error::ExecError;
pub use history::HistoryEntry;
pub use ids::{CollectionId, EnvironmentId, HistoryId, ItemId, TabId};
pub use request::{HttpMethod, HttpRequest, KeyValue, RequestSettings};
pub use response::{HttpResponse, ResponseCookie};
pub use vars::VariableSet;
pub use workspace::{AppSettings, RequestTab, Theme, WorkspaceState};
