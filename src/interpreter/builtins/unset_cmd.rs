//! unset - Remove variables/functions builtin
//!
//! Supports:
//! - unset VAR - remove variable
//! - unset -v VAR - remove variable (explicit)
//! - unset -f FUNC - remove function
//! - unset 'a[i]' - remove array element

use regex_lite::Regex;
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::result;
use crate::interpreter::helpers::nameref::{is_nameref, resolve_nameref};
use crate::interpreter::helpers::readonly::is_readonly;
use crate::interpreter::expansion::variable::{is_array, get_array_elements, ArrayIndex};

/// Check if a name is a valid bash variable name.
fn is_valid_variable_name(name: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    re.is_match(name)
}

/// Evaluate an array index expression.
/// Returns the evaluated numeric index.
fn evaluate_array_index(state: &InterpreterState, index_expr: &str) -> i64 {
    // Try to parse as integer
    if let Ok(index) = index_expr.parse::<i64>() {
        return index;
    }

    // Try to look up as variable
    if let Some(var_value) = state.env.get(index_expr) {
        if let Ok(index) = var_value.parse::<i64>() {
            return index;
        }
    }

    // Default to 0
    0
}

/// Handle the unset builtin command
pub fn handle_unset(
    state: &mut InterpreterState,
    args: &[String],
) -> ExecResult {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Variable,
        Function,
        Both,
    }

    let mut mode = Mode::Both; // Default: unset both var and func
    let mut stderr = String::new();
    let mut exit_code = 0;

    let array_element_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();

    for arg in args {
        // Handle flags
        if arg == "-v" {
            mode = Mode::Variable;
            continue;
        }
        if arg == "-f" {
            mode = Mode::Function;
            continue;
        }

        if mode == Mode::Function {
            state.functions.remove(arg);
            continue;
        }

        // Check for array element syntax: varName[index]
        if let Some(captures) = array_element_re.captures(arg) {
            let array_name = captures.get(1).unwrap().as_str();
            let index_expr = captures.get(2).unwrap().as_str();

            // Handle [@] or [*] - unset entire array
            if index_expr == "@" || index_expr == "*" {
                let elements = get_array_elements(state, array_name);
                for (idx, _) in elements {
                    let key = match idx {
                        ArrayIndex::Numeric(n) => n.to_string(),
                        ArrayIndex::String(s) => s,
                    };
                    state.env.remove(&format!("{}_{}", array_name, key));
                }
                state.env.remove(array_name);
                continue;
            }

            // Check if this is an associative array
            let is_assoc = state.associative_arrays
                .as_ref()
                .map_or(false, |a| a.contains(array_name));

            if is_assoc {
                // For associative arrays, use the key directly
                let key = index_expr.trim_matches(|c| c == '\'' || c == '"');
                state.env.remove(&format!("{}_{}", array_name, key));
                continue;
            }

            // Check if variable is an indexed array
            let is_indexed_array = is_array(state, array_name);

            // Check if variable was explicitly declared as a scalar
            let is_scalar = state.env.contains_key(array_name)
                && !is_indexed_array
                && !is_assoc;

            if is_scalar && mode == Mode::Variable {
                stderr.push_str(&format!("bash: unset: {}: not an array variable\n", array_name));
                exit_code = 1;
                continue;
            }

            // Indexed array: evaluate index
            let index = evaluate_array_index(state, index_expr);

            // Handle negative indices
            if index < 0 {
                let elements = get_array_elements(state, array_name);
                let len = elements.len() as i64;
                if len == 0 {
                    stderr.push_str(&format!("bash: unset: [{}]: bad array subscript\n", index));
                    exit_code = 1;
                    continue;
                }
                let actual_pos = len + index;
                if actual_pos < 0 {
                    stderr.push_str(&format!("bash: unset: [{}]: bad array subscript\n", index));
                    exit_code = 1;
                    continue;
                }
                let actual_index = &elements[actual_pos as usize].0;
                let key = match actual_index {
                    ArrayIndex::Numeric(n) => n.to_string(),
                    ArrayIndex::String(s) => s.clone(),
                };
                state.env.remove(&format!("{}_{}", array_name, key));
                continue;
            }

            // Positive index - just delete directly
            state.env.remove(&format!("{}_{}", array_name, index));
            continue;
        }

        // Regular variable
        // Validate variable name
        if !is_valid_variable_name(arg) {
            stderr.push_str(&format!("bash: unset: `{}': not a valid identifier\n", arg));
            exit_code = 1;
            continue;
        }

        let mut target_name = arg.clone();
        if is_nameref(state, arg) {
            let env_clone = state.env.clone();
            if let Some(resolved) = resolve_nameref(state, &env_clone, arg, None) {
                if resolved != *arg {
                    target_name = resolved;
                }
            }
        }

        // Check if variable is readonly
        if is_readonly(state, &target_name) {
            stderr.push_str(&format!("bash: unset: {}: cannot unset: readonly variable\n", target_name));
            exit_code = 1;
            continue;
        }

        // Delete the variable
        state.env.remove(&target_name);

        // Clear the export attribute
        if let Some(ref mut exported) = state.exported_vars {
            exported.remove(&target_name);
        }

        // If mode is "both", also delete function
        if mode == Mode::Both {
            state.functions.remove(arg);
        }
    }

    result("", &stderr, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_variable_name() {
        assert!(is_valid_variable_name("foo"));
        assert!(is_valid_variable_name("_foo"));
        assert!(is_valid_variable_name("foo123"));
        assert!(is_valid_variable_name("_123"));
        assert!(!is_valid_variable_name("123foo"));
        assert!(!is_valid_variable_name("foo-bar"));
        assert!(!is_valid_variable_name("foo.bar"));
    }

    #[test]
    fn test_evaluate_array_index() {
        let mut state = InterpreterState::default();
        assert_eq!(evaluate_array_index(&state, "5"), 5);
        assert_eq!(evaluate_array_index(&state, "-1"), -1);
        assert_eq!(evaluate_array_index(&state, "invalid"), 0);

        state.env.insert("i".to_string(), "10".to_string());
        assert_eq!(evaluate_array_index(&state, "i"), 10);
    }
}
