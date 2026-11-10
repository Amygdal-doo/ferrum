//! Base64 encode/decode of text, standard or URL-safe alphabet.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}

/// Encodes the UTF-8 bytes of `text`. URL-safe output has no padding
/// (as in JWTs); standard output is padded.
pub fn encode(text: &str, alphabet: Alphabet) -> String {
    match alphabet {
        Alphabet::Standard => STANDARD.encode(text),
        Alphabet::UrlSafe => URL_SAFE_NO_PAD.encode(text),
    }
}

/// Decodes to bytes. Whitespace is ignored and padding is optional.
pub fn decode_bytes(input: &str, alphabet: Alphabet) -> Result<Vec<u8>, String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let (padded, unpadded) = match alphabet {
        Alphabet::Standard => (&STANDARD, &STANDARD_NO_PAD),
        Alphabet::UrlSafe => (&URL_SAFE, &URL_SAFE_NO_PAD),
    };
    let result = if cleaned.ends_with('=') {
        padded.decode(&cleaned)
    } else {
        unpadded.decode(&cleaned)
    };
    result.map_err(|e| format!("Not valid Base64: {e}"))
}

/// Decodes to text; fails if the bytes are not UTF-8.
pub fn decode(input: &str, alphabet: Alphabet) -> Result<String, String> {
    let bytes = decode_bytes(input, alphabet)?;
    String::from_utf8(bytes)
        .map_err(|_| "Decoded bytes are not valid UTF-8 text (binary data)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_standard_with_padding() {
        assert_eq!(encode("hello", Alphabet::Standard), "aGVsbG8=");
        assert_eq!(encode("", Alphabet::Standard), "");
    }

    #[test]
    fn url_safe_uses_dash_underscore_without_padding() {
        // 0xfb 0xff → "+/8=" in standard.
        let text = "\u{00fb}\u{00ff}"; // UTF-8: c3 bb c3 bf
        assert_eq!(encode(text, Alphabet::Standard), "w7vDvw==");
        assert_eq!(encode(text, Alphabet::UrlSafe), "w7vDvw");
        assert_eq!(encode("??>", Alphabet::Standard), "Pz8+");
        assert_eq!(encode("??>", Alphabet::UrlSafe), "Pz8-");
    }

    #[test]
    fn decodes_with_or_without_padding_and_whitespace() {
        assert_eq!(decode("aGVsbG8=", Alphabet::Standard).unwrap(), "hello");
        assert_eq!(decode("aGVsbG8", Alphabet::Standard).unwrap(), "hello");
        assert_eq!(decode(" aGVs\nbG8= ", Alphabet::Standard).unwrap(), "hello");
        assert_eq!(decode("Pz8-", Alphabet::UrlSafe).unwrap(), "??>");
    }

    #[test]
    fn roundtrips_unicode() {
        let text = "héllo 👋 ✓";
        for alphabet in [Alphabet::Standard, Alphabet::UrlSafe] {
            assert_eq!(decode(&encode(text, alphabet), alphabet).unwrap(), text);
        }
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(decode("a$b", Alphabet::Standard).is_err());
        assert!(decode("Pz8-", Alphabet::Standard).is_err());
        assert!(decode("Pz8+", Alphabet::UrlSafe).is_err());
    }

    #[test]
    fn rejects_binary_as_text() {
        let err = decode("/w==", Alphabet::Standard).unwrap_err();
        assert!(err.contains("UTF-8"));
        assert_eq!(
            decode_bytes("/w==", Alphabet::Standard).unwrap(),
            vec![0xff]
        );
    }
}
