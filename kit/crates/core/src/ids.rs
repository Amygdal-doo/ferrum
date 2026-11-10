//! UUID v4 / v7 and ULID generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    UuidV4,
    UuidV7,
    Ulid,
}

impl IdKind {
    pub const ALL: [IdKind; 3] = [IdKind::UuidV4, IdKind::UuidV7, IdKind::Ulid];

    pub fn label(self) -> &'static str {
        match self {
            IdKind::UuidV4 => "UUID v4",
            IdKind::UuidV7 => "UUID v7",
            IdKind::Ulid => "ULID",
        }
    }
}

/// Upper bound for one batch, to keep the UI responsive.
pub const MAX_COUNT: usize = 10_000;

/// Generates `count` ids (clamped to `1..=MAX_COUNT`). UUIDs are lowercase
/// unless `uppercase`; ULIDs are always uppercase (Crockford base32).
pub fn generate(kind: IdKind, count: usize, uppercase: bool) -> Vec<String> {
    let count = count.clamp(1, MAX_COUNT);
    (0..count)
        .map(|_| {
            let id = match kind {
                IdKind::UuidV4 => uuid::Uuid::new_v4().to_string(),
                IdKind::UuidV7 => uuid::Uuid::now_v7().to_string(),
                IdKind::Ulid => return ulid::Ulid::new().to_string(),
            };
            if uppercase { id.to_uppercase() } else { id }
        })
        .collect()
}

/// Parses a count field: empty means 1; otherwise a number in `1..=MAX_COUNT`.
pub fn parse_count(input: &str) -> Result<usize, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(1);
    }
    input
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=MAX_COUNT).contains(n))
        .ok_or_else(|| format!("Count must be a number from 1 to {MAX_COUNT}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v4_has_version_nibble_and_is_unique() {
        let ids = generate(IdKind::UuidV4, 50, false);
        assert_eq!(ids.len(), 50);
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 50);
        for id in &ids {
            let parsed = uuid::Uuid::parse_str(id).unwrap();
            assert_eq!(parsed.get_version_num(), 4);
            assert_eq!(id, &id.to_lowercase());
        }
    }

    #[test]
    fn v7_is_time_ordered() {
        let ids = generate(IdKind::UuidV7, 20, true);
        for id in &ids {
            assert_eq!(uuid::Uuid::parse_str(id).unwrap().get_version_num(), 7);
            assert_eq!(id, &id.to_uppercase());
        }
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(sorted, ids);
    }

    #[test]
    fn ulid_format() {
        let ids = generate(IdKind::Ulid, 5, false);
        for id in ids {
            assert_eq!(id.len(), 26);
            assert!(ulid::Ulid::from_string(&id).is_ok());
        }
    }

    #[test]
    fn count_is_clamped() {
        assert_eq!(generate(IdKind::UuidV4, 0, false).len(), 1);
        assert_eq!(
            generate(IdKind::Ulid, MAX_COUNT + 5, false).len(),
            MAX_COUNT
        );
    }

    #[test]
    fn parses_count() {
        assert_eq!(parse_count(""), Ok(1));
        assert_eq!(parse_count(" 25 "), Ok(25));
        assert!(parse_count("0").is_err());
        assert!(parse_count("-3").is_err());
        assert!(parse_count("100001").is_err());
        assert!(parse_count("ten").is_err());
    }
}
