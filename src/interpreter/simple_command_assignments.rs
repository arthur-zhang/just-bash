//! Simple Command Assignment Handling
//!
//! Handles variable assignments in simple commands:
//! - Array assignments: VAR=(a b c)
//! - Subscript assignments: VAR[idx]=value
//! - Scalar assignments with nameref resolution

use std::collections::HashMap;
use crate::ast::types::{SimpleCommandNode, WordNode};
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::nameref::{is_nameref, resolve_nameref, resolve_nameref_for_assignment, get_nameref_target, NamerefAssignmentResult};
use crate::interpreter::helpers::readonly::is_readonly;

/// Result of processing assignments in a simple command
#[derive(Debug, Clone)]
pub struct AssignmentResult {
    /// Whether to continue to the next statement (skip command execution)
    pub continue_to_next: bool,
    /// Accumulated xtrace output for assignments
    pub xtrace_output: String,
    /// Temporary assignments for prefix bindings (FOO=bar cmd)
    pub temp_assignments: HashMap<String, Option<String>>,
    /// Error result if assignment failed
    pub error: Option<ExecResult>,
}

impl Default for AssignmentResult {
    fn default() -> Self {
        Self {
            continue_to_next: false,
            xtrace_output: String::new(),
            temp_assignments: HashMap::new(),
            error: None,
        }
    }
}

/// Result of processing a single assignment
#[derive(Debug, Clone)]
pub struct SingleAssignmentResult {
    pub continue_to_next: bool,
    pub xtrace_output: String,
    pub error: Option<ExecResult>,
}

impl Default for SingleAssignmentResult {
    fn default() -> Self {
        Self {
            continue_to_next: false,
            xtrace_output: String::new(),
            error: None,
        }
    }
}

/// Process all assignments in a simple command.
/// Returns assignment results including temp bindings and any errors.
pub fn process_assignments(
    state: &mut InterpreterState,
    node: &SimpleCommandNode,
    expand_word_fn: impl Fn(&mut InterpreterState, &WordNode) -> String,
) -> AssignmentResult {
    let mut result = AssignmentResult::default();

    for assignment in &node.assignments {
        let name = &assignment.name;

        // Handle array assignment: VAR=(a b c) or VAR+=(a b c)
        if let Some(ref array) = assignment.array {
            let array_result = process_array_assignment(
                state,
                node,
                name,
                array,
                assignment.append,
                &mut result.temp_assignments,
            );
            if let Some(error) = array_result.error {
                result.error = Some(error);
                return result;
            }
            result.xtrace_output.push_str(&array_result.xtrace_output);
            if array_result.continue_to_next {
                continue;
            }
        }

        let value = if let Some(ref value_word) = assignment.value {
            expand_word_fn(state, value_word)
        } else {
            String::new()
        };

        // Check for empty subscript assignment: a[]=value is invalid
        let empty_subscript_re = regex_lite::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[\]$").unwrap();
        if empty_subscript_re.is_match(name) {
            result.error = Some(ExecResult::new(
                String::new(),
                format!("bash: {}: bad array subscript\n", name),
                1,
            ));
            return result;
        }

        // Check for array subscript assignment: a[subscript]=value
        let subscript_re = regex_lite::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();
        if let Some(captures) = subscript_re.captures(name) {
            let array_name = captures.get(1).unwrap().as_str();
            let subscript_expr = captures.get(2).unwrap().as_str();
            let subscript_result = process_subscript_assignment(
                state,
                node,
                array_name,
                subscript_expr,
                &value,
                assignment.append,
                &mut result.temp_assignments,
            );
            if let Some(error) = subscript_result.error {
                result.error = Some(error);
                return result;
            }
            if subscript_result.continue_to_next {
                continue;
            }
        }

        // Handle scalar assignment
        let scalar_result = process_scalar_assignment(
            state,
            node,
            name,
            &value,
            assignment.append,
            &mut result.temp_assignments,
        );
        if let Some(error) = scalar_result.error {
            result.error = Some(error);
            return result;
        }
        result.xtrace_output.push_str(&scalar_result.xtrace_output);
    }

    result
}

/// Process an array assignment: VAR=(a b c) or VAR+=(a b c)
fn process_array_assignment(
    state: &mut InterpreterState,
    node: &SimpleCommandNode,
    name: &str,
    array: &[WordNode],
    append: bool,
    temp_assignments: &mut HashMap<String, Option<String>>,
) -> SingleAssignmentResult {
    let mut result = SingleAssignmentResult::default();

    // Check if trying to assign array to subscripted element: a[0]=(1 2) is invalid
    if regex_lite::Regex::new(r"\[.+\]$").unwrap().is_match(name) {
        result.error = Some(ExecResult::new(
            String::new(),
            format!("bash: {}: cannot assign list to array member\n", name),
            1,
        ));
        return result;
    }

    // Check if name is a nameref
    if is_nameref(state, name) {
        let target = get_nameref_target(state, &state.env.clone(), name);
        if target.is_none() || target.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            result.error = Some(ExecResult::new(String::new(), String::new(), 1));
            return result;
        }
        if let Some(resolved) = resolve_nameref(state, &state.env.clone(), name, None) {
            let at_pattern = regex_lite::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*\[@\]$").unwrap();
            if at_pattern.is_match(&resolved) {
                result.error = Some(ExecResult::new(
                    String::new(),
                    format!("bash: {}: cannot assign list to array member\n", name),
                    1,
                ));
                return result;
            }
        }
    }

    // Check if array variable is readonly
    if is_readonly(state, name) {
        if node.name.is_some() {
            result.xtrace_output.push_str(&format!("bash: {}: readonly variable\n", name));
            result.continue_to_next = true;
            return result;
        }
        result.error = Some(ExecResult::new(
            String::new(),
            format!("bash: {}: readonly variable\n", name),
            1,
        ));
        return result;
    }

    // Clear existing array elements if not appending
    if !append {
        clear_array_elements(state, name);
    }

    // Process array elements
    let start_index = if append {
        get_array_max_index(state, name).map(|i| i + 1).unwrap_or(0)
    } else {
        0
    };

    for (i, _element) in array.iter().enumerate() {
        // In a full implementation, we would expand each element
        // For now, just set placeholder values
        let env_key = format!("{}_{}", name, start_index + i);
        state.env.insert(env_key, String::new());
    }

    // For prefix assignments with a command, bash stringifies the array syntax
    if node.name.is_some() {
        temp_assignments.insert(name.to_string(), state.env.get(name).cloned());
        // Would stringify array here
    }

    result.continue_to_next = true;
    result
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
    state.env.remove(name);
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

/// Process a subscript assignment: VAR[idx]=value
fn process_subscript_assignment(
    state: &mut InterpreterState,
    node: &SimpleCommandNode,
    array_name: &str,
    subscript_expr: &str,
    value: &str,
    append: bool,
    temp_assignments: &mut HashMap<String, Option<String>>,
) -> SingleAssignmentResult {
    let mut result = SingleAssignmentResult::default();
    let mut resolved_array_name = array_name.to_string();

    // Check if arrayName is a nameref
    if is_nameref(state, array_name) {
        if let Some(resolved) = resolve_nameref(state, &state.env.clone(), array_name, None) {
            if resolved != array_name {
                if resolved.contains('[') {
                    result.error = Some(ExecResult::new(
                        String::new(),
                        format!("bash: `{}': not a valid identifier\n", resolved),
                        1,
                    ));
                    return result;
                }
                resolved_array_name = resolved;
            }
        }
    }

    // Check if array variable is readonly
    if is_readonly(state, &resolved_array_name) {
        if node.name.is_some() {
            result.continue_to_next = true;
            return result;
        }
        result.error = Some(ExecResult::new(
            String::new(),
            format!("bash: {}: readonly variable\n", resolved_array_name),
            1,
        ));
        return result;
    }

    // Compute the index
    let index = compute_array_index(state, subscript_expr);
    let env_key = format!("{}_{}", resolved_array_name, index);

    let final_value = if append {
        let existing = state.env.get(&env_key).cloned().unwrap_or_default();
        format!("{}{}", existing, value)
    } else {
        value.to_string()
    };

    if node.name.is_some() {
        temp_assignments.insert(env_key.clone(), state.env.get(&env_key).cloned());
        state.env.insert(env_key, final_value);
    } else {
        state.env.insert(env_key, final_value);
    }

    result.continue_to_next = true;
    result
}

/// Compute the index for an array subscript
fn compute_array_index(state: &InterpreterState, subscript_expr: &str) -> i64 {
    // Try to parse as integer
    if let Ok(index) = subscript_expr.parse::<i64>() {
        return index;
    }

    // Try to look up as variable
    if let Some(var_value) = state.env.get(subscript_expr) {
        if let Ok(index) = var_value.parse::<i64>() {
            return index;
        }
    }

    // Default to 0
    0
}

/// Process a scalar assignment
fn process_scalar_assignment(
    state: &mut InterpreterState,
    node: &SimpleCommandNode,
    name: &str,
    value: &str,
    append: bool,
    temp_assignments: &mut HashMap<String, Option<String>>,
) -> SingleAssignmentResult {
    let mut result = SingleAssignmentResult::default();

    // Resolve nameref
    let mut target_name = name.to_string();

    if is_nameref(state, name) {
        let env_clone = state.env.clone();
        match resolve_nameref_for_assignment(state, &env_clone, name, Some(value), None) {
            NamerefAssignmentResult::Target(resolved) => {
                target_name = resolved;
            }
            NamerefAssignmentResult::Skip => {
                result.continue_to_next = true;
                return result;
            }
            NamerefAssignmentResult::Circular => {
                result.error = Some(ExecResult::new(
                    String::new(),
                    format!("bash: {}: circular name reference\n", name),
                    1,
                ));
                return result;
            }
        }
    }

    // Check if variable is readonly
    if is_readonly(state, &target_name) {
        if node.name.is_some() {
            result.xtrace_output.push_str(&format!("bash: {}: readonly variable\n", target_name));
            result.continue_to_next = true;
            return result;
        }
        result.error = Some(ExecResult::new(
            String::new(),
            format!("bash: {}: readonly variable\n", target_name),
            1,
        ));
        return result;
    }

    // Handle append mode
    let final_value = if append {
        let existing = state.env.get(&target_name).cloned().unwrap_or_default();
        format!("{}{}", existing, value)
    } else {
        value.to_string()
    };

    // Compute actual env key (handle arrays)
    let actual_env_key = if is_array(state, &target_name) {
        format!("{}_0", target_name)
    } else {
        target_name.clone()
    };

    if node.name.is_some() {
        temp_assignments.insert(actual_env_key.clone(), state.env.get(&actual_env_key).cloned());
        state.env.insert(actual_env_key, final_value);
    } else {
        state.env.insert(actual_env_key, final_value);

        // Handle allexport option
        if state.options.allexport {
            if state.exported_vars.is_none() {
                state.exported_vars = Some(std::collections::HashSet::new());
            }
            if let Some(ref mut exported) = state.exported_vars {
                exported.insert(target_name);
            }
        }
    }

    result
}

/// Check if a variable is an array
fn is_array(state: &InterpreterState, name: &str) -> bool {
    let prefix = format!("{}_", name);
    state.env.keys().any(|k| k.starts_with(&prefix) && !k.contains("__"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_array() {
        let mut state = InterpreterState::default();
        assert!(!is_array(&state, "foo"));

        state.env.insert("foo_0".to_string(), "a".to_string());
        assert!(is_array(&state, "foo"));
    }

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
    fn test_compute_array_index() {
        let mut state = InterpreterState::default();
        assert_eq!(compute_array_index(&state, "5"), 5);
        assert_eq!(compute_array_index(&state, "-1"), -1);
        assert_eq!(compute_array_index(&state, "invalid"), 0);

        state.env.insert("i".to_string(), "10".to_string());
        assert_eq!(compute_array_index(&state, "i"), 10);
    }

    #[test]
    fn test_clear_array_elements() {
        let mut state = InterpreterState::default();
        state.env.insert("arr_0".to_string(), "a".to_string());
        state.env.insert("arr_1".to_string(), "b".to_string());
        state.env.insert("arr__length".to_string(), "2".to_string());
        state.env.insert("other".to_string(), "x".to_string());

        clear_array_elements(&mut state, "arr");

        assert!(!state.env.contains_key("arr_0"));
        assert!(!state.env.contains_key("arr_1"));
        assert!(state.env.contains_key("arr__length")); // __length is preserved
        assert!(state.env.contains_key("other"));
    }
}
