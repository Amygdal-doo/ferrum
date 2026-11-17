use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every failure the core reports. Messages are written for the user and
/// never contain secret values.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Wrong master password (or the vault file was modified).")]
    WrongPassword,
    #[error("The vault file is damaged or not a FerrumVariables vault: {0}")]
    CorruptVault(String),
    #[error("This vault was written by a newer FerrumVariables (format {0}).")]
    UnsupportedVersion(u32),
    #[error("A vault already exists at {0}.")]
    VaultExists(String),
    #[error("Invalid share key: {0}")]
    ShareKey(String),
    #[error("Not a FerrumVariables bundle: {0}")]
    BadBundle(String),
    #[error(
        "This bundle was encrypted with a different share key (bundle {bundle}, yours {yours})."
    )]
    KeyMismatch { bundle: String, yours: String },
    #[error("Could not decrypt the bundle: wrong share key, or the bundle was modified.")]
    BundleDecrypt,
    #[error("Invalid public key: {0}")]
    PublicKey(String),
    #[error(
        "This bundle is not encrypted to your key. Ask the sender to add your public key ({0}) to the project's team."
    )]
    NotARecipient(String),
    #[error("This bundle is encrypted to team members' public keys, not with a share key.")]
    NeedsIdentity,
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
