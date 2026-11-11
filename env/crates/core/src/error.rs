use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every failure the core reports. Messages are written for the user and
/// never contain secret values.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Wrong master password (or the vault file was modified).")]
    WrongPassword,
    #[error("The vault file is damaged or not a FerrumEnv vault: {0}")]
    CorruptVault(String),
    #[error("This vault was written by a newer FerrumEnv (format {0}).")]
    UnsupportedVersion(u32),
    #[error("A vault already exists at {0}.")]
    VaultExists(String),
    #[error("Invalid share key: {0}")]
    ShareKey(String),
    #[error("Not a FerrumEnv bundle: {0}")]
    BadBundle(String),
    #[error(
        "This bundle was encrypted with a different share key (bundle {bundle}, yours {yours})."
    )]
    KeyMismatch { bundle: String, yours: String },
    #[error("Could not decrypt the bundle: wrong share key, or the bundle was modified.")]
    BundleDecrypt,
    #[error("{0}")]
    Invalid(String),
    #[error("Key derivation failed: {0}")]
    Kdf(String),
    #[error("The system random number generator failed: {0}")]
    Random(String),
    #[error("Encryption failed.")]
    Encrypt,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
