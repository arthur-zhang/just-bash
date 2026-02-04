//! Parameter Operation Handlers
//!
//! Handles individual parameter expansion operations:
//! - DefaultValue, AssignDefault, UseAlternative, ErrorIfUnset
//! - PatternRemoval, PatternReplacement
//! - Length, Substring
//! - CaseModification, Transform
//! - Indirection, ArrayKeys, VarNamePrefix

use crate::interpreter::expansion::{
    apply_pattern_removal, expand_prompt, get_array_elements, get_var_names_with_prefix,
    get_variable, get_variable_attributes, is_variable_set, pattern_to_regex, quote_value,
    PatternRemovalSide,
};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Context with computed values used across multiple operation handlers.
#[derive(Debug, Clone)]
pub struct ParameterOpContext {
    pub value: String,
    pub is_unset: bool,
    pub is_empty: bool,
    pub effective_value: String,
    pub in_double_quotes: bool,
}

impl ParameterOpContext {
    /// Create a new ParameterOpContext for a given parameter.
    pub fn new(state: &InterpreterState, parameter: &str, in_double_quotes: bool) -> Self {
        let is_unset = !is_variable_set(state, parameter);
        let value = get_variable(state, parameter);
        let is_empty = value.is_empty();
        let effective_value = value.clone();

        Self {
            value,
            is_unset,
            is_empty,
            effective_value,
            in_double_quotes,
        }
    }
}

/// Check if default value should be used.
/// Returns true if the variable is unset, or if check_empty is true and the variable is empty.
pub fn should_use_default(op_ctx: &ParameterOpContext, check_empty: bool) -> bool {
    op_ctx.is_unset || (check_empty && op_ctx.is_empty)
}

/// Check if alternative value should be used.
/// Returns true if the variable is set (and non-empty if check_empty is true).
pub fn should_use_alternative(op_ctx: &ParameterOpContext, check_empty: bool) -> bool {
    !(op_ctx.is_unset || (check_empty && op_ctx.is_empty))
}

/// Apply pattern removal to a value.
/// side: "prefix" for # and ##, "suffix" for % and %%
/// greedy: true for ## and %%, false for # and %
pub fn apply_pattern_removal_op(
    value: &str,
    pattern_regex: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> String {
    apply_pattern_removal(value, pattern_regex, side, greedy)
}

/// Apply pattern replacement to a value.
/// replace_all: true for ${var//pattern/replacement}, false for ${var/pattern/replacement}
/// anchor_start: true for ${var/#pattern/replacement}
/// anchor_end: true for ${var/%pattern/replacement}
pub fn apply_pattern_replacement_op(
    value: &str,
    pattern_regex: &str,
    replacement: &str,
    replace_all: bool,
    anchor_start: bool,
    anchor_end: bool,
) -> String {
    let final_pattern = if anchor_start {
        format!("^{}", pattern_regex)
    } else if anchor_end {
        format!("{}$", pattern_regex)
    } else {
        pattern_regex.to_string()
    };

    match Regex::new(&final_pattern) {
        Ok(re) => {
            if replace_all {
                re.replace_all(value, replacement).to_string()
            } else {
                re.replace(value, replacement).to_string()
            }
        }
        Err(_) => value.to_string(),
    }
}

/// Get the length of a parameter value.
/// For arrays, returns the number of elements.
/// For strings, returns the character count.
pub fn get_parameter_length(state: &InterpreterState, parameter: &str) -> usize {
    // Check for array subscript
    let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[([@*])\]$").unwrap();
    if let Some(caps) = array_re.captures(parameter) {
        let array_name = caps.get(1).unwrap().as_str();
        let elements = get_array_elements(state, array_name);
        return elements.len();
    }

    // Regular variable - return string length
    let value = get_variable(state, parameter);
    value.chars().count()
}

/// Apply substring extraction to a value.
/// offset: starting position (can be negative for counting from end)
/// length: optional length (can be negative for counting from end)
pub fn apply_substring_op(value: &str, offset: i64, length: Option<i64>) -> Result<String, String> {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len() as i64;

    // Calculate start position
    let start = if offset < 0 {
        // Negative offset counts from end
        let computed = len + offset;
        if computed < 0 {
            0
        } else {
            computed as usize
        }
    } else {
        offset as usize
    };

    // If start is beyond the string, return empty
    if start >= chars.len() {
        return Ok(String::new());
    }

    // Calculate end position
    let end = match length {
        Some(l) if l < 0 => {
            // Negative length: count from end
            let computed = len + l;
            if computed < start as i64 {
                return Err("substring expression < 0".to_string());
            }
            computed as usize
        }
        Some(l) => (start + l as usize).min(chars.len()),
        None => chars.len(),
    };

    Ok(chars[start..end].iter().collect())
}

/// Apply case modification to a value.
/// operator: "U" for uppercase all, "u" for uppercase first, "L" for lowercase all, "l" for lowercase first
pub fn apply_case_modification(value: &str, operator: &str) -> String {
    match operator {
        "U" => value.to_uppercase(),
        "u" => {
            let mut chars = value.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        }
        "L" => value.to_lowercase(),
        "l" => {
            let mut chars = value.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_lowercase().to_string() + chars.as_str(),
            }
        }
        _ => value.to_string(),
    }
}

/// Apply transform operation to a value.
/// operator: "Q" for quoting, "P" for prompt expansion, "a" for attributes, etc.
pub fn apply_transform_op(state: &InterpreterState, parameter: &str, value: &str, operator: &str) -> String {
    match operator {
        "Q" => quote_value(value),
        "P" => expand_prompt(state, value),
        "a" => get_variable_attributes(state, parameter),
        "u" => {
            let mut chars = value.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        }
        "U" => value.to_uppercase(),
        "L" => value.to_lowercase(),
        _ => value.to_string(),
    }
}

/// Get array keys for ${!arr[@]} or ${!arr[*]}.
pub fn get_array_keys(state: &InterpreterState, array_name: &str, is_star: bool) -> Vec<String> {
    let elements = get_array_elements(state, array_name);
    let keys: Vec<String> = elements
        .iter()
        .map(|(idx, _)| match idx {
            crate::interpreter::expansion::ArrayIndex::Numeric(n) => n.to_string(),
            crate::interpreter::expansion::ArrayIndex::String(s) => s.clone(),
        })
        .collect();

    if is_star {
        // Join with IFS
        let ifs_sep = get_ifs_separator(&state.env);
        vec![keys.join(ifs_sep)]
    } else {
        keys
    }
}

/// Get variable names with a given prefix for ${!prefix*} or ${!prefix@}.
pub fn get_var_names_with_prefix_op(
    state: &InterpreterState,
    prefix: &str,
    is_star: bool,
) -> Vec<String> {
    let names = get_var_names_with_prefix(state, prefix);

    if is_star {
        // Join with IFS
        let ifs_sep = get_ifs_separator(&state.env);
        vec![names.join(ifs_sep)]
    } else {
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("var".to_string(), "hello world".to_string());
        env.insert("empty".to_string(), "".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_parameter_op_context() {
        let state = make_state();
        let ctx = ParameterOpContext::new(&state, "var", false);
        assert!(!ctx.is_unset);
        assert!(!ctx.is_empty);
        assert_eq!(ctx.value, "hello world");
    }

    #[test]
    fn test_parameter_op_context_unset() {
        let state = make_state();
        let ctx = ParameterOpContext::new(&state, "unset_var", false);
        assert!(ctx.is_unset);
        assert!(ctx.is_empty);
    }

    #[test]
    fn test_should_use_default() {
        let state = make_state();

        let ctx = ParameterOpContext::new(&state, "unset_var", false);
        assert!(should_use_default(&ctx, false));
        assert!(should_use_default(&ctx, true));

        let ctx = ParameterOpContext::new(&state, "empty", false);
        assert!(!should_use_default(&ctx, false));
        assert!(should_use_default(&ctx, true));

        let ctx = ParameterOpContext::new(&state, "var", false);
        assert!(!should_use_default(&ctx, false));
        assert!(!should_use_default(&ctx, true));
    }

    #[test]
    fn test_pattern_replacement() {
        let result = apply_pattern_replacement_op("hello world", "world", "rust", false, false, false);
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_pattern_replacement_all() {
        let result = apply_pattern_replacement_op("hello hello", "hello", "hi", true, false, false);
        assert_eq!(result, "hi hi");
    }

    #[test]
    fn test_substring() {
        assert_eq!(apply_substring_op("hello", 1, None).unwrap(), "ello");
        assert_eq!(apply_substring_op("hello", 1, Some(2)).unwrap(), "el");
        assert_eq!(apply_substring_op("hello", -2, None).unwrap(), "lo");
    }

    #[test]
    fn test_case_modification() {
        assert_eq!(apply_case_modification("hello", "U"), "HELLO");
        assert_eq!(apply_case_modification("hello", "u"), "Hello");
        assert_eq!(apply_case_modification("HELLO", "L"), "hello");
        assert_eq!(apply_case_modification("HELLO", "l"), "hELLO");
    }

    #[test]
    fn test_get_parameter_length() {
        let state = make_state();
        assert_eq!(get_parameter_length(&state, "var"), 11); // "hello world" = 11 chars
    }
}
