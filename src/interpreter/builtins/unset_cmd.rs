//! unset - Remove variables/functions builtin
//!
//! Supports:
//! - unset VAR - remove variable
//! - unset -v VAR - remove variable (explicit)
//! - unset -f FUNC - remove function
//! - unset 'a[i]' - remove array element
//!
//! Bash-specific unset scoping:
//! - local-unset (same scope): value-unset - clears value but keeps local cell
//! - dynamic-unset (different scope): cell-unset - removes local cell, exposes outer value

use regex_lite::Regex;
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::result;
use crate::interpreter::helpers::nameref::{is_nameref, resolve_nameref};
use crate::interpreter::helpers::readonly::is_readonly;
use crate::interpreter::expansion::variable::{is_array, get_array_elements, ArrayIndex};
use crate::interpreter::builtins::variable_assignment::{
    get_local_var_depth, clear_local_var_depth, pop_local_var_stack,
};

/// Check if a name is a valid bash variable name.
fn is_valid_variable_name(name: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    re.is_match(name)
}

/// Perform cell-unset for a local variable (dynamic-unset).
/// This removes the local cell and exposes the outer scope's value.
/// Uses the localVarStack for bash's localvar-nest behavior where multiple
/// nested local declarations can each be unset independently.
/// Returns true if a cell-unset was performed, false otherwise.
fn perform_cell_unset(state: &mut InterpreterState, var_name: &str) -> bool {
    // Check if this variable uses the localVarStack (for nested local declarations)
    let has_stack_entry = state.local_var_stack
        .as_ref()
        .map_or(false, |m| m.contains_key(var_name));

    if has_stack_entry {
        // This variable is managed by the localVarStack
        if let Some((saved_value, scope_index)) = pop_local_var_stack(state, var_name) {
            // Restore the value from the stack
            if let Some(value) = saved_value {
                state.env.insert(var_name.to_string(), value);
            } else {
                state.env.remove(var_name);
            }

            // Check if there are more entries in the stack
            let has_remaining = state.local_var_stack
                .as_ref()
                .map_or(false, |m| m.get(var_name).map_or(false, |s| !s.is_empty()));

            if !has_remaining {
                // No more nested locals - clear the tracking
                clear_local_var_depth(state, var_name);
                // Also clean up the empty stack entry
                if let Some(ref mut stack_map) = state.local_var_stack {
                    stack_map.remove(var_name);
                }
                // Mark this variable as "fully unset local" to prevent tempenv restoration
                if state.fully_unset_locals.is_none() {
                    state.fully_unset_locals = Some(std::collections::HashMap::new());
                }
                state.fully_unset_locals.as_mut().unwrap().insert(var_name.to_string(), scope_index);

                // Bash 5.1 behavior: after cell-unset removes all locals, also remove tempenv
                // binding to reveal the global value (not the tempenv value)
                handle_temp_env_unset(state, var_name);
            } else {
                // Update localVarDepth to point to the now-top entry's scope
                if let Some(ref stack_map) = state.local_var_stack {
                    if let Some(stack) = stack_map.get(var_name) {
                        if let Some(top_entry) = stack.last() {
                            if state.local_var_depth.is_none() {
                                state.local_var_depth = Some(std::collections::HashMap::new());
                            }
                            state.local_var_depth.as_mut().unwrap()
                                .insert(var_name.to_string(), (top_entry.scope_index + 1) as u32);
                        }
                    }
                }
            }
            return true;
        }
        // Stack was empty but variable was stack-managed - just delete and clear tracking
        state.env.remove(var_name);
        clear_local_var_depth(state, var_name);
        if let Some(ref mut stack_map) = state.local_var_stack {
            stack_map.remove(var_name);
        }
        // Mark as fully unset - use the outermost scope (0) since we don't know the original
        if state.fully_unset_locals.is_none() {
            state.fully_unset_locals = Some(std::collections::HashMap::new());
        }
        state.fully_unset_locals.as_mut().unwrap().insert(var_name.to_string(), 0);
        return true;
    }

    // Fall back to the old behavior for variables without stack entries
    // (for backwards compatibility with existing local declarations)
    for i in (0..state.local_scopes.len()).rev() {
        let scope = &state.local_scopes[i];
        if scope.contains_key(var_name) {
            // Found the scope - restore the outer value
            let outer_value = scope.get(var_name).cloned().flatten();
            if let Some(value) = outer_value {
                state.env.insert(var_name.to_string(), value);
            } else {
                state.env.remove(var_name);
            }
            // Remove from this scope so future lookups find the outer value
            state.local_scopes[i].remove(var_name);

            // Check if there's an outer scope that also has this variable
            let mut found_outer_scope = false;
            for j in (0..i).rev() {
                if state.local_scopes[j].contains_key(var_name) {
                    // Found an outer scope with this variable
                    // Scope at index j was created at callDepth j + 1
                    if state.local_var_depth.is_none() {
                        state.local_var_depth = Some(std::collections::HashMap::new());
                    }
                    state.local_var_depth.as_mut().unwrap()
                        .insert(var_name.to_string(), (j + 1) as u32);
                    found_outer_scope = true;
                    break;
                }
            }
            if !found_outer_scope {
                clear_local_var_depth(state, var_name);
            }
            return true;
        }
    }
    false
}

/// Handle unsetting a variable that may have a tempEnvBinding.
/// In bash, when you `unset v` where `v` was set by a prefix assignment (v=tempenv cmd),
/// it reveals the underlying (global) value instead of completely deleting the variable.
/// Returns true if a tempenv binding was found and handled, false otherwise.
fn handle_temp_env_unset(state: &mut InterpreterState, var_name: &str) -> bool {
    let bindings = match state.temp_env_bindings.as_mut() {
        Some(b) if !b.is_empty() => b,
        _ => return false,
    };

    // Search from innermost (most recent) to outermost tempEnvBinding
    for i in (0..bindings.len()).rev() {
        if bindings[i].contains_key(var_name) {
            // Found a tempenv binding for this variable
            // Restore the underlying value (what was saved when the tempenv was created)
            let underlying_value = bindings[i].get(var_name).cloned().flatten();
            if let Some(value) = underlying_value {
                state.env.insert(var_name.to_string(), value);
            } else {
                state.env.remove(var_name);
            }
            // Remove from this binding so future unsets will look at next layer
            bindings[i].remove(var_name);
            return true;
        }
    }
    false
}

/// Check if an index expression is a quoted string (single or double quotes).
/// These are treated as associative array keys, not numeric indices.
fn is_quoted_string_index(index_expr: &str) -> bool {
    (index_expr.starts_with('\'') && index_expr.ends_with('\''))
        || (index_expr.starts_with('"') && index_expr.ends_with('"'))
}

/// Evaluate an array index expression (can be arithmetic).
/// Returns the evaluated numeric index, or None if the expression is a quoted
/// string that should be treated as an associative array key.
fn evaluate_array_index(state: &mut InterpreterState, index_expr: &str) -> Option<i64> {
    use crate::interpreter::types::{InterpreterContext, ExecutionLimits};
    use crate::interpreter::arithmetic::evaluate_arithmetic;
    use crate::parser::parse_arith_expr;

    // If the index is a quoted string, it's meant for associative arrays only
    if is_quoted_string_index(index_expr) {
        return None;
    }

    // Try to parse and evaluate as arithmetic expression
    let limits = ExecutionLimits::default();
    let mut ctx = InterpreterContext::new(state, &limits);

    let (arith_expr, _) = parse_arith_expr(index_expr, 0);
    match evaluate_arithmetic(&mut ctx, &arith_expr, false) {
        Ok(result) => Some(result),
        Err(_) => {
            // If parsing fails, try to parse as simple number
            match index_expr.parse::<i64>() {
                Ok(num) => Some(num),
                Err(_) => Some(0),
            }
        }
    }
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

            // Indexed array: evaluate index as arithmetic expression
            let index = evaluate_array_index(state, index_expr);

            // If index is None, it's a quoted string key - error for indexed arrays
            // Only error if the variable is actually an indexed array
            if index.is_none() && is_indexed_array {
                stderr.push_str(&format!("bash: unset: {}: not a valid identifier\n", index_expr));
                exit_code = 1;
                continue;
            }

            // If variable doesn't exist at all and we have a quoted string key,
            // just silently succeed
            if index.is_none() {
                continue;
            }

            let index = index.unwrap();

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

        // Bash-specific unset scoping: check if this is a dynamic-unset
        let local_depth = get_local_var_depth(state, &target_name);
        let call_depth = state.call_depth;

        if let Some(depth) = local_depth {
            if depth != call_depth {
                // Dynamic-unset: called from a different scope than where local was declared
                // Perform cell-unset to expose outer value
                perform_cell_unset(state, &target_name);
            } else {
                // Local-unset: variable is local and we're in the same scope
                // Check if tempenv was accessed before local declaration
                let temp_env_accessed = state.accessed_temp_env_vars
                    .as_ref()
                    .map_or(false, |s| s.contains(&target_name));
                let temp_env_mutated = state.mutated_temp_env_vars
                    .as_ref()
                    .map_or(false, |s| s.contains(&target_name));
                let has_stack = state.local_var_stack
                    .as_ref()
                    .map_or(false, |m| m.contains_key(&target_name));

                if (temp_env_accessed || temp_env_mutated) && has_stack {
                    // Tempenv was accessed before local declaration - pop from stack to reveal the value
                    if let Some((saved_value, _)) = pop_local_var_stack(state, &target_name) {
                        if let Some(value) = saved_value {
                            state.env.insert(target_name.clone(), value);
                        } else {
                            state.env.remove(&target_name);
                        }
                    } else {
                        state.env.remove(&target_name);
                    }
                } else {
                    // Tempenv not accessed - just value-unset (delete)
                    state.env.remove(&target_name);
                }
            }
        } else if state.fully_unset_locals
            .as_ref()
            .map_or(false, |m| m.contains_key(&target_name))
        {
            // This variable was a local that has been fully unset
            // Don't restore from tempenv, just delete
            state.env.remove(&target_name);
        } else if !handle_temp_env_unset(state, &target_name) {
            // Not a local variable - check for tempenv binding
            // If found, reveal underlying value; otherwise just delete
            state.env.remove(&target_name);
        }

        // Clear the export attribute - when variable is unset, it loses its export status
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
    use std::collections::HashMap;

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
        assert_eq!(evaluate_array_index(&mut state, "5"), Some(5));
        assert_eq!(evaluate_array_index(&mut state, "-1"), Some(-1));
        // Arithmetic expression: 2+3
        assert_eq!(evaluate_array_index(&mut state, "2+3"), Some(5));

        state.env.insert("i".to_string(), "10".to_string());
        assert_eq!(evaluate_array_index(&mut state, "i"), Some(10));
        // Arithmetic with variable: i+5
        assert_eq!(evaluate_array_index(&mut state, "i+5"), Some(15));
    }

    #[test]
    fn test_evaluate_array_index_quoted_string() {
        let mut state = InterpreterState::default();
        // Quoted strings should return None (for associative arrays)
        assert_eq!(evaluate_array_index(&mut state, "'key'"), None);
        assert_eq!(evaluate_array_index(&mut state, "\"key\""), None);
    }

    #[test]
    fn test_handle_temp_env_unset() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "tempenv_value".to_string());

        // Set up tempenv binding
        let mut binding = HashMap::new();
        binding.insert("x".to_string(), Some("global_value".to_string()));
        state.temp_env_bindings = Some(vec![binding]);

        // Unset should reveal the underlying (global) value
        let result = handle_temp_env_unset(&mut state, "x");
        assert!(result);
        assert_eq!(state.env.get("x"), Some(&"global_value".to_string()));
    }

    #[test]
    fn test_handle_temp_env_unset_no_binding() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "value".to_string());

        // No tempenv binding - should return false
        let result = handle_temp_env_unset(&mut state, "x");
        assert!(!result);
        // Value should be unchanged
        assert_eq!(state.env.get("x"), Some(&"value".to_string()));
    }

    #[test]
    fn test_perform_cell_unset_with_local_scopes() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "local_value".to_string());

        // Set up a local scope with saved outer value
        let mut scope = HashMap::new();
        scope.insert("x".to_string(), Some("outer_value".to_string()));
        state.local_scopes.push(scope);

        // Cell-unset should restore the outer value
        let result = perform_cell_unset(&mut state, "x");
        assert!(result);
        assert_eq!(state.env.get("x"), Some(&"outer_value".to_string()));
    }

    #[test]
    fn test_perform_cell_unset_with_local_var_stack() {
        use crate::interpreter::types::LocalVarStackEntry;

        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "current_value".to_string());

        // Set up local var stack
        let entry = LocalVarStackEntry {
            value: Some("saved_value".to_string()),
            scope_index: 0,
        };
        state.local_var_stack = Some(HashMap::new());
        state.local_var_stack.as_mut().unwrap().insert("x".to_string(), vec![entry]);

        // Cell-unset should pop from stack and restore saved value
        let result = perform_cell_unset(&mut state, "x");
        assert!(result);
        assert_eq!(state.env.get("x"), Some(&"saved_value".to_string()));
    }

    #[test]
    fn test_handle_unset_basic() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "value".to_string());

        let result = handle_unset(&mut state, &["x".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(!state.env.contains_key("x"));
    }

    #[test]
    fn test_handle_unset_readonly() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "value".to_string());
        state.readonly_vars = Some(std::collections::HashSet::new());
        state.readonly_vars.as_mut().unwrap().insert("x".to_string());

        let result = handle_unset(&mut state, &["x".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("readonly"));
        // Value should still be there
        assert_eq!(state.env.get("x"), Some(&"value".to_string()));
    }

    #[test]
    fn test_handle_unset_with_tempenv() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "tempenv_value".to_string());

        // Set up tempenv binding
        let mut binding = HashMap::new();
        binding.insert("x".to_string(), Some("global_value".to_string()));
        state.temp_env_bindings = Some(vec![binding]);

        let result = handle_unset(&mut state, &["x".to_string()]);
        assert_eq!(result.exit_code, 0);
        // Should reveal the global value
        assert_eq!(state.env.get("x"), Some(&"global_value".to_string()));
    }
}
