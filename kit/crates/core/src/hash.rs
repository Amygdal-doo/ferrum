//! Message digests of UTF-8 text, as lowercase hex.

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashAlgorithm {
    pub const ALL: [HashAlgorithm; 4] = [
        HashAlgorithm::Md5,
        HashAlgorithm::Sha1,
        HashAlgorithm::Sha256,
        HashAlgorithm::Sha512,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HashAlgorithm::Md5 => "MD5",
            HashAlgorithm::Sha1 => "SHA-1",
            HashAlgorithm::Sha256 => "SHA-256",
            HashAlgorithm::Sha512 => "SHA-512",
        }
    }
}

pub fn hash(text: &str, algorithm: HashAlgorithm) -> String {
    let bytes = text.as_bytes();
    match algorithm {
        HashAlgorithm::Md5 => hex(&Md5::digest(bytes)),
        HashAlgorithm::Sha1 => hex(&Sha1::digest(bytes)),
        HashAlgorithm::Sha256 => hex(&Sha256::digest(bytes)),
        HashAlgorithm::Sha512 => hex(&Sha512::digest(bytes)),
    }
}

/// All digests in [`HashAlgorithm::ALL`] order.
pub fn hash_all(text: &str) -> Vec<(HashAlgorithm, String)> {
    HashAlgorithm::ALL
        .into_iter()
        .map(|algorithm| (algorithm, hash(text, algorithm)))
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_vectors() {
        assert_eq!(
            hash("", HashAlgorithm::Md5),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            hash("", HashAlgorithm::Sha1),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            hash("", HashAlgorithm::Sha256),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(hash("", HashAlgorithm::Sha512).starts_with("cf83e1357eefb8bdf1542850d66d8007"));
    }

    #[test]
    fn abc_vectors() {
        assert_eq!(
            hash("abc", HashAlgorithm::Md5),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            hash("abc", HashAlgorithm::Sha1),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash("abc", HashAlgorithm::Sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let sha512 = hash("abc", HashAlgorithm::Sha512);
        assert_eq!(sha512.len(), 128);
        assert!(sha512.starts_with("ddaf35a193617aba"));
    }

    #[test]
    fn hashes_utf8_bytes() {
        // "é" is c3 a9.
        assert_eq!(
            hash("é", HashAlgorithm::Md5),
            hash("\u{e9}", HashAlgorithm::Md5)
        );
        assert_ne!(hash("e", HashAlgorithm::Md5), hash("é", HashAlgorithm::Md5));
    }

    #[test]
    fn hash_all_in_order() {
        let all = hash_all("x");
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].0, HashAlgorithm::Md5);
        assert_eq!(all[3].1.len(), 128);
    }

    #[test]
    fn hex_encodes() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(hex(&[]), "");
    }
}
