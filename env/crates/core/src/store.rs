//! The vault file (`~/.ferrum/env/vault.json`):
//!
//! ```json
//! { "format": "ferrumenv-vault", "version": 1,
//!   "kdf": { "algorithm": "argon2id", "memory_kib": 65536, "iterations": 3,
//!            "parallelism": 1, "salt": "<base64>" },
//!   "cipher": "xchacha20poly1305", "nonce": "<base64>", "ciphertext": "<base64>" }
//! ```
//!
//! The ciphertext is the JSON-serialized [`Vault`]; the header fields are
//! bound to it as associated data. Every save writes atomically (temp +
//! rename, 0600) after copying the previous file to `vault.json.bak`.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, SALT_LEN, SecretKey};
use crate::fsutil::{copy_private, write_private};
use crate::model::{VAULT_VERSION, Vault};
use crate::{Error, Result};

const FORMAT: &str = "ferrumenv-vault";
const FILE_VERSION: u32 = 1;
const KDF_ALGORITHM: &str = "argon2id";
const CIPHER: &str = "xchacha20poly1305";

/// `~/.ferrum/env/vault.json`.
#[must_use]
pub fn default_vault_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join(".ferrum")
            .join("env")
            .join("vault.json"),
    )
}

/// The backup written before each save.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

#[derive(Serialize, Deserialize)]
struct VaultFile {
    format: String,
    version: u32,
    kdf: KdfHeader,
    cipher: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct KdfHeader {
    algorithm: String,
    #[serde(flatten)]
    params: KdfParams,
    salt: String,
}

fn aad(params: &KdfParams, salt_b64: &str) -> Vec<u8> {
    format!(
        "{FORMAT}\0v{FILE_VERSION}\0{KDF_ALGORITHM}\0{}\0{}\0{}\0{salt_b64}\0{CIPHER}",
        params.memory_kib, params.iterations, params.parallelism
    )
    .into_bytes()
}

/// An unlocked vault file: the derived key, held until the app locks.
/// Dropping it wipes the key.
pub struct Session {
    path: PathBuf,
    key: SecretKey,
    salt: [u8; SALT_LEN],
    params: KdfParams,
}

impl Session {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn params(&self) -> KdfParams {
        self.params
    }

    /// Creates a new vault file at `path`, refusing to overwrite one.
    pub fn create(path: &Path, password: &str, params: KdfParams, vault: &Vault) -> Result<Self> {
        if path.exists() {
            return Err(Error::VaultExists(path.display().to_string()));
        }
        let salt = crypto::random_bytes::<SALT_LEN>()?;
        let key = crypto::derive_key(password, &salt, params)?;
        let session = Self {
            path: path.to_path_buf(),
            key,
            salt,
            params,
        };
        session.save(vault)?;
        Ok(session)
    }

    /// Opens the vault at `path` with `password`.
    pub fn unlock(path: &Path, password: &str) -> Result<(Self, Vault)> {
        let text = std::fs::read_to_string(path)?;
        let file: VaultFile =
            serde_json::from_str(&text).map_err(|e| Error::CorruptVault(e.to_string()))?;
        if file.format != FORMAT {
            return Err(Error::CorruptVault("unknown format".into()));
        }
        if file.version > FILE_VERSION {
            return Err(Error::UnsupportedVersion(file.version));
        }
        if file.kdf.algorithm != KDF_ALGORITHM || file.cipher != CIPHER {
            return Err(Error::CorruptVault("unknown algorithm".into()));
        }
        file.kdf.params.validate()?;
        let salt: [u8; SALT_LEN] = decode(&file.kdf.salt)?
            .try_into()
            .map_err(|_| Error::CorruptVault("bad salt".into()))?;
        let nonce = decode(&file.nonce)?;
        let ciphertext = decode(&file.ciphertext)?;
        let key = crypto::derive_key(password, &salt, file.kdf.params)?;
        let plaintext = crypto::open(
            &key,
            &nonce,
            &ciphertext,
            &aad(&file.kdf.params, &file.kdf.salt),
        )
        .ok_or(Error::WrongPassword)?;
        let vault: Vault = serde_json::from_slice(&plaintext)
            .map_err(|e| Error::CorruptVault(format!("contents: {e}")))?;
        if vault.version > VAULT_VERSION {
            return Err(Error::UnsupportedVersion(vault.version));
        }
        Ok((
            Self {
                path: path.to_path_buf(),
                key,
                salt,
                params: file.kdf.params,
            },
            vault,
        ))
    }

    /// Encrypts `vault` with a fresh nonce and writes it.
    pub fn save(&self, vault: &Vault) -> Result<()> {
        let plaintext =
            Zeroizing::new(serde_json::to_vec(vault).map_err(|e| Error::Invalid(e.to_string()))?);
        let salt_b64 = STANDARD.encode(self.salt);
        let (nonce, ciphertext) =
            crypto::seal(&self.key, &plaintext, &aad(&self.params, &salt_b64))?;
        let file = VaultFile {
            format: FORMAT.into(),
            version: FILE_VERSION,
            kdf: KdfHeader {
                algorithm: KDF_ALGORITHM.into(),
                params: self.params,
                salt: salt_b64,
            },
            cipher: CIPHER.into(),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        let text =
            serde_json::to_string_pretty(&file).map_err(|e| Error::Invalid(e.to_string()))?;
        if self.path.exists() {
            copy_private(&self.path, &backup_path(&self.path))?;
        }
        write_private(&self.path, text.as_bytes())?;
        Ok(())
    }

    /// A second handle on the same vault and key, e.g. to run a re-key
    /// ([`Session::change_password`]) on a background thread.
    #[must_use]
    pub fn duplicate(&self) -> Self {
        Self {
            path: self.path.clone(),
            key: self.key.clone(),
            salt: self.salt,
            params: self.params,
        }
    }

    /// Checks `password` against this session's key without touching disk.
    pub fn verify_password(&self, password: &str) -> Result<bool> {
        let key = crypto::derive_key(password, &self.salt, self.params)?;
        Ok(crypto::constant_time_eq(key.as_ref(), self.key.as_ref()))
    }

    /// Re-keys the vault: checks `current`, derives a key from `new` with a
    /// new salt and `params`, and re-encrypts `vault` with it.
    pub fn change_password(
        &mut self,
        current: &str,
        new: &str,
        params: KdfParams,
        vault: &Vault,
    ) -> Result<()> {
        if !self.verify_password(current)? {
            return Err(Error::WrongPassword);
        }
        let salt = crypto::random_bytes::<SALT_LEN>()?;
        let key = crypto::derive_key(new, &salt, params)?;
        let previous = std::mem::replace(
            self,
            Self {
                path: self.path.clone(),
                key,
                salt,
                params,
            },
        );
        if let Err(e) = self.save(vault) {
            *self = previous;
            return Err(e);
        }
        Ok(())
    }
}

fn decode(text: &str) -> Result<Vec<u8>> {
    STANDARD
        .decode(text)
        .map_err(|_| Error::CorruptVault("bad base64".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Variable;

    const FAST: KdfParams = KdfParams::INSECURE_FAST;

    fn sample() -> Vault {
        let mut vault = Vault::default();
        let i = vault.add_project("shop", None).expect("project");
        vault.projects[i]
            .environment_mut("development")
            .upsert(Variable::new("STRIPE_KEY", "sk_test_123"));
        vault
    }

    #[test]
    fn create_unlock_save_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("env/vault.json");
        let vault = sample();
        let session = Session::create(&path, "correct horse", FAST, &vault).expect("create");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(!text.contains("sk_test_123"));
        assert!(!text.contains("STRIPE_KEY"));
        assert!(Session::create(&path, "x", FAST, &vault).is_err());

        let (_, opened) = Session::unlock(&path, "correct horse").expect("unlock");
        assert_eq!(opened, vault);

        let mut changed = opened.clone();
        changed.settings.auto_lock_minutes = 5;
        session.save(&changed).expect("save");
        assert!(backup_path(&path).exists());
        let (_, opened) = Session::unlock(&path, "correct horse").expect("unlock");
        assert_eq!(opened.settings.auto_lock_minutes, 5);
        // The backup is the previous version, still encrypted.
        let (_, old) = Session::unlock(&backup_path(&path), "correct horse").expect("backup");
        assert_eq!(old.settings.auto_lock_minutes, 30);
    }

    #[test]
    fn wrong_password_and_tampering_are_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.json");
        Session::create(&path, "correct horse", FAST, &sample()).expect("create");
        assert!(matches!(
            Session::unlock(&path, "wrong horse"),
            Err(Error::WrongPassword)
        ));

        let text = std::fs::read_to_string(&path).expect("read");
        let mut json: serde_json::Value = serde_json::from_str(&text).expect("json");
        // Flip a ciphertext byte.
        let ct = json["ciphertext"].as_str().expect("ct").to_string();
        let mut bytes = STANDARD.decode(&ct).expect("b64");
        bytes[3] ^= 0x40;
        json["ciphertext"] = STANDARD.encode(&bytes).into();
        std::fs::write(&path, json.to_string()).expect("write");
        assert!(matches!(
            Session::unlock(&path, "correct horse"),
            Err(Error::WrongPassword)
        ));

        // Header tampering (a different but valid salt length) also fails.
        json["ciphertext"] = ct.into();
        json["kdf"]["salt"] = STANDARD.encode([0u8; SALT_LEN]).into();
        std::fs::write(&path, json.to_string()).expect("write");
        assert!(Session::unlock(&path, "correct horse").is_err());

        json["kdf"]["memory_kib"] = u32::MAX.into();
        std::fs::write(&path, json.to_string()).expect("write");
        assert!(matches!(
            Session::unlock(&path, "correct horse"),
            Err(Error::CorruptVault(_))
        ));

        std::fs::write(&path, "garbage").expect("write");
        assert!(matches!(
            Session::unlock(&path, "correct horse"),
            Err(Error::CorruptVault(_))
        ));
        assert!(matches!(
            Session::unlock(&dir.path().join("missing.json"), "x"),
            Err(Error::Io(_))
        ));
    }

    #[test]
    fn newer_versions_are_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.json");
        Session::create(&path, "pw-pw-pw-pw", FAST, &Vault::default()).expect("create");
        let text = std::fs::read_to_string(&path).expect("read");
        let mut json: serde_json::Value = serde_json::from_str(&text).expect("json");
        json["version"] = 99.into();
        std::fs::write(&path, json.to_string()).expect("write");
        assert!(matches!(
            Session::unlock(&path, "pw-pw-pw-pw"),
            Err(Error::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn change_password_re_encrypts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.json");
        let vault = sample();
        let mut session = Session::create(&path, "old password", FAST, &vault).expect("create");
        assert!(session.verify_password("old password").expect("verify"));
        assert!(matches!(
            session.change_password("nope", "new password", FAST, &vault),
            Err(Error::WrongPassword)
        ));
        session
            .change_password("old password", "new password", FAST, &vault)
            .expect("change");
        assert!(Session::unlock(&path, "old password").is_err());
        let (_, opened) = Session::unlock(&path, "new password").expect("unlock");
        assert_eq!(opened, vault);
        session.save(&vault).expect("save with new key");
        assert!(Session::unlock(&path, "new password").is_ok());
    }
}
