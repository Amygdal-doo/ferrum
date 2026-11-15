//! Share bundles: some or all environments of a project, sealed with the
//! project's [`ShareKey`] and wrapped in a text armor that survives chat
//! and email:
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
//! The base64 body is a JSON envelope {format, version, project,
//! key_fingerprint, nonce, ciphertext}. The ciphertext is XChaCha20-Poly1305
//! over the JSON [`BundleContents`], with `ferrumenv-bundle\0v1\0<project>`
//! as associated data, so the format version and project name can't be
//! swapped. The key itself is never part of a bundle. A `.fenv` file holds
//! the same text.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto;
use crate::model::Environment;
use crate::share_key::ShareKey;
use crate::{Error, Result};

pub const BEGIN: &str = "-----BEGIN FERRUMENV BUNDLE-----";
pub const END: &str = "-----END FERRUMENV BUNDLE-----";
pub const FILE_EXTENSION: &str = "fenv";
const FORMAT: &str = "ferrumenv-bundle";
const VERSION: u32 = 1;

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
    key_fingerprint: String,
    nonce: String,
    ciphertext: String,
}

fn aad(project: &str) -> Vec<u8> {
    format!("{FORMAT}\0v{VERSION}\0{project}").into_bytes()
}

/// Seals `contents` with `key` and returns the armored text.
pub fn seal(contents: &BundleContents, key: &ShareKey) -> Result<String> {
    let plaintext =
        Zeroizing::new(serde_json::to_vec(contents).map_err(|e| Error::Invalid(e.to_string()))?);
    let (nonce, ciphertext) = crypto::seal(key.as_bytes(), &plaintext, &aad(&contents.project))?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        project: contents.project.clone(),
        key_fingerprint: key.fingerprint(),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    };
    let json = serde_json::to_vec(&envelope).map_err(|e| Error::Invalid(e.to_string()))?;
    let body = STANDARD.encode(json);
    let mut out = String::with_capacity(body.len() + 200);
    out.push_str(BEGIN);
    out.push('\n');
    out.push_str(&format!("Project: {}\n", printable(&contents.project)));
    out.push_str(&format!("Key: {}\n\n", envelope.key_fingerprint));
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
    pub key_fingerprint: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl std::fmt::Debug for SealedBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedBundle")
            .field("project", &self.project)
            .field("key_fingerprint", &self.key_fingerprint)
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
    if envelope.version != VERSION {
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
    Ok(SealedBundle {
        nonce: decode(&envelope.nonce)?,
        ciphertext: decode(&envelope.ciphertext)?,
        project: envelope.project,
        key_fingerprint: envelope.key_fingerprint,
    })
}

impl SealedBundle {
    /// Decrypts with `key`. A key with a different fingerprint is reported
    /// as [`Error::KeyMismatch`]; any other failure as
    /// [`Error::BundleDecrypt`].
    pub fn open(&self, key: &ShareKey) -> Result<BundleContents> {
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
}
