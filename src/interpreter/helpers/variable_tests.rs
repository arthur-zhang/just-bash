//! Variable test operators for bash conditionals.
//!
//! Implements the -v (variable is set) test operator.
//! Used in [[ ]] and test/[ ] commands.

use std::collections::HashMap;
use regex_lite::Regex;
use crate::interpreter::types::InterpreterState;
use crate::interpreter::helpers::array::{get_array_indices, get_assoc_array_keys};

/// Check if a variable is set (-v test).
/// Handles both simple variables and array element access with negative indices.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `env` - The environment variables
/// * `operand` - The variable name to test, may include array subscript (e.g., "arr[0]", "arr[-1]")
///
/// # Returns
/// A tuple of (is_set, stderr_message) where stderr_message contains any warnings
pub fn evaluate_variable_test(
    state: &InterpreterState,
    env: &HashMap<String, String>,
    operand: &str,
    current_line: Option<i32>,
) -> (bool, Option<String>) {
    // Check for array element syntax: var[index]
    let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();

    if let Some(caps) = array_re.captures(operand) {
        let array_name = &caps[1];
        let index_expr = &caps[2];

        // Check if this is an associative array
        let is_assoc = state.associative_arrays.as_ref().map_or(false, |a| a.contains(array_name));

        if is_assoc {
            // For associative arrays, use the key as-is (strip quotes if present)
            let mut key = index_expr.to_string();
            // Remove surrounding quotes if present
            if (key.starts_with('\'') && key.ends_with('\''))
                || (key.starts_with('"') && key.ends_with('"'))
            {
                if key.len() >= 2 {
                    key = key[1..key.len() - 1].to_string();
                }
            }
            // Expand variables in key
            let var_re = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
            let expanded_key = var_re.replace_all(&key, |caps: &regex_lite::Captures| {
                let var_name = &caps[1];
                env.get(var_name).cloned().unwrap_or_default()
            });
            let env_key = format!("{}_{}", array_name, expanded_key);
            return (env.contains_key(&env_key), None);
        }

        // Try to parse as numeric index
        let index: Option<i64> = if let Ok(n) = index_expr.parse::<i64>() {
            Some(n)
        } else {
            // Try looking up as variable
            env.get(index_expr).and_then(|v| v.parse::<i64>().ok())
        };

        if let Some(mut idx) = index {
            // Handle negative indices - bash counts from max_index + 1
            if idx < 0 {
                let indices = get_array_indices(env, array_name);
                let line_num = current_line.unwrap_or(0);
                if indices.is_empty() {
                    // Empty array with negative index - emit warning and return false
                    let stderr = format!("bash: line {}: {}: bad array subscript\n", line_num, array_name);
                    return (false, Some(stderr));
                }
                let max_index = *indices.iter().max().unwrap_or(&0);
                idx = max_index + 1 + idx;
                if idx < 0 {
                    // Out of bounds negative index - emit warning and return false
                    let stderr = format!("bash: line {}: {}: bad array subscript\n", line_num, array_name);
                    return (false, Some(stderr));
                }
            }

            let env_key = format!("{}_{}", array_name, idx);
            return (env.contains_key(&env_key), None);
        }

        // If we can't parse the index, return false
        return (false, None);
    }

    // Check if it's a regular variable
    if env.contains_key(operand) {
        return (true, None);
    }

    // Check if it's an array with elements (test -v arrayname without subscript)
    // For associative arrays, check if there are any keys
    if state.associative_arrays.as_ref().map_or(false, |a| a.contains(operand)) {
        return (!get_assoc_array_keys(env, operand).is_empty(), None);
    }

    // For indexed arrays, check if there are any indices
    (!get_array_indices(env, operand).is_empty(), None)
}

/// Check if a variable is a nameref (-R test).
pub fn evaluate_nameref_test(state: &InterpreterState, operand: &str) -> bool {
    state.namerefs.as_ref().map_or(false, |refs| refs.contains(operand))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    fn make_env() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn test_evaluate_variable_test_simple() {
        let state = make_state();
        let mut env = make_env();

        // Variable not set
        let (is_set, _) = evaluate_variable_test(&state, &env, "foo", None);
        assert!(!is_set);

        // Variable set
        env.insert("foo".to_string(), "bar".to_string());
        let (is_set, _) = evaluate_variable_test(&state, &env, "foo", None);
        assert!(is_set);
    }

    #[test]
    fn test_evaluate_variable_test_array_element() {
        let state = make_state();
        let mut env = make_env();

        // Array element not set
        let (is_set, _) = evaluate_variable_test(&state, &env, "arr[0]", None);
        assert!(!is_set);

        // Array element set
        env.insert("arr_0".to_string(), "value".to_string());
        let (is_set, _) = evaluate_variable_test(&state, &env, "arr[0]", None);
        assert!(is_set);
    }

    #[test]
    fn test_evaluate_variable_test_array_name() {
        let state = make_state();
        let mut env = make_env();

        // Array with no elements
        let (is_set, _) = evaluate_variable_test(&state, &env, "arr", None);
        assert!(!is_set);

        // Array with elements
        env.insert("arr_0".to_string(), "value".to_string());
        let (is_set, _) = evaluate_variable_test(&state, &env, "arr", None);
        assert!(is_set);
    }

    #[test]
    fn test_evaluate_variable_test_negative_index() {
        let state = make_state();
        let mut env = make_env();

        // Set up array with indices 0, 1, 2
        env.insert("arr_0".to_string(), "a".to_string());
        env.insert("arr_1".to_string(), "b".to_string());
        env.insert("arr_2".to_string(), "c".to_string());

        // arr[-1] should be arr[2] (max_index + 1 + (-1) = 2 + 1 - 1 = 2)
        let (is_set, _) = evaluate_variable_test(&state, &env, "arr[-1]", None);
        assert!(is_set);

        // arr[-4] should be out of bounds (2 + 1 - 4 = -1)
        let (is_set, stderr) = evaluate_variable_test(&state, &env, "arr[-4]", Some(1));
        assert!(!is_set);
        assert!(stderr.is_some());
        assert!(stderr.unwrap().contains("bad array subscript"));
    }

    #[test]
    fn test_evaluate_nameref_test() {
        let mut state = make_state();

        // Not a nameref
        assert!(!evaluate_nameref_test(&state, "foo"));

        // Mark as nameref
        state.namerefs = Some(std::collections::HashSet::new());
        state.namerefs.as_mut().unwrap().insert("foo".to_string());
        assert!(evaluate_nameref_test(&state, "foo"));
    }
}
