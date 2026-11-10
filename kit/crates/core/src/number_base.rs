//! Integer conversion between decimal, hexadecimal, octal and binary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Decimal,
    Hex,
    Octal,
    Binary,
}

impl Base {
    pub const ALL: [Base; 4] = [Base::Decimal, Base::Hex, Base::Octal, Base::Binary];

    pub fn radix(self) -> u32 {
        match self {
            Base::Decimal => 10,
            Base::Hex => 16,
            Base::Octal => 8,
            Base::Binary => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Base::Decimal => "Decimal",
            Base::Hex => "Hexadecimal",
            Base::Octal => "Octal",
            Base::Binary => "Binary",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Base::Decimal => "",
            Base::Hex => "0x",
            Base::Octal => "0o",
            Base::Binary => "0b",
        }
    }
}

/// Parses an integer written in `base`. Accepts a leading `-`, the base's
/// `0x`/`0o`/`0b` prefix, and `_`, space or `,` separators.
pub fn parse(input: &str, base: Base) -> Result<i128, String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !matches!(c, '_' | ' ' | ','))
        .collect();
    let (negative, digits) = match cleaned.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, cleaned.as_str()),
    };
    let prefix = base.prefix();
    let digits = if prefix.is_empty() {
        digits
    } else {
        digits
            .strip_prefix(prefix)
            .or_else(|| digits.strip_prefix(&prefix.to_uppercase()))
            .unwrap_or(digits)
    };
    if digits.is_empty() {
        return Err("Enter a number".into());
    }
    let magnitude = u128::from_str_radix(digits, base.radix()).map_err(|e| {
        use std::num::IntErrorKind;
        match e.kind() {
            IntErrorKind::PosOverflow => "Number is too large (max 128 bits)".to_string(),
            _ => format!("Not a valid {} number", base.label().to_lowercase()),
        }
    })?;
    if negative {
        0i128
            .checked_sub_unsigned(magnitude)
            .ok_or_else(|| "Number is too small (min −2¹²⁷)".to_string())
    } else {
        i128::try_from(magnitude).map_err(|_| "Number is too large (max 2¹²⁷−1)".to_string())
    }
}

/// `value` in `base`, lowercase, without prefix; binary is grouped in nibbles.
pub fn format(value: i128, base: Base) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    let digits = match base {
        Base::Decimal => magnitude.to_string(),
        Base::Hex => format!("{magnitude:x}"),
        Base::Octal => format!("{magnitude:o}"),
        Base::Binary => group(&format!("{magnitude:b}"), 4),
    };
    format!("{sign}{digits}")
}

/// Every base, in [`Base::ALL`] order.
pub fn convert_all(input: &str, from: Base) -> Result<Vec<(Base, String)>, String> {
    let value = parse(input, from)?;
    Ok(Base::ALL
        .into_iter()
        .map(|b| (b, format(value, b)))
        .collect())
}

/// Inserts a space every `size` digits from the right.
fn group(digits: &str, size: usize) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::with_capacity(chars.len() + chars.len() / size);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(size) {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_bases() {
        let all = convert_all("255", Base::Decimal).unwrap();
        assert_eq!(
            all,
            vec![
                (Base::Decimal, "255".into()),
                (Base::Hex, "ff".into()),
                (Base::Octal, "377".into()),
                (Base::Binary, "1111 1111".into())
            ]
        );
        assert_eq!(parse("0xFF", Base::Hex), Ok(255));
        assert_eq!(parse("0XfF", Base::Hex), Ok(255));
        assert_eq!(parse("0o17", Base::Octal), Ok(15));
        assert_eq!(parse("0b1010_1010", Base::Binary), Ok(170));
        assert_eq!(parse("1,000,000", Base::Decimal), Ok(1_000_000));
    }

    #[test]
    fn negatives() {
        assert_eq!(parse("-0x10", Base::Hex), Ok(-16));
        assert_eq!(format(-16, Base::Hex), "-10");
        assert_eq!(format(-5, Base::Binary), "-101");
        assert_eq!(parse(&i128::MIN.to_string(), Base::Decimal), Ok(i128::MIN));
    }

    #[test]
    fn zero_and_grouping() {
        assert_eq!(format(0, Base::Binary), "0");
        assert_eq!(format(16, Base::Binary), "1 0000");
        assert_eq!(format(1 << 20, Base::Hex), "100000");
    }

    #[test]
    fn errors() {
        assert!(parse("", Base::Decimal).is_err());
        assert!(parse("-", Base::Decimal).is_err());
        assert!(parse("12a", Base::Decimal).unwrap_err().contains("decimal"));
        assert!(parse("102", Base::Binary).is_err());
        assert!(parse("8", Base::Octal).is_err());
        assert!(
            parse(&"f".repeat(33), Base::Hex)
                .unwrap_err()
                .contains("too large")
        );
        assert!(parse(&u128::MAX.to_string(), Base::Decimal).is_err());
    }
}
