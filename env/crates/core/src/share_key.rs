//! Per-project share keys: 256 random bits a team exchanges once (out of
//! band) so anyone holding the key can seal and open that project's
//! bundles.
//!
//! Text form: `fek1_` + base64url(key ‖ checksum), where the checksum is the
//! first 4 bytes of SHA-256("ferrumenv-share-key-v1" ‖ key). A typo or a
//! truncated paste is caught before any decryption is attempted.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{KEY_LEN, constant_time_eq, random_bytes};
use crate::{Error, Result};

pub const PREFIX: &str = "fek1_";
const CHECKSUM_LEN: usize = 4;
const CHECKSUM_DOMAIN: &[u8] = b"ferrumenv-share-key-v1";
const FINGERPRINT_DOMAIN: &[u8] = b"ferrumenv-fingerprint-v1";

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ShareKey([u8; KEY_LEN]);

impl ShareKey {
    /// A new random key.
    pub fn generate() -> Result<Self> {
        Ok(Self(random_bytes::<KEY_LEN>()?))
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// `fek1_…`, the form shown to and copied by the user.
    #[must_use]
    pub fn encode(&self) -> Zeroizing<String> {
        let mut raw = Zeroizing::new(Vec::with_capacity(KEY_LEN + CHECKSUM_LEN));
        raw.extend_from_slice(&self.0);
        raw.extend_from_slice(&checksum(&self.0));
        Zeroizing::new(format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(raw.as_slice())
        ))
    }

    /// Parses the text form; whitespace (from line-wrapped pastes) is
    /// ignored.
    pub fn parse(text: &str) -> Result<Self> {
        let compact: Zeroizing<String> =
            Zeroizing::new(text.chars().filter(|c| !c.is_whitespace()).collect());
        let Some(body) = compact.strip_prefix(PREFIX) else {
            return Err(Error::ShareKey(format!("it should start with {PREFIX}")));
        };
        let raw = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(body)
                .map_err(|_| Error::ShareKey("it contains characters that don't belong".into()))?,
        );
        if raw.len() != KEY_LEN + CHECKSUM_LEN {
            return Err(Error::ShareKey(
                "it has the wrong length (partly copied?)".into(),
            ));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&raw[..KEY_LEN]);
        let key = Self(key);
        if !constant_time_eq(&checksum(&key.0), &raw[KEY_LEN..]) {
            return Err(Error::ShareKey("the checksum doesn't match (typo?)".into()));
        }
        Ok(key)
    }

    /// A short, non-secret id like `3f9a-07c2`, so two people can check
    /// they hold the same key without reading it out.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::new()
            .chain_update(FINGERPRINT_DOMAIN)
            .chain_update(self.0)
            .finalize();
        format!(
            "{:02x}{:02x}-{:02x}{:02x}",
            digest[0], digest[1], digest[2], digest[3]
        )
    }
}

impl PartialEq for ShareKey {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&self.0, &other.0)
    }
}

impl Eq for ShareKey {}

impl std::fmt::Debug for ShareKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShareKey({})", self.fingerprint())
    }
}

impl Serialize for ShareKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for ShareKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

fn checksum(key: &[u8; KEY_LEN]) -> [u8; CHECKSUM_LEN] {
    let digest = Sha256::new()
        .chain_update(CHECKSUM_DOMAIN)
        .chain_update(key)
        .finalize();
    let mut out = [0u8; CHECKSUM_LEN];
    out.copy_from_slice(&digest[..CHECKSUM_LEN]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_parses() {
        let key = ShareKey::generate().expect("key");
        let text = key.encode();
        assert!(text.starts_with(PREFIX));
        assert_eq!(text.len(), PREFIX.len() + 48);
        assert_eq!(ShareKey::parse(&text).expect("parse"), key);
        // Wrapped and padded with spaces, as pasted from chat.
        let wrapped = format!("  {}\n{}  ", &text[..20], &text[20..]);
        assert_eq!(ShareKey::parse(&wrapped).expect("parse"), key);
        assert_ne!(ShareKey::generate().expect("key"), key);
    }

    #[test]
    fn typos_are_detected() {
        let key = ShareKey::from_bytes([7; KEY_LEN]);
        let text = key.encode().to_string();
        // Change one character in the key part.
        let mut chars: Vec<char> = text.chars().collect();
        let i = PREFIX.len() + 3;
        chars[i] = if chars[i] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        let err = ShareKey::parse(&typo).expect_err("typo").to_string();
        assert!(err.contains("checksum"), "{err}");
        assert!(ShareKey::parse(&text[..text.len() - 2]).is_err());
        assert!(ShareKey::parse(&text[PREFIX.len()..]).is_err());
        assert!(ShareKey::parse("fek1_!!!").is_err());
        assert!(ShareKey::parse("").is_err());
    }

    #[test]
    fn fingerprint_and_debug_hide_the_key() {
        let key = ShareKey::from_bytes([1; KEY_LEN]);
        let fp = key.fingerprint();
        assert_eq!(fp.len(), 9);
        assert_eq!(fp, ShareKey::from_bytes([1; KEY_LEN]).fingerprint());
        assert_ne!(fp, ShareKey::from_bytes([2; KEY_LEN]).fingerprint());
        let debug = format!("{key:?}");
        assert!(!debug.contains(key.encode().as_str()));
        assert!(debug.contains(&fp));
    }

    #[test]
    fn serde_uses_the_text_form() {
        let key = ShareKey::from_bytes([9; KEY_LEN]);
        let json = serde_json::to_string(&key).expect("ser");
        assert_eq!(json, format!("\"{}\"", key.encode().as_str()));
        let back: ShareKey = serde_json::from_str(&json).expect("de");
        assert_eq!(back, key);
        assert!(serde_json::from_str::<ShareKey>("\"fek1_nope\"").is_err());
    }
}
