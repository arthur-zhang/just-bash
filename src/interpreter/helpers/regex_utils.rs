//! Regex helper functions for the interpreter.

/// Escape a string for use as a literal in a regex pattern.
/// All regex special characters are escaped.
pub fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
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
    fn test_escape_regex() {
        assert_eq!(escape_regex("hello"), "hello");
        assert_eq!(escape_regex("a.b"), "a\\.b");
        assert_eq!(escape_regex("a*b"), "a\\*b");
        assert_eq!(escape_regex("a+b"), "a\\+b");
        assert_eq!(escape_regex("a?b"), "a\\?b");
        assert_eq!(escape_regex("^$"), "\\^\\$");
        assert_eq!(escape_regex("a{1,2}"), "a\\{1,2\\}");
        assert_eq!(escape_regex("(a|b)"), "\\(a\\|b\\)");
        assert_eq!(escape_regex("[abc]"), "\\[abc\\]");
        assert_eq!(escape_regex("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_regex_complex() {
        assert_eq!(
            escape_regex("file.*.txt"),
            "file\\.\\*\\.txt"
        );
        assert_eq!(
            escape_regex("^start.*end$"),
            "\\^start\\.\\*end\\$"
        );
    }
}
