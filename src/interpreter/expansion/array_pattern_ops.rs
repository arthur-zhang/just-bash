//! Array Pattern Operations
//!
//! Handles pattern replacement and pattern removal for array expansions:
//! - "${arr[@]/pattern/replacement}" - pattern replacement
//! - "${arr[@]#pattern}" - prefix removal
//! - "${arr[@]%pattern}" - suffix removal

use crate::interpreter::expansion::{
    apply_pattern_removal, get_array_elements, pattern_to_regex, PatternRemovalSide,
};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Apply pattern replacement to a list of array values.
/// This is a pre-expanded version where the pattern regex and replacement are already resolved.
pub fn apply_array_pattern_replacement(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    regex_pattern: &str,
    replacement: &str,
    replace_all: bool,
) -> Vec<String> {
    // Get array elements
    let elements = get_array_elements(state, array_name);
    let mut values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    // If no elements, check for scalar (treat as single-element array)
    if values.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            values.push(scalar_value.clone());
        }
    }

    if values.is_empty() {
        return vec![];
    }

    // Apply replacement to each element
    let replaced_values: Vec<String> = match Regex::new(regex_pattern) {
        Ok(re) => values
            .iter()
            .map(|value| {
                if replace_all {
                    re.replace_all(value, replacement).to_string()
                } else {
                    re.replace(value, replacement).to_string()
                }
            })
            .collect(),
        Err(_) => values,
    };

    if is_star {
        // "${arr[*]/...}" - join all elements with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        vec![replaced_values.join(ifs_sep)]
    } else {
        // "${arr[@]/...}" - each element as a separate word
        replaced_values
    }
}

/// Apply pattern removal to a list of array values.
/// This is a pre-expanded version where the pattern regex is already resolved.
pub fn apply_array_pattern_removal(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    regex_str: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> Vec<String> {
    // Get array elements
    let elements = get_array_elements(state, array_name);
    let mut values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    // If no elements, check for scalar (treat as single-element array)
    if values.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            values.push(scalar_value.clone());
        }
    }

    if values.is_empty() {
        return vec![];
    }

    // Apply pattern removal to each element
    let result_values: Vec<String> = values
        .iter()
        .map(|value| apply_pattern_removal(value, regex_str, side, greedy))
        .collect();

    if is_star {
        // "${arr[*]#...}" - join all elements with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        vec![result_values.join(ifs_sep)]
    } else {
        // "${arr[@]#...}" - each element as a separate word
        result_values
    }
}

/// Build a regex pattern string from a simple pattern text.
/// For more complex patterns with word parts, use the interpreter's full expansion.
pub fn build_simple_pattern_regex(pattern: &str, greedy: bool, extglob: bool) -> String {
    pattern_to_regex(pattern, greedy, extglob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("arr_0".to_string(), "hello world".to_string());
        env.insert("arr_1".to_string(), "foo bar".to_string());
        env.insert("arr_2".to_string(), "hello foo".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_array_pattern_replacement() {
        let state = make_state();
        let result = apply_array_pattern_replacement(&state, "arr", false, "hello", "hi", false);
        assert_eq!(result, vec!["hi world", "foo bar", "hi foo"]);
    }

    #[test]
    fn test_array_pattern_replacement_star() {
        let state = make_state();
        let result = apply_array_pattern_replacement(&state, "arr", true, "hello", "hi", false);
        // Joined with default IFS (space)
        assert_eq!(result, vec!["hi world foo bar hi foo"]);
    }

    #[test]
    fn test_array_pattern_removal() {
        let state = make_state();
        let regex = pattern_to_regex("hello", false, false);
        let result = apply_array_pattern_removal(&state, "arr", false, &regex, PatternRemovalSide::Prefix, false);
        assert_eq!(result, vec![" world", "foo bar", " foo"]);
    }

    #[test]
    fn test_array_pattern_removal_empty_array() {
        let state = InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        };
        let result =
            apply_array_pattern_removal(&state, "nonexistent", false, ".*", PatternRemovalSide::Prefix, false);
        assert!(result.is_empty());
    }
}
