//! Array helper functions for the interpreter.
//!
//! Provides utilities for working with bash arrays (both indexed and associative).

use std::collections::HashMap;
use crate::{WordNode, WordPart, LiteralPart, GlobPart, SingleQuotedPart, DoubleQuotedPart, EscapedPart, BraceExpansionPart, TildeExpansionPart, BraceItem};

/// Get all indices of an array, sorted in ascending order.
/// Arrays are stored as `name_0`, `name_1`, etc. in the environment.
pub fn get_array_indices(env: &HashMap<String, String>, array_name: &str) -> Vec<i64> {
    let prefix = format!("{}_", array_name);
    let mut indices: Vec<i64> = Vec::new();

    for key in env.keys() {
        if let Some(index_str) = key.strip_prefix(&prefix) {
            if let Ok(index) = index_str.parse::<i64>() {
                // Only include numeric indices (not __length or other metadata)
                if index.to_string() == index_str {
                    indices.push(index);
                }
            }
        }
    }

    indices.sort();
    indices
}

/// Clear all elements of an array from the environment.
pub fn clear_array(env: &mut HashMap<String, String>, array_name: &str) {
    let prefix = format!("{}_", array_name);
    let keys_to_remove: Vec<String> = env
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();

    for key in keys_to_remove {
        env.remove(&key);
    }
}

/// Get all keys of an associative array.
/// For associative arrays, keys are stored as `name_key` where key is a string.
pub fn get_assoc_array_keys(env: &HashMap<String, String>, array_name: &str) -> Vec<String> {
    let prefix = format!("{}_", array_name);
    let metadata_suffix = format!("{}__length", array_name);
    let mut keys: Vec<String> = Vec::new();

    for env_key in env.keys() {
        // Skip the metadata entry (name__length)
        if env_key == &metadata_suffix {
            continue;
        }
        if let Some(key) = env_key.strip_prefix(&prefix) {
            // Skip if the key itself starts with underscore (would be part of metadata pattern)
            if key.starts_with("_length") {
                continue;
            }
            keys.push(key.to_string());
        }
    }

    keys.sort();
    keys
}

/// Remove surrounding quotes from a key string.
/// Handles 'key' and "key" → key
pub fn unquote_key(key: &str) -> &str {
    if key.len() >= 2
        && ((key.starts_with('\'') && key.ends_with('\''))
            || (key.starts_with('"') && key.ends_with('"')))
    {
        &key[1..key.len() - 1]
    } else {
        key
    }
}

/// Parsed keyed array element from an AST WordNode like [key]=value or [key]+=value.
#[derive(Debug, Clone)]
pub struct ParsedKeyedElement {
    pub key: String,
    pub value_parts: Vec<WordPart>,
    pub append: bool,
}

/// Parse a keyed array element from an AST WordNode like [key]=value or [key]+=value.
/// Returns None if not a keyed element pattern.
pub fn parse_keyed_element_from_word(word: &WordNode) -> Option<ParsedKeyedElement> {
    if word.parts.len() < 2 {
        return None;
    }

    let first = &word.parts[0];
    let second = &word.parts[1];

    // Check for [key]= or [key]+= pattern
    // First part should be a Glob with pattern like "[key]" or just "["
    let glob_pattern = match first {
        WordPart::Glob(GlobPart { pattern }) if pattern.starts_with('[') => pattern.as_str(),
        _ => return None,
    };

    let mut key: String;
    let mut second_part_index = 1;

    // Check if this is a nested bracket case by looking at second
    match second {
        WordPart::Literal(LiteralPart { value }) if value.starts_with(']') => {
            // Nested bracket case: [a[0]]= is parsed as Glob("[a[0]") + Literal("]=...")
            let after_bracket = &value[1..]; // Remove the leading ]

            if after_bracket.starts_with("+=") || after_bracket.starts_with('=') {
                key = glob_pattern[1..].to_string();
            } else if after_bracket.is_empty() {
                // The ] was the whole second part, check third part for = or +=
                if word.parts.len() < 3 {
                    return None;
                }
                let third = &word.parts[2];
                match third {
                    WordPart::Literal(LiteralPart { value }) if value.starts_with('=') || value.starts_with("+=") => {
                        key = glob_pattern[1..].to_string();
                        second_part_index = 2;
                    }
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        WordPart::DoubleQuoted(_) | WordPart::SingleQuoted(_) if glob_pattern == "[" => {
            // Double/single-quoted key case: ["key"]= or ['key']=
            if word.parts.len() < 3 {
                return None;
            }
            let third = &word.parts[2];
            match third {
                WordPart::Literal(LiteralPart { value }) if value.starts_with("]=") || value.starts_with("]+=") => {
                    // Extract key from the quoted part
                    key = match second {
                        WordPart::SingleQuoted(SingleQuotedPart { value }) => value.clone(),
                        WordPart::DoubleQuoted(DoubleQuotedPart { parts }) => {
                            let mut k = String::new();
                            for inner in parts {
                                match inner {
                                    WordPart::Literal(LiteralPart { value }) => k.push_str(value),
                                    WordPart::Escaped(EscapedPart { value }) => k.push_str(value),
                                    _ => {}
                                }
                            }
                            k
                        }
                        _ => return None,
                    };
                    second_part_index = 2;
                }
                _ => return None,
            }
        }
        WordPart::Literal(LiteralPart { value }) if glob_pattern.ends_with(']') => {
            // Normal case: [key]= where key has no nested brackets
            if !value.starts_with('=') && !value.starts_with("+=") {
                return None;
            }
            // Extract key from the Glob pattern (remove [ and ])
            key = glob_pattern[1..glob_pattern.len() - 1].to_string();
        }
        _ => return None,
    }

    // Remove surrounding quotes from key
    key = unquote_key(&key).to_string();

    // Get the actual content after = or += from second_part
    let second_part = &word.parts[second_part_index];
    let assignment_content = match second_part {
        WordPart::Literal(LiteralPart { value }) => {
            if value.starts_with("]=") {
                &value[1..]
            } else if value.starts_with("]+=") {
                &value[1..]
            } else {
                value.as_str()
            }
        }
        _ => return None,
    };

    // Determine if this is an append operation
    let append = assignment_content.starts_with("+=");
    if !append && !assignment_content.starts_with('=') {
        return None;
    }

    // Extract value parts: everything after the = (or +=)
    let mut value_parts: Vec<WordPart> = Vec::new();

    // The second part may have content after the = sign
    let eq_len = if append { 2 } else { 1 }; // "+=" vs "="
    let after_eq = &assignment_content[eq_len..];
    if !after_eq.is_empty() {
        value_parts.push(WordPart::Literal(LiteralPart { value: after_eq.to_string() }));
    }

    // Add remaining parts (parts[second_part_index+1], etc.)
    // Converting BraceExpansion to Literal
    for i in (second_part_index + 1)..word.parts.len() {
        let part = &word.parts[i];
        match part {
            WordPart::BraceExpansion(brace) => {
                // Convert brace expansion to literal string
                value_parts.push(WordPart::Literal(LiteralPart {
                    value: brace_to_literal(brace),
                }));
            }
            _ => value_parts.push(part.clone()),
        }
    }

    Some(ParsedKeyedElement { key, value_parts, append })
}

/// Convert a BraceExpansion node back to its literal form.
/// e.g., {a,b,c} or {1..5}
fn brace_to_literal(part: &BraceExpansionPart) -> String {
    let items: Vec<String> = part.items.iter().map(|item| {
        match item {
            BraceItem::Range { start, end, step, start_str, end_str } => {
                let start_s = start_str.as_ref().map_or_else(|| start.to_string(), |s| s.clone());
                let end_s = end_str.as_ref().map_or_else(|| end.to_string(), |s| s.clone());
                let mut range = format!("{}..{}", start_s, end_s);
                if let Some(s) = step {
                    range.push_str(&format!("..{}", s));
                }
                range
            }
            BraceItem::Word { word } => word_to_literal_string(word),
        }
    }).collect();
    format!("{{{}}}", items.join(","))
}

/// Extract literal string content from a Word node (without expansion).
/// This is used for parsing associative array element syntax like [key]=value
/// where the [key] part may be parsed as a Glob.
pub fn word_to_literal_string(word: &WordNode) -> String {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(LiteralPart { value }) => result.push_str(value),
            WordPart::Glob(GlobPart { pattern }) => result.push_str(pattern),
            WordPart::SingleQuoted(SingleQuotedPart { value }) => result.push_str(value),
            WordPart::DoubleQuoted(DoubleQuotedPart { parts }) => {
                for inner in parts {
                    match inner {
                        WordPart::Literal(LiteralPart { value }) => result.push_str(value),
                        WordPart::Escaped(EscapedPart { value }) => result.push_str(value),
                        _ => {}
                    }
                }
            }
            WordPart::Escaped(EscapedPart { value }) => result.push_str(value),
            WordPart::BraceExpansion(brace) => {
                result.push_str(&brace_to_literal(brace));
            }
            WordPart::TildeExpansion(TildeExpansionPart { user }) => {
                result.push('~');
                if let Some(u) = user {
                    result.push_str(u);
                }
            }
            _ => {}
        }
    }
    result
}

/// Get an array element value.
pub fn get_array_element<'a>(env: &'a HashMap<String, String>, array_name: &str, index: i64) -> Option<&'a String> {
    let key = format!("{}_{}", array_name, index);
    env.get(&key)
}

/// Set an array element value.
pub fn set_array_element(env: &mut HashMap<String, String>, array_name: &str, index: i64, value: String) {
    let key = format!("{}_{}", array_name, index);
    env.insert(key, value);
}

/// Get an associative array element value.
pub fn get_assoc_array_element<'a>(env: &'a HashMap<String, String>, array_name: &str, key: &str) -> Option<&'a String> {
    let env_key = format!("{}_{}", array_name, key);
    env.get(&env_key)
}

/// Set an associative array element value.
pub fn set_assoc_array_element(env: &mut HashMap<String, String>, array_name: &str, key: &str, value: String) {
    let env_key = format!("{}_{}", array_name, key);
    env.insert(env_key, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_array_indices() {
        let mut env = HashMap::new();
        env.insert("arr_0".to_string(), "a".to_string());
        env.insert("arr_2".to_string(), "b".to_string());
        env.insert("arr_5".to_string(), "c".to_string());
        env.insert("other".to_string(), "x".to_string());

        let indices = get_array_indices(&env, "arr");
        assert_eq!(indices, vec![0, 2, 5]);
    }

    #[test]
    fn test_get_array_indices_empty() {
        let env = HashMap::new();
        let indices = get_array_indices(&env, "arr");
        assert!(indices.is_empty());
    }

    #[test]
    fn test_clear_array() {
        let mut env = HashMap::new();
        env.insert("arr_0".to_string(), "a".to_string());
        env.insert("arr_1".to_string(), "b".to_string());
        env.insert("other".to_string(), "x".to_string());

        clear_array(&mut env, "arr");

        assert!(!env.contains_key("arr_0"));
        assert!(!env.contains_key("arr_1"));
        assert!(env.contains_key("other"));
    }

    #[test]
    fn test_get_assoc_array_keys() {
        let mut env = HashMap::new();
        env.insert("map_foo".to_string(), "1".to_string());
        env.insert("map_bar".to_string(), "2".to_string());
        env.insert("map_baz".to_string(), "3".to_string());
        env.insert("other".to_string(), "x".to_string());

        let keys = get_assoc_array_keys(&env, "map");
        assert_eq!(keys, vec!["bar", "baz", "foo"]);
    }

    #[test]
    fn test_unquote_key() {
        assert_eq!(unquote_key("'hello'"), "hello");
        assert_eq!(unquote_key("\"world\""), "world");
        assert_eq!(unquote_key("plain"), "plain");
        assert_eq!(unquote_key("'"), "'");
    }

    #[test]
    fn test_get_set_array_element() {
        let mut env = HashMap::new();
        set_array_element(&mut env, "arr", 0, "hello".to_string());
        set_array_element(&mut env, "arr", 5, "world".to_string());

        assert_eq!(get_array_element(&env, "arr", 0), Some(&"hello".to_string()));
        assert_eq!(get_array_element(&env, "arr", 5), Some(&"world".to_string()));
        assert_eq!(get_array_element(&env, "arr", 1), None);
    }

    #[test]
    fn test_get_set_assoc_array_element() {
        let mut env = HashMap::new();
        set_assoc_array_element(&mut env, "map", "foo", "bar".to_string());
        set_assoc_array_element(&mut env, "map", "baz", "qux".to_string());

        assert_eq!(get_assoc_array_element(&env, "map", "foo"), Some(&"bar".to_string()));
        assert_eq!(get_assoc_array_element(&env, "map", "baz"), Some(&"qux".to_string()));
        assert_eq!(get_assoc_array_element(&env, "map", "missing"), None);
    }
}
