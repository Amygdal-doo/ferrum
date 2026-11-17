//! Share bundles: some or all environments of a project, sealed and
//! wrapped in a text armor that survives chat and email:
//!
//! ```text
//! -----BEGIN FERRUMENV BUNDLE-----
//! Project: shop
//! Key: 3f9a-07c2
//!
//! eyJmb3JtYXQiOiJmZXJydW1lbnYtYnVuZGxlIiwidmVyc2lvbiI6MSwicHJvamVjdCI6…
//! -----END FERRUMENV BUNDLE-----
//! ```
//!
//! Two formats, both read:
//!
//! - **v1, share key** ([`seal`]): the base64 body is a JSON envelope
//!   {format, version, project, key_fingerprint, nonce, ciphertext}. The
//!   ciphertext is XChaCha20-Poly1305 under the project's [`ShareKey`] over
//!   the JSON [`BundleContents`], with `ferrumenv-bundle\0v1\0<project>` as
//!   associated data, so the format version and project name can't be
//!   swapped. The key itself is never part of a bundle.
//! - **v2, team members' public keys** ([`seal_for`]): the envelope holds
//!   the sender's public key and one stanza per recipient instead of a key
//!   fingerprint; see [`crate::recipients`]. The armor shows `From:` (the
//!   sender's fingerprint) instead of `Key:`.
//!
//! A `.fenv` file holds the same text.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto;
use crate::identity::{Identity, PublicKey};
use crate::model::Environment;
use crate::recipients::{self, Stanza};
use crate::share_key::ShareKey;
use crate::{Error, Result};

pub const BEGIN: &str = "-----BEGIN FERRUMENV BUNDLE-----";
pub const END: &str = "-----END FERRUMENV BUNDLE-----";
pub const FILE_EXTENSION: &str = "fenv";
const FORMAT: &str = "ferrumenv-bundle";
const VERSION: u32 = 1;
/// Encrypted to team members' public keys.
const RECIPIENTS_VERSION: u32 = 2;

/// What a bundle carries (encrypted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleContents {
    pub project: String,
    pub environments: Vec<Environment>,
    pub created_at: DateTime<Utc>,
    /// Free text from the sender, e.g. "rotated Stripe keys".
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    project: String,
    /// v1 only.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    key_fingerprint: String,
    /// v2 only: the sender's public key (base64).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    sender: String,
    /// v2 only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recipients: Vec<Stanza>,
    nonce: String,
    ciphertext: String,
}

fn aad(project: &str) -> Vec<u8> {
    format!("{FORMAT}\0v{VERSION}\0{project}").into_bytes()
}

/// Seals `contents` with `key` (format v1) and returns the armored text.
pub fn seal(contents: &BundleContents, key: &ShareKey) -> Result<String> {
    let plaintext = to_json(contents)?;
    let (nonce, ciphertext) = crypto::seal(key.as_bytes(), &plaintext, &aad(&contents.project))?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        project: contents.project.clone(),
        key_fingerprint: key.fingerprint(),
        sender: String::new(),
        recipients: Vec::new(),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    };
    let header = format!("Key: {}", envelope.key_fingerprint);
    armor(&envelope, &header)
}

/// Seals `contents` from `sender` to each of `recipients` (format v2).
/// Include the sender's own public key to be able to read it back.
pub fn seal_for(
    contents: &BundleContents,
    sender: &Identity,
    recipients: &[PublicKey],
) -> Result<String> {
    let plaintext = to_json(contents)?;
    let sealed = recipients::seal(FORMAT, &contents.project, &plaintext, sender, recipients)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: RECIPIENTS_VERSION,
        project: contents.project.clone(),
        key_fingerprint: String::new(),
        sender: STANDARD.encode(sealed.sender.as_bytes()),
        recipients: sealed.stanzas,
        nonce: STANDARD.encode(sealed.nonce),
        ciphertext: STANDARD.encode(sealed.ciphertext),
    };
    let header = format!("From: {}", sealed.sender.fingerprint());
    armor(&envelope, &header)
}

fn to_json(contents: &BundleContents) -> Result<Zeroizing<Vec<u8>>> {
    Ok(Zeroizing::new(
        serde_json::to_vec(contents).map_err(|e| Error::Invalid(e.to_string()))?,
    ))
}

fn armor(envelope: &Envelope, header: &str) -> Result<String> {
    let json = serde_json::to_vec(envelope).map_err(|e| Error::Invalid(e.to_string()))?;
    let body = STANDARD.encode(json);
    let mut out = String::with_capacity(body.len() + 200);
    out.push_str(BEGIN);
    out.push('\n');
    out.push_str(&format!("Project: {}\n", printable(&envelope.project)));
    out.push_str(&format!("{header}\n\n"));
    for chunk in body.as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push('\n');
    }
    out.push_str(END);
    out.push('\n');
    Ok(out)
}

fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// A parsed but still encrypted bundle.
pub struct SealedBundle {
    pub project: String,
    /// The share key's fingerprint (v1); empty for a v2 bundle.
    pub key_fingerprint: String,
    /// Who sealed it (v2); `None` for a share-key bundle.
    pub sender: Option<PublicKey>,
    recipients: Vec<Stanza>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl std::fmt::Debug for SealedBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedBundle")
            .field("project", &self.project)
            .field("key_fingerprint", &self.key_fingerprint)
            .field("sender", &self.sender)
            .finish_non_exhaustive()
    }
}

/// Finds and parses the armored bundle in `text` (surrounding chat text,
/// CRLF line endings and indentation are tolerated).
pub fn parse(text: &str) -> Result<SealedBundle> {
    let start = text
        .find(BEGIN)
        .ok_or_else(|| Error::BadBundle(format!("no \"{BEGIN}\" line found")))?;
    let rest = &text[start + BEGIN.len()..];
    let end = rest
        .find(END)
        .ok_or_else(|| Error::BadBundle("the END line is missing (partly copied?)".into()))?;
    let body: String = rest[..end]
        .lines()
        .map(str::trim)
        // Informational `Name: value` headers are skipped.
        .filter(|line| !line.contains(':'))
        .collect();
    let json = STANDARD
        .decode(body.as_bytes())
        .map_err(|_| Error::BadBundle("the text is damaged (bad base64)".into()))?;
    let envelope: Envelope = serde_json::from_slice(&json)
        .map_err(|_| Error::BadBundle("the text is damaged (bad envelope)".into()))?;
    if envelope.format != FORMAT {
        return Err(Error::BadBundle("unknown format".into()));
    }
    if envelope.version != VERSION && envelope.version != RECIPIENTS_VERSION {
        return Err(Error::BadBundle(format!(
            "made by a different FerrumVariables version (bundle format {})",
            envelope.version
        )));
    }
    let decode = |field: &str| {
        STANDARD
            .decode(field)
            .map_err(|_| Error::BadBundle("the text is damaged".into()))
    };
    let sender = if envelope.version == RECIPIENTS_VERSION {
        if envelope.recipients.is_empty() {
            return Err(Error::BadBundle("it has no recipients".into()));
        }
        let key = PublicKey::from_bytes(&decode(&envelope.sender)?)
            .map_err(|_| Error::BadBundle("the sender's key is damaged".into()))?;
        Some(key)
    } else {
        None
    };
    Ok(SealedBundle {
        nonce: decode(&envelope.nonce)?,
        ciphertext: decode(&envelope.ciphertext)?,
        project: envelope.project,
        key_fingerprint: envelope.key_fingerprint,
        sender,
        recipients: envelope.recipients,
    })
}

impl SealedBundle {
    /// Encrypted to team members' public keys (v2), not a share key.
    #[must_use]
    pub fn is_for_recipients(&self) -> bool {
        self.sender.is_some()
    }

    /// Decrypts a v1 bundle with `key`. A key with a different fingerprint
    /// is reported as [`Error::KeyMismatch`]; any other failure as
    /// [`Error::BundleDecrypt`].
    pub fn open(&self, key: &ShareKey) -> Result<BundleContents> {
        if self.is_for_recipients() {
            return Err(Error::NeedsIdentity);
        }
        let yours = key.fingerprint();
        if yours != self.key_fingerprint {
            return Err(Error::KeyMismatch {
                bundle: self.key_fingerprint.clone(),
                yours,
            });
        }
        let plaintext = crypto::open(
            key.as_bytes(),
            &self.nonce,
            &self.ciphertext,
            &aad(&self.project),
        )
        .ok_or(Error::BundleDecrypt)?;
        let contents: BundleContents =
            serde_json::from_slice(&plaintext).map_err(|_| Error::BundleDecrypt)?;
        if contents.project != self.project {
            return Err(Error::BundleDecrypt);
        }
        Ok(contents)
    }

    /// Decrypts a v2 bundle addressed to `me`. Returns the contents and
    /// the sender's public key, authenticated: check it against the
    /// project's members before trusting the contents.
    pub fn open_as(&self, me: &Identity) -> Result<(BundleContents, PublicKey)> {
        let Some(sender) = self.sender else {
            return Err(Error::BadBundle(
                "it is encrypted with a share key, not to team members".into(),
            ));
        };
        let nonce: [u8; crypto::NONCE_LEN] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| Error::BundleDecrypt)?;
        let sealed = recipients::Sealed {
            sender,
            stanzas: self.recipients.clone(),
            nonce,
            ciphertext: self.ciphertext.clone(),
        };
        let plaintext = recipients::open(FORMAT, &self.project, &sealed, me)?;
        let contents: BundleContents =
            serde_json::from_slice(&plaintext).map_err(|_| Error::BundleDecrypt)?;
        if contents.project != self.project {
            return Err(Error::BundleDecrypt);
        }
        Ok((contents, sender))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Variable;

    fn contents() -> BundleContents {
        let mut env = Environment::new("staging");
        env.upsert(Variable::new("STRIPE_KEY", "sk_live_abc"));
        env.upsert(Variable::new("PORT", "8080"));
        BundleContents {
            project: "shop".into(),
            environments: vec![env],
            created_at: Utc::now(),
            note: "new stripe key".into(),
        }
    }

    #[test]
    fn round_trip_through_chat() {
        let key = ShareKey::generate().expect("key");
        let original = contents();
        let armored = seal(&original, &key).expect("seal");
        assert!(armored.starts_with(BEGIN));
        assert!(armored.contains("Project: shop"));
        assert!(!armored.contains("sk_live_abc"));
        assert!(!armored.contains(key.encode().as_str()));
        assert!(
            armored
                .lines()
                .all(|l| l.len() <= 64 || l.starts_with("-----"))
        );
        // Quoted in a chat message with CRLF and indentation.
        let pasted = format!(
            "hey, here are the envs:\r\n{}\r\nthanks",
            armored
                .lines()
                .map(|l| format!("   {l}"))
                .collect::<Vec<_>>()
                .join("\r\n")
        );
        let sealed = parse(&pasted).expect("parse");
        assert_eq!(sealed.project, "shop");
        assert_eq!(sealed.key_fingerprint, key.fingerprint());
        assert_eq!(sealed.open(&key).expect("open"), original);
    }

    #[test]
    fn wrong_key_and_tampering() {
        let key = ShareKey::generate().expect("key");
        let armored = seal(&contents(), &key).expect("seal");
        let other = ShareKey::generate().expect("key");
        assert!(matches!(
            parse(&armored).expect("parse").open(&other),
            Err(Error::KeyMismatch { .. })
        ));

        // Rename the project in the envelope: AAD no longer matches.
        let sealed = parse(&armored).expect("parse");
        let renamed = SealedBundle {
            project: "other".into(),
            key_fingerprint: sealed.key_fingerprint.clone(),
            sender: None,
            recipients: Vec::new(),
            nonce: sealed.nonce.clone(),
            ciphertext: sealed.ciphertext.clone(),
        };
        assert!(matches!(renamed.open(&key), Err(Error::BundleDecrypt)));
        let mut flipped = parse(&armored).expect("parse");
        flipped.ciphertext[0] ^= 1;
        assert!(matches!(flipped.open(&key), Err(Error::BundleDecrypt)));

        assert!(matches!(parse("hello"), Err(Error::BadBundle(_))));
        let cut = &armored[..armored.len() / 2];
        assert!(matches!(parse(cut), Err(Error::BadBundle(_))));
        let garbled = armored.replacen("Key:", "Key:\n@@@@", 1);
        assert!(matches!(parse(&garbled), Err(Error::BadBundle(_))));
    }

    #[test]
    fn team_bundles_open_for_members_only() {
        let alice = Identity::generate().expect("id");
        let bob = Identity::generate().expect("id");
        let eve = Identity::generate().expect("id");
        let to = [
            alice.public_key().expect("pk"),
            bob.public_key().expect("pk"),
        ];
        let original = contents();
        let armored = seal_for(&original, &alice, &to).expect("seal");
        assert!(armored.contains("Project: shop"));
        let from = format!("From: {}", alice.public_key().expect("pk").fingerprint());
        assert!(armored.contains(&from));
        assert!(!armored.contains("sk_live_abc"));
        assert!(!armored.contains("STRIPE_KEY"));

        let sealed = parse(&armored).expect("parse");
        assert!(sealed.is_for_recipients());
        assert!(sealed.key_fingerprint.is_empty());
        let (opened, sender) = sealed.open_as(&bob).expect("bob opens");
        assert_eq!(opened, original);
        assert_eq!(sender, alice.public_key().expect("pk"));
        assert_eq!(sealed.open_as(&alice).expect("alice opens").0, original);
        assert!(matches!(sealed.open_as(&eve), Err(Error::NotARecipient(_))));
        // A share key can't open it, and a v1 bundle needs one.
        let key = ShareKey::generate().expect("key");
        assert!(matches!(sealed.open(&key), Err(Error::NeedsIdentity)));
        let v1 = parse(&seal(&original, &key).expect("seal")).expect("parse");
        assert!(!v1.is_for_recipients());
        assert!(v1.open_as(&bob).is_err());
        assert_eq!(v1.open(&key).expect("v1 still opens"), original);
    }

    #[test]
    fn team_bundle_tampering_is_detected() {
        let alice = Identity::generate().expect("id");
        let bob = Identity::generate().expect("id");
        let armored =
            seal_for(&contents(), &alice, &[bob.public_key().expect("pk")]).expect("seal");
        let mut renamed = parse(&armored).expect("parse");
        renamed.project = "other".into();
        assert!(matches!(renamed.open_as(&bob), Err(Error::BundleDecrypt)));
        let mut flipped = parse(&armored).expect("parse");
        flipped.ciphertext[0] ^= 1;
        assert!(matches!(flipped.open_as(&bob), Err(Error::BundleDecrypt)));
        let mut forged = parse(&armored).expect("parse");
        forged.sender = Some(Identity::generate().expect("id").public_key().expect("pk"));
        assert!(matches!(forged.open_as(&bob), Err(Error::BundleDecrypt)));
    }
}
