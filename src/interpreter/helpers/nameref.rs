//! Nameref (declare -n) support
//!
//! Namerefs are variables that reference other variables by name.
//! When a nameref is accessed, it transparently dereferences to the target variable.

use std::collections::{HashMap, HashSet};
use regex_lite::Regex;
use crate::interpreter::types::InterpreterState;

/// Check if a variable is a nameref.
pub fn is_nameref(state: &InterpreterState, name: &str) -> bool {
    state.namerefs.as_ref().map_or(false, |refs| refs.contains(name))
}

/// Mark a variable as a nameref.
pub fn mark_nameref(state: &mut InterpreterState, name: &str) {
    if state.namerefs.is_none() {
        state.namerefs = Some(HashSet::new());
    }
    state.namerefs.as_mut().unwrap().insert(name.to_string());
}

/// Remove the nameref attribute from a variable.
pub fn unmark_nameref(state: &mut InterpreterState, name: &str) {
    if let Some(ref mut refs) = state.namerefs {
        refs.remove(name);
    }
    if let Some(ref mut bound) = state.bound_namerefs {
        bound.remove(name);
    }
    if let Some(ref mut invalid) = state.invalid_namerefs {
        invalid.remove(name);
    }
}

/// Mark a nameref as having an "invalid" target at creation time.
/// Invalid namerefs always read/write their value directly, never resolving.
pub fn mark_nameref_invalid(state: &mut InterpreterState, name: &str) {
    if state.invalid_namerefs.is_none() {
        state.invalid_namerefs = Some(HashSet::new());
    }
    state.invalid_namerefs.as_mut().unwrap().insert(name.to_string());
}

/// Check if a nameref was created with an invalid target.
fn is_nameref_invalid(state: &InterpreterState, name: &str) -> bool {
    state.invalid_namerefs.as_ref().map_or(false, |refs| refs.contains(name))
}

/// Mark a nameref as "bound" - meaning its target existed at creation time.
/// This is kept for tracking purposes but is currently not used in resolution.
pub fn mark_nameref_bound(state: &mut InterpreterState, name: &str) {
    if state.bound_namerefs.is_none() {
        state.bound_namerefs = Some(HashSet::new());
    }
    state.bound_namerefs.as_mut().unwrap().insert(name.to_string());
}

/// Check if a name refers to a valid, existing variable or array element.
/// Used to determine if a nameref target is "real" or just a stored value.
pub fn target_exists(state: &InterpreterState, env: &HashMap<String, String>, target: &str) -> bool {
    // Check for array subscript
    let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();
    if let Some(caps) = array_re.captures(target) {
        let array_name = &caps[1];
        // Check if array exists (has any elements or is declared as assoc)
        let prefix = format!("{}_", array_name);
        let has_elements = env.keys().any(|k| k.starts_with(&prefix) && !k.contains("__"));
        let is_assoc = state.associative_arrays.as_ref().map_or(false, |a| a.contains(array_name));
        return has_elements || is_assoc;
    }

    // Check if it's an array (stored as target_0, target_1, etc.)
    let prefix = format!("{}_", target);
    let has_array_elements = env.keys().any(|k| k.starts_with(&prefix) && !k.contains("__"));
    if has_array_elements {
        return true;
    }

    // Check if scalar variable exists
    env.contains_key(target)
}

/// Resolve a nameref chain to the final variable name.
/// Returns the original name if it's not a nameref.
/// Returns None if circular reference detected.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `env` - The environment variables
/// * `name` - The variable name to resolve
/// * `max_depth` - Maximum chain depth to prevent infinite loops (default 100)
pub fn resolve_nameref(
    state: &InterpreterState,
    env: &HashMap<String, String>,
    name: &str,
    max_depth: Option<usize>,
) -> Option<String> {
    let max_depth = max_depth.unwrap_or(100);

    // If not a nameref, return as-is
    if !is_nameref(state, name) {
        return Some(name.to_string());
    }

    // If the nameref was created with an invalid target, it should never resolve.
    // It acts as a regular variable, returning its value directly.
    if is_nameref_invalid(state, name) {
        return Some(name.to_string());
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut current = name.to_string();
    let mut remaining_depth = max_depth;

    let valid_target_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\[.+\])?$").unwrap();

    while remaining_depth > 0 {
        remaining_depth -= 1;

        // Detect circular reference
        if seen.contains(&current) {
            return None;
        }
        seen.insert(current.clone());

        // If not a nameref, we've reached the target
        if !is_nameref(state, &current) {
            return Some(current);
        }

        // Get the target name from the variable's value
        let target = env.get(&current);
        let target_str = target.map(|s| s.as_str()).unwrap_or("");
        if target_str.is_empty() {
            // Empty or unset nameref - return the nameref itself
            return Some(current);
        }

        // Validate target is a valid variable name (not special chars like #, @, *, etc.)
        // Allow array subscripts like arr[0] or arr[@]
        // Note: Numeric-only targets like '1' are NOT valid - bash doesn't resolve namerefs
        // to positional parameters. The nameref keeps its literal value.
        if !valid_target_re.is_match(target_str) {
            // Invalid nameref target - return the nameref itself (bash behavior)
            return Some(current);
        }

        // Always resolve to the target for reading
        // (The target may not exist, which will result in empty string on read)
        current = target_str.to_string();
    }

    // Max depth exceeded - likely circular reference
    None
}

/// Get the target name of a nameref (what it points to).
/// Returns the variable's value if it's a nameref, None otherwise.
pub fn get_nameref_target(
    state: &InterpreterState,
    env: &HashMap<String, String>,
    name: &str,
) -> Option<String> {
    if !is_nameref(state, name) {
        return None;
    }
    env.get(name).cloned()
}

/// Result of resolving a nameref for assignment.
#[derive(Debug, Clone, PartialEq)]
pub enum NamerefAssignmentResult {
    /// The resolved target name
    Target(String),
    /// Skip the assignment (no-op) - nameref is empty and value is not an existing variable
    Skip,
    /// Circular reference detected
    Circular,
}

/// Resolve a nameref for assignment purposes.
/// Unlike resolve_nameref, this will resolve to the target variable name
/// even if the target doesn't exist yet (allowing creation).
///
/// # Arguments
/// * `state` - The interpreter state
/// * `env` - The environment variables
/// * `name` - The variable name to resolve
/// * `value_being_assigned` - The value being assigned (needed for empty nameref handling)
/// * `max_depth` - Maximum chain depth to prevent infinite loops
pub fn resolve_nameref_for_assignment(
    state: &InterpreterState,
    env: &HashMap<String, String>,
    name: &str,
    value_being_assigned: Option<&str>,
    max_depth: Option<usize>,
) -> NamerefAssignmentResult {
    let max_depth = max_depth.unwrap_or(100);

    // If not a nameref, return as-is
    if !is_nameref(state, name) {
        return NamerefAssignmentResult::Target(name.to_string());
    }

    // If the nameref was created with an invalid target, it should never resolve.
    // It acts as a regular variable, so assignment goes directly to it.
    if is_nameref_invalid(state, name) {
        return NamerefAssignmentResult::Target(name.to_string());
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut current = name.to_string();
    let mut remaining_depth = max_depth;

    let valid_name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    let valid_target_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\[.+\])?$").unwrap();

    while remaining_depth > 0 {
        remaining_depth -= 1;

        // Detect circular reference
        if seen.contains(&current) {
            return NamerefAssignmentResult::Circular;
        }
        seen.insert(current.clone());

        // If not a nameref, we've reached the target
        if !is_nameref(state, &current) {
            return NamerefAssignmentResult::Target(current);
        }

        // Get the target name from the variable's value
        let target = env.get(&current);
        let target_str = target.map(|s| s.as_str()).unwrap_or("");
        if target_str.is_empty() {
            // Empty or unset nameref - special handling based on value being assigned
            // If the value is a valid variable name AND that variable exists, set it as target
            // Otherwise, the assignment is a no-op
            if let Some(value) = value_being_assigned {
                let is_valid_name = valid_name_re.is_match(value);
                if is_valid_name && target_exists(state, env, value) {
                    // Value is an existing variable - set it as the target
                    return NamerefAssignmentResult::Target(current);
                }
                // Value is not an existing variable - skip assignment (no-op)
                return NamerefAssignmentResult::Skip;
            }
            // No value provided - return the nameref itself
            return NamerefAssignmentResult::Target(current);
        }

        // Validate target is a valid variable name (not special chars like #, @, *, etc.)
        // Allow array subscripts like arr[0] or arr[@]
        if !valid_target_re.is_match(target_str) {
            // Invalid nameref target - assign to the nameref itself
            return NamerefAssignmentResult::Target(current);
        }

        current = target_str.to_string();
    }

    // Max depth exceeded - likely circular reference
    NamerefAssignmentResult::Circular
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
    fn test_is_nameref() {
        let mut state = make_state();
        assert!(!is_nameref(&state, "foo"));

        mark_nameref(&mut state, "foo");
        assert!(is_nameref(&state, "foo"));
        assert!(!is_nameref(&state, "bar"));
    }

    #[test]
    fn test_unmark_nameref() {
        let mut state = make_state();
        mark_nameref(&mut state, "foo");
        mark_nameref_bound(&mut state, "foo");
        mark_nameref_invalid(&mut state, "foo");

        assert!(is_nameref(&state, "foo"));

        unmark_nameref(&mut state, "foo");
        assert!(!is_nameref(&state, "foo"));
    }

    #[test]
    fn test_resolve_nameref_not_nameref() {
        let state = make_state();
        let env = make_env();

        let result = resolve_nameref(&state, &env, "foo", None);
        assert_eq!(result, Some("foo".to_string()));
    }

    #[test]
    fn test_resolve_nameref_simple() {
        let mut state = make_state();
        let mut env = make_env();

        // foo -> bar
        mark_nameref(&mut state, "foo");
        env.insert("foo".to_string(), "bar".to_string());
        env.insert("bar".to_string(), "value".to_string());

        let result = resolve_nameref(&state, &env, "foo", None);
        assert_eq!(result, Some("bar".to_string()));
    }

    #[test]
    fn test_resolve_nameref_chain() {
        let mut state = make_state();
        let mut env = make_env();

        // a -> b -> c
        mark_nameref(&mut state, "a");
        mark_nameref(&mut state, "b");
        env.insert("a".to_string(), "b".to_string());
        env.insert("b".to_string(), "c".to_string());
        env.insert("c".to_string(), "value".to_string());

        let result = resolve_nameref(&state, &env, "a", None);
        assert_eq!(result, Some("c".to_string()));
    }

    #[test]
    fn test_resolve_nameref_circular() {
        let mut state = make_state();
        let mut env = make_env();

        // a -> b -> a (circular)
        mark_nameref(&mut state, "a");
        mark_nameref(&mut state, "b");
        env.insert("a".to_string(), "b".to_string());
        env.insert("b".to_string(), "a".to_string());

        let result = resolve_nameref(&state, &env, "a", None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_nameref_invalid_target() {
        let mut state = make_state();
        let mut env = make_env();

        // foo -> "123" (invalid target - numeric only)
        mark_nameref(&mut state, "foo");
        env.insert("foo".to_string(), "123".to_string());

        let result = resolve_nameref(&state, &env, "foo", None);
        assert_eq!(result, Some("foo".to_string()));
    }

    #[test]
    fn test_resolve_nameref_empty() {
        let mut state = make_state();
        let env = make_env();

        // foo is a nameref but has no value
        mark_nameref(&mut state, "foo");

        let result = resolve_nameref(&state, &env, "foo", None);
        assert_eq!(result, Some("foo".to_string()));
    }

    #[test]
    fn test_get_nameref_target() {
        let mut state = make_state();
        let mut env = make_env();

        // Not a nameref
        assert_eq!(get_nameref_target(&state, &env, "foo"), None);

        // Nameref with target
        mark_nameref(&mut state, "foo");
        env.insert("foo".to_string(), "bar".to_string());

        assert_eq!(get_nameref_target(&state, &env, "foo"), Some("bar".to_string()));
    }

    #[test]
    fn test_target_exists_scalar() {
        let state = make_state();
        let mut env = make_env();

        assert!(!target_exists(&state, &env, "foo"));

        env.insert("foo".to_string(), "value".to_string());
        assert!(target_exists(&state, &env, "foo"));
    }

    #[test]
    fn test_target_exists_array() {
        let state = make_state();
        let mut env = make_env();

        assert!(!target_exists(&state, &env, "arr"));

        // Add array element
        env.insert("arr_0".to_string(), "first".to_string());
        assert!(target_exists(&state, &env, "arr"));
    }

    #[test]
    fn test_resolve_nameref_for_assignment_skip() {
        let mut state = make_state();
        let env = make_env();

        // Empty nameref with non-existent variable value
        mark_nameref(&mut state, "ref");

        let result = resolve_nameref_for_assignment(&state, &env, "ref", Some("nonexistent"), None);
        assert_eq!(result, NamerefAssignmentResult::Skip);
    }

    #[test]
    fn test_resolve_nameref_for_assignment_to_existing() {
        let mut state = make_state();
        let mut env = make_env();

        // Empty nameref, but value being assigned IS an existing variable name
        mark_nameref(&mut state, "ref");
        env.insert("target".to_string(), "exists".to_string());

        let result = resolve_nameref_for_assignment(&state, &env, "ref", Some("target"), None);
        assert_eq!(result, NamerefAssignmentResult::Target("ref".to_string()));
    }
}
