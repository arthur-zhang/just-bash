/// AWK Type Coercion Utilities
///
/// Pure functions for converting between AWK's string and numeric types,
/// truthiness checking, value comparison, and output formatting.

use std::cmp::Ordering;

/// Parse leading numeric prefix from string using AWK semantics.
///
/// Trims whitespace first, then parses as much of the leading portion
/// as forms a valid number (including scientific notation).
/// Returns 0.0 for empty or non-numeric strings.
pub fn to_number(s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }

    // Try parsing the full trimmed string first
    if let Ok(n) = trimmed.parse::<f64>() {
        return n;
    }

    // Parse leading numeric prefix
    let bytes = trimmed.as_bytes();
    let mut i = 0;

    // Optional sign
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }

    let start_digits = i;
    // Integer part
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }

    // Decimal part
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }

    // Check we consumed at least one digit
    let has_digits = if bytes.get(start_digits) == Some(&b'.') {
        i > start_digits + 1
    } else {
        i > start_digits
    };

    if !has_digits {
        return 0.0;
    }

    // Optional exponent part
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let exp_start = i;
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let exp_digit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        // If no digits after 'e', revert exponent consumption
        if i == exp_digit_start {
            i = exp_start;
        }
    }

    let prefix = &trimmed[..i];
    prefix.parse::<f64>().unwrap_or(0.0)
}

/// Convert number to string using AWK conventions.
///
/// If the number is a finite integer that fits in i64, format without
/// decimal point. Otherwise format as float. NaN and Inf are handled.
pub fn to_string(n: f64) -> String {
    if n.is_nan() {
        return "nan".to_string();
    }
    if n.is_infinite() {
        if n.is_sign_positive() {
            return "inf".to_string();
        } else {
            return "-inf".to_string();
        }
    }
    if n == n.floor() && n.is_finite() && n.abs() < i64::MAX as f64 {
        return format!("{}", n as i64);
    }
    format!("{}", n)
}

/// Check AWK truthiness of a string value.
///
/// In AWK, empty string and the exact string "0" are falsy.
/// All other strings are truthy.
pub fn is_truthy(s: &str) -> bool {
    !s.is_empty() && s != "0"
}

/// Check if the entire string (after trimming) is a valid number.
///
/// Unlike `to_number` which parses a leading prefix, this requires
/// the whole trimmed string to be a valid numeric representation.
pub fn looks_like_number(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.parse::<f64>().is_ok()
}

/// Compare two string values using AWK comparison semantics.
///
/// If both values look like numbers, compare them numerically.
/// Otherwise, compare them lexicographically as strings.
pub fn compare_values(a: &str, b: &str) -> Ordering {
    if looks_like_number(a) && looks_like_number(b) {
        let na = to_number(a);
        let nb = to_number(b);
        na.partial_cmp(&nb).unwrap_or(Ordering::Equal)
    } else {
        a.cmp(b)
    }
}

/// Format a number using an OFMT-style format string.
///
/// If the number is an integer, returns the integer string directly.
/// Otherwise applies the printf-style format (e.g., "%.6g").
pub fn format_output(n: f64, ofmt: &str) -> String {
    // If it's an integer, just return the integer representation
    if n == n.floor() && n.is_finite() && n.abs() < i64::MAX as f64 {
        return format!("{}", n as i64);
    }

    // Parse the format string and apply it
    // Support common printf formats: %f, %e, %g with optional precision
    apply_printf_format(n, ofmt)
}

/// Apply a printf-style format string to a floating-point number.
fn apply_printf_format(n: f64, fmt: &str) -> String {
    // Parse format like "%.6g", "%f", "%.2f", etc.
    let bytes = fmt.as_bytes();
    let mut i = 0;

    // Find the '%' character
    while i < bytes.len() && bytes[i] != b'%' {
        i += 1;
    }
    if i >= bytes.len() {
        return to_string(n);
    }
    i += 1; // skip '%'

    // Parse optional precision: .N
    let mut precision: Option<usize> = None;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let mut prec = 0usize;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            prec = prec * 10 + (bytes[i] - b'0') as usize;
            i += 1;
        }
        precision = Some(prec);
    }

    // Parse format specifier
    if i >= bytes.len() {
        return to_string(n);
    }

    let prec = precision.unwrap_or(6);
    match bytes[i] {
        b'f' => format!("{:.prec$}", n),
        b'e' => format!("{:.prec$e}", n),
        b'E' => format!("{:.prec$E}", n),
        b'g' | b'G' => format_g(n, prec),
        _ => to_string(n),
    }
}

/// Format a number using %g-style formatting with the given precision.
fn format_g(n: f64, precision: usize) -> String {
    // %g uses the shorter of %e and %f, removing trailing zeros
    let prec = if precision == 0 { 1 } else { precision };
    let formatted = format!("{:.prec$e}", n);

    // Parse the exponent to decide between %e and %f style
    if let Some(e_pos) = formatted.find('e') {
        if let Ok(exp) = formatted[e_pos + 1..].parse::<i32>() {
            if exp >= -(prec as i32) && exp < prec as i32 {
                // Use fixed notation, trim trailing zeros
                let fixed = format!("{:.prec$}", n);
                return trim_trailing_zeros(&fixed);
            }
        }
    }

    // Use scientific notation, trim trailing zeros in mantissa
    trim_trailing_zeros_scientific(&formatted)
}

/// Remove trailing zeros after the decimal point in a fixed-format number.
fn trim_trailing_zeros(s: &str) -> String {
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0');
        if trimmed.ends_with('.') {
            trimmed[..trimmed.len() - 1].to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Remove trailing zeros in the mantissa of scientific notation.
fn trim_trailing_zeros_scientific(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let mantissa = &s[..e_pos];
        let exponent = &s[e_pos..];
        let trimmed = trim_trailing_zeros(mantissa);
        format!("{}{}", trimmed, exponent)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn test_to_number_integer() {
        assert_eq!(to_number("42"), 42.0);
    }

    #[test]
    fn test_to_number_float() {
        assert_eq!(to_number("3.14"), 3.14);
    }

    #[test]
    fn test_to_number_prefix() {
        assert_eq!(to_number("123abc"), 123.0);
    }

    #[test]
    fn test_to_number_non_numeric() {
        assert_eq!(to_number("abc"), 0.0);
    }

    #[test]
    fn test_to_number_empty() {
        assert_eq!(to_number(""), 0.0);
    }

    #[test]
    fn test_to_number_whitespace() {
        assert_eq!(to_number(" 42 "), 42.0);
    }

    #[test]
    fn test_to_number_scientific() {
        assert_eq!(to_number("1e5"), 100000.0);
    }

    #[test]
    fn test_to_number_negative() {
        assert_eq!(to_number("-3.14"), -3.14);
    }

    #[test]
    fn test_to_number_positive_sign() {
        assert_eq!(to_number("+5"), 5.0);
    }

    // to_string tests
    #[test]
    fn test_to_string_integer() {
        assert_eq!(to_string(42.0), "42");
    }

    #[test]
    fn test_to_string_float() {
        assert_eq!(to_string(3.14), "3.14");
    }

    #[test]
    fn test_to_string_zero() {
        assert_eq!(to_string(0.0), "0");
    }

    #[test]
    fn test_to_string_negative() {
        assert_eq!(to_string(-5.0), "-5");
    }

    #[test]
    fn test_to_string_large_integer() {
        assert_eq!(to_string(1000000.0), "1000000");
    }

    // is_truthy tests
    #[test]
    fn test_is_truthy_empty() {
        assert!(!is_truthy(""));
    }

    #[test]
    fn test_is_truthy_zero_string() {
        assert!(!is_truthy("0"));
    }

    #[test]
    fn test_is_truthy_one() {
        assert!(is_truthy("1"));
    }

    #[test]
    fn test_is_truthy_text() {
        assert!(is_truthy("abc"));
    }

    #[test]
    fn test_is_truthy_zero_point_zero() {
        assert!(is_truthy("0.0"));
    }

    #[test]
    fn test_is_truthy_space() {
        assert!(is_truthy(" "));
    }

    // looks_like_number tests
    #[test]
    fn test_looks_like_number_integer() {
        assert!(looks_like_number("42"));
    }

    #[test]
    fn test_looks_like_number_float() {
        assert!(looks_like_number("3.14"));
    }

    #[test]
    fn test_looks_like_number_whitespace() {
        assert!(looks_like_number(" 42 "));
    }

    #[test]
    fn test_looks_like_number_text() {
        assert!(!looks_like_number("abc"));
    }

    #[test]
    fn test_looks_like_number_scientific() {
        assert!(looks_like_number("1e5"));
    }

    #[test]
    fn test_looks_like_number_prefix_only() {
        assert!(!looks_like_number("123abc"));
    }

    #[test]
    fn test_looks_like_number_empty() {
        assert!(!looks_like_number(""));
    }

    #[test]
    fn test_looks_like_number_negative() {
        assert!(looks_like_number("-3"));
    }

    // compare_values tests
    #[test]
    fn test_compare_values_numeric_less() {
        assert_eq!(compare_values("3", "10"), Ordering::Less);
    }

    #[test]
    fn test_compare_values_string_less() {
        assert_eq!(compare_values("abc", "def"), Ordering::Less);
    }

    #[test]
    fn test_compare_values_numeric_greater() {
        assert_eq!(compare_values("10", "9"), Ordering::Greater);
    }
}
