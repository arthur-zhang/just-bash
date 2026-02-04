//! Quoting helpers for word expansion
//!
//! Handles quoting values for shell reuse (${var@Q} transformation).

/// Quote a value for safe reuse as shell input (${var@Q} transformation)
/// Uses single quotes with proper escaping for special characters.
/// Follows bash's quoting behavior:
/// - Simple strings without quotes: 'value'
/// - Strings with single quotes or control characters: $'value' with \' escaping
pub fn quote_value(value: &str) -> String {
    // Empty string becomes ''
    if value.is_empty() {
        return "''".to_string();
    }

    // Check if we need $'...' format - for control characters OR single quotes
    let needs_dollar_quote = value.chars().any(|c| {
        c == '\n' || c == '\r' || c == '\t' || c == '\'' || (c as u32) < 32 || c as u32 == 127
    });

    if needs_dollar_quote {
        // Use $'...' format for strings with control characters or single quotes
        let mut result = String::from("$'");
        for c in value.chars() {
            match c {
                '\'' => result.push_str("\\'"),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                _ => {
                    let code = c as u32;
                    if code < 32 || code == 127 {
                        // Use octal escapes like bash does (not hex)
                        result.push_str(&format!("\\{:03o}", code));
                    } else {
                        result.push(c);
                    }
                }
            }
        }
        result.push('\'');
        result
    } else {
        // For simple strings without control characters or single quotes, use single quotes
        format!("'{}'", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string() {
        assert_eq!(quote_value(""), "''");
    }

    #[test]
    fn test_simple_string() {
        assert_eq!(quote_value("hello"), "'hello'");
        assert_eq!(quote_value("hello world"), "'hello world'");
    }

    #[test]
    fn test_string_with_single_quote() {
        assert_eq!(quote_value("it's"), "$'it\\'s'");
        assert_eq!(quote_value("'quoted'"), "$'\\'quoted\\''");
    }

    #[test]
    fn test_string_with_newline() {
        assert_eq!(quote_value("line1\nline2"), "$'line1\\nline2'");
    }

    #[test]
    fn test_string_with_tab() {
        assert_eq!(quote_value("col1\tcol2"), "$'col1\\tcol2'");
    }

    #[test]
    fn test_string_with_backslash() {
        // Backslash alone doesn't trigger $'...' format, uses simple single quotes
        assert_eq!(quote_value("path\\to\\file"), "'path\\to\\file'");
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(quote_value("\x01"), "$'\\001'");
        assert_eq!(quote_value("\x7f"), "$'\\177'");
    }
}
