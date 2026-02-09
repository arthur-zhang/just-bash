//! Glob Helper Functions for GLOBIGNORE and Pattern-to-Regex
//!
//! This module provides helper functions for glob pattern handling that are
//! used by the GlobExpander. These functions are distinct from the pattern
//! matching in `interpreter::expansion::pattern` which is for parameter
//! expansion contexts.
//!
//! ## Functions
//!
//! - `split_globignore_patterns` — Split GLOBIGNORE env var on colons
//! - `globignore_pattern_to_regex` — Convert GLOBIGNORE pattern to regex (star does not match `/`)
//! - `glob_to_regex` — Convert glob pattern to regex for filename matching

use std::collections::HashMap;

lazy_static::lazy_static! {
    /// Valid POSIX character class names mapped to regex equivalents.
    /// Self-contained copy (not imported from pattern.rs) to keep this module independent.
    static ref POSIX_CLASSES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("alnum", "a-zA-Z0-9");
        m.insert("alpha", "a-zA-Z");
        m.insert("ascii", "\\x00-\\x7F");
        m.insert("blank", " \\t");
        m.insert("cntrl", "\\x00-\\x1F\\x7F");
        m.insert("digit", "0-9");
        m.insert("graph", "!-~");
        m.insert("lower", "a-z");
        m.insert("print", " -~");
        m.insert("punct", "!-/:-@\\[-`{-~");
        m.insert("space", " \\t\\n\\r\\f\\v");
        m.insert("upper", "A-Z");
        m.insert("word", "a-zA-Z0-9_");
        m.insert("xdigit", "0-9A-Fa-f");
        m
    };
}

/// Split the GLOBIGNORE environment variable value on colons, preserving
/// colons that appear inside `[...]` character classes (including POSIX
/// classes like `[[:alnum:]]`) and escaped colons (`\:`).
///
/// # Examples
/// ```
/// use just_bash::shell::glob_helpers::split_globignore_patterns;
/// assert_eq!(split_globignore_patterns("*.txt:*.log"), vec!["*.txt", "*.log"]);
/// ```
pub fn split_globignore_patterns(globignore: &str) -> Vec<String> {
    if globignore.is_empty() {
        return Vec::new();
    }

    let mut patterns: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = globignore.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle escaped characters — \: is not a separator
        if c == '\\' && i + 1 < chars.len() {
            current.push(c);
            current.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // Handle character classes [...] — colons inside are not separators
        if c == '[' {
            let class_end = find_bracket_end(&chars, i);
            if class_end != usize::MAX {
                // Copy the entire bracket expression
                for j in i..=class_end {
                    current.push(chars[j]);
                }
                i = class_end + 1;
                continue;
            }
        }

        // Colon is the separator
        if c == ':' {
            patterns.push(current);
            current = String::new();
            i += 1;
            continue;
        }

        current.push(c);
        i += 1;
    }

    // Push the last segment
    patterns.push(current);

    // Filter out empty patterns
    patterns.into_iter().filter(|p| !p.is_empty()).collect()
}

/// Convert a GLOBIGNORE pattern to a regex string where `*` does NOT match `/`.
///
/// This is used to test whether a filename should be excluded from glob results.
/// In GLOBIGNORE context, `*` matches any sequence of characters except `/`,
/// and `?` matches any single character except `/`.
///
/// The result is anchored with `^...$`.
pub fn globignore_pattern_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' && i + 1 < chars.len() {
            // Escaped character — literal match
            let next = chars[i + 1];
            if is_regex_special(next) {
                regex.push('\\');
            }
            regex.push(next);
            i += 2;
        } else if c == '*' {
            // Star does NOT match /
            regex.push_str("[^/]*");
            i += 1;
        } else if c == '?' {
            // Question mark does NOT match /
            regex.push_str("[^/]");
            i += 1;
        } else if c == '[' {
            let class_end = find_bracket_end(&chars, i);
            if class_end == usize::MAX {
                // No matching ], treat as literal
                regex.push_str("\\[");
                i += 1;
            } else {
                let class_content: String = chars[i + 1..class_end].iter().collect();
                regex.push_str(&convert_char_class(&class_content));
                i = class_end + 1;
            }
        } else if is_regex_special(c) {
            regex.push('\\');
            regex.push(c);
            i += 1;
        } else {
            regex.push(c);
            i += 1;
        }
    }

    regex.push('$');
    regex
}

/// Convert a glob pattern to a regex string for filename matching.
///
/// Unlike `globignore_pattern_to_regex`, here `*` matches anything (including `/`)
/// because this is used for matching individual filenames against patterns
/// (used by GlobExpander.matchPattern).
///
/// When `extglob` is true, extended glob patterns are supported:
/// - `@(pat1|pat2)` — match exactly one of the patterns
/// - `*(pat1|pat2)` — match zero or more occurrences
/// - `+(pat1|pat2)` — match one or more occurrences
/// - `?(pat1|pat2)` — match zero or one occurrence
/// - `!(pat1|pat2)` — match anything except the patterns
///
/// The result is anchored with `^...$`.
pub fn glob_to_regex(pattern: &str, extglob: bool) -> String {
    let inner = glob_to_regex_inner(pattern, extglob);
    format!("^{}$", inner)
}

/// Inner (unanchored) glob-to-regex conversion, used recursively for extglob alternatives.
fn glob_to_regex_inner(pattern: &str, extglob: bool) -> String {
    let mut regex = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for extglob patterns: @(...), *(...), +(...), ?(...), !(...)
        if extglob
            && (c == '@' || c == '*' || c == '+' || c == '?' || c == '!')
            && i + 1 < chars.len()
            && chars[i + 1] == '('
        {
            let close_idx = find_matching_paren(&chars, i + 1);
            if close_idx != usize::MAX {
                let content: String = chars[i + 2..close_idx].iter().collect();
                let alternatives = split_extglob_alternatives(&content);
                let alt_regexes: Vec<String> = alternatives
                    .iter()
                    .map(|alt| glob_to_regex_inner(alt, extglob))
                    .collect();
                let alt_group = alt_regexes.join("|");

                match c {
                    '@' => regex.push_str(&format!("(?:{})", alt_group)),
                    '*' => regex.push_str(&format!("(?:{})*", alt_group)),
                    '+' => regex.push_str(&format!("(?:{})+", alt_group)),
                    '?' => regex.push_str(&format!("(?:{})?", alt_group)),
                    '!' => regex.push_str(&format!("(?!(?:{})$).*", alt_group)),
                    _ => {}
                }
                i = close_idx + 1;
                continue;
            }
        }

        if c == '\\' && i + 1 < chars.len() {
            // Escaped character — literal match
            let next = chars[i + 1];
            if is_regex_special(next) {
                regex.push('\\');
            }
            regex.push(next);
            i += 2;
        } else if c == '*' {
            // Star matches anything (including /)
            regex.push_str(".*");
            i += 1;
        } else if c == '?' {
            // Question mark matches any single character
            regex.push('.');
            i += 1;
        } else if c == '[' {
            let class_end = find_bracket_end(&chars, i);
            if class_end == usize::MAX {
                // No matching ], treat as literal
                regex.push_str("\\[");
                i += 1;
            } else {
                let class_content: String = chars[i + 1..class_end].iter().collect();
                regex.push_str(&convert_char_class(&class_content));
                i = class_end + 1;
            }
        } else if is_regex_special(c) {
            regex.push('\\');
            regex.push(c);
            i += 1;
        } else {
            regex.push(c);
            i += 1;
        }
    }

    regex
}

// ---------------------------------------------------------------------------
// Private helper functions
// ---------------------------------------------------------------------------

/// Check if a character is a regex special character that needs escaping.
fn is_regex_special(c: char) -> bool {
    "\\^$.|+(){}[]*?".contains(c)
}

/// Find the end of a bracket expression `[...]` starting at `start` (where
/// `chars[start]` is `[`). Returns the index of the closing `]`, or
/// `usize::MAX` if no matching `]` is found.
///
/// Handles:
/// - `[!...]` and `[^...]` negation (the `]` right after `[!` or `[^` is literal)
/// - `]` immediately after `[` is literal
/// - POSIX classes `[:name:]` inside the bracket
/// - Escaped characters `\]`
fn find_bracket_end(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;

    // Handle negation prefix
    if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
        i += 1;
    }

    // A ] immediately after [ or [! or [^ is literal, not closing
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    }

    while i < chars.len() {
        // Handle escape sequences
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }

        if chars[i] == ']' {
            return i;
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

    usize::MAX
}

/// Convert the content inside a shell character class `[...]` to a regex
/// character class. The input is the content between `[` and `]` (exclusive).
///
/// Handles `!` or `^` negation at the start, POSIX classes `[:name:]`,
/// escape sequences, and ranges.
fn convert_char_class(content: &str) -> String {
    let mut result = String::from("[");
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    // Handle negation: bash uses ! but regex uses ^
    if !chars.is_empty() && (chars[0] == '!' || chars[0] == '^') {
        result.push('^');
        i += 1;
    }

    while i < chars.len() {
        // Handle POSIX classes like [:alpha:]
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_pos) = rest.find(":]") {
                let class_name: String = chars[i + 2..i + 2 + close_pos].iter().collect();
                if let Some(expansion) = POSIX_CLASSES.get(class_name.as_str()) {
                    result.push_str(expansion);
                }
                i = i + 2 + close_pos + 2;
                continue;
            }
        }

        // Handle escape sequences
        if chars[i] == '\\' && i + 1 < chars.len() {
            result.push('\\');
            result.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // Regular character
        result.push(chars[i]);
        i += 1;
    }

    result.push(']');
    result
}

/// Find the matching closing parenthesis for an open paren at `open_idx`,
/// handling nesting. Returns `usize::MAX` if not found.
fn find_matching_paren(chars: &[char], open_idx: usize) -> usize {
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
                return i;
            }
        }
        i += 1;
    }
    usize::MAX
}

/// Split extglob pattern content on `|`, handling nested parentheses.
fn split_extglob_alternatives(content: &str) -> Vec<String> {
    let mut alternatives: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    // -----------------------------------------------------------------------
    // split_globignore_patterns tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_simple_colon_separated() {
        assert_eq!(
            split_globignore_patterns("*.txt:*.log"),
            vec!["*.txt", "*.log"]
        );
    }

    #[test]
    fn test_split_posix_class_preserved() {
        // Colon inside POSIX class [:alnum:] is not a separator
        assert_eq!(
            split_globignore_patterns("[[:alnum:]]*:*.txt"),
            vec!["[[:alnum:]]*", "*.txt"]
        );
    }

    #[test]
    fn test_split_escaped_colon() {
        assert_eq!(
            split_globignore_patterns("a\\:b:c"),
            vec!["a\\:b", "c"]
        );
    }

    #[test]
    fn test_split_empty_string() {
        let result: Vec<String> = split_globignore_patterns("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_single_pattern() {
        assert_eq!(
            split_globignore_patterns("single"),
            vec!["single"]
        );
    }

    #[test]
    fn test_split_multiple_posix_classes() {
        assert_eq!(
            split_globignore_patterns("[[:digit:]][[:alpha:]]:*.rs"),
            vec!["[[:digit:]][[:alpha:]]", "*.rs"]
        );
    }

    #[test]
    fn test_split_negated_bracket() {
        assert_eq!(
            split_globignore_patterns("[!abc]*:*.log"),
            vec!["[!abc]*", "*.log"]
        );
    }

    #[test]
    fn test_split_caret_negated_bracket() {
        assert_eq!(
            split_globignore_patterns("[^abc]*:*.log"),
            vec!["[^abc]*", "*.log"]
        );
    }

    // -----------------------------------------------------------------------
    // globignore_pattern_to_regex tests
    // -----------------------------------------------------------------------

    fn gi_matches(pattern: &str, text: &str) -> bool {
        let regex_str = globignore_pattern_to_regex(pattern);
        let re = Regex::new(&regex_str).unwrap();
        re.is_match(text)
    }

    #[test]
    fn test_gi_star_matches_filename() {
        assert!(gi_matches("*", "foo"));
    }

    #[test]
    fn test_gi_star_does_not_match_path() {
        assert!(!gi_matches("*", "dir/foo"));
    }

    #[test]
    fn test_gi_star_txt_matches_file() {
        assert!(gi_matches("*.txt", "file.txt"));
    }

    #[test]
    fn test_gi_star_txt_does_not_match_path() {
        assert!(!gi_matches("*.txt", "dir/file.txt"));
    }

    #[test]
    fn test_gi_question_matches_single_char() {
        assert!(gi_matches("?", "a"));
    }

    #[test]
    fn test_gi_question_does_not_match_slash() {
        assert!(!gi_matches("?", "/"));
    }

    #[test]
    fn test_gi_char_class_matches() {
        assert!(gi_matches("[abc]*", "afile"));
    }

    #[test]
    fn test_gi_char_class_does_not_match() {
        assert!(!gi_matches("[abc]*", "dfile"));
    }

    // -----------------------------------------------------------------------
    // glob_to_regex tests
    // -----------------------------------------------------------------------

    fn glob_matches(pattern: &str, text: &str, extglob: bool) -> bool {
        let regex_str = glob_to_regex(pattern, extglob);
        let re = Regex::new(&regex_str).unwrap();
        re.is_match(text)
    }

    #[test]
    fn test_glob_star_matches_anything() {
        assert!(glob_matches("*", "anything", false));
    }

    #[test]
    fn test_glob_star_txt_matches() {
        assert!(glob_matches("*.txt", "file.txt", false));
    }

    #[test]
    fn test_glob_star_txt_does_not_match_rs() {
        assert!(!glob_matches("*.txt", "file.rs", false));
    }

    #[test]
    fn test_glob_question_matches_single() {
        assert!(glob_matches("?", "a", false));
    }

    #[test]
    fn test_glob_question_does_not_match_two() {
        assert!(!glob_matches("?", "ab", false));
    }

    #[test]
    fn test_glob_char_class_matches() {
        assert!(glob_matches("[abc]", "a", false));
    }

    #[test]
    fn test_glob_char_class_does_not_match() {
        assert!(!glob_matches("[abc]", "d", false));
    }

    #[test]
    fn test_glob_negated_class_matches() {
        assert!(glob_matches("[!abc]", "d", false));
    }

    #[test]
    fn test_glob_negated_class_does_not_match() {
        assert!(!glob_matches("[!abc]", "a", false));
    }

    #[test]
    fn test_glob_posix_digit_matches() {
        assert!(glob_matches("[[:digit:]]", "5", false));
    }

    #[test]
    fn test_glob_posix_digit_does_not_match_alpha() {
        assert!(!glob_matches("[[:digit:]]", "a", false));
    }

    #[test]
    fn test_glob_extglob_at_matches() {
        assert!(glob_matches("@(foo|bar)", "foo", true));
        assert!(glob_matches("@(foo|bar)", "bar", true));
        assert!(!glob_matches("@(foo|bar)", "baz", true));
    }

    #[test]
    fn test_glob_extglob_star_matches() {
        // *(ab) matches zero or more occurrences of "ab"
        assert!(glob_matches("*(ab)", "", true));
        assert!(glob_matches("*(ab)", "ab", true));
        assert!(glob_matches("*(ab)", "abab", true));
        assert!(!glob_matches("*(ab)", "abc", true));
    }

    #[test]
    fn test_glob_extglob_plus_matches() {
        // +(ab) matches one or more occurrences of "ab"
        assert!(!glob_matches("+(ab)", "", true));
        assert!(glob_matches("+(ab)", "ab", true));
        assert!(glob_matches("+(ab)", "abab", true));
    }

    #[test]
    fn test_glob_extglob_question_matches() {
        // ?(ab) matches zero or one occurrence of "ab"
        assert!(glob_matches("?(ab)", "", true));
        assert!(glob_matches("?(ab)", "ab", true));
        assert!(!glob_matches("?(ab)", "abab", true));
    }

    #[test]
    fn test_glob_escaped_star_matches_literal() {
        assert!(glob_matches("\\*", "*", false));
        assert!(!glob_matches("\\*", "foo", false));
    }

    #[test]
    fn test_glob_escaped_question_matches_literal() {
        assert!(glob_matches("\\?", "?", false));
        assert!(!glob_matches("\\?", "a", false));
    }

    #[test]
    fn test_glob_posix_alpha_class() {
        assert!(glob_matches("[[:alpha:]]", "a", false));
        assert!(glob_matches("[[:alpha:]]", "Z", false));
        assert!(!glob_matches("[[:alpha:]]", "5", false));
    }

    #[test]
    fn test_glob_posix_alnum_class() {
        assert!(glob_matches("[[:alnum:]]", "a", false));
        assert!(glob_matches("[[:alnum:]]", "5", false));
        assert!(!glob_matches("[[:alnum:]]", "!", false));
    }

    #[test]
    fn test_glob_dot_is_literal() {
        // A literal dot in the pattern should match a dot, not any character
        assert!(glob_matches("file.txt", "file.txt", false));
        assert!(!glob_matches("file.txt", "fileatxt", false));
    }

    #[test]
    fn test_glob_caret_negated_class() {
        assert!(glob_matches("[^abc]", "d", false));
        assert!(!glob_matches("[^abc]", "a", false));
    }

    #[test]
    fn test_glob_range_class() {
        assert!(glob_matches("[a-z]", "m", false));
        assert!(!glob_matches("[a-z]", "M", false));
    }

    #[test]
    fn test_glob_extglob_not_enabled() {
        // When extglob is false, @(...) is not treated as extglob
        // The @ is literal, ( is escaped, etc.
        assert!(!glob_matches("@(foo|bar)", "foo", false));
    }

    #[test]
    fn test_glob_star_matches_empty() {
        assert!(glob_matches("*", "", false));
    }

    #[test]
    fn test_glob_complex_pattern() {
        assert!(glob_matches("*.tar.gz", "archive.tar.gz", false));
        assert!(!glob_matches("*.tar.gz", "archive.tar.bz2", false));
    }

    #[test]
    fn test_glob_unclosed_bracket_is_literal() {
        // An unclosed [ should be treated as a literal [
        assert!(glob_matches("\\[abc", "[abc", false));
    }
}