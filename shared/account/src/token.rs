//! Token newtypes whose `Debug` output is redacted, and the in-memory
//! access token with its expiry.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Refresh a little before the server's 15 minutes run out, so a request
/// started now does not race the expiry.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(60);

macro_rules! secret {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The raw value, for an `Authorization` header or the keychain.
            #[must_use]
            pub fn expose(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

secret!(
    RefreshToken,
    "A refresh token (`frt_…`). Lives in the keychain only."
);
secret!(
    AccessToken,
    "An access token (`fat_…`). Lives in memory only."
);
secret!(AuthCode, "A one-time authorization code (`fac_…`).");

/// An access token and when it stops working.
#[derive(Debug, Clone)]
pub struct AccessGrant {
    pub token: AccessToken,
    pub expires_at: Instant,
}

impl AccessGrant {
    #[must_use]
    pub fn new(token: AccessToken, expires_in_secs: u64, now: Instant) -> Self {
        Self {
            token,
            expires_at: now + Duration::from_secs(expires_in_secs),
        }
    }

    /// Whether to refresh before using the token at `now`.
    #[must_use]
    pub fn needs_refresh(&self, now: Instant) -> bool {
        now + REFRESH_MARGIN >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let shown = format!(
            "{:?} {:?} {:?}",
            RefreshToken::new("frt_secretvalue"),
            AccessToken::new("fat_secretvalue"),
            AuthCode::new("fac_secretvalue")
        );
        assert!(!shown.contains("secretvalue"), "{shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn refreshes_a_minute_before_expiry() {
        let now = Instant::now();
        let grant = AccessGrant::new(AccessToken::new("fat_x"), 900, now);
        assert!(!grant.needs_refresh(now));
        assert!(!grant.needs_refresh(now + Duration::from_secs(839)));
        assert!(grant.needs_refresh(now + Duration::from_secs(840)));
        assert!(grant.needs_refresh(now + Duration::from_secs(900)));
        assert!(grant.needs_refresh(now + Duration::from_secs(5000)));
        // A server answering with a very short lifetime always refreshes.
        assert!(AccessGrant::new(AccessToken::new("fat_y"), 30, now).needs_refresh(now));
    }
}
