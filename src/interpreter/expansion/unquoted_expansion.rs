//! Unquoted Expansion Handlers
//!
//! Provides helper functions for unquoted expansions that need special handling:
//! - Unquoted $@ and $* (with and without prefix/suffix)
//! - Unquoted ${arr[@]} and ${arr[*]}
//! - IFS splitting and glob expansion for unquoted contexts

use crate::interpreter::expansion::{get_array_elements, get_var_names_with_prefix};
use crate::interpreter::helpers::{get_ifs, get_ifs_separator, split_by_ifs_for_expansion};
use crate::interpreter::InterpreterState;

/// Result type for unquoted expansion handlers.
#[derive(Debug, Clone)]
pub struct UnquotedExpansionResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Split a value by IFS for unquoted expansion.
/// This is used when expanding unquoted variables, command substitutions, etc.
pub fn split_unquoted_value(value: &str, state: &InterpreterState) -> Vec<String> {
    let ifs = get_ifs(&state.env);
    split_by_ifs_for_expansion(value, ifs)
}

/// Expand unquoted array ${arr[@]} or ${arr[*]}.
/// For [@], each element is split by IFS.
/// For [*], all elements are joined with IFS first char, then split.
pub fn expand_unquoted_array(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    if values.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${arr[*]} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = values.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${arr[@]} - each element is split by IFS
        let mut result = Vec::new();
        for value in values {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted positional parameters $@ or $*.
/// For $@, each parameter is split by IFS.
/// For $*, all parameters are joined with IFS first char, then split.
pub fn expand_unquoted_positional(
    state: &InterpreterState,
    is_star: bool,
) -> UnquotedExpansionResult {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    if is_star {
        // $* - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = params.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // $@ - each parameter is split by IFS
        let mut result = Vec::new();
        for param in params {
            let split_values = split_unquoted_value(&param, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted ${!prefix@} or ${!prefix*} - variable name prefix expansion.
pub fn expand_unquoted_var_name_prefix(
    state: &InterpreterState,
    prefix: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let names = get_var_names_with_prefix(state, prefix);

    if names.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${!prefix*} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = names.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${!prefix@} - each name is split by IFS
        let mut result = Vec::new();
        for name in names {
            let split_values = split_unquoted_value(&name, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted ${!arr[@]} or ${!arr[*]} - array keys expansion.
pub fn expand_unquoted_array_keys(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let keys: Vec<String> = elements
        .iter()
        .map(|(idx, _)| match idx {
            crate::interpreter::expansion::ArrayIndex::Numeric(n) => n.to_string(),
            crate::interpreter::expansion::ArrayIndex::String(s) => s.clone(),
        })
        .collect();

    if keys.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${!arr[*]} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = keys.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${!arr[@]} - each key is split by IFS
        let mut result = Vec::new();
        for key in keys {
            let split_values = split_unquoted_value(&key, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state_with_array(name: &str, values: &[&str]) -> InterpreterState {
        let mut env = HashMap::new();
        for (i, v) in values.iter().enumerate() {
            env.insert(format!("{}_{}", name, i), v.to_string());
        }
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_split_unquoted_value() {
        let state = InterpreterState::default();
        let result = split_unquoted_value("hello world", &state);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_array_at() {
        let state = make_state_with_array("arr", &["a b", "c d"]);
        let result = expand_unquoted_array(&state, "arr", false);
        // Each element is split by IFS
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_array_star() {
        let state = make_state_with_array("arr", &["a b", "c d"]);
        let result = expand_unquoted_array(&state, "arr", true);
        // Join with space, then split: "a b c d" -> ["a", "b", "c", "d"]
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_positional_at() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "a b".to_string());
        state.env.insert("2".to_string(), "c d".to_string());

        let result = expand_unquoted_positional(&state, false);
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_positional_star() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "a b".to_string());
        state.env.insert("2".to_string(), "c d".to_string());

        let result = expand_unquoted_positional(&state, true);
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_var_name_prefix() {
        let mut state = InterpreterState::default();
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());
        state.env.insert("PWD".to_string(), "/home".to_string());

        let result = expand_unquoted_var_name_prefix(&state, "P", false);
        assert!(result.values.contains(&"PATH".to_string()));
        assert!(result.values.contains(&"PWD".to_string()));
    }
}
