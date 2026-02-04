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

// ============================================================================
// Higher-level operation handlers with callback support
// ============================================================================

/// Handle DefaultValue operation: ${param:-word}
/// Returns the default value if the variable is unset (or empty if check_empty is true).
pub fn handle_default_value<F>(
    op_ctx: &ParameterOpContext,
    check_empty: bool,
    expand_default: F,
) -> String
where
    F: FnOnce() -> String,
{
    if should_use_default(op_ctx, check_empty) {
        expand_default()
    } else {
        op_ctx.effective_value.clone()
    }
}

/// Handle AssignDefault operation: ${param:=word}
/// Assigns and returns the default value if the variable is unset (or empty if check_empty is true).
///
/// # Arguments
/// * `op_ctx` - Parameter operation context
/// * `parameter` - The parameter name (may include array subscript)
/// * `check_empty` - Whether to also check for empty value
/// * `expand_default` - Function to expand the default word
/// * `assign_var` - Function to assign the value to the variable
pub fn handle_assign_default<F, A>(
    op_ctx: &ParameterOpContext,
    parameter: &str,
    check_empty: bool,
    expand_default: F,
    assign_var: A,
) -> String
where
    F: FnOnce() -> String,
    A: FnOnce(&str, &str),
{
    if should_use_default(op_ctx, check_empty) {
        let default_value = expand_default();
        assign_var(parameter, &default_value);
        default_value
    } else {
        op_ctx.effective_value.clone()
    }
}

/// Error result for ErrorIfUnset operation
#[derive(Debug, Clone)]
pub struct ErrorIfUnsetResult {
    pub value: Option<String>,
    pub error_message: Option<String>,
}

/// Handle ErrorIfUnset operation: ${param:?word}
/// Returns an error if the variable is unset (or empty if check_empty is true).
pub fn handle_error_if_unset<F>(
    op_ctx: &ParameterOpContext,
    parameter: &str,
    check_empty: bool,
    expand_message: Option<F>,
) -> ErrorIfUnsetResult
where
    F: FnOnce() -> String,
{
    let should_error = op_ctx.is_unset || (check_empty && op_ctx.is_empty);
    if should_error {
        let message = if let Some(expand_fn) = expand_message {
            expand_fn()
        } else {
            format!("{}: parameter null or not set", parameter)
        };
        ErrorIfUnsetResult {
            value: None,
            error_message: Some(format!("bash: {}\n", message)),
        }
    } else {
        ErrorIfUnsetResult {
            value: Some(op_ctx.effective_value.clone()),
            error_message: None,
        }
    }
}

/// Handle UseAlternative operation: ${param:+word}
/// Returns the alternative value if the variable is set (and non-empty if check_empty is true).
pub fn handle_use_alternative<F>(
    op_ctx: &ParameterOpContext,
    check_empty: bool,
    expand_alternative: F,
) -> String
where
    F: FnOnce() -> String,
{
    if should_use_alternative(op_ctx, check_empty) {
        expand_alternative()
    } else {
        String::new()
    }
}

/// Handle Indirection operation: ${!param}
/// Returns the value of the variable whose name is stored in param.
pub fn handle_indirection(
    state: &InterpreterState,
    parameter: &str,
) -> Result<String, String> {
    let target_name = get_variable(state, parameter);

    if target_name.is_empty() {
        return Ok(String::new());
    }

    // Validate target name
    let valid_name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\[.+\])?$").unwrap();
    if !valid_name_re.is_match(&target_name) {
        return Err(format!("bash: ${{{}}}: bad substitution", parameter));
    }

    Ok(get_variable(state, &target_name))
}

/// Handle Length operation with special cases: ${#param}
/// For arrays, returns the number of elements.
/// For strings, returns the character count.
/// Handles special cases like FUNCNAME and BASH_LINENO.
pub fn get_parameter_length_extended(
    state: &InterpreterState,
    parameter: &str,
    is_array_subscript: bool,
) -> usize {
    // Check for array subscript
    let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[([@*])\]$").unwrap();
    if let Some(caps) = array_re.captures(parameter) {
        let array_name = caps.get(1).unwrap().as_str();
        let elements = get_array_elements(state, array_name);
        return elements.len();
    }

    // Check for scalar variable with array subscript syntax (returns 1 if set)
    if is_array_subscript {
        let value = get_variable(state, parameter);
        if !value.is_empty() {
            return 1;
        }
        return 0;
    }

    // Regular variable - return string length
    let value = get_variable(state, parameter);
    value.chars().count()
}

/// Apply substring extraction with array support.
/// For arrays, applies to each element.
pub fn apply_substring_to_array(
    elements: &[(String, String)],
    offset: i64,
    length: Option<i64>,
) -> Result<Vec<String>, String> {
    let mut results = Vec::new();
    for (_, value) in elements {
        results.push(apply_substring_op(value, offset, length)?);
    }
    Ok(results)
}

/// Apply case modification to array elements.
pub fn apply_case_modification_to_array(
    elements: &[(String, String)],
    operator: &str,
    pattern: Option<&str>,
) -> Vec<String> {
    elements
        .iter()
        .map(|(_, value)| {
            if let Some(_pat) = pattern {
                // Pattern-based case modification (e.g., ${var^^[aeiou]})
                // For now, apply to all characters matching the pattern
                // This is a simplified implementation
                apply_case_modification(value, operator)
            } else {
                apply_case_modification(value, operator)
            }
        })
        .collect()
}

/// Apply transform operation with additional operators.
/// Supports: Q, P, a, A, E, K, k, u, U, L
pub fn apply_transform_op_extended(
    state: &InterpreterState,
    parameter: &str,
    value: &str,
    operator: &str,
) -> String {
    match operator {
        "Q" => quote_value(value),
        "P" => expand_prompt(state, value),
        "a" => get_variable_attributes(state, parameter),
        "A" => {
            // Assignment format: declare -a name='(values)' or name='value'
            let attrs = get_variable_attributes(state, parameter);
            if attrs.contains('a') || attrs.contains('A') {
                let elements = get_array_elements(state, parameter);
                let values: Vec<String> = elements.iter().map(|(_, v)| format!("\"{}\"", v)).collect();
                format!("declare -{} {}=({})", attrs, parameter, values.join(" "))
            } else {
                format!("declare -{} {}=\"{}\"", attrs, parameter, value)
            }
        }
        "E" => {
            // Escape expansion (like $'...')
            // Process escape sequences
            let mut result = String::new();
            let mut chars = value.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('r') => result.push('\r'),
                        Some('\\') => result.push('\\'),
                        Some('\'') => result.push('\''),
                        Some('"') => result.push('"'),
                        Some('a') => result.push('\x07'),
                        Some('b') => result.push('\x08'),
                        Some('e') | Some('E') => result.push('\x1b'),
                        Some('f') => result.push('\x0c'),
                        Some('v') => result.push('\x0b'),
                        Some(other) => {
                            result.push('\\');
                            result.push(other);
                        }
                        None => result.push('\\'),
                    }
                } else {
                    result.push(c);
                }
            }
            result
        }
        "K" | "k" => {
            // Quoted key-value format for associative arrays
            // K: "key" "value", k: key value
            let elements = get_array_elements(state, parameter);
            let pairs: Vec<String> = elements
                .iter()
                .map(|(idx, val)| {
                    let key = match idx {
                        crate::interpreter::expansion::ArrayIndex::Numeric(n) => n.to_string(),
                        crate::interpreter::expansion::ArrayIndex::String(s) => s.clone(),
                    };
                    if operator == "K" {
                        format!("\"{}\" \"{}\"", key, val)
                    } else {
                        format!("{} {}", key, val)
                    }
                })
                .collect();
            pairs.join(" ")
        }
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
