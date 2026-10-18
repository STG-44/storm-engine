/// C atol-style decimal prefix, using explicit low 32-bit arithmetic. Inputs
/// outside a C long's range were undefined/platform dependent in the original.
pub(super) fn parse_dword(value: &str) -> u32 {
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let number = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0_u32, |n, b| {
            n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
        });
    if negative {
        number.wrapping_neg()
    } else {
        number
    }
}

/// C atof decimal-prefix behavior; parses in double precision before narrowing.
/// Hex float spellings are deliberately not part of this first native subset.
pub(super) fn parse_float(value: &str) -> f32 {
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let bytes = value.as_bytes();
    let start = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let special = value.get(start..).unwrap_or("");
    if special
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("inf"))
    {
        return if bytes.first() == Some(&b'-') {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    if special
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("nan"))
    {
        return f32::NAN;
    }
    let mut end = start;
    let mut digits = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
        digits += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let first_digit = exponent;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent += 1;
        }
        if exponent > first_digit {
            end = exponent;
        }
    }
    value[..end].parse::<f64>().unwrap_or(0.0) as f32
}

/// sprintf("%g", float promoted to double): six significant digits, trimmed
/// fractional zeroes and a signed exponent padded to at least two digits.
pub fn format_float(value: f32) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        }
        .into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    let value = f64::from(value);
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("scientific notation");
    let exponent: i32 = exponent.parse().expect("formatted exponent");
    let trim = |text: &str| text.trim_end_matches('0').trim_end_matches('.').to_owned();
    if !(-4..6).contains(&exponent) {
        format!("{}e{exponent:+03}", trim(mantissa))
    } else {
        let fixed = format!("{value:.precision$}", precision = (5 - exponent) as usize);
        if fixed.contains('.') {
            trim(&fixed)
        } else {
            fixed
        }
    }
}
