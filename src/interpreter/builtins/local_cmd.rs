//! local - Declare local variables in functions builtin
//!
//! Supports:
//! - local VAR - declare local variable
//! - local VAR=value - declare and assign
//! - local -n VAR - declare nameref
//! - local -a VAR - declare array
//! - local VAR=(a b c) - declare array with values

use regex_lite::Regex;
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::{result, failure};
use crate::interpreter::helpers::nameref::mark_nameref;
use crate::interpreter::helpers::readonly::is_readonly;
use crate::interpreter::builtins::declare_array_parsing::parse_array_elements;

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

            // Evaluate the index
            let index: i64 = index_expr.parse().unwrap_or(0);

            // Set the array element
            state.env.insert(format!("{}_{}", name, index), index_value.to_string());

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

        // Save previous value for scope restoration
        let was_already_local = state.local_scopes[current_scope_idx].contains_key(&name);
        if !was_already_local {
            let saved_value = state.env.get(&name).cloned();
            state.local_scopes[current_scope_idx].insert(name.clone(), saved_value);
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
        } else if !was_already_local {
            // `local v` without assignment: make the variable unset
            state.env.remove(&name);
        }

        // Mark as nameref if -n flag was used
        if declare_nameref {
            mark_nameref(state, &name);
        }
    }

    result("", &stderr, exit_code)
}

/// Save a variable to the current scope for later restoration
fn save_to_scope(state: &mut InterpreterState, scope_idx: usize, name: &str) {
    if !state.local_scopes[scope_idx].contains_key(name) {
        let saved_value = state.env.get(name).cloned();
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

    #[test]
    fn test_get_array_max_index() {
        let mut state = InterpreterState::default();
        assert_eq!(get_array_max_index(&state, "arr"), None);

        state.env.insert("arr_0".to_string(), "a".to_string());
        state.env.insert("arr_5".to_string(), "b".to_string());
        state.env.insert("arr_3".to_string(), "c".to_string());
        assert_eq!(get_array_max_index(&state, "arr"), Some(5));
    }
}
