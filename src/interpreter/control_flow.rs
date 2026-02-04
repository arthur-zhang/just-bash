//! Control Flow Execution
//!
//! Handles control flow constructs:
//! - if/elif/else
//! - for loops
//! - C-style for loops
//! - while loops
//! - until loops
//! - case statements
//! - break/continue

use regex_lite::Regex;

/// Validate that a variable name is a valid identifier.
/// Returns true if valid, false otherwise.
pub fn is_valid_identifier(name: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    re.is_match(name)
}

/// Case statement terminator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseTerminator {
    /// ;; - stop, no fall-through
    Break,
    /// ;& - unconditional fall-through (execute next body without pattern check)
    FallThrough,
    /// ;;& - continue pattern matching (check next case patterns)
    ContinueMatching,
}

impl CaseTerminator {
    /// Parse a terminator string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            ";;" => Some(CaseTerminator::Break),
            ";&" => Some(CaseTerminator::FallThrough),
            ";;&" => Some(CaseTerminator::ContinueMatching),
            _ => None,
        }
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            CaseTerminator::Break => ";;",
            CaseTerminator::FallThrough => ";&",
            CaseTerminator::ContinueMatching => ";;&",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("foo123"));
        assert!(is_valid_identifier("_123"));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
        assert!(!is_valid_identifier("foo bar"));
        assert!(!is_valid_identifier(""));
    }

    #[test]
    fn test_case_terminator() {
        assert_eq!(CaseTerminator::from_str(";;"), Some(CaseTerminator::Break));
        assert_eq!(CaseTerminator::from_str(";&"), Some(CaseTerminator::FallThrough));
        assert_eq!(CaseTerminator::from_str(";;&"), Some(CaseTerminator::ContinueMatching));
        assert_eq!(CaseTerminator::from_str("invalid"), None);

        assert_eq!(CaseTerminator::Break.as_str(), ";;");
        assert_eq!(CaseTerminator::FallThrough.as_str(), ";&");
        assert_eq!(CaseTerminator::ContinueMatching.as_str(), ";;&");
    }
}
