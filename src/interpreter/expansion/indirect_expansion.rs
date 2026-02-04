//! Indirect Array Expansion Handlers
//!
//! Handles "${!ref}" style indirect expansions where ref points to an array:
//! - "${!ref}" where ref='arr[@]' or ref='arr[*]'
//! - "${!ref:offset}" and "${!ref:offset:length}" - array slicing via indirection
//! - "${!ref:-default}" and "${!ref:+alternative}" - default/alternative via indirection
//! - "${ref+${!ref}}" - indirect in alternative value
//! - "${!ref+${!ref}}" - indirect with inner alternative

use crate::interpreter::expansion::{get_array_elements, get_variable, is_variable_set, get_variable_attributes, ArrayIndex};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Result type for indirect expansion handlers.
#[derive(Debug, Clone)]
pub struct IndirectExpansionResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Parse an array reference like "arr[@]" or "arr[*]"
/// Returns (array_name, is_star) if it matches, None otherwise.
pub fn parse_array_reference(ref_value: &str) -> Option<(String, bool)> {
    let re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[([@*])\]$").ok()?;
    let caps = re.captures(ref_value)?;
    let array_name = caps.get(1)?.as_str().to_string();
    let is_star = caps.get(2)?.as_str() == "*";
    Some((array_name, is_star))
}

/// Handle simple indirect array expansion "${!ref}" where ref='arr[@]' or ref='arr[*]'.
/// Returns the array elements directly.
pub fn expand_indirect_array(
    state: &InterpreterState,
    ref_var_name: &str,
) -> Option<IndirectExpansionResult> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);

    if !elements.is_empty() {
        let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
        if is_star {
            // arr[*] - join with IFS into one word
            let ifs_sep = get_ifs_separator(&state.env);
            return Some(IndirectExpansionResult {
                values: vec![values.join(ifs_sep)],
                quoted: true,
            });
        }
        // arr[@] - each element as a separate word
        return Some(IndirectExpansionResult {
            values,
            quoted: true,
        });
    }

    // No array elements - check for scalar variable
    if let Some(scalar_value) = state.env.get(&array_name) {
        return Some(IndirectExpansionResult {
            values: vec![scalar_value.clone()],
            quoted: true,
        });
    }

    // Variable is unset - return empty
    Some(IndirectExpansionResult {
        values: vec![],
        quoted: true,
    })
}

/// Handle indirect array slicing "${!ref:offset}" or "${!ref:offset:length}".
/// offset and length should be pre-evaluated.
pub fn expand_indirect_array_slicing(
    state: &InterpreterState,
    ref_var_name: &str,
    offset: i64,
    length: Option<i64>,
) -> Option<Result<IndirectExpansionResult, String>> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);

    // For sparse arrays, offset refers to index position
    let start_idx: usize = if offset < 0 {
        if !elements.is_empty() {
            let last_idx = match &elements[elements.len() - 1].0 {
                ArrayIndex::Numeric(n) => *n,
                _ => 0,
            };
            let target_index = last_idx + 1 + offset;
            if target_index < 0 {
                return Some(Ok(IndirectExpansionResult {
                    values: vec![],
                    quoted: true,
                }));
            }
            elements
                .iter()
                .position(|(idx, _)| match idx {
                    ArrayIndex::Numeric(n) => *n >= target_index,
                    _ => false,
                })
                .unwrap_or(elements.len())
        } else {
            0
        }
    } else {
        elements
            .iter()
            .position(|(idx, _)| match idx {
                ArrayIndex::Numeric(n) => *n >= offset,
                _ => false,
            })
            .unwrap_or(elements.len())
    };

    let sliced_values: Vec<String> = if let Some(len) = length {
        if len < 0 {
            return Some(Err(format!(
                "{}[@]: substring expression < 0",
                array_name
            )));
        }
        elements
            .iter()
            .skip(start_idx)
            .take(len as usize)
            .map(|(_, v)| v.clone())
            .collect()
    } else {
        elements.iter().skip(start_idx).map(|(_, v)| v.clone()).collect()
    };

    if sliced_values.is_empty() {
        return Some(Ok(IndirectExpansionResult {
            values: vec![],
            quoted: true,
        }));
    }

    if is_star {
        let ifs_sep = get_ifs_separator(&state.env);
        Some(Ok(IndirectExpansionResult {
            values: vec![sliced_values.join(ifs_sep)],
            quoted: true,
        }))
    } else {
        Some(Ok(IndirectExpansionResult {
            values: sliced_values,
            quoted: true,
        }))
    }
}

/// Handle indirect array with default value "${!ref:-default}".
/// Returns (should_use_default, array_values) where:
/// - should_use_default: true if the default value should be used
/// - array_values: the array values if not using default
pub fn check_indirect_array_default(
    state: &InterpreterState,
    ref_var_name: &str,
    check_empty: bool,
) -> Option<(bool, IndirectExpansionResult)> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);
    let is_empty = elements.is_empty();
    let is_unset = elements.is_empty() && !state.env.contains_key(&array_name);

    let should_use_default = is_unset || (check_empty && is_empty);

    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
    let result = if is_star {
        let ifs_sep = get_ifs_separator(&state.env);
        IndirectExpansionResult {
            values: vec![values.join(ifs_sep)],
            quoted: true,
        }
    } else {
        IndirectExpansionResult {
            values,
            quoted: true,
        }
    };

    Some((should_use_default, result))
}

/// Handle indirect array with alternative value "${!ref:+alternative}".
/// Returns (should_use_alternative, array_values) where:
/// - should_use_alternative: true if the alternative value should be used
/// - array_values: the array values if not using alternative
pub fn check_indirect_array_alternative(
    state: &InterpreterState,
    ref_var_name: &str,
    check_empty: bool,
) -> Option<(bool, IndirectExpansionResult)> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);
    let is_empty = elements.is_empty();
    let is_unset = elements.is_empty() && !state.env.contains_key(&array_name);

    // UseAlternative: return alternative if set and non-empty
    let should_use_alternative = !is_unset && !(check_empty && is_empty);

    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
    let result = if is_star {
        let ifs_sep = get_ifs_separator(&state.env);
        IndirectExpansionResult {
            values: vec![values.join(ifs_sep)],
            quoted: true,
        }
    } else {
        IndirectExpansionResult {
            values,
            quoted: true,
        }
    };

    Some((should_use_alternative, result))
}

/// Handle indirect array transform "${!ref@a}" - get attributes.
pub fn expand_indirect_array_attributes(
    state: &InterpreterState,
    ref_var_name: &str,
) -> Option<IndirectExpansionResult> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);
    let attrs = get_variable_attributes(state, &array_name);

    let values: Vec<String> = elements.iter().map(|_| attrs.clone()).collect();

    if is_star {
        let ifs_sep = get_ifs_separator(&state.env);
        Some(IndirectExpansionResult {
            values: vec![values.join(ifs_sep)],
            quoted: true,
        })
    } else {
        Some(IndirectExpansionResult {
            values,
            quoted: true,
        })
    }
}

/// Handle "${!ref}" where ref='@' or ref='*' - indirect positional parameters.
pub fn expand_indirect_positional(
    state: &InterpreterState,
    ref_var_name: &str,
) -> Option<IndirectExpansionResult> {
    let ref_value = get_variable(state, ref_var_name);

    if ref_value == "@" || ref_value == "*" {
        let num_params: i32 = state
            .env
            .get("#")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut params = Vec::new();
        for i in 1..=num_params {
            params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
        }

        if ref_value == "*" {
            // ref='*' - join with IFS into one word (like "$*")
            let ifs_sep = get_ifs_separator(&state.env);
            return Some(IndirectExpansionResult {
                values: vec![params.join(ifs_sep)],
                quoted: true,
            });
        }
        // ref='@' - each param as a separate word (like "$@")
        return Some(IndirectExpansionResult {
            values: params,
            quoted: true,
        });
    }

    None
}

// ============================================================================
// Indirect in Alternative/Default Value Handlers
// ============================================================================

/// Context for checking if a variable should use alternative/default.
#[derive(Debug, Clone)]
pub struct IndirectAlternativeContext {
    /// Whether the outer variable is set
    pub is_set: bool,
    /// Whether the outer variable is empty
    pub is_empty: bool,
    /// Whether to check for empty (colon variant)
    pub check_empty: bool,
}

/// Handle ${ref+${!ref}} or ${ref-${!ref}} - indirect in alternative/default value.
/// This handles patterns like: ${hooksSlice+"${!hooksSlice}"} which should preserve element boundaries.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `outer_var_name` - The outer variable name (e.g., "hooksSlice")
/// * `inner_ref_var_name` - The inner reference variable name (e.g., "hooksSlice")
/// * `is_alternative` - true for UseAlternative (+), false for DefaultValue (-)
/// * `check_empty` - Whether to check for empty (colon variant)
///
/// # Returns
/// * `None` if the inner reference doesn't point to an array
/// * `Some((should_expand, result))` where:
///   - `should_expand`: true if the alternative/default should be expanded
///   - `result`: the array values if should_expand is true
pub fn check_indirect_in_alternative(
    state: &InterpreterState,
    outer_var_name: &str,
    inner_ref_var_name: &str,
    is_alternative: bool,
    check_empty: bool,
) -> Option<(bool, IndirectExpansionResult)> {
    // Get the value of the inner reference variable to see if it points to an array
    let ref_value = get_variable(state, inner_ref_var_name);
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Check if we should use the alternative/default
    let is_set = is_variable_set(state, outer_var_name);
    let outer_value = get_variable(state, outer_var_name);
    let is_empty = outer_value.is_empty();

    let should_expand = if is_alternative {
        // ${var+word} - expand if var IS set (and non-empty if :+)
        is_set && !(check_empty && is_empty)
    } else {
        // ${var-word} - expand if var is NOT set (or empty if :-)
        !is_set || (check_empty && is_empty)
    };

    if should_expand {
        // Expand the inner indirect array reference
        let elements = get_array_elements(state, &array_name);
        if !elements.is_empty() {
            let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
            if is_star {
                // arr[*] - join with IFS into one word
                let ifs_sep = get_ifs_separator(&state.env);
                return Some((true, IndirectExpansionResult {
                    values: vec![values.join(ifs_sep)],
                    quoted: true,
                }));
            }
            // arr[@] - each element as a separate word (quoted)
            return Some((true, IndirectExpansionResult {
                values,
                quoted: true,
            }));
        }
        // No array elements - check for scalar variable
        if let Some(scalar_value) = state.env.get(&array_name) {
            return Some((true, IndirectExpansionResult {
                values: vec![scalar_value.clone()],
                quoted: true,
            }));
        }
        // Variable is unset - return empty
        return Some((true, IndirectExpansionResult {
            values: vec![],
            quoted: true,
        }));
    }

    // Don't expand the alternative - return empty
    Some((false, IndirectExpansionResult {
        values: vec![],
        quoted: false,
    }))
}

/// Handle ${!ref+${!ref}} or ${!ref-${!ref}} - indirect with inner alternative/default value.
/// This handles patterns like: ${!hooksSlice+"${!hooksSlice}"} which should preserve element boundaries.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `outer_ref_var_name` - The outer reference variable name
/// * `inner_ref_var_name` - The inner reference variable name
/// * `is_alternative` - true for UseAlternative (+), false for DefaultValue (-)
/// * `check_empty` - Whether to check for empty (colon variant)
///
/// # Returns
/// * `None` if the inner reference doesn't point to an array
/// * `Some((should_expand, result))` where:
///   - `should_expand`: true if the alternative/default should be expanded
///   - `result`: the array values if should_expand is true
pub fn check_indirection_with_inner_alternative(
    state: &InterpreterState,
    outer_ref_var_name: &str,
    inner_ref_var_name: &str,
    is_alternative: bool,
    check_empty: bool,
) -> Option<(bool, IndirectExpansionResult)> {
    // Get the value of the inner reference variable to see if it points to an array
    let ref_value = get_variable(state, inner_ref_var_name);
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // First resolve the outer indirection
    let outer_ref_value = get_variable(state, outer_ref_var_name);

    // Check if we should use the alternative/default
    let is_set = is_variable_set(state, outer_ref_var_name);
    let is_empty = outer_ref_value.is_empty();

    let should_expand = if is_alternative {
        // ${!var+word} - expand if the indirect target IS set (and non-empty if :+)
        is_set && !(check_empty && is_empty)
    } else {
        // ${!var-word} - expand if the indirect target is NOT set (or empty if :-)
        !is_set || (check_empty && is_empty)
    };

    if should_expand {
        // Expand the inner indirect array reference
        let elements = get_array_elements(state, &array_name);
        if !elements.is_empty() {
            let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
            if is_star {
                // arr[*] - join with IFS into one word
                let ifs_sep = get_ifs_separator(&state.env);
                return Some((true, IndirectExpansionResult {
                    values: vec![values.join(ifs_sep)],
                    quoted: true,
                }));
            }
            // arr[@] - each element as a separate word (quoted)
            return Some((true, IndirectExpansionResult {
                values,
                quoted: true,
            }));
        }
        // No array elements - check for scalar variable
        if let Some(scalar_value) = state.env.get(&array_name) {
            return Some((true, IndirectExpansionResult {
                values: vec![scalar_value.clone()],
                quoted: true,
            }));
        }
        // Variable is unset - return empty
        return Some((true, IndirectExpansionResult {
            values: vec![],
            quoted: true,
        }));
    }

    // Don't expand the alternative - fall through to return empty or the outer value
    Some((false, IndirectExpansionResult {
        values: vec![],
        quoted: false,
    }))
}

/// Handle indirect array with AssignDefault "${!ref:=default}".
/// Returns (should_assign, array_name, array_values) where:
/// - should_assign: true if the default value should be assigned
/// - array_name: the target array name for assignment
/// - array_values: the current array values
pub fn check_indirect_array_assign_default(
    state: &InterpreterState,
    ref_var_name: &str,
    check_empty: bool,
) -> Option<(bool, String, IndirectExpansionResult)> {
    // Get the value of the reference variable
    let ref_value = get_variable(state, ref_var_name);
    if ref_value.is_empty() {
        return None;
    }

    // Check if it's an array reference
    let (array_name, is_star) = parse_array_reference(&ref_value)?;

    // Get array elements
    let elements = get_array_elements(state, &array_name);
    let is_empty = elements.is_empty();
    let is_unset = elements.is_empty() && !state.env.contains_key(&array_name);

    let should_assign = is_unset || (check_empty && is_empty);

    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
    let result = if is_star {
        let ifs_sep = get_ifs_separator(&state.env);
        IndirectExpansionResult {
            values: vec![values.join(ifs_sep)],
            quoted: true,
        }
    } else {
        IndirectExpansionResult {
            values,
            quoted: true,
        }
    };

    Some((should_assign, array_name, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state_with_array(name: &str, values: &[&str]) -> InterpreterState {
        let mut env = HashMap::new();
        for (i, v) in values.iter().enumerate() {
            env.insert(format!("{}_{}", name, i), v.to_string());
        }
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_parse_array_reference() {
        assert_eq!(
            parse_array_reference("arr[@]"),
            Some(("arr".to_string(), false))
        );
        assert_eq!(
            parse_array_reference("arr[*]"),
            Some(("arr".to_string(), true))
        );
        assert_eq!(parse_array_reference("arr"), None);
        assert_eq!(parse_array_reference("arr[0]"), None);
    }

    #[test]
    fn test_expand_indirect_array_at() {
        let mut state = make_state_with_array("arr", &["a", "b", "c"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());

        let result = expand_indirect_array(&state, "ref").unwrap();
        assert_eq!(result.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_expand_indirect_array_star() {
        let mut state = make_state_with_array("arr", &["a", "b", "c"]);
        state.env.insert("ref".to_string(), "arr[*]".to_string());

        let result = expand_indirect_array(&state, "ref").unwrap();
        assert_eq!(result.values, vec!["a b c"]);
    }

    #[test]
    fn test_expand_indirect_positional_at() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "3".to_string());
        state.env.insert("1".to_string(), "a".to_string());
        state.env.insert("2".to_string(), "b".to_string());
        state.env.insert("3".to_string(), "c".to_string());
        state.env.insert("ref".to_string(), "@".to_string());

        let result = expand_indirect_positional(&state, "ref").unwrap();
        assert_eq!(result.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_expand_indirect_positional_star() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "3".to_string());
        state.env.insert("1".to_string(), "a".to_string());
        state.env.insert("2".to_string(), "b".to_string());
        state.env.insert("3".to_string(), "c".to_string());
        state.env.insert("ref".to_string(), "*".to_string());

        let result = expand_indirect_positional(&state, "ref").unwrap();
        assert_eq!(result.values, vec!["a b c"]);
    }

    #[test]
    fn test_check_indirect_array_default() {
        let mut state = make_state_with_array("arr", &["a", "b"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());

        let (should_use_default, result) =
            check_indirect_array_default(&state, "ref", false).unwrap();
        assert!(!should_use_default);
        assert_eq!(result.values, vec!["a", "b"]);
    }

    #[test]
    fn test_check_indirect_array_default_empty() {
        let mut state = InterpreterState::default();
        state.env.insert("ref".to_string(), "empty[@]".to_string());

        let (should_use_default, result) =
            check_indirect_array_default(&state, "ref", false).unwrap();
        assert!(should_use_default);
        assert!(result.values.is_empty());
    }

    #[test]
    fn test_check_indirect_in_alternative_set() {
        let mut state = make_state_with_array("arr", &["a", "b", "c"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());
        state.env.insert("outer".to_string(), "value".to_string());

        // ${outer+"${!ref}"} - outer is set, should expand
        let (should_expand, result) =
            check_indirect_in_alternative(&state, "outer", "ref", true, false).unwrap();
        assert!(should_expand);
        assert_eq!(result.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_check_indirect_in_alternative_unset() {
        let mut state = make_state_with_array("arr", &["a", "b", "c"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());
        // outer is not set

        // ${outer+"${!ref}"} - outer is unset, should not expand
        let (should_expand, _result) =
            check_indirect_in_alternative(&state, "outer", "ref", true, false).unwrap();
        assert!(!should_expand);
    }

    #[test]
    fn test_check_indirect_in_default_unset() {
        let mut state = make_state_with_array("arr", &["a", "b", "c"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());
        // outer is not set

        // ${outer-"${!ref}"} - outer is unset, should expand
        let (should_expand, result) =
            check_indirect_in_alternative(&state, "outer", "ref", false, false).unwrap();
        assert!(should_expand);
        assert_eq!(result.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_check_indirection_with_inner_alternative() {
        let mut state = make_state_with_array("arr", &["x", "y"]);
        state.env.insert("ref".to_string(), "arr[@]".to_string());
        state.env.insert("outer".to_string(), "something".to_string());

        // ${!outer+"${!ref}"} - outer is set, should expand
        let (should_expand, result) =
            check_indirection_with_inner_alternative(&state, "outer", "ref", true, false).unwrap();
        assert!(should_expand);
        assert_eq!(result.values, vec!["x", "y"]);
    }

    #[test]
    fn test_check_indirect_array_assign_default() {
        let mut state = InterpreterState::default();
        state.env.insert("ref".to_string(), "empty[@]".to_string());

        let (should_assign, array_name, _result) =
            check_indirect_array_assign_default(&state, "ref", false).unwrap();
        assert!(should_assign);
        assert_eq!(array_name, "empty");
    }
}
