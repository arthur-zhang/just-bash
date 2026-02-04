//! Array Word Expansion Handlers
//!
//! Handles complex array expansion cases in word expansion:
//! - "${arr[@]}" and "${arr[*]}" - array element expansion
//! - "${arr[@]:-default}" - array with defaults
//! - "${arr[@]:offset:length}" - array slicing
//! - "${arr[@]/pattern/replacement}" - pattern replacement
//! - "${arr[@]#pattern}" - pattern removal
//! - "${arr[@]@op}" - transform operations

use crate::ast::types::{DoubleQuotedPart, LiteralPart, ParameterExpansionPart, WordPart};
use crate::interpreter::expansion::get_array_elements;
use crate::interpreter::helpers::{get_nameref_target, is_nameref};
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Result type for array expansion handlers.
/// `None` means the handler doesn't apply to this case.
#[derive(Debug, Clone)]
pub struct ArrayExpansionResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Handle simple "${arr[@]}" expansion without operations.
/// Returns each array element as a separate word.
pub fn handle_simple_array_expansion(
    state: &InterpreterState,
    word_parts: &[WordPart],
) -> Option<ArrayExpansionResult> {
    if word_parts.len() != 1 {
        return None;
    }

    let dq_part = match &word_parts[0] {
        WordPart::DoubleQuoted(dq) => dq,
        _ => return None,
    };

    if dq_part.parts.len() != 1 {
        return None;
    }

    let param_part = match &dq_part.parts[0] {
        WordPart::ParameterExpansion(pe) => pe,
        _ => return None,
    };

    // Check if it's ONLY the array expansion (like "${a[@]}") without operations
    if param_part.operation.is_some() {
        return None;
    }

    // Match array[@] pattern
    let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(@)\]$").unwrap();
    let caps = array_re.captures(&param_part.parameter)?;
    let array_name = caps.get(1)?.as_str();

    // Special case: if arrayName is a nameref pointing to array[@],
    // ${ref[@]} doesn't do double indirection - it returns empty
    if is_nameref(state, array_name) {
        if let Some(target) = get_nameref_target(state, &state.env, array_name) {
            if target.ends_with("[@]") || target.ends_with("[*]") {
                return Some(ArrayExpansionResult {
                    values: vec![],
                    quoted: true,
                });
            }
        }
    }

    let elements = get_array_elements(state, array_name);
    if !elements.is_empty() {
        return Some(ArrayExpansionResult {
            values: elements.into_iter().map(|(_, v)| v).collect(),
            quoted: true,
        });
    }

    // No array elements - check for scalar variable
    if let Some(scalar_value) = state.env.get(array_name) {
        return Some(ArrayExpansionResult {
            values: vec![scalar_value.clone()],
            quoted: true,
        });
    }

    // Variable is unset - return empty
    Some(ArrayExpansionResult {
        values: vec![],
        quoted: true,
    })
}

/// Handle namerefs pointing to array[@] - "${ref}" where ref='arr[@]'
/// When a nameref points to array[@], expanding "$ref" should produce multiple words
pub fn handle_nameref_array_expansion(
    state: &InterpreterState,
    word_parts: &[WordPart],
) -> Option<ArrayExpansionResult> {
    if word_parts.len() != 1 {
        return None;
    }

    let dq_part = match &word_parts[0] {
        WordPart::DoubleQuoted(dq) => dq,
        _ => return None,
    };

    if dq_part.parts.len() != 1 {
        return None;
    }

    let var_name = match &dq_part.parts[0] {
        WordPart::ParameterExpansion(pe) => {
            if pe.operation.is_some() {
                return None;
            }
            &pe.parameter
        }
        _ => return None,
    };

    // Check if it's a simple variable name (not already an array subscript)
    let simple_var_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    if !simple_var_re.is_match(var_name) || !is_nameref(state, var_name) {
        return None;
    }

    let target = get_nameref_target(state, &state.env, var_name)?;

    // Check if resolved target is array[@]
    let target_array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(@)\]$").unwrap();
    let caps = target_array_re.captures(&target)?;
    let array_name = caps.get(1)?.as_str();

    let elements = get_array_elements(state, array_name);
    if !elements.is_empty() {
        return Some(ArrayExpansionResult {
            values: elements.into_iter().map(|(_, v)| v).collect(),
            quoted: true,
        });
    }

    // No array elements - check for scalar variable
    if let Some(scalar_value) = state.env.get(array_name) {
        return Some(ArrayExpansionResult {
            values: vec![scalar_value.clone()],
            quoted: true,
        });
    }

    // Variable is unset - return empty
    Some(ArrayExpansionResult {
        values: vec![],
        quoted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_handle_simple_array_expansion_empty() {
        let state = make_state();
        // Test with non-matching pattern
        let parts = vec![WordPart::Literal(LiteralPart {
            value: "foo".to_string(),
        })];
        assert!(handle_simple_array_expansion(&state, &parts).is_none());
    }

    #[test]
    fn test_handle_simple_array_expansion_with_array() {
        let mut state = make_state();
        state.env.insert("arr_0".to_string(), "first".to_string());
        state.env.insert("arr_1".to_string(), "second".to_string());
        state.env.insert("arr_2".to_string(), "third".to_string());

        // Create "${arr[@]}" word parts
        let parts = vec![WordPart::DoubleQuoted(DoubleQuotedPart {
            parts: vec![WordPart::ParameterExpansion(ParameterExpansionPart {
                parameter: "arr[@]".to_string(),
                operation: None,
            })],
        })];

        let result = handle_simple_array_expansion(&state, &parts);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.values, vec!["first", "second", "third"]);
        assert!(result.quoted);
    }

    #[test]
    fn test_handle_simple_array_expansion_scalar() {
        let mut state = make_state();
        state.env.insert("s".to_string(), "scalar".to_string());

        // Create "${s[@]}" word parts
        let parts = vec![WordPart::DoubleQuoted(DoubleQuotedPart {
            parts: vec![WordPart::ParameterExpansion(ParameterExpansionPart {
                parameter: "s[@]".to_string(),
                operation: None,
            })],
        })];

        let result = handle_simple_array_expansion(&state, &parts);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.values, vec!["scalar"]);
    }
}
