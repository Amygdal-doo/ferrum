//! The two primitives FerrumVariables uses: Argon2id to turn the master password
//! into a key, and XChaCha20-Poly1305 (random 192-bit nonces, so nonces
//! never need a counter) to seal data with associated data bound to it.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Error, Result};

pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;

/// A 256-bit key, wiped from memory on drop.
pub type SecretKey = Zeroizing<[u8; KEY_LEN]>;

/// Argon2id cost parameters, stored in the vault header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory in KiB.
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    /// 64 MiB, 3 passes, 1 lane: roughly half a second on a laptop.
    pub const DEFAULT: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };

    /// Cheap parameters for tests only.
    pub const INSECURE_FAST: Self = Self {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    };

    /// Rejects parameters a tampered header could use to exhaust memory or
    /// time, and ones Argon2 itself refuses.
    pub fn validate(&self) -> Result<()> {
        let ok = (1..=16).contains(&self.parallelism)
            && (8 * self.parallelism..=4 * 1024 * 1024).contains(&self.memory_kib)
            && (1..=64).contains(&self.iterations);
        if ok {
            Ok(())
        } else {
            Err(Error::CorruptVault(format!(
                "unreasonable key derivation parameters (m={} KiB, t={}, p={})",
                self.memory_kib, self.iterations, self.parallelism
            )))
        }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// `N` bytes from the operating system's CSPRNG.
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| Error::Random(e.to_string()))?;
    Ok(bytes)
}

/// Argon2id(password, salt) → 256-bit key.
pub fn derive_key(password: &str, salt: &[u8], params: KdfParams) -> Result<SecretKey> {
    params.validate()?;
    let argon_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|e| Error::Kdf(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| Error::Kdf(e.to_string()))?;
    Ok(key)
}

/// Encrypts `plaintext` under `key` with a fresh random nonce.
pub fn seal(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>)> {
    let nonce = random_bytes::<NONCE_LEN>()?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Encrypt)?;
    Ok((nonce, ciphertext))
}

/// Decrypts and authenticates; `None` for a wrong key, wrong associated
/// data or any modification.
#[must_use]
pub fn open(
    key: &[u8; KEY_LEN],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Option<Zeroizing<Vec<u8>>> {
    if nonce.len() != NONCE_LEN {
        return None;
    }
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .ok()
        .map(Zeroizing::new)
}

/// Equality without an early exit, for comparing keys.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic_and_salted() {
        let p = KdfParams::INSECURE_FAST;
        let a = derive_key("hunter2 hunter2", b"0123456789abcdef", p).expect("derive");
        let b = derive_key("hunter2 hunter2", b"0123456789abcdef", p).expect("derive");
        let c = derive_key("hunter2 hunter2", b"fedcba9876543210", p).expect("derive");
        let d = derive_key("hunter3 hunter2", b"0123456789abcdef", p).expect("derive");
        assert_eq!(*a, *b);
        assert_ne!(*a, *c);
        assert_ne!(*a, *d);
    }

    #[test]
    fn hostile_params_are_rejected() {
        let huge = KdfParams {
            memory_kib: u32::MAX,
            iterations: 3,
            parallelism: 1,
        };
        assert!(derive_key("x", b"0123456789abcdef", huge).is_err());
        let zero = KdfParams {
            iterations: 0,
            ..KdfParams::DEFAULT
        };
        assert!(zero.validate().is_err());
        assert!(KdfParams::DEFAULT.validate().is_ok());
    }

    #[test]
    fn seal_open_round_trip_and_tamper_detection() {
        let key = random_bytes::<KEY_LEN>().expect("key");
        let (nonce, mut ct) = seal(&key, b"secret", b"aad").expect("seal");
        assert_eq!(
            open(&key, &nonce, &ct, b"aad").expect("open").as_slice(),
            b"secret"
        );
        assert!(open(&key, &nonce, &ct, b"other aad").is_none());
        let other = random_bytes::<KEY_LEN>().expect("key");
        assert!(open(&other, &nonce, &ct, b"aad").is_none());
        assert!(open(&key, &nonce[..12], &ct, b"aad").is_none());
        ct[0] ^= 1;
        assert!(open(&key, &nonce, &ct, b"aad").is_none());
        // Fresh nonce every time.
        let (n2, _) = seal(&key, b"secret", b"aad").expect("seal");
        assert_ne!(nonce, n2);
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
