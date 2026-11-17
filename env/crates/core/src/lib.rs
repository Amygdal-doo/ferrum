//! FerrumVariables core: everything except the UI.
//!
//! - [`model`]: the vault (projects → environments → variables).
//! - [`crypto`]: Argon2id key derivation and XChaCha20-Poly1305 sealing.
//! - [`store`]: the encrypted vault file and the unlocked [`store::Session`].
//! - [`share_key`] and [`bundle`]: per-project keys and encrypted bundles
//!   exchanged between developers.
//! - [`identity`] and [`recipients`]: per-person X25519 keys, so bundles
//!   can be encrypted to each team member instead of a shared key.
//! - [`sync`]: keeping a project in step through a shared folder of team
//!   bundles, with three-way merges and explicit conflicts.
//! - [`leaks`]: finding stored secret values in a project's files and git
//!   history (by hash; values are never printed).
//! - [`dotenv`]: `.env` parsing, writing and `.gitignore` checks.
//! - [`diff`]: comparing environments and previewing imports.
//! - [`lock`] and [`password`]: auto-lock timing and password strength.
//!
//! Secrets live in types that wipe themselves on drop (`zeroize`) and are
//! never printed by `Debug`.

pub mod bundle;
pub mod crypto;
pub mod diff;
pub mod dotenv;
mod error;
mod fsutil;
pub mod identity;
pub mod leaks;
pub mod lock;
pub mod model;
pub mod password;
pub mod recipients;
pub mod share_key;
pub mod store;
pub mod sync;

pub use error::{Error, Result};
pub use fsutil::write_private;
pub use identity::{Identity, Member, PublicKey};
pub use model::{Environment, Project, Settings, Variable, Vault};
pub use share_key::ShareKey;
