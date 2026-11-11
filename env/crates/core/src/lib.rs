//! FerrumEnv core: everything except the UI.
//!
//! - [`model`]: the vault (projects → environments → variables).
//! - [`crypto`]: Argon2id key derivation and XChaCha20-Poly1305 sealing.
//! - [`store`]: the encrypted vault file and the unlocked [`store::Session`].
//! - [`share_key`] and [`bundle`]: per-project keys and encrypted bundles
//!   exchanged between developers.
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
pub mod lock;
pub mod model;
pub mod password;
pub mod share_key;
pub mod store;

pub use error::{Error, Result};
pub use fsutil::write_private;
pub use model::{Environment, Project, Settings, Variable, Vault};
pub use share_key::ShareKey;
