//! Glob Helper Functions
//!
//! Functions for handling glob patterns, escaping, and unescaping.

use regex_lite::Regex;

/// Check if a string contains glob patterns, including extglob when enabled.
pub fn has_glob_pattern(value: &str, extglob: bool) -> bool {
    // Standard glob characters
    if value.chars().any(|c| c == '*' || c == '?' || c == '[') {
        return true;
    }
    // Extglob patterns: @(...), *(...), +(...), ?(...), !(...)
    if extglob {
        let extglob_re = Regex::new(r"[@*+?!]\(").unwrap();
        if extglob_re.is_match(value) {
            return true;
        }
    }
    false
}

/// Unescape a glob pattern - convert escaped glob chars to literal chars.
/// For example, [\]_ (escaped pattern) becomes [\\]_ (literal string).
///
/// This is used when we need to take a pattern that was built with escaped
/// glob characters and convert it back to a literal string (e.g., for
/// no-match fallback when nullglob is off).
///
/// Note: The input is expected to be a pattern string where backslashes escape
/// the following character. For patterns like "test\\[*" (user input: test\[*)
/// the output is "\\_" (with processed escapes), not [\\]_ (raw pattern).
pub fn unescape_glob_pattern(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            // Backslash escapes the next character - output just the escaped char
            result.push(chars[i + 1]);
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Escape glob metacharacters in a string for literal matching.
/// Includes extglob metacharacters: ( ) |
pub fn escape_glob_chars(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '*' | '?' | '[' | ']' | '\\' | '(' | ')' | '|' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Escape regex metacharacters in a string for literal matching.
/// Used when quoted patterns are used with =~ operator.
pub fn escape_regex_chars(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '\\' | '^' | '$' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_glob_pattern() {
        assert!(has_glob_pattern("*.txt", false));
        assert!(has_glob_pattern("file?.rs", false));
        assert!(has_glob_pattern("[abc]", false));
        assert!(!has_glob_pattern("plain.txt", false));

        // Extglob patterns
        assert!(!has_glob_pattern("@(a|b)", false));
        assert!(has_glob_pattern("@(a|b)", true));
        assert!(has_glob_pattern("*(foo)", true));
        assert!(has_glob_pattern("+(bar)", true));
        assert!(has_glob_pattern("?(baz)", true));
        assert!(has_glob_pattern("!(qux)", true));
    }

    #[test]
    fn test_unescape_glob_pattern() {
        assert_eq!(unescape_glob_pattern(r"\*"), "*");
        assert_eq!(unescape_glob_pattern(r"\?\["), "?[");
        assert_eq!(unescape_glob_pattern(r"a\*b"), "a*b");
        assert_eq!(unescape_glob_pattern("plain"), "plain");
    }

    #[test]
    fn test_escape_glob_chars() {
        assert_eq!(escape_glob_chars("*"), r"\*");
        assert_eq!(escape_glob_chars("?"), r"\?");
        assert_eq!(escape_glob_chars("[a]"), r"\[a\]");
        assert_eq!(escape_glob_chars("plain"), "plain");
        assert_eq!(escape_glob_chars("a|b"), r"a\|b");
    }

    #[test]
    fn test_escape_regex_chars() {
        assert_eq!(escape_regex_chars("a.b"), r"a\.b");
        assert_eq!(escape_regex_chars("a*b"), r"a\*b");
        assert_eq!(escape_regex_chars("[a]"), r"\[a\]");
        assert_eq!(escape_regex_chars("^$"), r"\^\$");
        assert_eq!(escape_regex_chars("plain"), "plain");
    }
}
