//! Exact decoding of PostgreSQL binary NUMERIC into its canonical text form.
//!
//! SQLx's BigDecimal conversion drops the server's display scale (`dscale`)
//! and cannot represent NaN/Infinity, so we decode the wire format directly.
//! Layout: ndigits i16, weight i16, sign u16, dscale u16, then ndigits base-10000 i16 digits.

const NUMERIC_POS: u16 = 0x0000;
const NUMERIC_NEG: u16 = 0x4000;
const NUMERIC_NAN: u16 = 0xC000;
const NUMERIC_PINF: u16 = 0xD000;
const NUMERIC_NINF: u16 = 0xF000;

pub fn numeric_to_string(bytes: &[u8]) -> Option<String> {
    let word = |i: usize| -> Option<u16> {
        bytes
            .get(i * 2..i * 2 + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let ndigits = usize::from(word(0)?);
    let weight = i64::from(word(1)? as i16);
    let sign = word(2)?;
    let dscale = usize::from(word(3)?);
    let digits: Vec<u16> = (0..ndigits).map(|i| word(4 + i)).collect::<Option<_>>()?;

    let negative = match sign {
        NUMERIC_POS => false,
        NUMERIC_NEG => true,
        NUMERIC_NAN => return Some("NaN".into()),
        NUMERIC_PINF => return Some("Infinity".into()),
        NUMERIC_NINF => return Some("-Infinity".into()),
        _ => return None,
    };
    let digit_at = |idx: i64| -> u16 {
        usize::try_from(idx)
            .ok()
            .and_then(|i| digits.get(i).copied())
            .unwrap_or(0)
    };

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if weight < 0 || ndigits == 0 {
        out.push('0');
    } else {
        out.push_str(&digit_at(0).to_string());
        for idx in 1..=weight {
            out.push_str(&format!("{:04}", digit_at(idx)));
        }
    }
    if dscale > 0 {
        let mut frac = String::with_capacity(dscale + 4);
        let mut group = 1;
        while frac.len() < dscale {
            frac.push_str(&format!("{:04}", digit_at(weight + group)));
            group += 1;
        }
        frac.truncate(dscale);
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(weight: i16, sign: u16, dscale: u16, digits: &[u16]) -> Vec<u8> {
        let mut v = Vec::new();
        for w in [digits.len() as u16, weight as u16, sign, dscale]
            .iter()
            .chain(digits)
        {
            v.extend_from_slice(&w.to_be_bytes());
        }
        v
    }

    #[test]
    fn integers_and_fractions_keep_scale() {
        // 12345.670 => digits [1, 2345, 6700], weight 1, dscale 3
        assert_eq!(
            numeric_to_string(&encode(1, NUMERIC_POS, 3, &[1, 2345, 6700])).as_deref(),
            Some("12345.670")
        );
        // -0.0012 => digits [12], weight -1, dscale 4
        assert_eq!(
            numeric_to_string(&encode(-1, NUMERIC_NEG, 4, &[12])).as_deref(),
            Some("-0.0012")
        );
        // 0.00000001 => digits [1] at weight -2 (10000^-2), dscale 8
        assert_eq!(
            numeric_to_string(&encode(-2, NUMERIC_POS, 8, &[1])).as_deref(),
            Some("0.00000001")
        );
        // 10000 => digits [1], weight 1 (trailing zero groups omitted)
        assert_eq!(
            numeric_to_string(&encode(1, NUMERIC_POS, 0, &[1])).as_deref(),
            Some("10000")
        );
        // 0.00 => no digits, dscale 2
        assert_eq!(
            numeric_to_string(&encode(0, NUMERIC_POS, 2, &[])).as_deref(),
            Some("0.00")
        );
    }

    #[test]
    fn special_values() {
        assert_eq!(
            numeric_to_string(&encode(0, NUMERIC_NAN, 0, &[])).as_deref(),
            Some("NaN")
        );
        assert_eq!(
            numeric_to_string(&encode(0, NUMERIC_NINF, 0, &[])).as_deref(),
            Some("-Infinity")
        );
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert_eq!(numeric_to_string(&[0, 1]), None);
        assert_eq!(numeric_to_string(&encode(0, 0x1234, 0, &[])), None);
    }
}
