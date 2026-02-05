//! Variable assignment helpers for declare, readonly, local, export builtins.

use crate::interpreter::types::InterpreterState;
use crate::interpreter::builtins::declare_array_parsing::parse_array_elements;
use crate::interpreter::helpers::readonly::check_readonly_error;

/// Result of parsing an assignment argument.
#[derive(Debug, Clone)]
pub struct ParsedAssignment {
    pub name: String,
    pub is_array: bool,
    pub array_elements: Option<Vec<String>>,
    pub value: Option<String>,
    /// For array index assignment: a[index]=value
    pub array_index: Option<String>,
}

/// Parse an assignment argument like "name=value", "name=(a b c)", or "name[index]=value".
pub fn parse_assignment(arg: &str) -> ParsedAssignment {
    // Check for array assignment: name=(...)
    // Use a simple approach: find the pattern name=(...) where ... can contain anything
    if let Some(eq_pos) = arg.find('=') {
        let name = &arg[..eq_pos];
        let rest = &arg[eq_pos + 1..];

        // Check if it's a valid identifier
        if is_valid_identifier(name) {
            // Check for array assignment: name=(...)
            if rest.starts_with('(') && rest.ends_with(')') {
                let inner = &rest[1..rest.len() - 1];
                return ParsedAssignment {
                    name: name.to_string(),
                    is_array: true,
                    array_elements: Some(parse_array_elements(inner)),
                    value: None,
                    array_index: None,
                };
            }
        }
    }

    // Check for array index assignment: name[index]=value
    if let Some(bracket_start) = arg.find('[') {
        if let Some(bracket_end) = arg.find(']') {
            if bracket_end > bracket_start {
                let name = &arg[..bracket_start];
                let index = &arg[bracket_start + 1..bracket_end];
                let rest = &arg[bracket_end + 1..];

                if rest.starts_with('=') && is_valid_identifier(name) {
                    let value = &rest[1..];
                    return ParsedAssignment {
                        name: name.to_string(),
                        is_array: false,
                        array_elements: None,
                        value: Some(value.to_string()),
                        array_index: Some(index.to_string()),
                    };
                }
            }
        }
    }

    // Check for scalar assignment: name=value
    if let Some(eq_pos) = arg.find('=') {
        let name = &arg[..eq_pos];
        let value = &arg[eq_pos + 1..];
        return ParsedAssignment {
            name: name.to_string(),
            is_array: false,
            array_elements: None,
            value: Some(value.to_string()),
            array_index: None,
        };
    }

    // Just a name, no value
    ParsedAssignment {
        name: arg.to_string(),
        is_array: false,
        array_elements: None,
        value: None,
        array_index: None,
    }
}

/// Check if a string is a valid shell identifier.
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !matches!(first, b'a'..=b'z' | b'A'..=b'Z' | b'_') {
        return false;
    }
    bytes[1..].iter().all(|&b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

/// Options for setting a variable.
#[derive(Debug, Clone, Default)]
pub struct SetVariableOptions {
    pub make_readonly: bool,
    pub check_readonly: bool,
}

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Set a variable from a parsed assignment.
/// Returns an error result if the variable is readonly, otherwise Ok(()).
pub fn set_variable(
    state: &mut InterpreterState,
    assignment: &ParsedAssignment,
    options: &SetVariableOptions,
) -> Result<(), BuiltinResult> {
    let name = &assignment.name;

    // Check if variable is readonly (if checking is enabled)
    if options.check_readonly {
        if let Err(err) = check_readonly_error(state, name, "assignment") {
            return Err((String::new(), err.stderr, 1));
        }
    }

    if assignment.is_array {
        if let Some(ref elements) = assignment.array_elements {
            // Set array elements
            for (i, elem) in elements.iter().enumerate() {
                state.env.insert(format!("{}_{}", name, i), elem.clone());
            }
            state.env.insert(format!("{}__length", name), elements.len().to_string());
        }
    } else if let Some(ref index_str) = assignment.array_index {
        if let Some(ref value) = assignment.value {
            // Array index assignment: a[index]=value
            // For now, try to parse as a simple number
            let index: i64 = index_str.trim().parse().unwrap_or(0);
            state.env.insert(format!("{}_{}", name, index), value.clone());

            // Update array length if needed (sparse arrays may have gaps)
            let length_key = format!("{}__length", name);
            let current_length: i64 = state.env.get(&length_key)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if index >= current_length {
                state.env.insert(length_key, (index + 1).to_string());
            }
        }
    } else if let Some(ref value) = assignment.value {
        // Set scalar value
        state.env.insert(name.clone(), value.clone());
    }

    // Mark as readonly if requested
    if options.make_readonly {
        use crate::interpreter::helpers::readonly::mark_readonly;
        mark_readonly(state, name);
    }

    Ok(())
}

/// Get the call depth at which a local variable was declared.
/// Returns None if the variable is not a local variable.
pub fn get_local_var_depth(state: &InterpreterState, name: &str) -> Option<u32> {
    state.local_var_depth.as_ref().and_then(|map| map.get(name).copied())
}

/// Clear the local variable depth tracking for a variable.
/// Called when a local variable is cell-unset (dynamic-unset).
pub fn clear_local_var_depth(state: &mut InterpreterState, name: &str) {
    if let Some(ref mut map) = state.local_var_depth {
        map.remove(name);
    }
}

/// Push the current value of a variable onto the local var stack.
/// Used for bash's localvar-nest behavior where nested local declarations
/// each create a new cell that can be unset independently.
pub fn push_local_var_stack(
    state: &mut InterpreterState,
    name: &str,
    current_value: Option<String>,
) {
    use crate::interpreter::types::LocalVarStackEntry;

    if state.local_var_stack.is_none() {
        state.local_var_stack = Some(std::collections::HashMap::new());
    }

    let scope_index = state.local_scopes.len().saturating_sub(1);
    let entry = LocalVarStackEntry {
        value: current_value,
        scope_index,
    };

    let stack_map = state.local_var_stack.as_mut().unwrap();
    stack_map.entry(name.to_string()).or_insert_with(Vec::new).push(entry);
}

/// Pop the top entry from the local var stack for a variable.
/// Returns the saved value and scope index if there was an entry, or None if the stack was empty.
pub fn pop_local_var_stack(
    state: &mut InterpreterState,
    name: &str,
) -> Option<(Option<String>, usize)> {
    let stack_map = state.local_var_stack.as_mut()?;
    let stack = stack_map.get_mut(name)?;
    let entry = stack.pop()?;
    Some((entry.value, entry.scope_index))
}

/// Clear all local var stack entries for a specific scope index.
/// Called when a function returns and its local scope is popped.
pub fn clear_local_var_stack_for_scope(state: &mut InterpreterState, scope_index: usize) {
    let stack_map = match state.local_var_stack.as_mut() {
        Some(map) => map,
        None => return,
    };

    let mut empty_keys = Vec::new();

    for (name, stack) in stack_map.iter_mut() {
        // Remove entries from the top of the stack that belong to this scope
        while !stack.is_empty() && stack.last().map(|e| e.scope_index) == Some(scope_index) {
            stack.pop();
        }
        // Track empty entries for cleanup
        if stack.is_empty() {
            empty_keys.push(name.clone());
        }
    }

    // Clean up empty entries
    for key in empty_keys {
        stack_map.remove(&key);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assignment_scalar() {
        let result = parse_assignment("foo=bar");
        assert_eq!(result.name, "foo");
        assert!(!result.is_array);
        assert_eq!(result.value, Some("bar".to_string()));
        assert!(result.array_index.is_none());
    }

    #[test]
    fn test_parse_assignment_array() {
        let result = parse_assignment("arr=(a b c)");
        assert_eq!(result.name, "arr");
        assert!(result.is_array);
        assert_eq!(result.array_elements, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
    }

    #[test]
    fn test_parse_assignment_array_index() {
        let result = parse_assignment("arr[5]=value");
        assert_eq!(result.name, "arr");
        assert!(!result.is_array);
        assert_eq!(result.array_index, Some("5".to_string()));
        assert_eq!(result.value, Some("value".to_string()));
    }

    #[test]
    fn test_parse_assignment_name_only() {
        let result = parse_assignment("varname");
        assert_eq!(result.name, "varname");
        assert!(!result.is_array);
        assert!(result.value.is_none());
        assert!(result.array_index.is_none());
    }

    #[test]
    fn test_parse_assignment_empty_value() {
        let result = parse_assignment("foo=");
        assert_eq!(result.name, "foo");
        assert_eq!(result.value, Some("".to_string()));
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("foo123"));
        assert!(is_valid_identifier("FOO_BAR"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
    }

    #[test]
    fn test_set_variable_scalar() {
        let mut state = InterpreterState::default();
        let assignment = parse_assignment("x=hello");
        let options = SetVariableOptions::default();
        let result = set_variable(&mut state, &assignment, &options);
        assert!(result.is_ok());
        assert_eq!(state.env.get("x"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_set_variable_array() {
        let mut state = InterpreterState::default();
        let assignment = parse_assignment("arr=(one two three)");
        let options = SetVariableOptions::default();
        let result = set_variable(&mut state, &assignment, &options);
        assert!(result.is_ok());
        assert_eq!(state.env.get("arr_0"), Some(&"one".to_string()));
        assert_eq!(state.env.get("arr_1"), Some(&"two".to_string()));
        assert_eq!(state.env.get("arr_2"), Some(&"three".to_string()));
        assert_eq!(state.env.get("arr__length"), Some(&"3".to_string()));
    }

    #[test]
    fn test_set_variable_array_index() {
        let mut state = InterpreterState::default();
        let assignment = parse_assignment("arr[10]=value");
        let options = SetVariableOptions::default();
        let result = set_variable(&mut state, &assignment, &options);
        assert!(result.is_ok());
        assert_eq!(state.env.get("arr_10"), Some(&"value".to_string()));
        assert_eq!(state.env.get("arr__length"), Some(&"11".to_string()));
    }
}
