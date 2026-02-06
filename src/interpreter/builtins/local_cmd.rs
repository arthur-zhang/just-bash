//! local - Declare local variables in functions builtin
//!
//! Supports:
//! - local VAR - declare local variable
//! - local VAR=value - declare and assign
//! - local -n VAR - declare nameref
//! - local -a VAR - declare array
//! - local VAR=(a b c) - declare array with values

use regex_lite::Regex;
use crate::interpreter::arithmetic::evaluate_array_index;
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::{result, failure};
use crate::interpreter::helpers::nameref::mark_nameref;
use crate::interpreter::helpers::readonly::is_readonly;
use crate::interpreter::builtins::declare_array_parsing::parse_array_elements;
use crate::interpreter::builtins::variable_assignment::{push_local_var_stack, get_local_var_depth};
use crate::interpreter::builtins::declare_cmd::mark_local_var_depth;

/// Handle the local builtin command
pub fn handle_local(
    state: &mut InterpreterState,
    args: &[String],
) -> ExecResult {
    if state.local_scopes.is_empty() {
        return failure("bash: local: can only be used in a function\n");
    }

    let current_scope_idx = state.local_scopes.len() - 1;
    let mut stderr = String::new();
    let mut exit_code = 0;
    let mut declare_nameref = false;
    let mut declare_array = false;

    // Parse flags
    let mut processed_args: Vec<String> = Vec::new();
    for arg in args {
        if arg == "-n" {
            declare_nameref = true;
        } else if arg == "-a" {
            declare_array = true;
        } else if arg == "-p" {
            // Print mode - ignored for now
        } else if arg.starts_with('-') && !arg.contains('=') {
            // Handle combined flags like -na
            for flag in arg[1..].chars() {
                match flag {
                    'n' => declare_nameref = true,
                    'a' => declare_array = true,
                    'p' => {} // Print mode - ignored
                    _ => {} // Other flags ignored
                }
            }
        } else {
            processed_args.push(arg.clone());
        }
    }

    // Handle local without args: print local variables
    if processed_args.is_empty() {
        let mut stdout = String::new();
        let current_scope = &state.local_scopes[current_scope_idx];
        let mut local_names: Vec<&String> = current_scope.keys()
            .filter(|key| !key.contains("__") && !key.ends_with("_0"))
            .collect();
        local_names.sort();

        for name in local_names {
            if let Some(value) = state.env.get(name) {
                stdout.push_str(&format!("{}={}\n", name, value));
            }
        }
        return result(&stdout, "", 0);
    }

    let valid_name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    let array_assign_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)=\((.*)\)$").unwrap();
    let array_append_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\+=\((.*)\)$").unwrap();
    let append_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\+=(.*)$").unwrap();
    let index_assign_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[([^\]]+)\]=(.*)$").unwrap();

    for arg in &processed_args {
        let name: String;
        let value: Option<String>;

        // Check for array assignment: name=(...)
        if let Some(captures) = array_assign_re.captures(arg) {
            name = captures.get(1).unwrap().as_str().to_string();
            let content = captures.get(2).unwrap().as_str();

            // Validate variable name
            if !valid_name_re.is_match(&name) {
                stderr.push_str(&format!("bash: local: `{}': not a valid identifier\n", arg));
                exit_code = 1;
                continue;
            }

            // Check if variable is readonly
            if is_readonly(state, &name) {
                stderr.push_str(&format!("bash: {}: readonly variable\n", name));
                exit_code = 1;
                continue;
            }

            // Save previous value for scope restoration
            save_to_scope(state, current_scope_idx, &name);

            // Clear existing array elements
            clear_array_elements(state, &name);

            // Parse and set array elements
            let elements = parse_array_elements(content);
            for (i, elem) in elements.iter().enumerate() {
                state.env.insert(format!("{}_{}", name, i), elem.clone());
            }

            // Track local variable depth for bash-specific unset scoping
            mark_local_var_depth(state, &name);

            // Mark as nameref if -n flag was used
            if declare_nameref {
                mark_nameref(state, &name);
            }
            continue;
        }

        // Check for array append syntax: local NAME+=(...)
        if let Some(captures) = array_append_re.captures(arg) {
            name = captures.get(1).unwrap().as_str().to_string();
            let content = captures.get(2).unwrap().as_str();

            // Check if variable is readonly
            if is_readonly(state, &name) {
                stderr.push_str(&format!("bash: {}: readonly variable\n", name));
                exit_code = 1;
                continue;
            }

            // Save previous value for scope restoration
            save_to_scope(state, current_scope_idx, &name);

            // Parse new elements
            let new_elements = parse_array_elements(content);

            // Get current highest index
            let start_index = get_array_max_index(state, &name).map(|i| i + 1).unwrap_or(0);

            // Append new elements
            for (i, elem) in new_elements.iter().enumerate() {
                state.env.insert(format!("{}_{}", name, start_index + i), elem.clone());
            }

            // Track local variable depth for bash-specific unset scoping
            mark_local_var_depth(state, &name);

            // Mark as nameref if -n flag was used
            if declare_nameref {
                mark_nameref(state, &name);
            }
            continue;
        }

        // Check for += append syntax (scalar append)
        if let Some(captures) = append_re.captures(arg) {
            name = captures.get(1).unwrap().as_str().to_string();
            let append_value = captures.get(2).unwrap().as_str();

            // Check if variable is readonly
            if is_readonly(state, &name) {
                stderr.push_str(&format!("bash: {}: readonly variable\n", name));
                exit_code = 1;
                continue;
            }

            // Save previous value for scope restoration
            save_to_scope(state, current_scope_idx, &name);

            // Append to existing value
            let existing = state.env.get(&name).cloned().unwrap_or_default();
            state.env.insert(name.clone(), format!("{}{}", existing, append_value));

            // Track local variable depth for bash-specific unset scoping
            mark_local_var_depth(state, &name);

            // Mark as nameref if -n flag was used
            if declare_nameref {
                mark_nameref(state, &name);
            }
            continue;
        }

        // Check for array index assignment: name[index]=value
        if let Some(captures) = index_assign_re.captures(arg) {
            name = captures.get(1).unwrap().as_str().to_string();
            let index_expr = captures.get(2).unwrap().as_str();
            let index_value = captures.get(3).unwrap().as_str();

            // Check if variable is readonly
            if is_readonly(state, &name) {
                stderr.push_str(&format!("bash: {}: readonly variable\n", name));
                exit_code = 1;
                continue;
            }

            // Save previous value for scope restoration
            save_to_scope(state, current_scope_idx, &name);

            // Evaluate the index (can be arithmetic expression)
            let index: i64 = evaluate_array_index(state, index_expr);

            // Set the array element
            state.env.insert(format!("{}_{}", name, index), index_value.to_string());

            // Track local variable depth for bash-specific unset scoping
            mark_local_var_depth(state, &name);

            // Mark as nameref if -n flag was used
            if declare_nameref {
                mark_nameref(state, &name);
            }
            continue;
        }

        // Regular assignment or declaration
        if arg.contains('=') {
            let eq_idx = arg.find('=').unwrap();
            name = arg[..eq_idx].to_string();
            value = Some(arg[eq_idx + 1..].to_string());
        } else {
            name = arg.clone();
            value = None;
        }

        // Validate variable name
        if !valid_name_re.is_match(&name) {
            stderr.push_str(&format!("bash: local: `{}': not a valid identifier\n", arg));
            exit_code = 1;
            continue;
        }

        // Check if variable was already local BEFORE we potentially add it to scope
        let was_already_local = state.local_scopes[current_scope_idx].contains_key(&name);

        // For bash's localvar-nest behavior: always push the current value to the stack
        // when there's an assignment. This allows nested local declarations to each have
        // their own cell that can be unset independently.
        if value.is_some() {
            let saved_value = get_value_for_local_var_stack(state, &name);
            push_local_var_stack(state, &name, saved_value);
        }

        // Save previous value for scope restoration (considering tempenv)
        if !was_already_local {
            let saved_value = get_underlying_value(state, &name);
            state.local_scopes[current_scope_idx].insert(name.clone(), saved_value);

            // Also save array elements if -a flag is used
            if declare_array {
                let prefix = format!("{}_", name);
                let keys_to_save: Vec<String> = state.env.keys()
                    .filter(|k| k.starts_with(&prefix) && !k.contains("__"))
                    .cloned()
                    .collect();
                for key in keys_to_save {
                    if !state.local_scopes[current_scope_idx].contains_key(&key) {
                        let saved = state.env.get(&key).cloned();
                        state.local_scopes[current_scope_idx].insert(key, saved);
                    }
                }
            }
        }

        // If -a flag is used, create an empty local array
        if declare_array && value.is_none() {
            clear_array_elements(state, &name);
        } else if let Some(v) = value {
            // Check if variable is readonly
            if is_readonly(state, &name) {
                stderr.push_str(&format!("bash: {}: readonly variable\n", name));
                exit_code = 1;
                continue;
            }

            // For namerefs, validate the target
            if declare_nameref && !v.is_empty() {
                let valid_target_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\[.+\])?$").unwrap();
                if !valid_target_re.is_match(&v) {
                    stderr.push_str(&format!("bash: local: `{}': invalid variable name for name reference\n", v));
                    exit_code = 1;
                    continue;
                }
            }
            state.env.insert(name.clone(), v);

            // If allexport is enabled, auto-export the variable
            if state.options.allexport {
                if state.exported_vars.is_none() {
                    state.exported_vars = Some(std::collections::HashSet::new());
                }
                if let Some(ref mut exported) = state.exported_vars {
                    exported.insert(name.clone());
                }
            }
        } else {
            // `local v` without assignment: bash behavior is:
            // - If the variable is already local in current scope, keep its value
            // - If there's a tempenv binding, inherit that value
            // - Otherwise, the variable is unset (not inherited from global)
            let has_temp_env_binding = state.temp_env_bindings
                .as_ref()
                .map(|bindings| bindings.iter().any(|b| b.contains_key(&name)))
                .unwrap_or(false);
            if !was_already_local && !has_temp_env_binding {
                // Not already local, no tempenv binding - make the variable unset
                state.env.remove(&name);
            }
            // If already local or has tempenv binding, keep the current value
        }

        // Track local variable depth for bash-specific unset scoping
        mark_local_var_depth(state, &name);

        // Mark as nameref if -n flag was used
        if declare_nameref {
            mark_nameref(state, &name);
        }
    }

    result("", &stderr, exit_code)
}

/// Save a variable to the current scope for later restoration.
/// Handles tempenv bindings: when there's a tempenv binding, save the underlying
/// (global) value, not the tempenv value, so dynamic-unset reveals the correct value.
fn save_to_scope(state: &mut InterpreterState, scope_idx: usize, name: &str) {
    if !state.local_scopes[scope_idx].contains_key(name) {
        // Get the value to save, considering tempenv bindings
        let saved_value = get_underlying_value(state, name);
        state.local_scopes[scope_idx].insert(name.to_string(), saved_value);

        // Also save array elements
        let prefix = format!("{}_", name);
        let keys_to_save: Vec<String> = state.env.keys()
            .filter(|k| k.starts_with(&prefix) && !k.contains("__"))
            .cloned()
            .collect();
        for key in keys_to_save {
            if !state.local_scopes[scope_idx].contains_key(&key) {
                let saved = state.env.get(&key).cloned();
                state.local_scopes[scope_idx].insert(key, saved);
            }
        }
    }
}

/// Get the underlying value of a variable, looking through tempenv bindings.
/// If there's a tempenv binding, return the value from before the tempenv was applied.
fn get_underlying_value(state: &InterpreterState, name: &str) -> Option<String> {
    // Check if there's a tempenv binding for this variable
    if let Some(ref temp_env_bindings) = state.temp_env_bindings {
        // Search from the most recent binding backwards
        for bindings in temp_env_bindings.iter().rev() {
            if bindings.contains_key(name) {
                // Found a tempenv binding - return the saved underlying value
                return bindings.get(name).cloned().flatten();
            }
        }
    }
    // No tempenv binding, return the current env value
    state.env.get(name).cloned()
}

/// Get the value to save for localvar-nest behavior.
/// This considers whether the tempenv was accessed or mutated.
fn get_value_for_local_var_stack(state: &InterpreterState, name: &str) -> Option<String> {
    // Check if there's a tempenv binding
    if let Some(ref temp_env_bindings) = state.temp_env_bindings {
        let temp_env_accessed = state.accessed_temp_env_vars
            .as_ref()
            .map(|s| s.contains(name))
            .unwrap_or(false);
        let temp_env_mutated = state.mutated_temp_env_vars
            .as_ref()
            .map(|s| s.contains(name))
            .unwrap_or(false);

        if !temp_env_accessed && !temp_env_mutated {
            // Tempenv was NOT accessed - save the underlying value for dynamic-unset
            for bindings in temp_env_bindings.iter().rev() {
                if bindings.contains_key(name) {
                    return bindings.get(name).cloned().flatten();
                }
            }
        }
        // If accessed or mutated, fall through to return current env value
    }
    state.env.get(name).cloned()
}

/// Clear existing array elements for a variable
fn clear_array_elements(state: &mut InterpreterState, name: &str) {
    let prefix = format!("{}_", name);
    let keys_to_remove: Vec<String> = state.env.keys()
        .filter(|k| k.starts_with(&prefix) && !k.contains("__"))
        .cloned()
        .collect();
    for key in keys_to_remove {
        state.env.remove(&key);
    }
}

/// Get the maximum index of an array
fn get_array_max_index(state: &InterpreterState, name: &str) -> Option<usize> {
    let prefix = format!("{}_", name);
    let mut max_index: Option<usize> = None;

    for key in state.env.keys() {
        if key.starts_with(&prefix) && !key.contains("__") {
            let suffix = &key[prefix.len()..];
            if let Ok(index) = suffix.parse::<usize>() {
                max_index = Some(max_index.map(|m| m.max(index)).unwrap_or(index));
            }
        }
    }

    max_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_get_array_max_index() {
        let mut state = InterpreterState::default();
        assert_eq!(get_array_max_index(&state, "arr"), None);

        state.env.insert("arr_0".to_string(), "a".to_string());
        state.env.insert("arr_5".to_string(), "b".to_string());
        state.env.insert("arr_3".to_string(), "c".to_string());
        assert_eq!(get_array_max_index(&state, "arr"), Some(5));
    }

    #[test]
    fn test_handle_local_outside_function() {
        let mut state = InterpreterState::default();
        // No local scopes - should fail
        let result = handle_local(&mut state, &["x=1".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("can only be used in a function"));
    }

    #[test]
    fn test_handle_local_simple_assignment() {
        let mut state = InterpreterState::default();
        // Add a local scope (simulating being inside a function)
        state.local_scopes.push(HashMap::new());

        let result = handle_local(&mut state, &["x=hello".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("x"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_handle_local_saves_previous_value() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "global".to_string());
        state.local_scopes.push(HashMap::new());

        let result = handle_local(&mut state, &["x=local".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("x"), Some(&"local".to_string()));
        // The previous value should be saved in the scope
        assert_eq!(state.local_scopes[0].get("x"), Some(&Some("global".to_string())));
    }

    #[test]
    fn test_handle_local_without_value_unsets() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "global".to_string());
        state.local_scopes.push(HashMap::new());

        // local x without assignment should unset the variable
        let result = handle_local(&mut state, &["x".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("x"), None);
    }

    #[test]
    fn test_get_underlying_value_no_tempenv() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "value".to_string());
        assert_eq!(get_underlying_value(&state, "x"), Some("value".to_string()));
    }

    #[test]
    fn test_get_underlying_value_with_tempenv() {
        let mut state = InterpreterState::default();
        state.env.insert("x".to_string(), "tempenv_value".to_string());

        // Set up tempenv binding
        let mut binding = HashMap::new();
        binding.insert("x".to_string(), Some("global_value".to_string()));
        state.temp_env_bindings = Some(vec![binding]);

        // Should return the underlying (global) value, not the tempenv value
        assert_eq!(get_underlying_value(&state, "x"), Some("global_value".to_string()));
    }
}
