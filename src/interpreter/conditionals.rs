//! Conditional Expression Evaluation
//!
//! Handles:
//! - [[ ... ]] conditional commands
//! - [ ... ] and test commands
//! - File tests (-f, -d, -e, etc.)
//! - String tests (-z, -n, =, !=)
//! - Numeric comparisons (-eq, -ne, -lt, etc.)
//! - Pattern matching (==, =~)

use crate::interpreter::expansion::pattern::pattern_to_regex;
use crate::interpreter::types::InterpreterState;
use regex_lite::Regex;

/// Match a value against a glob pattern.
///
/// # Arguments
/// * `value` - The string to match
/// * `pattern` - The glob pattern
/// * `nocasematch` - If true, use case-insensitive matching
/// * `extglob` - If true, enable extended glob patterns
pub fn match_pattern(value: &str, pattern: &str, nocasematch: bool, extglob: bool) -> bool {
    let regex_str = format!("^{}$", pattern_to_regex(pattern, true, extglob));
    match Regex::new(&regex_str) {
        Ok(re) => {
            if nocasematch {
                // For case-insensitive, convert both to lowercase
                let lower_value = value.to_lowercase();
                let lower_pattern = format!("^{}$", pattern_to_regex(&pattern.to_lowercase(), true, extglob));
                match Regex::new(&lower_pattern) {
                    Ok(lower_re) => lower_re.is_match(&lower_value),
                    Err(_) => false,
                }
            } else {
                re.is_match(value)
            }
        }
        Err(_) => false,
    }
}

/// Evaluate -o option test (check if shell option is enabled).
/// Maps option names to interpreter state flags.
pub fn evaluate_shell_option(state: &InterpreterState, option: &str) -> bool {
    match option {
        // Implemented options (set -o)
        "errexit" | "e" => state.options.errexit,
        "nounset" | "u" => state.options.nounset,
        "pipefail" => state.options.pipefail,
        "xtrace" | "x" => state.options.xtrace,
        "noglob" | "f" => state.options.noglob,
        "noclobber" | "C" => state.options.noclobber,
        "allexport" | "a" => state.options.allexport,
        // Unknown or unimplemented option - return false
        _ => false,
    }
}

/// Parse a number in base N (2-64).
/// Digit values: 0-9=0-9, a-z=10-35, A-Z=36-61, @=62, _=63
fn parse_base_n(digits: &str, base: u32) -> Option<i64> {
    let mut result: i64 = 0;
    for c in digits.chars() {
        let digit_value = if c >= '0' && c <= '9' {
            (c as u32) - ('0' as u32)
        } else if c >= 'a' && c <= 'z' {
            (c as u32) - ('a' as u32) + 10
        } else if c >= 'A' && c <= 'Z' {
            (c as u32) - ('A' as u32) + 36
        } else if c == '@' {
            62
        } else if c == '_' {
            63
        } else {
            return None;
        };
        if digit_value >= base {
            return None;
        }
        result = result * (base as i64) + (digit_value as i64);
    }
    Some(result)
}

/// Parse a bash numeric value, supporting:
/// - Decimal: 42, -42
/// - Octal: 0777, -0123
/// - Hex: 0xff, 0xFF, -0xff
/// - Base-N: 64#a, 2#1010
/// - Strings are coerced to 0
pub fn parse_numeric(value: &str) -> i64 {
    let value = value.trim();
    if value.is_empty() {
        return 0;
    }

    // Handle negative numbers
    let (negative, value) = if value.starts_with('-') {
        (true, &value[1..])
    } else if value.starts_with('+') {
        (false, &value[1..])
    } else {
        (false, value)
    };

    let result = if let Some(caps) = Regex::new(r"^(\d+)#([a-zA-Z0-9@_]+)$").ok().and_then(|re| re.captures(value)) {
        // Base-N syntax: base#value
        let base: u32 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
        let digits = caps.get(2).unwrap().as_str();
        if base >= 2 && base <= 64 {
            parse_base_n(digits, base).unwrap_or(0)
        } else {
            0
        }
    } else if value.starts_with("0x") || value.starts_with("0X") {
        // Hex: 0x or 0X
        i64::from_str_radix(&value[2..], 16).unwrap_or(0)
    } else if value.starts_with('0') && value.len() > 1 && value.chars().skip(1).all(|c| c >= '0' && c <= '7') {
        // Octal: starts with 0 followed by digits (0-7)
        i64::from_str_radix(&value[1..], 8).unwrap_or(0)
    } else {
        // Decimal
        value.parse::<i64>().unwrap_or(0)
    };

    if negative { -result } else { result }
}

/// Result of parsing a decimal number.
#[derive(Debug, Clone)]
pub struct ParseNumericResult {
    pub value: i64,
    pub valid: bool,
}

/// Parse a number as plain decimal (for test/[ command).
/// Unlike parse_numeric, this does NOT interpret octal/hex/base-N.
/// Leading zeros are treated as decimal.
/// Returns { value, valid } - valid is false if input is invalid.
pub fn parse_numeric_decimal(value: &str) -> ParseNumericResult {
    let value = value.trim();
    if value.is_empty() {
        return ParseNumericResult { value: 0, valid: true };
    }

    // Handle negative numbers
    let (negative, value) = if value.starts_with('-') {
        (true, &value[1..])
    } else if value.starts_with('+') {
        (false, &value[1..])
    } else {
        (false, value)
    };

    // Check if it's a valid decimal number (only digits)
    if !value.chars().all(|c| c.is_ascii_digit()) {
        return ParseNumericResult { value: 0, valid: false };
    }

    // Always parse as decimal (base 10)
    match value.parse::<i64>() {
        Ok(n) => ParseNumericResult {
            value: if negative { -n } else { n },
            valid: true,
        },
        Err(_) => ParseNumericResult { value: 0, valid: false },
    }
}

/// Convert a POSIX Extended Regular Expression to Rust regex syntax.
///
/// Key differences handled:
/// 1. `[]...]` - In POSIX, `]` is literal when first in class. In Rust regex, need `\]`
/// 2. `[^]...]` - Same with negated class
/// 3. `[[:class:]]` - POSIX character classes need conversion
pub fn posix_ere_to_regex(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Handle backslash escapes - skip the escaped character
        if chars[i] == '\\' && i + 1 < chars.len() {
            result.push(chars[i]);
            result.push(chars[i + 1]);
            i += 2;
        } else if chars[i] == '[' {
            // Found start of character class
            let (converted, end_index) = convert_posix_char_class(&chars, i);
            result.push_str(&converted);
            i = end_index;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Convert a POSIX character class starting at `start_index` (where chars[start_index] === '[')
/// to Rust regex character class syntax.
///
/// Returns the converted class and the index after the closing `]`.
fn convert_posix_char_class(chars: &[char], start_index: usize) -> (String, usize) {
    let mut i = start_index + 1;
    let mut result = String::from("[");

    // Handle negation: [^ or [!
    if i < chars.len() && (chars[i] == '^' || chars[i] == '!') {
        result.push('^');
        i += 1;
    }

    // In POSIX, ] is literal when it's the first char (after optional ^)
    let mut has_literal_close_bracket = false;
    if i < chars.len() && chars[i] == ']' {
        has_literal_close_bracket = true;
        i += 1;
    }

    // In POSIX, [ can also be literal when first (after optional ^ and ])
    let mut has_literal_open_bracket = false;
    if i < chars.len() && chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] != ':' {
        has_literal_open_bracket = true;
        i += 1;
    }

    // Collect the rest of the character class content
    let mut class_content = String::new();
    let mut found_close = false;

    while i < chars.len() {
        let ch = chars[i];

        if ch == ']' {
            // End of character class
            found_close = true;
            i += 1;
            break;
        }

        // Handle POSIX character classes like [:alpha:]
        if ch == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(end_pos) = rest.find(":]") {
                let class_name: String = chars[i + 2..i + 2 + end_pos].iter().collect();
                class_content.push_str(&posix_class_to_regex(&class_name));
                i = i + 2 + end_pos + 2;
                continue;
            }
        }

        // Handle collating elements [.ch.] and equivalence classes [=ch=]
        if ch == '[' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '.' || next == '=' {
                let end_marker = format!("{}]", next);
                let rest: String = chars[i + 2..].iter().collect();
                if let Some(end_pos) = rest.find(&end_marker) {
                    let content: String = chars[i + 2..i + 2 + end_pos].iter().collect();
                    class_content.push_str(&content);
                    i = i + 2 + end_pos + 2;
                    continue;
                }
            }
        }

        // Handle escape sequences
        if ch == '\\' && i + 1 < chars.len() {
            class_content.push(ch);
            class_content.push(chars[i + 1]);
            i += 2;
            continue;
        }

        class_content.push(ch);
        i += 1;
    }

    if !found_close {
        // No closing bracket found - return as literal [
        return ("\\[".to_string(), start_index + 1);
    }

    // Build the regex-compatible character class
    // If we had literal ] at the start, escape it
    if has_literal_close_bracket {
        result.push_str("\\]");
    }

    // If we had literal [ at the start, escape it
    if has_literal_open_bracket {
        result.push_str("\\[");
    }

    // Add the rest of the content
    result.push_str(&class_content);
    result.push(']');

    (result, i)
}

/// Convert POSIX character class name to regex equivalent.
fn posix_class_to_regex(class_name: &str) -> String {
    match class_name {
        "alnum" => "a-zA-Z0-9".to_string(),
        "alpha" => "a-zA-Z".to_string(),
        "ascii" => "\\x00-\\x7F".to_string(),
        "blank" => " \\t".to_string(),
        "cntrl" => "\\x00-\\x1F\\x7F".to_string(),
        "digit" => "0-9".to_string(),
        "graph" => "!-~".to_string(),
        "lower" => "a-z".to_string(),
        "print" => " -~".to_string(),
        "punct" => "!-/:-@\\[-`{-~".to_string(),
        "space" => " \\t\\n\\r\\f\\v".to_string(),
        "upper" => "A-Z".to_string(),
        "word" => "a-zA-Z0-9_".to_string(),
        "xdigit" => "0-9A-Fa-f".to_string(),
        _ => String::new(),
    }
}

/// Compute the fixed length of a pattern, if it has one.
/// Returns None if the pattern has variable length (contains *, +, etc.).
/// Used to optimize !() extglob patterns.
pub fn compute_pattern_length(pattern: &str, extglob: bool) -> Option<usize> {
    let mut length = 0;
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for extglob patterns
        if extglob
            && (c == '@' || c == '*' || c == '+' || c == '?' || c == '!')
            && i + 1 < chars.len()
            && chars[i + 1] == '('
        {
            let close_idx = find_matching_paren(&chars, i + 1);
            if close_idx.is_some() {
                let close = close_idx.unwrap();
                if c == '@' {
                    // @() matches exactly one occurrence - get length of alternatives
                    let content: String = chars[i + 2..close].iter().collect();
                    let alts = split_extglob_alternatives(&content);
                    let alt_lengths: Vec<Option<usize>> = alts
                        .iter()
                        .map(|a| compute_pattern_length(a, extglob))
                        .collect();
                    // All alternatives must have same length for fixed length
                    if alt_lengths.iter().all(|l| l.is_some())
                        && alt_lengths.iter().all(|l| l == &alt_lengths[0])
                    {
                        length += alt_lengths[0].unwrap();
                        i = close + 1;
                        continue;
                    }
                    return None; // Variable length
                }
                // *, +, ?, ! all have variable length
                return None;
            }
        }

        if c == '*' {
            return None; // Variable length
        }
        if c == '?' {
            length += 1;
            i += 1;
            continue;
        }
        if c == '[' {
            // Character class matches exactly 1 char
            let close_idx = find_char_class_end(&chars, i);
            if close_idx.is_some() {
                length += 1;
                i = close_idx.unwrap() + 1;
                continue;
            }
            // No closing bracket - treat as literal
            length += 1;
            i += 1;
            continue;
        }
        if c == '\\' {
            // Escaped char
            length += 1;
            i += 2;
            continue;
        }
        // Regular character
        length += 1;
        i += 1;
    }

    Some(length)
}

/// Find the matching closing parenthesis, handling nesting.
fn find_matching_paren(chars: &[char], open_idx: usize) -> Option<usize> {
    let mut depth = 1;
    let mut i = open_idx + 1;
    while i < chars.len() && depth > 0 {
        let c = chars[i];
        if c == '\\' {
            i += 2; // Skip escaped char
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Split extglob pattern content on | handling nested patterns.
fn split_extglob_alternatives(content: &str) -> Vec<String> {
    let mut alternatives: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // Escaped character
            current.push(c);
            if i + 1 < chars.len() {
                current.push(chars[i + 1]);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if c == '(' {
            depth += 1;
            current.push(c);
        } else if c == ')' {
            depth -= 1;
            current.push(c);
        } else if c == '|' && depth == 0 {
            alternatives.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
        i += 1;
    }
    alternatives.push(current);
    alternatives
}

/// Find the end of a character class starting at position i (where chars[i] is '[').
fn find_char_class_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;

    // Handle negation
    if i < chars.len() && chars[i] == '^' {
        i += 1;
    }

    // A ] immediately after [ or [^ is literal, not closing
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    }

    while i < chars.len() {
        // Handle escape sequences - \] should not end the class
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }

        if chars[i] == ']' {
            return Some(i);
        }

        // Handle POSIX classes [:name:]
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_pos) = rest.find(":]") {
                i = i + 2 + close_pos + 2;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// Escape regex metacharacters in a string.
pub fn escape_regex_chars(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if "\\^$.|+*?()[]{}".contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_pattern_simple() {
        assert!(match_pattern("hello", "hello", false, false));
        assert!(!match_pattern("hello", "world", false, false));
        assert!(match_pattern("hello", "*", false, false));
        assert!(match_pattern("hello", "h*", false, false));
        assert!(match_pattern("hello", "*o", false, false));
        assert!(match_pattern("hello", "h*o", false, false));
    }

    #[test]
    fn test_match_pattern_question_mark() {
        assert!(match_pattern("hello", "h?llo", false, false));
        assert!(match_pattern("hello", "?????", false, false));
        assert!(!match_pattern("hello", "????", false, false));
    }

    #[test]
    fn test_match_pattern_nocasematch() {
        assert!(match_pattern("HELLO", "hello", true, false));
        assert!(match_pattern("Hello", "HELLO", true, false));
        assert!(!match_pattern("HELLO", "hello", false, false));
    }

    #[test]
    fn test_evaluate_shell_option() {
        let mut state = InterpreterState::default();

        // Default values
        assert!(!evaluate_shell_option(&state, "errexit"));
        assert!(!evaluate_shell_option(&state, "e"));

        // Set errexit
        state.options.errexit = true;
        assert!(evaluate_shell_option(&state, "errexit"));
        assert!(evaluate_shell_option(&state, "e"));

        // Unknown option
        assert!(!evaluate_shell_option(&state, "unknown"));
    }

    #[test]
    fn test_parse_numeric_decimal() {
        assert_eq!(parse_numeric("42"), 42);
        assert_eq!(parse_numeric("-42"), -42);
        assert_eq!(parse_numeric("+42"), 42);
        assert_eq!(parse_numeric(""), 0);
    }

    #[test]
    fn test_parse_numeric_octal() {
        assert_eq!(parse_numeric("0777"), 511);
        assert_eq!(parse_numeric("0123"), 83);
    }

    #[test]
    fn test_parse_numeric_hex() {
        assert_eq!(parse_numeric("0xff"), 255);
        assert_eq!(parse_numeric("0xFF"), 255);
        assert_eq!(parse_numeric("0x10"), 16);
    }

    #[test]
    fn test_parse_numeric_base_n() {
        assert_eq!(parse_numeric("2#1010"), 10);
        assert_eq!(parse_numeric("16#ff"), 255);
        assert_eq!(parse_numeric("64#a"), 10);
    }

    #[test]
    fn test_parse_numeric_decimal_only() {
        let result = parse_numeric_decimal("42");
        assert!(result.valid);
        assert_eq!(result.value, 42);

        let result = parse_numeric_decimal("-42");
        assert!(result.valid);
        assert_eq!(result.value, -42);

        // Hex is invalid for decimal parsing
        let result = parse_numeric_decimal("0xff");
        assert!(!result.valid);

        // Letters are invalid
        let result = parse_numeric_decimal("abc");
        assert!(!result.valid);
    }

    #[test]
    fn test_posix_ere_to_regex() {
        // Simple pattern
        assert_eq!(posix_ere_to_regex("abc"), "abc");

        // Character class with ] first
        let result = posix_ere_to_regex("[]]");
        assert!(result.contains("\\]"));
    }

    #[test]
    fn test_compute_pattern_length() {
        assert_eq!(compute_pattern_length("abc", false), Some(3));
        assert_eq!(compute_pattern_length("a?c", false), Some(3));
        assert_eq!(compute_pattern_length("a*c", false), None);
        assert_eq!(compute_pattern_length("[abc]", false), Some(1));
    }

    #[test]
    fn test_escape_regex_chars() {
        assert_eq!(escape_regex_chars("hello"), "hello");
        assert_eq!(escape_regex_chars("a.b"), "a\\.b");
        assert_eq!(escape_regex_chars("a*b"), "a\\*b");
        assert_eq!(escape_regex_chars("(a|b)"), "\\(a\\|b\\)");
    }
}
