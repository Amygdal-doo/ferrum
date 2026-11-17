//! Sealing a bundle to several public keys (bundle format v2), in the
//! style of age's X25519 recipients and HPKE's authenticated mode, built
//! only from primitives the vault already uses or that are standard:
//!
//! 1. A random 256-bit file key seals the payload with XChaCha20-Poly1305;
//!    the associated data binds the format, the project and the sender's
//!    public key `S`.
//! 2. For each recipient `R`, a fresh ephemeral key `e` (public `E`) and
//!    the sender's identity `s` give
//!    `wrap = HKDF-SHA256(salt = E ‖ S ‖ R, ikm = X25519(e, R) ‖ X25519(s, R),
//!    info = "ferrumenv-bundle-recipient-v2")`, and the file key is sealed
//!    under `wrap` with SHA-256(payload nonce ‖ ciphertext) in the
//!    associated data.
//!
//! The static `X25519(s, R)` term authenticates the sender: only the
//! holder of `s` (or `R` itself) can produce a stanza that `R` opens, so a
//! bundle dropped into a shared folder by someone else is refused. Hashing
//! the payload into every stanza stops one recipient from swapping the
//! payload while keeping the sender's stanzas for the others.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN};
use crate::identity::{Identity, PublicKey, hkdf_sha256};
use crate::{Error, Result};

const INFO: &[u8] = b"ferrumenv-bundle-recipient-v2";

/// One recipient's copy of the file key (all fields base64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Stanza {
    pub recipient: String,
    pub ephemeral: String,
    pub nonce: String,
    pub wrapped: String,
}

/// The payload of a v2 bundle, as sealed.
pub(crate) struct Sealed {
    pub sender: PublicKey,
    pub stanzas: Vec<Stanza>,
    pub nonce: [u8; crypto::NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

pub(crate) fn payload_aad(format: &str, project: &str, sender: &PublicKey) -> Vec<u8> {
    let mut aad = format!("{format}\0v2\0{project}\0").into_bytes();
    aad.extend_from_slice(sender.as_bytes());
    aad
}

fn stanza_aad(format: &str, project: &str, nonce: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let digest = Sha256::new()
        .chain_update(nonce)
        .chain_update(ciphertext)
        .finalize();
    let mut aad = format!("{format}\0v2\0{project}\0recipient\0").into_bytes();
    aad.extend_from_slice(&digest);
    aad
}

fn wrap_key(
    ephemeral_dh: &crypto::SecretKey,
    static_dh: &crypto::SecretKey,
    ephemeral: &PublicKey,
    sender: &PublicKey,
    recipient: &PublicKey,
) -> Result<crypto::SecretKey> {
    let mut ikm = Zeroizing::new(Vec::with_capacity(2 * KEY_LEN));
    ikm.extend_from_slice(ephemeral_dh.as_ref());
    ikm.extend_from_slice(static_dh.as_ref());
    let mut salt = Vec::with_capacity(3 * KEY_LEN);
    salt.extend_from_slice(ephemeral.as_bytes());
    salt.extend_from_slice(sender.as_bytes());
    salt.extend_from_slice(recipient.as_bytes());
    hkdf_sha256(&salt, &ikm, INFO)
}

/// Seals `plaintext` from `sender` to every key of `recipients` (duplicates
/// are dropped).
pub(crate) fn seal(
    format: &str,
    project: &str,
    plaintext: &[u8],
    sender: &Identity,
    recipients: &[PublicKey],
) -> Result<Sealed> {
    if recipients.is_empty() {
        return Err(Error::Invalid("Add at least one team member.".into()));
    }
    let sender_public = sender.public_key()?;
    let file_key = Zeroizing::new(crypto::random_bytes::<KEY_LEN>()?);
    let (nonce, ciphertext) = crypto::seal(
        &file_key,
        plaintext,
        &payload_aad(format, project, &sender_public),
    )?;
    let aad = stanza_aad(format, project, &nonce, &ciphertext);
    let mut seen = Vec::new();
    let mut stanzas = Vec::new();
    for recipient in recipients {
        if seen.contains(recipient) {
            continue;
        }
        seen.push(*recipient);
        let ephemeral = Identity::generate()?;
        let ephemeral_public = ephemeral.public_key()?;
        let key = wrap_key(
            &ephemeral.agree(recipient)?,
            &sender.agree(recipient)?,
            &ephemeral_public,
            &sender_public,
            recipient,
        )?;
        let (wrap_nonce, wrapped) = crypto::seal(&key, file_key.as_ref(), &aad)?;
        stanzas.push(Stanza {
            recipient: STANDARD.encode(recipient.as_bytes()),
            ephemeral: STANDARD.encode(ephemeral_public.as_bytes()),
            nonce: STANDARD.encode(wrap_nonce),
            wrapped: STANDARD.encode(wrapped),
        });
    }
    Ok(Sealed {
        sender: sender_public,
        stanzas,
        nonce,
        ciphertext,
    })
}

/// Opens a v2 payload as `me`. Fails with [`Error::NotARecipient`] when no
/// stanza is addressed to `me`, [`Error::BundleDecrypt`] on any tampering.
pub(crate) fn open(
    format: &str,
    project: &str,
    sealed: &Sealed,
    me: &Identity,
) -> Result<Zeroizing<Vec<u8>>> {
    let my_public = me.public_key()?;
    let mine = STANDARD.encode(my_public.as_bytes());
    let stanza = sealed
        .stanzas
        .iter()
        .find(|s| s.recipient == mine)
        .ok_or_else(|| Error::NotARecipient(my_public.fingerprint()))?;
    let decode = |field: &str| STANDARD.decode(field).map_err(|_| Error::BundleDecrypt);
    let ephemeral = PublicKey::from_bytes(&decode(&stanza.ephemeral)?)?;
    let key = wrap_key(
        &me.agree(&ephemeral)?,
        &me.agree(&sealed.sender)?,
        &ephemeral,
        &sealed.sender,
        &my_public,
    )?;
    let aad = stanza_aad(format, project, &sealed.nonce, &sealed.ciphertext);
    let file_key = crypto::open(
        &key,
        &decode(&stanza.nonce)?,
        &decode(&stanza.wrapped)?,
        &aad,
    )
    .ok_or(Error::BundleDecrypt)?;
    let file_key: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new(
        file_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::BundleDecrypt)?,
    );
    crypto::open(
        &file_key,
        &sealed.nonce,
        &sealed.ciphertext,
        &payload_aad(format, project, &sealed.sender),
    )
    .ok_or(Error::BundleDecrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people(n: usize) -> Vec<Identity> {
        (0..n)
            .map(|_| Identity::generate().expect("identity"))
            .collect()
    }

    #[test]
    fn every_recipient_opens_and_others_cannot() {
        let ids = people(4);
        let (sender, a, b, outsider) = (&ids[0], &ids[1], &ids[2], &ids[3]);
        let to: Vec<PublicKey> = [sender, a, b, a]
            .iter()
            .map(|i| i.public_key().expect("pk"))
            .collect();
        let sealed = seal("f", "shop", b"secret", sender, &to).expect("seal");
        assert_eq!(sealed.stanzas.len(), 3, "duplicates dropped");
        for who in [sender, a, b] {
            assert_eq!(
                open("f", "shop", &sealed, who).expect("open").as_slice(),
                b"secret"
            );
        }
        assert!(matches!(
            open("f", "shop", &sealed, outsider),
            Err(Error::NotARecipient(_))
        ));
        assert!(matches!(
            open("f", "other", &sealed, a),
            Err(Error::BundleDecrypt)
        ));
        assert!(seal("f", "shop", b"x", sender, &[]).is_err());
    }

    #[test]
    fn sender_is_authenticated() {
        let ids = people(3);
        let (sender, a, forger) = (&ids[0], &ids[1], &ids[2]);
        let to = [a.public_key().expect("pk")];
        let mut sealed = seal("f", "shop", b"secret", sender, &to).expect("seal");
        // Claiming another sender breaks both the stanza and the payload.
        sealed.sender = forger.public_key().expect("pk");
        assert!(matches!(
            open("f", "shop", &sealed, a),
            Err(Error::BundleDecrypt)
        ));
    }

    #[test]
    fn a_recipient_cannot_swap_the_payload() {
        let ids = people(3);
        let (sender, a, b) = (&ids[0], &ids[1], &ids[2]);
        let to: Vec<PublicKey> = [a, b].iter().map(|i| i.public_key().expect("pk")).collect();
        let original = seal("f", "shop", b"real", sender, &to).expect("seal");
        // `a` knows the file key; re-seal another payload under it and keep
        // the sender's stanza for `b`.
        let file_key = {
            let mine = STANDARD.encode(a.public_key().expect("pk").as_bytes());
            let stanza = original
                .stanzas
                .iter()
                .find(|s| s.recipient == mine)
                .expect("stanza");
            let eph = PublicKey::from_bytes(&STANDARD.decode(&stanza.ephemeral).expect("b64"))
                .expect("eph");
            let key = wrap_key(
                &a.agree(&eph).expect("dh"),
                &a.agree(&original.sender).expect("dh"),
                &eph,
                &original.sender,
                &a.public_key().expect("pk"),
            )
            .expect("wrap");
            let aad = stanza_aad("f", "shop", &original.nonce, &original.ciphertext);
            crypto::open(
                &key,
                &STANDARD.decode(&stanza.nonce).expect("b64"),
                &STANDARD.decode(&stanza.wrapped).expect("b64"),
                &aad,
            )
            .expect("file key")
        };
        let key: [u8; KEY_LEN] = file_key.as_slice().try_into().expect("len");
        let (nonce, ciphertext) =
            crypto::seal(&key, b"forged", &payload_aad("f", "shop", &original.sender))
                .expect("seal");
        let forged = Sealed {
            sender: original.sender,
            stanzas: original.stanzas.clone(),
            nonce,
            ciphertext,
        };
        assert!(matches!(
            open("f", "shop", &forged, b),
            Err(Error::BundleDecrypt)
        ));
    }
}
