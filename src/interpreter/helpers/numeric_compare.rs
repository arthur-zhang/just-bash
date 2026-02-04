//! Numeric comparison helper for conditionals.
//! Handles -eq, -ne, -lt, -le, -gt, -ge operators.

/// Numeric comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericOp {
    /// -eq: equal
    Eq,
    /// -ne: not equal
    Ne,
    /// -lt: less than
    Lt,
    /// -le: less than or equal
    Le,
    /// -gt: greater than
    Gt,
    /// -ge: greater than or equal
    Ge,
}

impl NumericOp {
    /// Parse a numeric operator from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "-eq" => Some(NumericOp::Eq),
            "-ne" => Some(NumericOp::Ne),
            "-lt" => Some(NumericOp::Lt),
            "-le" => Some(NumericOp::Le),
            "-gt" => Some(NumericOp::Gt),
            "-ge" => Some(NumericOp::Ge),
            _ => None,
        }
    }

    /// Get the operator string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            NumericOp::Eq => "-eq",
            NumericOp::Ne => "-ne",
            NumericOp::Lt => "-lt",
            NumericOp::Le => "-le",
            NumericOp::Gt => "-gt",
            NumericOp::Ge => "-ge",
        }
    }
}

/// Check if an operator string is a numeric comparison operator.
pub fn is_numeric_op(op: &str) -> bool {
    matches!(op, "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge")
}

/// Compare two numbers using a numeric comparison operator.
pub fn compare_numeric(op: NumericOp, left: i64, right: i64) -> bool {
    match op {
        NumericOp::Eq => left == right,
        NumericOp::Ne => left != right,
        NumericOp::Lt => left < right,
        NumericOp::Le => left <= right,
        NumericOp::Gt => left > right,
        NumericOp::Ge => left >= right,
    }
}

/// Compare two numbers using an operator string.
pub fn compare_numeric_str(op: &str, left: i64, right: i64) -> Option<bool> {
    NumericOp::from_str(op).map(|op| compare_numeric(op, left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_numeric_op() {
        assert!(is_numeric_op("-eq"));
        assert!(is_numeric_op("-ne"));
        assert!(is_numeric_op("-lt"));
        assert!(is_numeric_op("-le"));
        assert!(is_numeric_op("-gt"));
        assert!(is_numeric_op("-ge"));
        assert!(!is_numeric_op("-z"));
        assert!(!is_numeric_op("="));
    }

    #[test]
    fn test_compare_numeric() {
        assert!(compare_numeric(NumericOp::Eq, 5, 5));
        assert!(!compare_numeric(NumericOp::Eq, 5, 6));

        assert!(compare_numeric(NumericOp::Ne, 5, 6));
        assert!(!compare_numeric(NumericOp::Ne, 5, 5));

        assert!(compare_numeric(NumericOp::Lt, 5, 6));
        assert!(!compare_numeric(NumericOp::Lt, 5, 5));
        assert!(!compare_numeric(NumericOp::Lt, 6, 5));

        assert!(compare_numeric(NumericOp::Le, 5, 6));
        assert!(compare_numeric(NumericOp::Le, 5, 5));
        assert!(!compare_numeric(NumericOp::Le, 6, 5));

        assert!(compare_numeric(NumericOp::Gt, 6, 5));
        assert!(!compare_numeric(NumericOp::Gt, 5, 5));
        assert!(!compare_numeric(NumericOp::Gt, 5, 6));

        assert!(compare_numeric(NumericOp::Ge, 6, 5));
        assert!(compare_numeric(NumericOp::Ge, 5, 5));
        assert!(!compare_numeric(NumericOp::Ge, 5, 6));
    }

    #[test]
    fn test_compare_numeric_negative() {
        assert!(compare_numeric(NumericOp::Lt, -5, 0));
        assert!(compare_numeric(NumericOp::Lt, -10, -5));
        assert!(compare_numeric(NumericOp::Eq, -5, -5));
    }
}
