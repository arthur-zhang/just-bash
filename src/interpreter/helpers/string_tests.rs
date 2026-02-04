//! String test helper for conditionals.
//! Handles -z (empty) and -n (non-empty) operators.

/// String test operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringTestOp {
    /// -z: true if string is empty
    Empty,
    /// -n: true if string is non-empty
    NonEmpty,
}

impl StringTestOp {
    /// Parse a string test operator from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "-z" => Some(StringTestOp::Empty),
            "-n" => Some(StringTestOp::NonEmpty),
            _ => None,
        }
    }

    /// Get the operator string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            StringTestOp::Empty => "-z",
            StringTestOp::NonEmpty => "-n",
        }
    }
}

/// Check if an operator string is a string test operator.
pub fn is_string_test_op(op: &str) -> bool {
    matches!(op, "-z" | "-n")
}

/// Evaluate a string test operator.
pub fn evaluate_string_test(op: StringTestOp, value: &str) -> bool {
    match op {
        StringTestOp::Empty => value.is_empty(),
        StringTestOp::NonEmpty => !value.is_empty(),
    }
}

/// Evaluate a string test operator from string.
pub fn evaluate_string_test_str(op: &str, value: &str) -> Option<bool> {
    StringTestOp::from_str(op).map(|op| evaluate_string_test(op, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_string_test_op() {
        assert!(is_string_test_op("-z"));
        assert!(is_string_test_op("-n"));
        assert!(!is_string_test_op("-e"));
        assert!(!is_string_test_op("="));
    }

    #[test]
    fn test_evaluate_string_test() {
        assert!(evaluate_string_test(StringTestOp::Empty, ""));
        assert!(!evaluate_string_test(StringTestOp::Empty, "hello"));
        assert!(!evaluate_string_test(StringTestOp::NonEmpty, ""));
        assert!(evaluate_string_test(StringTestOp::NonEmpty, "hello"));
    }

    #[test]
    fn test_evaluate_string_test_str() {
        assert_eq!(evaluate_string_test_str("-z", ""), Some(true));
        assert_eq!(evaluate_string_test_str("-z", "hello"), Some(false));
        assert_eq!(evaluate_string_test_str("-n", ""), Some(false));
        assert_eq!(evaluate_string_test_str("-n", "hello"), Some(true));
        assert_eq!(evaluate_string_test_str("-x", ""), None);
    }
}
