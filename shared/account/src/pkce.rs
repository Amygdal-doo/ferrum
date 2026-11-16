//! PKCE (RFC 7636, S256) and the random `state` of one sign-in.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::{AccountError, Result};

/// `base64url(n random bytes)`, unpadded.
pub fn random_b64(bytes: usize) -> Result<String> {
    let mut buf = vec![0_u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| AccountError::Invalid(format!("no randomness: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// The verifier kept by the app and the challenge sent in the authorize URL.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// A fresh 43-character verifier (32 random bytes).
    pub fn generate() -> Result<Self> {
        Ok(Self::from_verifier(random_b64(32)?))
    }

    #[must_use]
    pub fn from_verifier(verifier: String) -> Self {
        let challenge = challenge(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

/// `base64url(sha256(verifier))`, unpadded (the S256 method).
#[must_use]
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_rfc_7636_example() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_are_43_url_safe_chars_and_unique() {
        let a = Pkce::generate().expect("pkce");
        let b = Pkce::generate().expect("pkce");
        assert_eq!(a.verifier.len(), 43);
        assert!(
            a.verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_ne!(a.verifier, b.verifier);
        assert_eq!(a.challenge, challenge(&a.verifier));
    }
}
