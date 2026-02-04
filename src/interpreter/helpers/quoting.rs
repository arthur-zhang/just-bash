//! Shell value quoting utilities
//!
//! Provides functions for quoting values in shell output format,
//! used by both `set` and `declare/typeset` builtins.

/// Check if a character needs $'...' quoting (control characters only)
/// Bash uses $'...' only for control characters (0x00-0x1F, 0x7F).
/// Valid UTF-8 characters above 0x7F are output with regular single quotes.
fn needs_dollar_quoting(value: &str) -> bool {
    for c in value.chars() {
        let code = c as u32;
        // Only control characters need $'...' quoting
        if code < 0x20 || code == 0x7f {
            return true;
        }
    }
    false
}

/// Quote a value for shell output using $'...' quoting (bash ANSI-C quoting)
/// Only used for values containing control characters.
fn dollar_quote(value: &str) -> String {
    let mut result = String::from("$'");

    for c in value.chars() {
        let code = c as u32;

        match code {
            0x07 => result.push_str("\\a"),   // bell
            0x08 => result.push_str("\\b"),   // backspace
            0x09 => result.push_str("\\t"),   // tab
            0x0a => result.push_str("\\n"),   // newline
            0x0b => result.push_str("\\v"),   // vertical tab
            0x0c => result.push_str("\\f"),   // form feed
            0x0d => result.push_str("\\r"),   // carriage return
            0x1b => result.push_str("\\e"),   // escape (bash extension)
            0x27 => result.push_str("\\'"),   // single quote
            0x5c => result.push_str("\\\\"),  // backslash
            _ if code < 0x20 || code == 0x7f => {
                // Other control characters: use octal notation (bash uses \NNN)
                result.push_str(&format!("\\{:03o}", code));
            }
            _ => {
                // Pass through normal characters including UTF-8 (code > 0x7f)
                result.push(c);
            }
        }
    }

    result.push('\'');
    result
}

/// Check if a string contains only safe characters that don't need quoting.
/// Safe chars: alphanumerics, underscore, slash, dot, colon, hyphen, at, percent, plus, comma, equals
fn is_safe_value(value: &str) -> bool {
    value.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '_' | '/' | '.' | ':' | '-' | '@' | '%' | '+' | ',' | '=')
    })
}

/// Quote a value for shell output (used by 'set' and 'typeset' with no args)
/// Matches bash's output format:
/// - No quotes for simple alphanumeric values
/// - Single quotes for values with spaces or shell metacharacters
/// - $'...' quoting for values with control characters
pub fn quote_value(value: &str) -> String {
    // If value contains control characters or non-printable, use $'...' quoting
    if needs_dollar_quoting(value) {
        return dollar_quote(value);
    }

    // If value contains no special chars, return as-is
    if is_safe_value(value) {
        return value.to_string();
    }

    // Use single quotes for values with spaces or shell metacharacters
    // Escape embedded single quotes as '\''
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Quote a value for array element output
/// Uses $'...' for control characters, double quotes otherwise
pub fn quote_array_value(value: &str) -> String {
    // If value needs $'...' quoting, use it
    if needs_dollar_quoting(value) {
        return dollar_quote(value);
    }
    // For array elements, bash always uses double quotes
    // Escape backslashes and double quotes
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

/// Quote a value for declare -p output
/// Uses $'...' for control characters, double quotes otherwise
pub fn quote_declare_value(value: &str) -> String {
    // If value needs $'...' quoting, use it
    if needs_dollar_quoting(value) {
        return dollar_quote(value);
    }
    // Otherwise use double quotes with escaping
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_value_simple() {
        assert_eq!(quote_value("hello"), "hello");
        assert_eq!(quote_value("foo123"), "foo123");
        assert_eq!(quote_value("/usr/bin"), "/usr/bin");
        assert_eq!(quote_value("a.b.c"), "a.b.c");
    }

    #[test]
    fn test_quote_value_with_spaces() {
        assert_eq!(quote_value("hello world"), "'hello world'");
        assert_eq!(quote_value("foo bar baz"), "'foo bar baz'");
    }

    #[test]
    fn test_quote_value_with_single_quote() {
        assert_eq!(quote_value("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_quote_value_with_control_chars() {
        assert_eq!(quote_value("hello\nworld"), "$'hello\\nworld'");
        assert_eq!(quote_value("tab\there"), "$'tab\\there'");
        assert_eq!(quote_value("\x07bell"), "$'\\abell'");
    }

    #[test]
    fn test_quote_array_value() {
        assert_eq!(quote_array_value("hello"), "\"hello\"");
        assert_eq!(quote_array_value("hello world"), "\"hello world\"");
        assert_eq!(quote_array_value("with\"quote"), "\"with\\\"quote\"");
        assert_eq!(quote_array_value("with\\backslash"), "\"with\\\\backslash\"");
    }

    #[test]
    fn test_quote_declare_value() {
        assert_eq!(quote_declare_value("hello"), "\"hello\"");
        assert_eq!(quote_declare_value("with\"quote"), "\"with\\\"quote\"");
    }

    #[test]
    fn test_dollar_quote_control_chars() {
        assert_eq!(dollar_quote("\x00"), "$'\\000'");
        assert_eq!(dollar_quote("\x1f"), "$'\\037'");
        assert_eq!(dollar_quote("\x7f"), "$'\\177'");
    }
}
