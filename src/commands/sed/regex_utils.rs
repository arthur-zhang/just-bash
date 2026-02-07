//! Regex conversion utilities for sed command
//!
//! Handles conversion between Basic Regular Expressions (BRE) and Extended Regular Expressions (ERE),
//! POSIX character class expansion, and pattern normalization for the Rust regex crate.

/// Map POSIX character class names to their character ranges.
fn posix_class(name: &str) -> Option<&'static str> {
    match name {
        "alnum" => Some("a-zA-Z0-9"),
        "alpha" => Some("a-zA-Z"),
        "ascii" => Some("\\x00-\\x7F"),
        "blank" => Some(" \\t"),
        "cntrl" => Some("\\x00-\\x1F\\x7F"),
        "digit" => Some("0-9"),
        "graph" => Some("!-~"),
        "lower" => Some("a-z"),
        "print" => Some(" -~"),
        "punct" => Some("!-/:-@\\[-`{-~"),
        "space" => Some(" \\t\\n\\r\\x0C\\x0B"),
        "upper" => Some("A-Z"),
        "word" => Some("a-zA-Z0-9_"),
        "xdigit" => Some("0-9A-Fa-f"),
        _ => None,
    }
}

/// Convert Basic Regular Expression (BRE) to Extended Regular Expression (ERE).
///
/// In BRE: `+`, `?`, `|`, `(`, `)` are literal; `\+`, `\?`, `\|`, `\(`, `\)` are special
/// In ERE: those chars are special without backslash
///
/// Also handles:
/// - Bracket expressions `[...]` - copy contents mostly verbatim
/// - POSIX character classes `[[:alpha:]]` -> `[a-zA-Z]`
/// - Negated POSIX classes `[^[:digit:]]` -> `[^0-9]`
/// - POSIX classes inside brackets `[a[:space:]b]` -> `[a \tb]`
/// - `]` at start of bracket (literal in POSIX, needs escaping for Rust regex)
/// - Escape sequences `\t`, `\n`, `\r` to actual characters
/// - `^` anchor (only special at start or after `(`)
/// - `$` anchor (only special at end or before `)`)
pub fn bre_to_ere(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    let mut in_bracket = false;

    while i < chars.len() {
        // Handle bracket expressions - copy contents mostly verbatim
        if chars[i] == '[' && !in_bracket {
            // Check for standalone POSIX character classes like [[:space:]]
            if i + 2 < chars.len() && chars[i + 1] == '[' && chars[i + 2] == ':' {
                if let Some(close_idx) = find_posix_close(&chars, i + 3) {
                    let class_name: String = chars[i + 3..close_idx].iter().collect();
                    if let Some(js_class) = posix_class(&class_name) {
                        result.push('[');
                        result.push_str(js_class);
                        result.push(']');
                        i = close_idx + 3; // Skip past :]]
                        continue;
                    }
                }
            }

            // Check for negated standalone POSIX classes [^[:space:]]
            if i + 3 < chars.len()
                && chars[i + 1] == '^'
                && chars[i + 2] == '['
                && chars[i + 3] == ':'
            {
                if let Some(close_idx) = find_posix_close(&chars, i + 4) {
                    let class_name: String = chars[i + 4..close_idx].iter().collect();
                    if let Some(js_class) = posix_class(&class_name) {
                        result.push_str("[^");
                        result.push_str(js_class);
                        result.push(']');
                        i = close_idx + 3; // Skip past :]]
                        continue;
                    }
                }
            }

            // Start of bracket expression
            result.push('[');
            i += 1;
            in_bracket = true;

            // Handle negation at start
            if i < chars.len() && chars[i] == '^' {
                result.push('^');
                i += 1;
            }

            // Handle ] at start (it's literal in POSIX, needs escaping for Rust regex)
            if i < chars.len() && chars[i] == ']' {
                result.push_str("\\]");
                i += 1;
            }
            continue;
        }

        // Inside bracket expression - copy verbatim until closing ]
        if in_bracket {
            if chars[i] == ']' {
                result.push(']');
                i += 1;
                in_bracket = false;
                continue;
            }

            // Handle POSIX classes inside bracket expressions like [a[:space:]b]
            if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == ':' {
                if let Some(close_idx) = find_posix_close_inside(&chars, i + 2) {
                    let class_name: String = chars[i + 2..close_idx].iter().collect();
                    if let Some(js_class) = posix_class(&class_name) {
                        result.push_str(js_class);
                        i = close_idx + 2; // Skip past :]
                        continue;
                    }
                }
            }

            // Handle backslash escapes inside brackets
            if chars[i] == '\\' && i + 1 < chars.len() {
                result.push(chars[i]);
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }

            result.push(chars[i]);
            i += 1;
            continue;
        }

        // Outside bracket expressions - handle BRE to ERE conversion
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // BRE escaped chars that become special in ERE
            if next == '+' || next == '?' || next == '|' {
                result.push(next); // Remove backslash to make it special
                i += 2;
                continue;
            }
            if next == '(' || next == ')' {
                result.push(next); // Remove backslash for grouping
                i += 2;
                continue;
            }
            if next == '{' || next == '}' {
                result.push(next); // Remove backslash for quantifiers
                i += 2;
                continue;
            }
            // Convert escape sequences to actual characters (GNU extension)
            if next == 't' {
                result.push('\t');
                i += 2;
                continue;
            }
            if next == 'n' {
                result.push('\n');
                i += 2;
                continue;
            }
            if next == 'r' {
                result.push('\r');
                i += 2;
                continue;
            }
            // Keep other escaped chars as-is
            result.push(chars[i]);
            result.push(next);
            i += 2;
            continue;
        }

        // ERE special chars that should be literal in BRE (without backslash)
        if chars[i] == '+' || chars[i] == '?' || chars[i] == '|' || chars[i] == '(' || chars[i] == ')'
        {
            result.push('\\');
            result.push(chars[i]); // Add backslash to make it literal
            i += 1;
            continue;
        }

        // Handle ^ anchor: In BRE, ^ is only an anchor at the start of the pattern
        // or immediately after \( (which becomes ( in ERE). When ^ appears
        // elsewhere, it should be treated as a literal character.
        if chars[i] == '^' {
            // Check if we're at the start of result OR after an opening group paren
            let is_anchor = result.is_empty() || result.ends_with('(');
            if !is_anchor {
                result.push_str("\\^"); // Escape to make it literal in ERE
                i += 1;
                continue;
            }
        }

        // Handle $ anchor: In BRE, $ is only an anchor at the end of the pattern
        // or immediately before \) (which becomes ) in ERE). When $ appears
        // elsewhere, it should be treated as a literal character.
        if chars[i] == '$' {
            // Check if we're at the end of pattern OR before a closing group
            let is_end = i == chars.len() - 1;
            // Check if next char is \) in original BRE pattern
            let before_group_close =
                i + 2 < chars.len() && chars[i + 1] == '\\' && chars[i + 2] == ')';
            if !is_end && !before_group_close {
                result.push_str("\\$"); // Escape to make it literal in ERE
                i += 1;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Find the closing :]] for a standalone POSIX class starting at position `start`.
/// Returns the index of the first `:` in `:]]`.
fn find_posix_close(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < chars.len() {
        if chars[i] == ':' && chars[i + 1] == ']' && chars[i + 2] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find the closing :] for a POSIX class inside a bracket expression starting at position `start`.
/// Returns the index of the first `:` in `:]`.
fn find_posix_close_inside(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == ':' && chars[i + 1] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Normalize regex patterns for the Rust regex crate.
///
/// Converts GNU sed extensions to Rust-compatible syntax:
/// - `{,n}` -> `{0,n}` (GNU extension: "0 to n times")
pub fn normalize_for_rust(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut result = String::new();
    let mut in_bracket = false;
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' && !in_bracket {
            in_bracket = true;
            result.push('[');
            i += 1;
            // Handle negation and ] at start
            if i < chars.len() && chars[i] == '^' {
                result.push('^');
                i += 1;
            }
            if i < chars.len() && chars[i] == ']' {
                result.push(']');
                i += 1;
            }
            continue;
        } else if chars[i] == ']' && in_bracket {
            in_bracket = false;
            result.push(']');
            i += 1;
            continue;
        } else if !in_bracket && i + 1 < chars.len() && chars[i] == '{' && chars[i + 1] == ',' {
            // Found {,n} pattern - convert to {0,n}
            result.push_str("{0,");
            i += 2; // Skip the { and ,
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Escape pattern space for the `l` (list) command.
///
/// Shows non-printable characters as escape sequences and ends with `$`.
pub fn escape_for_list(input: &str) -> String {
    let mut result = String::new();

    for ch in input.chars() {
        let code = ch as u32;

        match ch {
            '\\' => result.push_str("\\\\"),
            '\t' => result.push_str("\\t"),
            '\n' => result.push_str("$\n"),
            '\r' => result.push_str("\\r"),
            '\x07' => result.push_str("\\a"),
            '\x08' => result.push_str("\\b"),
            '\x0C' => result.push_str("\\f"),
            '\x0B' => result.push_str("\\v"),
            _ if code < 32 || code >= 127 => {
                // Non-printable: show as octal
                result.push_str(&format!("\\{:03o}", code));
            }
            _ => result.push(ch),
        }
    }

    result.push('$');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bre_to_ere_basic() {
        // \+ becomes + (special)
        assert_eq!(bre_to_ere(r"\+"), "+");
        // \( becomes ( (grouping)
        assert_eq!(bre_to_ere(r"\(foo\)"), "(foo)");
    }

    #[test]
    fn test_bre_literal_plus() {
        // bare + is literal in BRE, becomes \+ in ERE
        assert_eq!(bre_to_ere("+"), r"\+");
    }

    #[test]
    fn test_posix_class_expansion() {
        assert_eq!(bre_to_ere("[[:alpha:]]"), "[a-zA-Z]");
        assert_eq!(bre_to_ere("[[:digit:]]"), "[0-9]");
    }

    #[test]
    fn test_negated_posix_class() {
        assert_eq!(bre_to_ere("[^[:digit:]]"), "[^0-9]");
    }

    #[test]
    fn test_posix_class_inside_bracket() {
        // [a[:space:]b] should expand [:space:] inside
        let result = bre_to_ere("[a[:space:]b]");
        assert!(result.contains(" \\t"));
    }

    #[test]
    fn test_escape_for_list() {
        assert_eq!(escape_for_list("hello"), "hello$");
        assert_eq!(escape_for_list("a\tb"), "a\\tb$");
        assert_eq!(escape_for_list("a\nb"), "a$\nb$");
        assert_eq!(escape_for_list("a\\b"), "a\\\\b$");
    }

    #[test]
    fn test_normalize_gnu_quantifier() {
        assert_eq!(normalize_for_rust("{,3}"), "{0,3}");
        assert_eq!(normalize_for_rust("a{,2}b"), "a{0,2}b");
    }

    #[test]
    fn test_bracket_expression_passthrough() {
        // Simple bracket expressions should pass through
        assert_eq!(bre_to_ere("[abc]"), "[abc]");
        assert_eq!(bre_to_ere("[a-z]"), "[a-z]");
    }

    #[test]
    fn test_bre_escape_sequences() {
        // \t should become actual tab
        assert_eq!(bre_to_ere(r"\t"), "\t");
        // \n should become actual newline
        assert_eq!(bre_to_ere(r"\n"), "\n");
    }

    #[test]
    fn test_bracket_with_literal_close() {
        // ] at start of bracket is literal
        let result = bre_to_ere("[]abc]");
        assert!(result.contains("\\]")); // Should be escaped for Rust regex
    }
}
