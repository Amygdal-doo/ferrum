//! Per-person keys, so a team shares bundles without a shared password:
//! every vault holds one X25519 [`Identity`] (the private key, encrypted
//! with the rest of the vault), whose [`PublicKey`] is handed out freely.
//! A project lists its [`Member`]s' public keys and bundles are encrypted
//! to each of them (see [`crate::recipients`]).
//!
//! Text form of a public key: `fepk1_` + base64url(key ‖ checksum), the
//! checksum being the first 4 bytes of SHA-256("ferrumenv-public-key-v1" ‖
//! key), like [`crate::ShareKey`]. The fingerprint is 96 bits (`3f9a-07c2-
//! …`, six groups): long enough that nobody can grind a key with the same
//! fingerprint, short enough to read out on a call.
//!
//! X25519 and HKDF come from `aws-lc-rs`, already linked into every Ferrum
//! app through rustls.

use aws_lc_rs::agreement::{self, PrivateKey, UnparsedPublicKey, X25519};
use aws_lc_rs::hkdf;
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{KEY_LEN, SecretKey, constant_time_eq, random_bytes};
use crate::{Error, Result};

pub const PUBLIC_PREFIX: &str = "fepk1_";
const CHECKSUM_LEN: usize = 4;
const CHECKSUM_DOMAIN: &[u8] = b"ferrumenv-public-key-v1";
const FINGERPRINT_DOMAIN: &[u8] = b"ferrumenv-public-key-fingerprint-v1";
const FINGERPRINT_BYTES: usize = 12;

/// Your X25519 private key. Lives only inside the encrypted vault; wiped
/// on drop and never printed.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Identity([u8; KEY_LEN]);

impl Identity {
    /// A new random key pair.
    pub fn generate() -> Result<Self> {
        let identity = Self(random_bytes::<KEY_LEN>()?);
        // Fails only for a broken backend; better now than at first share.
        identity.private_key()?;
        Ok(identity)
    }

    fn private_key(&self) -> Result<PrivateKey> {
        PrivateKey::from_private_key(&X25519, &self.0)
            .map_err(|_| Error::PublicKey("the private key was rejected".into()))
    }

    /// The public half, to give to teammates.
    pub fn public_key(&self) -> Result<PublicKey> {
        let public = self
            .private_key()?
            .compute_public_key()
            .map_err(|_| Error::PublicKey("could not compute the public key".into()))?;
        let bytes: [u8; KEY_LEN] = public
            .as_ref()
            .try_into()
            .map_err(|_| Error::PublicKey("unexpected public key length".into()))?;
        Ok(PublicKey(bytes))
    }

    /// X25519(self, peer). Low-order peer keys (an all-zero result) are
    /// refused.
    pub(crate) fn agree(&self, peer: &PublicKey) -> Result<SecretKey> {
        let private = self.private_key()?;
        agreement::agree(
            &private,
            UnparsedPublicKey::new(&X25519, peer.0),
            Error::PublicKey("that public key cannot be used".into()),
            |shared| {
                if shared.len() != KEY_LEN || constant_time_eq(shared, &[0u8; KEY_LEN]) {
                    return Err(Error::PublicKey("that public key cannot be used".into()));
                }
                let mut out = Zeroizing::new([0u8; KEY_LEN]);
                out.copy_from_slice(shared);
                Ok(out)
            },
        )
    }
}

impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&self.0, &other.0)
    }
}

impl Eq for Identity {}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.public_key() {
            Ok(public) => write!(f, "Identity({})", public.fingerprint()),
            Err(_) => f.write_str("Identity(<invalid>)"),
        }
    }
}

impl Serialize for Identity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&Zeroizing::new(STANDARD.encode(self.0)))
    }
}

impl<'de> Deserialize<'de> for Identity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        let raw = Zeroizing::new(
            STANDARD
                .decode(text.as_bytes())
                .map_err(|_| serde::de::Error::custom("bad identity encoding"))?,
        );
        let bytes: [u8; KEY_LEN] = raw
            .as_slice()
            .try_into()
            .map_err(|_| serde::de::Error::custom("bad identity length"))?;
        Ok(Self(bytes))
    }
}

/// Someone's X25519 public key. Not secret.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; KEY_LEN]);

impl PublicKey {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// From raw bytes (e.g. inside a bundle), checked like [`Self::parse`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let bytes: [u8; KEY_LEN] = bytes
            .try_into()
            .map_err(|_| Error::PublicKey("it has the wrong length".into()))?;
        let key = Self(bytes);
        key.check_usable()?;
        Ok(key)
    }

    /// `fepk1_…`, the form copied and pasted between teammates.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut raw = Vec::with_capacity(KEY_LEN + CHECKSUM_LEN);
        raw.extend_from_slice(&self.0);
        raw.extend_from_slice(&checksum(&self.0));
        format!("{PUBLIC_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw))
    }

    /// Parses the text form; whitespace is ignored. Typos (checksum) and
    /// keys that can't be used for X25519 (low-order points) are refused.
    pub fn parse(text: &str) -> Result<Self> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(body) = compact.strip_prefix(PUBLIC_PREFIX) else {
            return Err(Error::PublicKey(format!(
                "it should start with {PUBLIC_PREFIX}"
            )));
        };
        let raw = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| Error::PublicKey("it contains characters that don't belong".into()))?;
        if raw.len() != KEY_LEN + CHECKSUM_LEN {
            return Err(Error::PublicKey(
                "it has the wrong length (partly copied?)".into(),
            ));
        }
        let mut bytes = [0u8; KEY_LEN];
        bytes.copy_from_slice(&raw[..KEY_LEN]);
        if !constant_time_eq(&checksum(&bytes), &raw[KEY_LEN..]) {
            return Err(Error::PublicKey(
                "the checksum doesn't match (typo?)".into(),
            ));
        }
        let key = Self(bytes);
        key.check_usable()?;
        Ok(key)
    }

    /// A throwaway agreement: refuses keys whose shared secret would be
    /// all zeros.
    fn check_usable(&self) -> Result<()> {
        Identity::generate()?.agree(self).map(drop)
    }

    /// `3f9a-07c2-11ab-9e00-4c1d-d2e7`: compare it with the owner (out loud,
    /// in person) before trusting a pasted key.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::new()
            .chain_update(FINGERPRINT_DOMAIN)
            .chain_update(self.0)
            .finalize();
        digest[..FINGERPRINT_BYTES]
            .chunks(2)
            .map(|pair| format!("{:02x}{:02x}", pair[0], pair[1]))
            .collect::<Vec<_>>()
            .join("-")
    }
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey({})", self.fingerprint())
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
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

/// A teammate a project's bundles are encrypted to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Free text, e.g. "Ana (laptop)".
    pub name: String,
    pub public_key: PublicKey,
}

/// HKDF-SHA256(salt, ikm, info) → a 256-bit key.
pub(crate) fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<SecretKey> {
    struct Len;
    impl hkdf::KeyType for Len {
        fn len(&self) -> usize {
            KEY_LEN
        }
    }
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(ikm)
        .expand(&[info], Len)
        .and_then(|okm| okm.fill(out.as_mut()))
        .map_err(|_| Error::Encrypt)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> [u8; KEY_LEN] {
        let mut out = [0u8; KEY_LEN];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex");
        }
        out
    }

    #[test]
    fn rfc7748_diffie_hellman_vector() {
        // RFC 7748 §6.1 (Alice and Bob).
        let alice = Identity(hex(
            "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
        ));
        let bob = Identity(hex(
            "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
        ));
        assert_eq!(
            alice.public_key().expect("pk").0,
            hex("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
        );
        let shared = hex("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
        let bob_public = bob.public_key().expect("pk");
        let alice_public = alice.public_key().expect("pk");
        assert_eq!(*alice.agree(&bob_public).expect("agree"), shared);
        assert_eq!(*bob.agree(&alice_public).expect("agree"), shared);
    }

    #[test]
    fn public_keys_encode_parse_and_reject_typos() {
        let me = Identity::generate().expect("identity");
        let public = me.public_key().expect("pk");
        let text = public.encode();
        assert!(text.starts_with(PUBLIC_PREFIX));
        assert_eq!(PublicKey::parse(&text).expect("parse"), public);
        let wrapped = format!("  {}\n{} ", &text[..20], &text[20..]);
        assert_eq!(PublicKey::parse(&wrapped).expect("parse"), public);

        let mut chars: Vec<char> = text.chars().collect();
        let i = PUBLIC_PREFIX.len() + 5;
        chars[i] = if chars[i] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert!(PublicKey::parse(&typo).is_err());
        assert!(PublicKey::parse(&text[..text.len() - 3]).is_err());
        assert!(PublicKey::parse("fek1_abc").is_err());
        // A share key is not a public key.
        let share = crate::ShareKey::generate().expect("share").encode();
        assert!(PublicKey::parse(&share).is_err());
    }

    #[test]
    fn low_order_keys_are_refused() {
        let zero = [0u8; KEY_LEN];
        assert!(PublicKey::from_bytes(&zero).is_err());
        let mut raw = zero.to_vec();
        raw.extend_from_slice(&checksum(&zero));
        let text = format!("{PUBLIC_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw));
        assert!(PublicKey::parse(&text).is_err());
        let mut one = [0u8; KEY_LEN];
        one[0] = 1;
        assert!(PublicKey::from_bytes(&one).is_err());
    }

    #[test]
    fn fingerprint_debug_and_serde() {
        let me = Identity::generate().expect("identity");
        let public = me.public_key().expect("pk");
        let fp = public.fingerprint();
        assert_eq!(fp.len(), 6 * 4 + 5);
        assert_ne!(
            fp,
            Identity::generate()
                .expect("id")
                .public_key()
                .expect("pk")
                .fingerprint()
        );
        let debug = format!("{me:?}");
        assert!(debug.contains(&fp));
        assert!(!debug.contains(&STANDARD.encode(me.0)));

        let json = serde_json::to_string(&me).expect("ser");
        let back: Identity = serde_json::from_str(&json).expect("de");
        assert_eq!(back, me);
        let member = Member {
            name: "Ana".into(),
            public_key: public,
        };
        let json = serde_json::to_string(&member).expect("ser");
        assert!(json.contains(&public.encode()));
        assert_eq!(serde_json::from_str::<Member>(&json).expect("de"), member);
    }

    #[test]
    fn hkdf_matches_rfc5869_case_1() {
        let ikm = [0x0bu8; 22];
        let salt: Vec<u8> = (0u8..=0x0c).collect();
        let info: Vec<u8> = (0xf0u8..=0xf9).collect();
        let okm = hkdf_sha256(&salt, &ikm, &info).expect("hkdf");
        assert_eq!(
            *okm,
            hex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf")
        );
    }
}
