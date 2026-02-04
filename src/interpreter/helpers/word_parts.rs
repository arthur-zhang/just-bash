//! Word Part Helper Functions
//!
//! Provides common operations on WordPart types to eliminate duplication
//! across expansion and word parsing.

use crate::{WordPart, LiteralPart, SingleQuotedPart, EscapedPart, DoubleQuotedPart, ParameterExpansionPart};

/// Get the literal string value from a word part.
/// Returns the value for Literal, SingleQuoted, and Escaped parts.
/// Returns None for complex parts that require expansion.
pub fn get_literal_value(part: &WordPart) -> Option<&str> {
    match part {
        WordPart::Literal(LiteralPart { value }) => Some(value),
        WordPart::SingleQuoted(SingleQuotedPart { value }) => Some(value),
        WordPart::Escaped(EscapedPart { value }) => Some(value),
        _ => None,
    }
}

/// Check if a word part is "quoted" - meaning glob characters should be treated literally.
/// A part is quoted if it is:
/// - SingleQuoted
/// - Escaped
/// - DoubleQuoted (entirely quoted)
/// - Literal with empty value (doesn't affect quoting)
pub fn is_quoted_part(part: &WordPart) -> bool {
    match part {
        WordPart::SingleQuoted(_) => true,
        WordPart::Escaped(_) => true,
        WordPart::DoubleQuoted(_) => true,
        WordPart::Literal(LiteralPart { value }) => value.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_literal_value() {
        assert_eq!(
            get_literal_value(&WordPart::Literal(LiteralPart { value: "hello".to_string() })),
            Some("hello")
        );
        assert_eq!(
            get_literal_value(&WordPart::SingleQuoted(SingleQuotedPart { value: "world".to_string() })),
            Some("world")
        );
        assert_eq!(
            get_literal_value(&WordPart::Escaped(EscapedPart { value: "n".to_string() })),
            Some("n")
        );
        assert_eq!(
            get_literal_value(&WordPart::ParameterExpansion(ParameterExpansionPart {
                parameter: "var".to_string(),
                operation: None,
            })),
            None
        );
    }

    #[test]
    fn test_is_quoted_part() {
        assert!(is_quoted_part(&WordPart::SingleQuoted(SingleQuotedPart { value: "test".to_string() })));
        assert!(is_quoted_part(&WordPart::Escaped(EscapedPart { value: "n".to_string() })));
        assert!(is_quoted_part(&WordPart::DoubleQuoted(DoubleQuotedPart { parts: vec![] })));
        assert!(is_quoted_part(&WordPart::Literal(LiteralPart { value: "".to_string() })));
        assert!(!is_quoted_part(&WordPart::Literal(LiteralPart { value: "test".to_string() })));
        assert!(!is_quoted_part(&WordPart::ParameterExpansion(ParameterExpansionPart {
            parameter: "var".to_string(),
            operation: None,
        })));
    }
}
