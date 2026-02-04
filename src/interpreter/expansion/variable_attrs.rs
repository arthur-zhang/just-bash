//! Variable Attributes
//!
//! Functions for getting variable attributes (${var@a} transformation).

use crate::interpreter::helpers::{is_nameref, is_readonly};
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Get the attributes of a variable for ${var@a} transformation.
/// Returns a string with attribute flags (e.g., "ar" for readonly array).
///
/// Attribute flags (in order):
/// - a: indexed array
/// - A: associative array
/// - i: integer
/// - n: nameref
/// - r: readonly
/// - x: exported
pub fn get_variable_attributes(state: &InterpreterState, name: &str) -> String {
    // Handle special variables (like ?, $, etc.) - they have no attributes
    let var_name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    if !var_name_re.is_match(name) {
        return String::new();
    }

    let mut attrs = String::new();

    // Check for indexed array (has numeric elements via name_0, name_1, etc. or __length marker)
    let length_key = format!("{}__length", name);
    let is_indexed_array = state.env.contains_key(&length_key)
        || state.env.keys().any(|k| {
            if let Some(suffix) = k.strip_prefix(&format!("{}_", name)) {
                suffix.chars().all(|c| c.is_ascii_digit())
            } else {
                false
            }
        });

    // Check for associative array
    let is_assoc_array = state
        .associative_arrays
        .as_ref()
        .map(|aa| aa.contains(name))
        .unwrap_or(false);

    // Add array attributes (indexed before associative)
    if is_indexed_array && !is_assoc_array {
        attrs.push('a');
    }
    if is_assoc_array {
        attrs.push('A');
    }

    // Check for integer attribute
    if state
        .integer_vars
        .as_ref()
        .map(|iv| iv.contains(name))
        .unwrap_or(false)
    {
        attrs.push('i');
    }

    // Check for nameref attribute
    if is_nameref(state, name) {
        attrs.push('n');
    }

    // Check for readonly attribute
    if is_readonly(state, name) {
        attrs.push('r');
    }

    // Check for exported attribute
    if state
        .exported_vars
        .as_ref()
        .map(|ev| ev.contains(name))
        .unwrap_or(false)
    {
        attrs.push('x');
    }

    attrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn make_state() -> InterpreterState {
        InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_special_var_no_attrs() {
        let state = make_state();
        assert_eq!(get_variable_attributes(&state, "?"), "");
        assert_eq!(get_variable_attributes(&state, "$"), "");
        assert_eq!(get_variable_attributes(&state, "1"), "");
    }

    #[test]
    fn test_plain_var_no_attrs() {
        let state = make_state();
        assert_eq!(get_variable_attributes(&state, "foo"), "");
    }

    #[test]
    fn test_exported_var() {
        let mut state = make_state();
        let mut exported = HashSet::new();
        exported.insert("PATH".to_string());
        state.exported_vars = Some(exported);
        assert_eq!(get_variable_attributes(&state, "PATH"), "x");
    }

    #[test]
    fn test_readonly_var() {
        let mut state = make_state();
        let mut readonly = HashSet::new();
        readonly.insert("CONST".to_string());
        state.readonly_vars = Some(readonly);
        assert_eq!(get_variable_attributes(&state, "CONST"), "r");
    }

    #[test]
    fn test_indexed_array() {
        let mut state = make_state();
        state.env.insert("arr__length".to_string(), "3".to_string());
        assert_eq!(get_variable_attributes(&state, "arr"), "a");
    }
}
