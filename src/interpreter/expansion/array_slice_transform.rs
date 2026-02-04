//! Array Slicing and Transform Operations
//!
//! Handles array expansion with slicing and transform operators:
//! - "${arr[@]:offset}" and "${arr[@]:offset:length}" - array slicing
//! - "${arr[@]@a}", "${arr[@]@P}", "${arr[@]@Q}" - transform operations

use crate::interpreter::expansion::{
    expand_prompt, get_array_elements, get_variable_attributes, quote_value,
};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;

/// Apply array slicing operation.
/// offset and length should be pre-evaluated (arithmetic evaluation requires async).
pub fn apply_array_slicing(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    offset: i64,
    length: Option<i64>,
) -> Result<Vec<String>, String> {
    // Slicing associative arrays doesn't make sense - error out
    if state
        .associative_arrays
        .as_ref()
        .map(|aa| aa.contains(array_name))
        .unwrap_or(false)
    {
        return Err(format!(
            "bash: ${{{}[@]: 0: 3}}: bad substitution",
            array_name
        ));
    }

    // Get array elements (sorted by index)
    let elements = get_array_elements(state, array_name);

    // For sparse arrays, offset refers to index position, not element position
    // Find the first element whose index >= offset (or computed index for negative offset)
    let start_idx: usize;
    if offset < 0 {
        // Negative offset: count from maxIndex + 1
        if !elements.is_empty() {
            let last_idx = match &elements[elements.len() - 1].0 {
                crate::interpreter::expansion::ArrayIndex::Numeric(n) => *n,
                _ => 0,
            };
            let target_index = last_idx + 1 + offset;
            // If target index is negative, return empty (out of bounds)
            if target_index < 0 {
                return Ok(vec![]);
            }
            // Find first element with index >= targetIndex
            start_idx = elements
                .iter()
                .position(|(idx, _)| match idx {
                    crate::interpreter::expansion::ArrayIndex::Numeric(n) => *n >= target_index,
                    _ => false,
                })
                .unwrap_or(elements.len());
        } else {
            start_idx = 0;
        }
    } else {
        // Positive offset: find first element with index >= offset
        start_idx = elements
            .iter()
            .position(|(idx, _)| match idx {
                crate::interpreter::expansion::ArrayIndex::Numeric(n) => *n >= offset,
                _ => false,
            })
            .unwrap_or(elements.len());
    }

    let sliced_values: Vec<String> = if let Some(len) = length {
        if len < 0 {
            // Negative length is an error for array slicing in bash
            return Err(format!(
                "{}[@]: substring expression < 0",
                array_name
            ));
        }
        // Take 'length' elements starting from start_idx
        elements
            .iter()
            .skip(start_idx)
            .take(len as usize)
            .map(|(_, v)| v.clone())
            .collect()
    } else {
        // Take all elements starting from start_idx
        elements.iter().skip(start_idx).map(|(_, v)| v.clone()).collect()
    };

    if sliced_values.is_empty() {
        return Ok(vec![]);
    }

    if is_star {
        // "${arr[*]:n:m}" - join with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        Ok(vec![sliced_values.join(ifs_sep)])
    } else {
        // "${arr[@]:n:m}" - each element as a separate word
        Ok(sliced_values)
    }
}

/// Apply array transform operations like @a, @P, @Q, @u, @U, @L.
pub fn apply_array_transform(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    operator: &str,
) -> Vec<String> {
    // Get array elements
    let elements = get_array_elements(state, array_name);

    // If no elements, check for scalar (treat as single-element array)
    if elements.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            // Scalar variable - return based on operator
            let result_value = match operator {
                "a" => String::new(), // Scalars have no array attribute
                "P" => expand_prompt(state, scalar_value),
                "Q" => quote_value(scalar_value),
                "u" => {
                    let mut chars = scalar_value.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    }
                }
                "U" => scalar_value.to_uppercase(),
                "L" => scalar_value.to_lowercase(),
                _ => scalar_value.clone(),
            };
            return vec![result_value];
        }
        // Variable is unset
        if is_star {
            return vec![String::new()];
        }
        return vec![];
    }

    // Get the attribute for this array (same for all elements)
    let array_attr = get_variable_attributes(state, array_name);

    // Transform each element based on operator
    let transformed_values: Vec<String> = match operator {
        "a" => {
            // Return attribute letter for each element
            // All elements of the same array have the same attribute
            elements.iter().map(|_| array_attr.clone()).collect()
        }
        "P" => {
            // Apply prompt expansion to each element
            elements
                .iter()
                .map(|(_, v)| expand_prompt(state, v))
                .collect()
        }
        "Q" => {
            // Quote each element
            elements.iter().map(|(_, v)| quote_value(v)).collect()
        }
        "u" => {
            // Capitalize first character only (ucfirst)
            elements
                .iter()
                .map(|(_, v)| {
                    let mut chars = v.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    }
                })
                .collect()
        }
        "U" => {
            // Uppercase all characters
            elements.iter().map(|(_, v)| v.to_uppercase()).collect()
        }
        "L" => {
            // Lowercase all characters
            elements.iter().map(|(_, v)| v.to_lowercase()).collect()
        }
        _ => elements.iter().map(|(_, v)| v.clone()).collect(),
    };

    if is_star {
        // "${arr[*]@X}" - join all values with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        vec![transformed_values.join(ifs_sep)]
    } else {
        // "${arr[@]@X}" - each value as a separate word
        transformed_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("arr_0".to_string(), "hello".to_string());
        env.insert("arr_1".to_string(), "world".to_string());
        env.insert("arr_2".to_string(), "foo".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_array_slicing_basic() {
        let state = make_state();
        let result = apply_array_slicing(&state, "arr", false, 1, None).unwrap();
        assert_eq!(result, vec!["world", "foo"]);
    }

    #[test]
    fn test_array_slicing_with_length() {
        let state = make_state();
        let result = apply_array_slicing(&state, "arr", false, 0, Some(2)).unwrap();
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_array_slicing_star() {
        let state = make_state();
        let result = apply_array_slicing(&state, "arr", true, 0, None).unwrap();
        assert_eq!(result, vec!["hello world foo"]);
    }

    #[test]
    fn test_array_transform_uppercase() {
        let state = make_state();
        let result = apply_array_transform(&state, "arr", false, "U");
        assert_eq!(result, vec!["HELLO", "WORLD", "FOO"]);
    }

    #[test]
    fn test_array_transform_lowercase() {
        let mut state = make_state();
        state.env.insert("upper_0".to_string(), "HELLO".to_string());
        state.env.insert("upper_1".to_string(), "WORLD".to_string());
        let result = apply_array_transform(&state, "upper", false, "L");
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_array_transform_quote() {
        let state = make_state();
        let result = apply_array_transform(&state, "arr", false, "Q");
        assert_eq!(result, vec!["'hello'", "'world'", "'foo'"]);
    }

    #[test]
    fn test_array_transform_star() {
        let state = make_state();
        let result = apply_array_transform(&state, "arr", true, "U");
        assert_eq!(result, vec!["HELLO WORLD FOO"]);
    }
}
