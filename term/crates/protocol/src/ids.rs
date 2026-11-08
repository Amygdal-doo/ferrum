//! Short numeric ids shown as `s1`, `w2`, `b3` (stable for the server's lifetime).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

macro_rules! short_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, ()> {
                s.strip_prefix($prefix)
                    .and_then(|n| n.parse().ok())
                    .map(Self)
                    .ok_or(())
            }
        }
    };
}

short_id!(SessionId, "s");
short_id!(WindowId, "w");
short_id!(BlockId, "b");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse() {
        assert_eq!(BlockId(7).to_string(), "b7");
        assert_eq!("b7".parse::<BlockId>(), Ok(BlockId(7)));
        assert!("w7".parse::<BlockId>().is_err());
        assert_eq!("s12".parse::<SessionId>(), Ok(SessionId(12)));
    }
}
