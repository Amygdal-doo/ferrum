//! JWT decoding (header + payload). Signatures are never verified.

use crate::base64::{Alphabet, decode_bytes};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// A registered time claim (`exp`, `iat`, `nbf`) as a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeClaim {
    pub name: &'static str,
    pub seconds: i64,
    pub utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedJwt {
    pub header: Value,
    pub payload: Value,
    pub header_pretty: String,
    pub payload_pretty: String,
    /// `alg` from the header, if present.
    pub algorithm: Option<String>,
    pub time_claims: Vec<TimeClaim>,
    /// Signature segment as given (base64url); empty for unsecured tokens.
    pub signature: String,
}

/// Validity relative to a point in time, from `exp` and `nbf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Valid,
    Expired,
    NotYetValid,
    /// No `exp` claim.
    NoExpiry,
}

impl DecodedJwt {
    pub fn claim(&self, name: &str) -> Option<&TimeClaim> {
        self.time_claims.iter().find(|c| c.name == name)
    }

    pub fn validity(&self, now: DateTime<Utc>) -> Validity {
        let now = now.timestamp();
        if self.claim("nbf").is_some_and(|nbf| now < nbf.seconds) {
            return Validity::NotYetValid;
        }
        match self.claim("exp") {
            Some(exp) if now >= exp.seconds => Validity::Expired,
            Some(_) => Validity::Valid,
            None => Validity::NoExpiry,
        }
    }
}

pub fn decode(token: &str) -> Result<DecodedJwt, String> {
    let token = token.trim();
    let token = token
        .strip_prefix("Bearer ")
        .or_else(|| token.strip_prefix("bearer "))
        .unwrap_or(token)
        .trim();
    if token.is_empty() {
        return Err("Paste a token".into());
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() == 5 {
        return Err(
            "This looks like an encrypted JWE (5 parts); only signed JWTs can be decoded".into(),
        );
    }
    if parts.len() != 3 {
        return Err(format!(
            "A JWT has 3 dot-separated parts; this has {}",
            parts.len()
        ));
    }
    let header = segment(parts[0], "header")?;
    let payload = segment(parts[1], "payload")?;
    if !header.is_object() {
        return Err("Header is not a JSON object".into());
    }
    let time_claims = ["exp", "iat", "nbf"]
        .into_iter()
        .filter_map(|name| {
            let seconds = payload.get(name)?.as_f64()? as i64;
            Some(TimeClaim {
                name,
                seconds,
                utc: DateTime::from_timestamp(seconds, 0),
            })
        })
        .collect();
    let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
    Ok(DecodedJwt {
        algorithm: header
            .get("alg")
            .and_then(Value::as_str)
            .map(str::to_string),
        header_pretty: pretty(&header),
        payload_pretty: pretty(&payload),
        header,
        payload,
        time_claims,
        signature: parts[2].to_string(),
    })
}

fn segment(part: &str, name: &str) -> Result<Value, String> {
    let bytes = decode_bytes(part, Alphabet::UrlSafe)
        .map_err(|_| format!("The {name} is not valid base64url"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("The {name} is not valid JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64::encode;

    fn token(header: &str, payload: &str) -> String {
        format!(
            "{}.{}.c2ln",
            encode(header, Alphabet::UrlSafe),
            encode(payload, Alphabet::UrlSafe)
        )
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn decodes_well_known_token() {
        // jwt.io example token.
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let decoded = decode(jwt).unwrap();
        assert_eq!(decoded.algorithm.as_deref(), Some("HS256"));
        assert_eq!(decoded.payload["name"], "John Doe");
        assert!(
            decoded
                .payload_pretty
                .contains("\n  \"sub\": \"1234567890\"")
        );
        let iat = decoded.claim("iat").unwrap();
        assert_eq!(iat.seconds, 1_516_239_022);
        assert_eq!(iat.utc.unwrap().to_rfc3339(), "2018-01-18T01:30:22+00:00");
        assert_eq!(decoded.validity(Utc::now()), Validity::NoExpiry);
    }

    #[test]
    fn expiry_and_not_before() {
        let jwt = token(r#"{"alg":"none"}"#, r#"{"exp":2000,"nbf":1000,"iat":900}"#);
        let decoded = decode(&jwt).unwrap();
        assert_eq!(decoded.validity(at(500)), Validity::NotYetValid);
        assert_eq!(decoded.validity(at(1500)), Validity::Valid);
        assert_eq!(decoded.validity(at(2000)), Validity::Expired);
        assert_eq!(decoded.time_claims.len(), 3);
    }

    #[test]
    fn accepts_bearer_prefix_and_whitespace() {
        let jwt = format!("  Bearer {}\n", token(r#"{"alg":"HS256"}"#, r#"{"a":1}"#));
        assert_eq!(decode(&jwt).unwrap().payload["a"], 1);
    }

    #[test]
    fn unsecured_token_with_empty_signature() {
        let jwt = format!(
            "{}.{}.",
            encode(r#"{"alg":"none"}"#, Alphabet::UrlSafe),
            encode("{}", Alphabet::UrlSafe)
        );
        assert_eq!(decode(&jwt).unwrap().signature, "");
    }

    #[test]
    fn rejects_malformed_tokens() {
        assert!(decode("").is_err());
        assert!(decode("abc").unwrap_err().contains("3 dot-separated"));
        assert!(decode("a.b.c.d.e").unwrap_err().contains("JWE"));
        assert!(decode("!!!.e30.x").unwrap_err().contains("header"));
        let not_json = format!("{}.e30.x", encode("nope", Alphabet::UrlSafe));
        assert!(decode(&not_json).unwrap_err().contains("not valid JSON"));
        let array_header = format!("{}.e30.x", encode("[1]", Alphabet::UrlSafe));
        assert!(decode(&array_header).is_err());
    }

    #[test]
    fn ignores_non_numeric_time_claims() {
        let decoded = decode(&token("{}", r#"{"exp":"soon","iat":1.9}"#)).unwrap();
        assert!(decoded.claim("exp").is_none());
        assert_eq!(decoded.claim("iat").unwrap().seconds, 1);
    }
}
