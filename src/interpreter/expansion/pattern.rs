//! Pattern Matching
//!
//! Converts shell glob patterns to regex equivalents for pattern matching
//! in parameter expansion (${var%pattern}, ${var/pattern/replacement}, etc.)
//! and case statements.
//!
//! ## Error Handling
//!
//! This module follows bash's behavior for invalid patterns:
//! - Invalid character ranges (e.g., `[z-a]`) result in regex compilation failure
//! - Unknown POSIX classes (e.g., `[:foo:]`) produce empty match groups
//! - Unclosed character classes (`[abc`) are treated as literal `[`
//!
//! Callers should wrap regex compilation in try/catch to handle invalid patterns.

use std::collections::HashMap;

lazy_static::lazy_static! {
    /// Valid POSIX character class names
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

/// Convert a shell glob pattern to a regex string.
/// @param pattern - The glob pattern (*, ?, [...])
/// @param greedy - Whether * should be greedy (true for suffix matching, false for prefix)
/// @param extglob - Whether to support extended glob patterns (@(...), *(...), +(...), ?(...), !(...))
pub fn pattern_to_regex(pattern: &str, greedy: bool, extglob: bool) -> String {
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
            // Find the matching closing paren (handle nesting)
            let close_idx = find_matching_paren(&chars, i + 1);
            if close_idx != usize::MAX {
                let content: String = chars[i + 2..close_idx].iter().collect();
                // Split on | but handle nested extglob patterns
                let alternatives = split_extglob_alternatives(&content);
                // Convert each alternative recursively
                let alt_regexes: Vec<String> = alternatives
                    .iter()
                    .map(|alt| pattern_to_regex(alt, greedy, extglob))
                    .collect();
                let alt_group = if !alt_regexes.is_empty() {
                    alt_regexes.join("|")
                } else {
                    "(?:)".to_string()
                };

                match c {
                    '@' => {
                        // @(...) - match exactly one of the patterns
                        regex.push_str(&format!("(?:{})", alt_group));
                    }
                    '*' => {
                        // *(...) - match zero or more occurrences
                        regex.push_str(&format!("(?:{})*", alt_group));
                    }
                    '+' => {
                        // +(...) - match one or more occurrences
                        regex.push_str(&format!("(?:{})+", alt_group));
                    }
                    '?' => {
                        // ?(...) - match zero or one occurrence
                        regex.push_str(&format!("(?:{})?", alt_group));
                    }
                    '!' => {
                        // !(...) - match anything except the patterns
                        // This is tricky - we need a negative lookahead anchored to the end
                        regex.push_str(&format!("(?!(?:{})$).*", alt_group));
                    }
                    _ => {}
                }
                i = close_idx + 1;
                continue;
            }
        }

        if c == '\\' {
            // Shell escape: \X means literal X
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                // Escape for regex if it's a regex special char
                if is_regex_special(next) {
                    regex.push('\\');
                    regex.push(next);
                } else {
                    regex.push(next);
                }
                i += 2;
            } else {
                // Trailing backslash - treat as literal
                regex.push_str("\\\\");
                i += 1;
            }
        } else if c == '*' {
            regex.push_str(if greedy { ".*" } else { ".*?" });
            i += 1;
        } else if c == '?' {
            regex.push('.');
            i += 1;
        } else if c == '[' {
            // Character class - find the matching ]
            let class_end = find_char_class_end(&chars, i);
            if class_end == usize::MAX {
                // No matching ], escape the [
                regex.push_str("\\[");
                i += 1;
            } else {
                // Extract and convert the character class
                let class_content: String = chars[i + 1..class_end].iter().collect();
                regex.push_str(&convert_char_class(&class_content));
                i = class_end + 1;
            }
        } else if "^$.|+(){}".contains(c) {
            // Escape regex special chars (but NOT [ and ] - handled above, and NOT \\ - handled above)
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

/// Check if a character is a regex special character
fn is_regex_special(c: char) -> bool {
    "\\^$.|+(){}[]*?".contains(c)
}

/// Find the matching closing parenthesis, handling nesting
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

/// Split extglob pattern content on | handling nested patterns
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

/// Find the end of a character class starting at position i (where chars[i] is '[')
fn find_char_class_end(chars: &[char], start: usize) -> usize {
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
            i += 2; // Skip both the backslash and the escaped character
            continue;
        }

        if chars[i] == ']' {
            return i;
        }

        // Handle single quotes inside character class (bash extension)
        if chars[i] == '\'' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_quote) = rest.find('\'') {
                i = i + 1 + close_quote + 1;
                continue;
            }
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

/// Convert a shell character class content to regex equivalent.
/// Input is the content inside [...], e.g., ":alpha:" for [[:alpha:]]
fn convert_char_class(content: &str) -> String {
    let mut result = String::from("[");
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    // Handle negation
    if !chars.is_empty() && (chars[0] == '^' || chars[0] == '!') {
        result.push('^');
        i += 1;
    }

    while i < chars.len() {
        // Handle single quotes inside character class (bash extension)
        // '...' makes its content literal, including ] and -
        if chars[i] == '\'' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_quote) = rest.find('\'') {
                // Add quoted content as literal characters
                let quoted: String = chars[i + 1..i + 1 + close_quote].iter().collect();
                for ch in quoted.chars() {
                    // Escape regex special chars inside character class
                    if ch == '\\' {
                        result.push_str("\\\\");
                    } else if ch == ']' {
                        result.push_str("\\]");
                    } else if ch == '^' && result == "[" {
                        result.push_str("\\^");
                    } else {
                        result.push(ch);
                    }
                }
                i = i + 1 + close_quote + 1;
                continue;
            }
        }

        // Handle POSIX classes like [:alpha:]
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_pos) = rest.find(":]") {
                let posix_class: String = chars[i + 2..i + 2 + close_pos].iter().collect();
                result.push_str(posix_class_to_regex(&posix_class));
                i = i + 2 + close_pos + 2;
                continue;
            }
        }

        // Handle literal characters (escape regex special chars inside class)
        let c = chars[i];
        if c == '\\' {
            // Escape sequence
            if i + 1 < chars.len() {
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
            } else {
                result.push_str("\\\\");
                i += 1;
            }
        } else if c == '-' && i > 0 && i < chars.len() - 1 {
            // Range separator
            result.push('-');
            i += 1;
        } else if c == '^' && i == 0 {
            // Negation at start
            result.push('^');
            i += 1;
        } else {
            // Regular character - some need escaping in regex char class
            if c == ']' && i == 0 {
                result.push_str("\\]");
            } else {
                result.push(c);
            }
            i += 1;
        }
    }

    result.push(']');
    result
}

/// Convert POSIX character class name to regex equivalent.
/// Returns empty string for unknown class names (matches bash behavior).
fn posix_class_to_regex(name: &str) -> &'static str {
    POSIX_CLASSES.get(name).copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_patterns() {
        assert_eq!(pattern_to_regex("*", true, false), ".*");
        assert_eq!(pattern_to_regex("*", false, false), ".*?");
        assert_eq!(pattern_to_regex("?", true, false), ".");
        assert_eq!(pattern_to_regex("abc", true, false), "abc");
    }

    #[test]
    fn test_escaped_chars() {
        assert_eq!(pattern_to_regex("\\*", true, false), "\\*");
        assert_eq!(pattern_to_regex("\\?", true, false), "\\?");
        assert_eq!(pattern_to_regex("\\[", true, false), "\\[");
    }

    #[test]
    fn test_character_class() {
        assert_eq!(pattern_to_regex("[abc]", true, false), "[abc]");
        assert_eq!(pattern_to_regex("[a-z]", true, false), "[a-z]");
        assert_eq!(pattern_to_regex("[^abc]", true, false), "[^abc]");
    }

    #[test]
    fn test_extglob_patterns() {
        assert_eq!(pattern_to_regex("@(a|b)", true, true), "(?:a|b)");
        assert_eq!(pattern_to_regex("*(a|b)", true, true), "(?:a|b)*");
        assert_eq!(pattern_to_regex("+(a|b)", true, true), "(?:a|b)+");
        assert_eq!(pattern_to_regex("?(a|b)", true, true), "(?:a|b)?");
    }

    #[test]
    fn test_posix_classes() {
        assert_eq!(pattern_to_regex("[[:alpha:]]", true, false), "[a-zA-Z]");
        assert_eq!(pattern_to_regex("[[:digit:]]", true, false), "[0-9]");
    }
}
