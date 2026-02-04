//! String comparison helpers for conditionals.
//!
//! Consolidates string comparison logic (=, ==, !=) used in:
//! - [[ ]] conditional expressions (with optional pattern matching)
//! - test/[ ] command (literal comparison only)

/// String comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringCompareOp {
    /// = or ==: equal
    Eq,
    /// !=: not equal
    Ne,
}

impl StringCompareOp {
    /// Parse a string comparison operator from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "=" | "==" => Some(StringCompareOp::Eq),
            "!=" => Some(StringCompareOp::Ne),
            _ => None,
        }
    }

    /// Get the operator string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            StringCompareOp::Eq => "==",
            StringCompareOp::Ne => "!=",
        }
    }
}

/// Check if an operator string is a string comparison operator.
pub fn is_string_compare_op(op: &str) -> bool {
    matches!(op, "=" | "==" | "!=")
}

/// Compare two strings using the specified operator (literal comparison).
///
/// For pattern matching comparison, use `compare_strings_with_pattern`.
pub fn compare_strings(op: StringCompareOp, left: &str, right: &str) -> bool {
    let is_equal = left == right;
    match op {
        StringCompareOp::Eq => is_equal,
        StringCompareOp::Ne => !is_equal,
    }
}

/// Compare two strings with case-insensitive option.
pub fn compare_strings_nocase(op: StringCompareOp, left: &str, right: &str, nocasematch: bool) -> bool {
    let is_equal = if nocasematch {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    };
    match op {
        StringCompareOp::Eq => is_equal,
        StringCompareOp::Ne => !is_equal,
    }
}

/// Compare two strings using an operator string.
pub fn compare_strings_str(op: &str, left: &str, right: &str) -> Option<bool> {
    StringCompareOp::from_str(op).map(|op| compare_strings(op, left, right))
}

/// Compare two strings with pattern matching support.
///
/// # Arguments
/// * `op` - The comparison operator
/// * `left` - Left operand (the string to match)
/// * `right` - Right operand (the pattern when use_pattern is true)
/// * `use_pattern` - If true, use glob pattern matching for equality
/// * `nocasematch` - If true, use case-insensitive comparison
/// * `extglob` - If true, enable extended glob patterns (requires pattern_matcher)
/// * `pattern_matcher` - Optional function to perform pattern matching
///
/// When `use_pattern` is true and `pattern_matcher` is provided, the right operand
/// is treated as a glob pattern. Otherwise, literal comparison is used.
pub fn compare_strings_with_pattern<F>(
    op: StringCompareOp,
    left: &str,
    right: &str,
    use_pattern: bool,
    nocasematch: bool,
    _extglob: bool,
    pattern_matcher: Option<F>,
) -> bool
where
    F: Fn(&str, &str, bool) -> bool,
{
    let is_equal = if use_pattern {
        if let Some(matcher) = pattern_matcher {
            matcher(left, right, nocasematch)
        } else {
            // Fallback to literal comparison if no pattern matcher provided
            if nocasematch {
                left.eq_ignore_ascii_case(right)
            } else {
                left == right
            }
        }
    } else if nocasematch {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    };

    match op {
        StringCompareOp::Eq => is_equal,
        StringCompareOp::Ne => !is_equal,
    }
}

/// Compare two strings with pattern matching using string operator.
pub fn compare_strings_with_pattern_str<F>(
    op: &str,
    left: &str,
    right: &str,
    use_pattern: bool,
    nocasematch: bool,
    extglob: bool,
    pattern_matcher: Option<F>,
) -> Option<bool>
where
    F: Fn(&str, &str, bool) -> bool,
{
    StringCompareOp::from_str(op).map(|op| {
        compare_strings_with_pattern(op, left, right, use_pattern, nocasematch, extglob, pattern_matcher)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_string_compare_op() {
        assert!(is_string_compare_op("="));
        assert!(is_string_compare_op("=="));
        assert!(is_string_compare_op("!="));
        assert!(!is_string_compare_op("-eq"));
        assert!(!is_string_compare_op("<"));
    }

    #[test]
    fn test_compare_strings() {
        assert!(compare_strings(StringCompareOp::Eq, "hello", "hello"));
        assert!(!compare_strings(StringCompareOp::Eq, "hello", "world"));
        assert!(compare_strings(StringCompareOp::Ne, "hello", "world"));
        assert!(!compare_strings(StringCompareOp::Ne, "hello", "hello"));
    }

    #[test]
    fn test_compare_strings_nocase() {
        assert!(compare_strings_nocase(StringCompareOp::Eq, "Hello", "hello", true));
        assert!(!compare_strings_nocase(StringCompareOp::Eq, "Hello", "hello", false));
        assert!(compare_strings_nocase(StringCompareOp::Ne, "Hello", "world", true));
    }

    #[test]
    fn test_compare_strings_str() {
        assert_eq!(compare_strings_str("=", "a", "a"), Some(true));
        assert_eq!(compare_strings_str("==", "a", "a"), Some(true));
        assert_eq!(compare_strings_str("!=", "a", "b"), Some(true));
        assert_eq!(compare_strings_str("-eq", "a", "a"), None);
    }
}
