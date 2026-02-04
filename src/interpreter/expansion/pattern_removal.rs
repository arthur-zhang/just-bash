//! Pattern Removal Helpers
//!
//! Functions for ${var#pattern}, ${var%pattern}, ${!prefix*} etc.

use crate::interpreter::InterpreterState;
use regex_lite::Regex;
use std::collections::HashSet;

/// Apply pattern removal (prefix or suffix strip) to a single value.
/// Used by both scalar and vectorized array operations.
pub fn apply_pattern_removal(
    value: &str,
    regex_str: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> String {
    // Note: regex-lite doesn't support 's' flag (dotall), but for most patterns this works
    if side == PatternRemovalSide::Prefix {
        // Prefix removal: greedy matches longest from start, non-greedy matches shortest
        let pattern = format!("^{}", regex_str);
        if let Ok(re) = Regex::new(&pattern) {
            return re.replace(value, "").to_string();
        }
        return value.to_string();
    }

    // Suffix removal needs special handling because we need to find
    // the rightmost (shortest) or leftmost (longest) match
    let pattern = format!("{}$", regex_str);
    if let Ok(re) = Regex::new(&pattern) {
        if greedy {
            // %% - longest match: use regex directly (finds leftmost match)
            return re.replace(value, "").to_string();
        }
        // % - shortest match: find rightmost position where pattern matches to end
        let chars: Vec<char> = value.chars().collect();
        for i in (0..=chars.len()).rev() {
            let suffix: String = chars[i..].iter().collect();
            if re.is_match(&suffix) {
                return chars[..i].iter().collect();
            }
        }
    }
    value.to_string()
}

/// Side for pattern removal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternRemovalSide {
    Prefix,
    Suffix,
}

/// Get variable names that match a given prefix.
/// Used for ${!prefix*} and ${!prefix@} expansions.
/// Handles arrays properly - includes array base names from __length markers,
/// excludes internal storage keys like arr_0, arr__length.
pub fn get_var_names_with_prefix(state: &InterpreterState, prefix: &str) -> Vec<String> {
    let env_keys: Vec<&String> = state.env.keys().collect();
    let mut matching_vars: HashSet<String> = HashSet::new();

    // Get sets of array names for filtering
    let assoc_arrays = state
        .associative_arrays
        .as_ref()
        .cloned()
        .unwrap_or_default();

    let mut indexed_arrays: HashSet<String> = HashSet::new();
    let indexed_pattern = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)_\d+$").unwrap();
    let length_pattern = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)__length$").unwrap();

    // Find indexed arrays by looking for _\d+$ patterns
    for k in &env_keys {
        if let Some(caps) = indexed_pattern.captures(k) {
            if let Some(m) = caps.get(1) {
                indexed_arrays.insert(m.as_str().to_string());
            }
        }
        if let Some(caps) = length_pattern.captures(k) {
            if let Some(m) = caps.get(1) {
                indexed_arrays.insert(m.as_str().to_string());
            }
        }
    }

    // Helper to check if a key is an associative array element
    let is_assoc_array_element = |key: &str| -> bool {
        for array_name in &assoc_arrays {
            let elem_prefix = format!("{}_", array_name);
            if key.starts_with(&elem_prefix) && key != array_name {
                return true;
            }
        }
        false
    };

    let trailing_digits = Regex::new(r"_\d+$").unwrap();

    for k in &env_keys {
        if k.starts_with(prefix) {
            // Check if this is an internal array storage key
            if k.contains("__") {
                // For __length markers, add the base array name
                if let Some(caps) = length_pattern.captures(k) {
                    if let Some(m) = caps.get(1) {
                        let base_name = m.as_str();
                        if base_name.starts_with(prefix) {
                            matching_vars.insert(base_name.to_string());
                        }
                    }
                }
                // Skip other internal markers
            } else if trailing_digits.is_match(k) {
                // Skip indexed array element storage (arr_0)
                // But add the base array name if it matches
                if let Some(caps) = indexed_pattern.captures(k) {
                    if let Some(m) = caps.get(1) {
                        let base_name = m.as_str();
                        if base_name.starts_with(prefix) {
                            matching_vars.insert(base_name.to_string());
                        }
                    }
                }
            } else if is_assoc_array_element(k) {
                // Skip associative array elements
            } else {
                // Regular variable
                matching_vars.insert(k.to_string());
            }
        }
    }

    let mut result: Vec<String> = matching_vars.into_iter().collect();
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_prefix_removal() {
        assert_eq!(
            apply_pattern_removal("hello world", "hello ", PatternRemovalSide::Prefix, false),
            "world"
        );
        assert_eq!(
            apply_pattern_removal("hello world", ".*o ", PatternRemovalSide::Prefix, false),
            "world"
        );
    }

    #[test]
    fn test_suffix_removal() {
        assert_eq!(
            apply_pattern_removal("hello world", " world", PatternRemovalSide::Suffix, false),
            "hello"
        );
    }

    #[test]
    fn test_get_var_names_with_prefix() {
        let mut state = InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        };
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());
        state.env.insert("PWD".to_string(), "/home".to_string());
        state.env.insert("HOME".to_string(), "/home/user".to_string());

        let result = get_var_names_with_prefix(&state, "P");
        assert!(result.contains(&"PATH".to_string()));
        assert!(result.contains(&"PWD".to_string()));
        assert!(!result.contains(&"HOME".to_string()));
    }
}
