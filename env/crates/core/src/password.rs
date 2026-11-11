//! Master password rules and a rough strength hint.

/// Minimum master password length.
pub const MIN_LENGTH: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    TooShort,
    Weak,
    Fair,
    Good,
    Strong,
}

impl Strength {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Strength::TooShort => "Too short",
            Strength::Weak => "Weak",
            Strength::Fair => "Fair",
            Strength::Good => "Good",
            Strength::Strong => "Strong",
        }
    }

    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Strength::TooShort => "Use at least 10 characters.",
            Strength::Weak | Strength::Fair => {
                "Longer is better: try a passphrase of four or more random words."
            }
            Strength::Good | Strength::Strong => "Good. Store it somewhere safe.",
        }
    }
}

const COMMON: [&str; 8] = [
    "password", "123456", "qwerty", "letmein", "welcome", "admin", "iloveyou", "monkey",
];

/// Estimates strength from length and character variety (bits ≈ length ×
/// log2(alphabet)), with penalties for repeats and common words.
#[must_use]
pub fn strength(password: &str) -> Strength {
    let length = password.chars().count();
    if length < MIN_LENGTH {
        return Strength::TooShort;
    }
    let mut pool = 0u32;
    if password.chars().any(|c| c.is_ascii_lowercase()) {
        pool += 26;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        pool += 26;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        pool += 10;
    }
    if password
        .chars()
        .any(|c| c.is_ascii_punctuation() || c == ' ')
    {
        pool += 33;
    }
    if !password.is_ascii() {
        pool += 100;
    }
    let mut unique: Vec<char> = password.chars().collect();
    unique.sort_unstable();
    unique.dedup();
    // Repeated characters add little.
    let effective = length.min(unique.len() * 3) as f64;
    let mut bits = effective * f64::from(pool.max(1)).log2();
    let lower = password.to_lowercase();
    if COMMON.iter().any(|w| lower.contains(w)) {
        bits -= 30.0;
    }
    match bits {
        b if b < 45.0 => Strength::Weak,
        b if b < 60.0 => Strength::Fair,
        b if b < 80.0 => Strength::Good,
        _ => Strength::Strong,
    }
}

/// Checks a new password and its confirmation.
pub fn validate_new(password: &str, confirm: &str) -> Result<(), String> {
    if password.chars().count() < MIN_LENGTH {
        return Err(format!(
            "The master password needs at least {MIN_LENGTH} characters."
        ));
    }
    if password != confirm {
        return Err("The two passwords don't match.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_levels() {
        assert_eq!(strength("short"), Strength::TooShort);
        assert_eq!(strength("aaaaaaaaaaaa"), Strength::Weak);
        assert_eq!(strength("password1234"), Strength::Weak);
        assert!(strength("correct horse battery staple") >= Strength::Good);
        assert_eq!(strength("T7#qL9!vR2@xW4$z"), Strength::Strong);
        assert!(strength("abcdefghijkl") < Strength::Good);
    }

    #[test]
    fn validation() {
        assert!(validate_new("short", "short").is_err());
        assert!(validate_new("long enough pw", "long enough px").is_err());
        assert!(validate_new("long enough pw", "long enough pw").is_ok());
        assert_eq!(Strength::Good.label(), "Good");
        assert!(!Strength::TooShort.hint().is_empty());
    }
}
