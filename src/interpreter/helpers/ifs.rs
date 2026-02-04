//! IFS (Internal Field Separator) Handling
//!
//! Centralized utilities for IFS-based word splitting used by:
//! - Word expansion (unquoted variable expansion)
//! - read builtin
//! - ${!prefix*} and ${!arr[*]} expansions

use std::collections::{HashMap, HashSet};

/// Default IFS value: space, tab, newline
pub const DEFAULT_IFS: &str = " \t\n";

/// IFS whitespace characters
const IFS_WHITESPACE: &str = " \t\n";

/// Get the effective IFS value from environment.
/// Returns DEFAULT_IFS if IFS is undefined, or the actual value (including empty string).
pub fn get_ifs(env: &HashMap<String, String>) -> &str {
    env.get("IFS").map(|s| s.as_str()).unwrap_or(DEFAULT_IFS)
}

/// Check if IFS is set to empty string (disables word splitting).
pub fn is_ifs_empty(env: &HashMap<String, String>) -> bool {
    env.get("IFS").map_or(false, |s| s.is_empty())
}

/// Check if IFS contains only whitespace characters (space, tab, newline).
/// This affects how empty fields are handled in $@ and $* expansion.
/// When IFS has non-whitespace chars, empty params are preserved.
/// When IFS has only whitespace, empty params are dropped.
pub fn is_ifs_whitespace_only(env: &HashMap<String, String>) -> bool {
    let ifs = get_ifs(env);
    if ifs.is_empty() {
        return true; // Empty IFS counts as "whitespace only" for this purpose
    }
    ifs.chars().all(|c| c == ' ' || c == '\t' || c == '\n')
}

/// Build a regex-safe pattern from IFS characters for use in character classes.
/// E.g., for IFS=" \t\n", returns " \\t\\n" (escaped for [pattern] use)
pub fn build_ifs_char_class_pattern(ifs: &str) -> String {
    let mut result = String::new();
    for c in ifs.chars() {
        match c {
            '\\' | '^' | '$' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '-' => {
                result.push('\\');
                result.push(c);
            }
            '\t' => result.push_str("\\t"),
            '\n' => result.push_str("\\n"),
            _ => result.push(c),
        }
    }
    result
}

/// Get the first character of IFS (used for joining with $* and ${!prefix*}).
/// Returns space if IFS is undefined, empty string if IFS is empty.
pub fn get_ifs_separator(env: &HashMap<String, String>) -> &str {
    match env.get("IFS") {
        None => " ",
        Some(s) if s.is_empty() => "",
        Some(s) => &s[0..s.chars().next().map_or(0, |c| c.len_utf8())],
    }
}

/// Check if a character is an IFS whitespace character.
fn is_ifs_whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\n'
}

/// Split IFS characters into whitespace and non-whitespace sets.
fn categorize_ifs(ifs: &str) -> (HashSet<char>, HashSet<char>) {
    let mut whitespace = HashSet::new();
    let mut non_whitespace = HashSet::new();
    for ch in ifs.chars() {
        if is_ifs_whitespace(ch) {
            whitespace.insert(ch);
        } else {
            non_whitespace.insert(ch);
        }
    }
    (whitespace, non_whitespace)
}

/// Result of IFS splitting for read builtin.
#[derive(Debug, Clone)]
pub struct IfsReadSplitResult {
    pub words: Vec<String>,
    pub word_starts: Vec<usize>,
}

/// Advanced IFS splitting for the read builtin with proper whitespace/non-whitespace handling.
///
/// IFS has two types of characters:
/// - Whitespace (space, tab, newline): Multiple consecutive ones are collapsed,
///   leading/trailing are stripped
/// - Non-whitespace (like 'x', ':'): Create empty fields when consecutive,
///   trailing ones preserved (except the final delimiter)
///
/// # Arguments
/// * `value` - String to split
/// * `ifs` - IFS characters to split on
/// * `max_split` - Maximum number of splits (for read with multiple vars, the last gets the rest)
/// * `raw` - If true, backslash escaping is disabled (like read -r)
pub fn split_by_ifs_for_read(
    value: &str,
    ifs: &str,
    max_split: Option<usize>,
    raw: bool,
) -> IfsReadSplitResult {
    // Empty IFS means no splitting
    if ifs.is_empty() {
        if value.is_empty() {
            return IfsReadSplitResult { words: vec![], word_starts: vec![] };
        }
        return IfsReadSplitResult { words: vec![value.to_string()], word_starts: vec![0] };
    }

    let (whitespace, non_whitespace) = categorize_ifs(ifs);
    let mut words: Vec<String> = Vec::new();
    let mut word_starts: Vec<usize> = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut pos = 0;

    // Skip leading IFS whitespace
    while pos < chars.len() && whitespace.contains(&chars[pos]) {
        pos += 1;
    }

    // If we've consumed all input, return empty result
    if pos >= chars.len() {
        return IfsReadSplitResult { words: vec![], word_starts: vec![] };
    }

    // Check for leading non-whitespace delimiter (creates empty field)
    if non_whitespace.contains(&chars[pos]) {
        words.push(String::new());
        word_starts.push(pos);
        pos += 1;
        // Skip any whitespace after the delimiter
        while pos < chars.len() && whitespace.contains(&chars[pos]) {
            pos += 1;
        }
    }

    // Now process words
    while pos < chars.len() {
        // Check if we've reached max_split limit
        if let Some(max) = max_split {
            if words.len() >= max {
                break;
            }
        }

        let word_start = pos;
        word_starts.push(word_start);

        // Collect characters until we hit an IFS character
        let mut word = String::new();
        while pos < chars.len() {
            let ch = chars[pos];
            // In non-raw mode, backslash escapes the next character
            if !raw && ch == '\\' {
                pos += 1; // skip backslash
                if pos < chars.len() {
                    word.push(chars[pos]);
                    pos += 1; // skip escaped character
                }
                continue;
            }
            // Check if current char is IFS
            if whitespace.contains(&ch) || non_whitespace.contains(&ch) {
                break;
            }
            word.push(ch);
            pos += 1;
        }

        words.push(word);

        if pos >= chars.len() {
            break;
        }

        // Now handle the delimiter(s)
        // Skip IFS whitespace
        while pos < chars.len() && whitespace.contains(&chars[pos]) {
            pos += 1;
        }

        // Check for non-whitespace delimiter
        if pos < chars.len() && non_whitespace.contains(&chars[pos]) {
            pos += 1;

            // Skip whitespace after non-whitespace delimiter
            while pos < chars.len() && whitespace.contains(&chars[pos]) {
                pos += 1;
            }

            // Check for another non-whitespace delimiter (creates empty field)
            while pos < chars.len() && non_whitespace.contains(&chars[pos]) {
                // Check max_split
                if let Some(max) = max_split {
                    if words.len() >= max {
                        break;
                    }
                }
                // Empty field for this delimiter
                words.push(String::new());
                word_starts.push(pos);
                pos += 1;
                // Skip whitespace after
                while pos < chars.len() && whitespace.contains(&chars[pos]) {
                    pos += 1;
                }
            }
        }
    }

    IfsReadSplitResult { words, word_starts }
}

/// Result of splitByIfsForExpansionEx with leading/trailing delimiter info.
#[derive(Debug, Clone)]
pub struct IfsExpansionSplitResult {
    pub words: Vec<String>,
    /// True if the value started with an IFS whitespace delimiter
    pub had_leading_delimiter: bool,
    /// True if the value ended with an IFS delimiter
    pub had_trailing_delimiter: bool,
}

/// Extended IFS splitting that tracks trailing delimiters.
/// This is needed for proper word boundary handling when literal text follows an expansion.
pub fn split_by_ifs_for_expansion_ex(value: &str, ifs: &str) -> IfsExpansionSplitResult {
    // Empty IFS means no splitting
    if ifs.is_empty() {
        return IfsExpansionSplitResult {
            words: if value.is_empty() { vec![] } else { vec![value.to_string()] },
            had_leading_delimiter: false,
            had_trailing_delimiter: false,
        };
    }

    // Empty value means no words
    if value.is_empty() {
        return IfsExpansionSplitResult {
            words: vec![],
            had_leading_delimiter: false,
            had_trailing_delimiter: false,
        };
    }

    let (whitespace, non_whitespace) = categorize_ifs(ifs);
    let mut words: Vec<String> = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut pos = 0;
    let mut had_leading_delimiter = false;
    let mut had_trailing_delimiter = false;

    // Skip leading IFS whitespace
    let leading_start = pos;
    while pos < chars.len() && whitespace.contains(&chars[pos]) {
        pos += 1;
    }
    if pos > leading_start {
        had_leading_delimiter = true;
    }

    // If we've consumed all input, return empty result
    if pos >= chars.len() {
        return IfsExpansionSplitResult {
            words: vec![],
            had_leading_delimiter: true,
            had_trailing_delimiter: true,
        };
    }

    // Check for leading non-whitespace delimiter (creates empty field)
    if non_whitespace.contains(&chars[pos]) {
        words.push(String::new());
        pos += 1;
        while pos < chars.len() && whitespace.contains(&chars[pos]) {
            pos += 1;
        }
    }

    // Now process words
    while pos < chars.len() {
        let word_start = pos;

        // Collect characters until we hit an IFS character
        while pos < chars.len() {
            let ch = chars[pos];
            if whitespace.contains(&ch) || non_whitespace.contains(&ch) {
                break;
            }
            pos += 1;
        }

        words.push(chars[word_start..pos].iter().collect());

        if pos >= chars.len() {
            had_trailing_delimiter = false;
            break;
        }

        // Now handle the delimiter(s)
        let before_delimiter_pos = pos;
        while pos < chars.len() && whitespace.contains(&chars[pos]) {
            pos += 1;
        }

        // Check for non-whitespace delimiter
        if pos < chars.len() && non_whitespace.contains(&chars[pos]) {
            pos += 1;

            while pos < chars.len() && whitespace.contains(&chars[pos]) {
                pos += 1;
            }

            // Check for more non-whitespace delimiters (creates empty fields)
            while pos < chars.len() && non_whitespace.contains(&chars[pos]) {
                words.push(String::new());
                pos += 1;
                while pos < chars.len() && whitespace.contains(&chars[pos]) {
                    pos += 1;
                }
            }
        }

        // If we've consumed all input, we ended on a delimiter
        if pos >= chars.len() && pos > before_delimiter_pos {
            had_trailing_delimiter = true;
        }
    }

    IfsExpansionSplitResult { words, had_leading_delimiter, had_trailing_delimiter }
}

/// IFS splitting for word expansion (unquoted $VAR, $*, etc.).
pub fn split_by_ifs_for_expansion(value: &str, ifs: &str) -> Vec<String> {
    split_by_ifs_for_expansion_ex(value, ifs).words
}

/// Check if string contains any non-whitespace IFS chars.
fn contains_non_ws_ifs(value: &str, non_whitespace: &HashSet<char>) -> bool {
    value.chars().any(|ch| non_whitespace.contains(&ch))
}

/// Strip trailing IFS from the last variable in read builtin.
///
/// Bash behavior:
/// 1. Strip trailing IFS whitespace characters (but NOT if they're escaped by backslash)
/// 2. If there's a single trailing IFS non-whitespace character, strip it ONLY IF
///    there are no other non-ws IFS chars in the content (excluding the trailing one)
pub fn strip_trailing_ifs_whitespace(value: &str, ifs: &str, raw: bool) -> String {
    if ifs.is_empty() {
        return value.to_string();
    }

    let (whitespace, non_whitespace) = categorize_ifs(ifs);
    let chars: Vec<char> = value.chars().collect();

    // First strip trailing whitespace IFS, but stop if we hit an escaped character
    let mut end = chars.len();
    while end > 0 {
        if !whitespace.contains(&chars[end - 1]) {
            break;
        }
        // In non-raw mode, check if this char is escaped by a backslash
        if !raw && end >= 2 {
            let mut backslash_count = 0;
            let mut pos = end - 2;
            while pos > 0 && chars[pos] == '\\' {
                backslash_count += 1;
                pos -= 1;
            }
            if pos == 0 && chars[0] == '\\' {
                backslash_count += 1;
            }
            // If odd number of backslashes, the char is escaped - stop stripping
            if backslash_count % 2 == 1 {
                break;
            }
        }
        end -= 1;
    }

    let result: String = chars[..end].iter().collect();

    // Check for trailing single IFS non-whitespace char
    if !result.is_empty() && non_whitespace.contains(&chars[end - 1]) {
        // In non-raw mode, check if this char is escaped
        if !raw && result.len() >= 2 {
            let result_chars: Vec<char> = result.chars().collect();
            let mut backslash_count = 0;
            let mut pos = result_chars.len() - 2;
            while pos > 0 && result_chars[pos] == '\\' {
                backslash_count += 1;
                pos -= 1;
            }
            if pos == 0 && result_chars[0] == '\\' {
                backslash_count += 1;
            }
            if backslash_count % 2 == 1 {
                return result;
            }
        }

        // Only strip if there are NO other non-ws IFS chars in the rest of the string
        let content_without_trailing: String = chars[..end - 1].iter().collect();
        if !contains_non_ws_ifs(&content_without_trailing, &non_whitespace) {
            return content_without_trailing;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn make_env_with_ifs(ifs: &str) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("IFS".to_string(), ifs.to_string());
        env
    }

    #[test]
    fn test_get_ifs_default() {
        let env = make_env();
        assert_eq!(get_ifs(&env), DEFAULT_IFS);
    }

    #[test]
    fn test_get_ifs_custom() {
        let env = make_env_with_ifs(":");
        assert_eq!(get_ifs(&env), ":");
    }

    #[test]
    fn test_get_ifs_empty() {
        let env = make_env_with_ifs("");
        assert_eq!(get_ifs(&env), "");
    }

    #[test]
    fn test_is_ifs_empty() {
        let env = make_env();
        assert!(!is_ifs_empty(&env));

        let env = make_env_with_ifs("");
        assert!(is_ifs_empty(&env));

        let env = make_env_with_ifs(" ");
        assert!(!is_ifs_empty(&env));
    }

    #[test]
    fn test_is_ifs_whitespace_only() {
        let env = make_env();
        assert!(is_ifs_whitespace_only(&env));

        let env = make_env_with_ifs(" \t\n");
        assert!(is_ifs_whitespace_only(&env));

        let env = make_env_with_ifs(":");
        assert!(!is_ifs_whitespace_only(&env));

        let env = make_env_with_ifs(" :");
        assert!(!is_ifs_whitespace_only(&env));
    }

    #[test]
    fn test_get_ifs_separator() {
        let env = make_env();
        assert_eq!(get_ifs_separator(&env), " ");

        let env = make_env_with_ifs("");
        assert_eq!(get_ifs_separator(&env), "");

        let env = make_env_with_ifs(":");
        assert_eq!(get_ifs_separator(&env), ":");

        let env = make_env_with_ifs(":,");
        assert_eq!(get_ifs_separator(&env), ":");
    }

    #[test]
    fn test_split_by_ifs_for_expansion_simple() {
        let words = split_by_ifs_for_expansion("a b c", " ");
        assert_eq!(words, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_by_ifs_for_expansion_empty_ifs() {
        let words = split_by_ifs_for_expansion("a b c", "");
        assert_eq!(words, vec!["a b c"]);
    }

    #[test]
    fn test_split_by_ifs_for_expansion_colon() {
        let words = split_by_ifs_for_expansion("a:b:c", ":");
        assert_eq!(words, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_by_ifs_for_expansion_consecutive_delimiters() {
        let words = split_by_ifs_for_expansion("a::b", ":");
        assert_eq!(words, vec!["a", "", "b"]);
    }

    #[test]
    fn test_split_by_ifs_for_read_simple() {
        let result = split_by_ifs_for_read("a b c", " ", None, false);
        assert_eq!(result.words, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_by_ifs_for_read_with_max_split() {
        let result = split_by_ifs_for_read("a b c d", " ", Some(2), false);
        assert_eq!(result.words, vec!["a", "b"]);
    }

    #[test]
    fn test_strip_trailing_ifs_whitespace() {
        assert_eq!(strip_trailing_ifs_whitespace("hello  ", " ", false), "hello");
        assert_eq!(strip_trailing_ifs_whitespace("hello", " ", false), "hello");
        assert_eq!(strip_trailing_ifs_whitespace("ax", "x ", false), "a");
        assert_eq!(strip_trailing_ifs_whitespace("axx", "x ", false), "axx");
    }
}
