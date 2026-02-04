//! Array Expansion with Prefix/Suffix Handlers
//!
//! Handles array expansions that have adjacent text in double quotes:
//! - "${prefix}${arr[@]#pattern}${suffix}" - pattern removal with prefix/suffix
//! - "${prefix}${arr[@]/pattern/replacement}${suffix}" - pattern replacement with prefix/suffix
//! - "${prefix}${arr[@]}${suffix}" - simple array expansion with prefix/suffix
//! - "${arr[@]:-${default[@]}}" - array default/alternative values

use crate::interpreter::expansion::{
    apply_pattern_removal, get_array_elements, get_variable, is_variable_set, pattern_to_regex,
    PatternRemovalSide,
};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Result type for array expansion handlers.
#[derive(Debug, Clone)]
pub struct ArrayPrefixSuffixResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Apply prefix and suffix to array elements.
/// For [@], prefix is joined to first element, suffix to last.
/// For [*], all elements are joined with IFS, then prefix and suffix are added.
pub fn apply_prefix_suffix_to_array(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    prefix: &str,
    suffix: &str,
) -> ArrayPrefixSuffixResult {
    // Get array elements
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    // If no elements, check for scalar (treat as single-element array)
    if values.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            // Scalar treated as single-element array
            return ArrayPrefixSuffixResult {
                values: vec![format!("{}{}{}", prefix, scalar_value, suffix)],
                quoted: true,
            };
        }
        // Variable is unset or empty array
        if is_star {
            // "${arr[*]}" with empty array produces one empty word (prefix + "" + suffix)
            return ArrayPrefixSuffixResult {
                values: vec![format!("{}{}", prefix, suffix)],
                quoted: true,
            };
        }
        // "${arr[@]}" with empty array produces no words (unless there's prefix/suffix)
        let combined = format!("{}{}", prefix, suffix);
        return ArrayPrefixSuffixResult {
            values: if combined.is_empty() {
                vec![]
            } else {
                vec![combined]
            },
            quoted: true,
        };
    }

    if is_star {
        // "${arr[*]}" - join all elements with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${arr[@]}" - each element is a separate word
    // Join prefix with first, suffix with last
    if values.len() == 1 {
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(values.len());
    result.push(format!("{}{}", prefix, values[0]));
    for v in &values[1..values.len() - 1] {
        result.push(v.clone());
    }
    result.push(format!("{}{}", values[values.len() - 1], suffix));

    ArrayPrefixSuffixResult {
        values: result,
        quoted: true,
    }
}

/// Apply pattern removal with prefix/suffix to array elements.
pub fn apply_pattern_removal_with_prefix_suffix(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    prefix: &str,
    suffix: &str,
    regex_str: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> ArrayPrefixSuffixResult {
    // Get array elements
    let elements = get_array_elements(state, array_name);
    let mut values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    // If no elements, check for scalar (treat as single-element array)
    if values.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            values = vec![scalar_value.clone()];
        } else {
            // Variable is unset or empty array
            if is_star {
                return ArrayPrefixSuffixResult {
                    values: vec![format!("{}{}", prefix, suffix)],
                    quoted: true,
                };
            }
            let combined = format!("{}{}", prefix, suffix);
            return ArrayPrefixSuffixResult {
                values: if combined.is_empty() {
                    vec![]
                } else {
                    vec![combined]
                },
                quoted: true,
            };
        }
    }

    // Apply pattern removal to each element
    let values: Vec<String> = values
        .iter()
        .map(|v| apply_pattern_removal(v, regex_str, side, greedy))
        .collect();

    if is_star {
        // "${arr[*]#...}" - join all elements with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${arr[@]#...}" - each element is a separate word
    if values.len() == 1 {
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(values.len());
    result.push(format!("{}{}", prefix, values[0]));
    for v in &values[1..values.len() - 1] {
        result.push(v.clone());
    }
    result.push(format!("{}{}", values[values.len() - 1], suffix));

    ArrayPrefixSuffixResult {
        values: result,
        quoted: true,
    }
}

/// Apply pattern replacement with prefix/suffix to array elements.
pub fn apply_pattern_replacement_with_prefix_suffix(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    prefix: &str,
    suffix: &str,
    regex_pattern: &str,
    replacement: &str,
    replace_all: bool,
) -> ArrayPrefixSuffixResult {
    // Get array elements
    let elements = get_array_elements(state, array_name);
    let mut values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    // If no elements, check for scalar (treat as single-element array)
    if values.is_empty() {
        if let Some(scalar_value) = state.env.get(array_name) {
            values = vec![scalar_value.clone()];
        } else {
            // Variable is unset or empty array
            if is_star {
                return ArrayPrefixSuffixResult {
                    values: vec![format!("{}{}", prefix, suffix)],
                    quoted: true,
                };
            }
            let combined = format!("{}{}", prefix, suffix);
            return ArrayPrefixSuffixResult {
                values: if combined.is_empty() {
                    vec![]
                } else {
                    vec![combined]
                },
                quoted: true,
            };
        }
    }

    // Apply pattern replacement to each element
    let values: Vec<String> = match Regex::new(regex_pattern) {
        Ok(re) => values
            .iter()
            .map(|v| {
                if replace_all {
                    re.replace_all(v, replacement).to_string()
                } else {
                    re.replace(v, replacement).to_string()
                }
            })
            .collect(),
        Err(_) => values,
    };

    if is_star {
        // "${arr[*]/...}" - join all elements with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${arr[@]/...}" - each element is a separate word
    if values.len() == 1 {
        return ArrayPrefixSuffixResult {
            values: vec![format!("{}{}{}", prefix, values[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(values.len());
    result.push(format!("{}{}", prefix, values[0]));
    for v in &values[1..values.len() - 1] {
        result.push(v.clone());
    }
    result.push(format!("{}{}", values[values.len() - 1], suffix));

    ArrayPrefixSuffixResult {
        values: result,
        quoted: true,
    }
}

/// Handle array default value expansion.
/// Returns the default array elements if the main array is unset/empty.
pub fn handle_array_default_value(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    default_array_name: &str,
    default_is_star: bool,
    check_empty: bool,
    use_alternative: bool,
) -> Option<ArrayPrefixSuffixResult> {
    let elements = get_array_elements(state, array_name);
    let is_set = !elements.is_empty() || state.env.contains_key(array_name);
    let is_empty = elements.is_empty()
        || (elements.len() == 1 && elements.iter().all(|(_, v)| v.is_empty()));

    let should_use_alternate = if use_alternative {
        is_set && !(check_empty && is_empty)
    } else {
        !is_set || (check_empty && is_empty)
    };

    // If not using alternate, return the original array value
    if !should_use_alternate {
        if !elements.is_empty() {
            let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();
            if is_star {
                let ifs_sep = get_ifs_separator(&state.env);
                return Some(ArrayPrefixSuffixResult {
                    values: vec![values.join(ifs_sep)],
                    quoted: true,
                });
            }
            return Some(ArrayPrefixSuffixResult {
                values,
                quoted: true,
            });
        }
        if let Some(scalar_value) = state.env.get(array_name) {
            return Some(ArrayPrefixSuffixResult {
                values: vec![scalar_value.clone()],
                quoted: true,
            });
        }
        return Some(ArrayPrefixSuffixResult {
            values: vec![],
            quoted: true,
        });
    }

    // Use the default array
    let default_elements = get_array_elements(state, default_array_name);
    if !default_elements.is_empty() {
        let values: Vec<String> = default_elements.into_iter().map(|(_, v)| v).collect();
        if default_is_star || is_star {
            let ifs_sep = get_ifs_separator(&state.env);
            return Some(ArrayPrefixSuffixResult {
                values: vec![values.join(ifs_sep)],
                quoted: true,
            });
        }
        return Some(ArrayPrefixSuffixResult {
            values,
            quoted: true,
        });
    }

    // Default array is empty - check for scalar
    if let Some(scalar_value) = state.env.get(default_array_name) {
        return Some(ArrayPrefixSuffixResult {
            values: vec![scalar_value.clone()],
            quoted: true,
        });
    }

    // Default is unset
    Some(ArrayPrefixSuffixResult {
        values: vec![],
        quoted: true,
    })
}

/// Handle scalar variable default value with array default.
pub fn handle_scalar_default_with_array(
    state: &InterpreterState,
    var_name: &str,
    default_array_name: &str,
    default_is_star: bool,
    check_empty: bool,
    use_alternative: bool,
) -> Option<ArrayPrefixSuffixResult> {
    let is_set = is_variable_set(state, var_name);
    let var_value = get_variable(state, var_name);
    let is_empty = var_value.is_empty();

    let should_use_alternate = if use_alternative {
        is_set && !(check_empty && is_empty)
    } else {
        !is_set || (check_empty && is_empty)
    };

    // If not using alternate, return the scalar value
    if !should_use_alternate {
        return Some(ArrayPrefixSuffixResult {
            values: vec![var_value],
            quoted: true,
        });
    }

    // Use the default array
    let default_elements = get_array_elements(state, default_array_name);
    if !default_elements.is_empty() {
        let values: Vec<String> = default_elements.into_iter().map(|(_, v)| v).collect();
        if default_is_star {
            let ifs_sep = get_ifs_separator(&state.env);
            return Some(ArrayPrefixSuffixResult {
                values: vec![values.join(ifs_sep)],
                quoted: true,
            });
        }
        return Some(ArrayPrefixSuffixResult {
            values,
            quoted: true,
        });
    }

    // Default array is empty - check for scalar
    if let Some(scalar_value) = state.env.get(default_array_name) {
        return Some(ArrayPrefixSuffixResult {
            values: vec![scalar_value.clone()],
            quoted: true,
        });
    }

    // Default is unset
    Some(ArrayPrefixSuffixResult {
        values: vec![],
        quoted: true,
    })
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
    fn test_prefix_suffix_at() {
        let state = make_state();
        let result = apply_prefix_suffix_to_array(&state, "arr", false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-hello", "world", "foo-suf"]);
    }

    #[test]
    fn test_prefix_suffix_star() {
        let state = make_state();
        let result = apply_prefix_suffix_to_array(&state, "arr", true, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-hello world foo-suf"]);
    }

    #[test]
    fn test_prefix_suffix_single_element() {
        let mut state = make_state();
        state.env.clear();
        state.env.insert("single_0".to_string(), "only".to_string());
        let result = apply_prefix_suffix_to_array(&state, "single", false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-only-suf"]);
    }

    #[test]
    fn test_prefix_suffix_empty_array() {
        let state = InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        };
        let result = apply_prefix_suffix_to_array(&state, "empty", false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre--suf"]);
    }

    #[test]
    fn test_pattern_removal_with_prefix_suffix() {
        let state = make_state();
        // Pattern "h*o" matches "hello" entirely (h + ell + o), leaving ""
        let regex = pattern_to_regex("h*o", false, false);
        let result = apply_pattern_removal_with_prefix_suffix(
            &state,
            "arr",
            false,
            "pre-",
            "-suf",
            &regex,
            PatternRemovalSide::Prefix,
            false,
        );
        // "hello" -> "" (h*o matches "hello"), "world" -> "world", "foo" -> "foo"
        // With prefix/suffix: ["pre-", "world", "foo-suf"]
        assert_eq!(result.values, vec!["pre-", "world", "foo-suf"]);
    }
}
