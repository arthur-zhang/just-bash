//! Unquoted Expansion Handlers
//!
//! Provides helper functions for unquoted expansions that need special handling:
//! - Unquoted $@ and $* (with and without prefix/suffix)
//! - Unquoted ${arr[@]} and ${arr[*]}
//! - Unquoted ${@:offset} and ${*:offset} slicing
//! - Unquoted ${@#pattern} and ${*#pattern} pattern removal
//! - Unquoted ${arr[@]/pattern/replacement} pattern replacement
//! - IFS splitting and glob expansion for unquoted contexts

use crate::interpreter::expansion::{
    apply_pattern_removal, expand_glob_pattern, get_array_elements, get_var_names_with_prefix,
    has_glob_pattern, PatternRemovalSide,
};
use crate::interpreter::helpers::{
    get_ifs, get_ifs_separator, is_ifs_empty, split_by_ifs_for_expansion,
};
use crate::interpreter::InterpreterState;
use regex_lite::Regex;
use std::path::Path;

/// Result type for unquoted expansion handlers.
#[derive(Debug, Clone)]
pub struct UnquotedExpansionResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Split a value by IFS for unquoted expansion.
/// This is used when expanding unquoted variables, command substitutions, etc.
pub fn split_unquoted_value(value: &str, state: &InterpreterState) -> Vec<String> {
    let ifs = get_ifs(&state.env);
    split_by_ifs_for_expansion(value, ifs)
}

/// Expand unquoted array ${arr[@]} or ${arr[*]}.
/// For [@], each element is split by IFS.
/// For [*], all elements are joined with IFS first char, then split.
pub fn expand_unquoted_array(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    if values.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${arr[*]} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = values.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${arr[@]} - each element is split by IFS
        let mut result = Vec::new();
        for value in values {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted positional parameters $@ or $*.
/// For $@, each parameter is split by IFS.
/// For $*, all parameters are joined with IFS first char, then split.
pub fn expand_unquoted_positional(
    state: &InterpreterState,
    is_star: bool,
) -> UnquotedExpansionResult {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    if is_star {
        // $* - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = params.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // $@ - each parameter is split by IFS
        let mut result = Vec::new();
        for param in params {
            let split_values = split_unquoted_value(&param, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted ${!prefix@} or ${!prefix*} - variable name prefix expansion.
pub fn expand_unquoted_var_name_prefix(
    state: &InterpreterState,
    prefix: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let names = get_var_names_with_prefix(state, prefix);

    if names.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${!prefix*} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = names.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${!prefix@} - each name is split by IFS
        let mut result = Vec::new();
        for name in names {
            let split_values = split_unquoted_value(&name, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted ${!arr[@]} or ${!arr[*]} - array keys expansion.
pub fn expand_unquoted_array_keys(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let keys: Vec<String> = elements
        .iter()
        .map(|(idx, _)| match idx {
            crate::interpreter::expansion::ArrayIndex::Numeric(n) => n.to_string(),
            crate::interpreter::expansion::ArrayIndex::String(s) => s.clone(),
        })
        .collect();

    if keys.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    if is_star {
        // ${!arr[*]} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = keys.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${!arr[@]} - each key is split by IFS
        let mut result = Vec::new();
        for key in keys {
            let split_values = split_unquoted_value(&key, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted array with pattern removal ${arr[@]#pattern} or ${arr[@]##pattern}.
pub fn expand_unquoted_array_pattern_removal(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    pattern_regex: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    if values.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    // Apply pattern removal to each element
    let processed: Vec<String> = values
        .iter()
        .map(|v| apply_pattern_removal(v, pattern_regex, side, greedy))
        .collect();

    if is_star {
        // ${arr[*]#...} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = processed.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${arr[@]#...} - each element is split by IFS
        let mut result = Vec::new();
        for value in processed {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted array with pattern replacement ${arr[@]/pattern/replacement}.
pub fn expand_unquoted_array_pattern_replacement(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    pattern_regex: &str,
    replacement: &str,
    replace_all: bool,
    anchor_start: bool,
    anchor_end: bool,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    if values.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    // Build final pattern with anchors
    let final_pattern = if anchor_start {
        format!("^{}", pattern_regex)
    } else if anchor_end {
        format!("{}$", pattern_regex)
    } else {
        pattern_regex.to_string()
    };

    // Apply pattern replacement to each element
    let processed: Vec<String> = match Regex::new(&final_pattern) {
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
        // ${arr[*]/...} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = processed.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${arr[@]/...} - each element is split by IFS
        let mut result = Vec::new();
        for value in processed {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted positional parameters with slicing ${@:offset} or ${*:offset}.
pub fn expand_unquoted_positional_slice(
    state: &InterpreterState,
    is_star: bool,
    offset: i64,
    length: Option<i64>,
) -> UnquotedExpansionResult {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    // Calculate start position
    let start = if offset < 0 {
        let computed = params.len() as i64 + offset;
        if computed < 0 { 0 } else { computed as usize }
    } else {
        offset as usize
    };

    // Calculate end position
    let end = match length {
        Some(len) if len < 0 => {
            let computed = params.len() as i64 + len;
            if computed < start as i64 {
                return UnquotedExpansionResult {
                    values: vec![],
                    quoted: false,
                };
            }
            computed as usize
        }
        Some(len) => (start + len as usize).min(params.len()),
        None => params.len(),
    };

    if start >= params.len() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let sliced: Vec<String> = params[start..end].to_vec();

    if is_star {
        // ${*:offset} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = sliced.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${@:offset} - each parameter is split by IFS
        let mut result = Vec::new();
        for param in sliced {
            let split_values = split_unquoted_value(&param, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted array with slicing ${arr[@]:offset} or ${arr[@]:offset:length}.
pub fn expand_unquoted_array_slice(
    state: &InterpreterState,
    array_name: &str,
    is_star: bool,
    offset: i64,
    length: Option<i64>,
) -> UnquotedExpansionResult {
    let elements = get_array_elements(state, array_name);
    let values: Vec<String> = elements.into_iter().map(|(_, v)| v).collect();

    if values.is_empty() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    // Calculate start position
    let start = if offset < 0 {
        let computed = values.len() as i64 + offset;
        if computed < 0 { 0 } else { computed as usize }
    } else {
        offset as usize
    };

    // Calculate end position
    let end = match length {
        Some(len) if len < 0 => {
            let computed = values.len() as i64 + len;
            if computed < start as i64 {
                return UnquotedExpansionResult {
                    values: vec![],
                    quoted: false,
                };
            }
            computed as usize
        }
        Some(len) => (start + len as usize).min(values.len()),
        None => values.len(),
    };

    if start >= values.len() {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let sliced: Vec<String> = values[start..end].to_vec();

    if is_star {
        // ${arr[*]:offset} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = sliced.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${arr[@]:offset} - each element is split by IFS
        let mut result = Vec::new();
        for value in sliced {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted positional parameters with pattern removal ${@#pattern} or ${*#pattern}.
pub fn expand_unquoted_positional_pattern_removal(
    state: &InterpreterState,
    is_star: bool,
    pattern_regex: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> UnquotedExpansionResult {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        return UnquotedExpansionResult {
            values: vec![],
            quoted: false,
        };
    }

    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    // Apply pattern removal to each positional parameter
    let processed: Vec<String> = params
        .iter()
        .map(|p| apply_pattern_removal(p, pattern_regex, side, greedy))
        .collect();

    if is_star {
        // ${*#...} - join with IFS first char, then split
        let ifs_sep = get_ifs_separator(&state.env);
        let joined = processed.join(ifs_sep);
        let split_values = split_unquoted_value(&joined, state);
        UnquotedExpansionResult {
            values: split_values,
            quoted: false,
        }
    } else {
        // ${@#...} - each parameter is split by IFS
        let mut result = Vec::new();
        for value in processed {
            let split_values = split_unquoted_value(&value, state);
            result.extend(split_values);
        }
        UnquotedExpansionResult {
            values: result,
            quoted: false,
        }
    }
}

/// Expand unquoted positional parameters with prefix/suffix (e.g., =$@= or =$*=).
/// Note: `_is_star` is kept for API consistency but both $@ and $* behave identically
/// when unquoted with prefix/suffix.
pub fn expand_unquoted_positional_with_prefix_suffix(
    state: &InterpreterState,
    _is_star: bool,
    prefix: &str,
    suffix: &str,
    cwd: &Path,
    noglob: bool,
    failglob: bool,
    nullglob: bool,
    extglob: bool,
) -> Result<UnquotedExpansionResult, String> {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        // No params - just return prefix+suffix if non-empty
        let combined = format!("{}{}", prefix, suffix);
        if combined.is_empty() {
            return Ok(UnquotedExpansionResult {
                values: vec![],
                quoted: false,
            });
        }
        return Ok(UnquotedExpansionResult {
            values: vec![combined],
            quoted: false,
        });
    }

    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    // Both unquoted $@ and unquoted $* behave the same way:
    // Each param becomes a separate word, then each is subject to IFS splitting.
    // First, attach prefix to first param, suffix to last param
    let mut raw_words: Vec<String> = Vec::new();
    for (i, param) in params.iter().enumerate() {
        let mut word = param.clone();
        if i == 0 {
            word = format!("{}{}", prefix, word);
        }
        if i == params.len() - 1 {
            word = format!("{}{}", word, suffix);
        }
        raw_words.push(word);
    }

    // Now apply IFS splitting
    let ifs = get_ifs(&state.env);
    let ifs_empty = is_ifs_empty(&state.env);

    let mut words: Vec<String> = if ifs_empty {
        // Empty IFS - no splitting, filter out empty words
        raw_words.into_iter().filter(|w| !w.is_empty()).collect()
    } else {
        let mut result = Vec::new();
        for word in raw_words {
            if word.is_empty() {
                continue;
            }
            let parts = split_by_ifs_for_expansion(&word, ifs);
            result.extend(parts);
        }
        result
    };

    // Apply glob expansion to each word
    if !words.is_empty() {
        words = apply_glob_expansion(&words, cwd, noglob, failglob, nullglob, extglob)?;
    }

    Ok(UnquotedExpansionResult {
        values: words,
        quoted: false,
    })
}

/// Apply glob expansion to a list of values.
/// If noglob is set, returns the values unchanged.
/// If a pattern has no matches and failglob is set, returns an error.
/// If a pattern has no matches and nullglob is set, the pattern is dropped.
/// Otherwise, returns the pattern unchanged.
pub fn apply_glob_expansion(
    values: &[String],
    cwd: &Path,
    noglob: bool,
    failglob: bool,
    nullglob: bool,
    extglob: bool,
) -> Result<Vec<String>, String> {
    if noglob {
        return Ok(values.to_vec());
    }

    let mut expanded: Vec<String> = Vec::new();
    for value in values {
        if has_glob_pattern(value, extglob) {
            let result = expand_glob_pattern(value, cwd, failglob, nullglob, extglob)?;
            if result.values.is_empty() && !nullglob {
                // No matches and not nullglob - keep original
                expanded.push(value.clone());
            } else {
                expanded.extend(result.values);
            }
        } else {
            expanded.push(value.clone());
        }
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::expansion::pattern_to_regex;
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
    fn test_split_unquoted_value() {
        let state = InterpreterState::default();
        let result = split_unquoted_value("hello world", &state);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_array_at() {
        let state = make_state_with_array("arr", &["a b", "c d"]);
        let result = expand_unquoted_array(&state, "arr", false);
        // Each element is split by IFS
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_array_star() {
        let state = make_state_with_array("arr", &["a b", "c d"]);
        let result = expand_unquoted_array(&state, "arr", true);
        // Join with space, then split: "a b c d" -> ["a", "b", "c", "d"]
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_positional_at() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "a b".to_string());
        state.env.insert("2".to_string(), "c d".to_string());

        let result = expand_unquoted_positional(&state, false);
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_positional_star() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "a b".to_string());
        state.env.insert("2".to_string(), "c d".to_string());

        let result = expand_unquoted_positional(&state, true);
        assert_eq!(result.values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_expand_unquoted_var_name_prefix() {
        let mut state = InterpreterState::default();
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());
        state.env.insert("PWD".to_string(), "/home".to_string());

        let result = expand_unquoted_var_name_prefix(&state, "P", false);
        assert!(result.values.contains(&"PATH".to_string()));
        assert!(result.values.contains(&"PWD".to_string()));
    }

    #[test]
    fn test_expand_unquoted_array_pattern_removal() {
        let state = make_state_with_array("arr", &["hello", "world"]);
        let regex = pattern_to_regex("h*", false, false);
        let result = expand_unquoted_array_pattern_removal(
            &state,
            "arr",
            false,
            &regex,
            PatternRemovalSide::Prefix,
            false,
        );
        // "hello" -> "ello", "world" -> "world"
        assert_eq!(result.values, vec!["ello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_array_slice() {
        let state = make_state_with_array("arr", &["a", "b", "c", "d"]);
        let result = expand_unquoted_array_slice(&state, "arr", false, 1, Some(2));
        assert_eq!(result.values, vec!["b", "c"]);
    }

    #[test]
    fn test_expand_unquoted_positional_slice() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "4".to_string());
        state.env.insert("1".to_string(), "a".to_string());
        state.env.insert("2".to_string(), "b".to_string());
        state.env.insert("3".to_string(), "c".to_string());
        state.env.insert("4".to_string(), "d".to_string());

        let result = expand_unquoted_positional_slice(&state, false, 1, Some(2));
        assert_eq!(result.values, vec!["b", "c"]);
    }

    #[test]
    fn test_expand_unquoted_positional_pattern_removal_at() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "hello".to_string());
        state.env.insert("2".to_string(), "world".to_string());

        let regex = pattern_to_regex("h*", false, false);
        let result = expand_unquoted_positional_pattern_removal(
            &state,
            false,
            &regex,
            PatternRemovalSide::Prefix,
            false,
        );
        // "hello" -> "ello", "world" -> "world"
        assert_eq!(result.values, vec!["ello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_positional_pattern_removal_star() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "hello".to_string());
        state.env.insert("2".to_string(), "world".to_string());

        let regex = pattern_to_regex("o*", false, false);
        let result = expand_unquoted_positional_pattern_removal(
            &state,
            true,
            &regex,
            PatternRemovalSide::Suffix,
            false,
        );
        // "hello" -> "hell", "world" -> "w" (non-greedy suffix removal)
        // Joined with space: "hell w", then split: ["hell", "w"]
        assert_eq!(result.values, vec!["hell", "w"]);
    }

    #[test]
    fn test_expand_unquoted_positional_pattern_removal_empty() {
        let state = InterpreterState::default();
        let regex = pattern_to_regex("*", true, false);
        let result = expand_unquoted_positional_pattern_removal(
            &state,
            false,
            &regex,
            PatternRemovalSide::Prefix,
            true,
        );
        assert_eq!(result.values, Vec::<String>::new());
    }

    #[test]
    fn test_expand_unquoted_positional_with_prefix_suffix() {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), "2".to_string());
        state.env.insert("1".to_string(), "a".to_string());
        state.env.insert("2".to_string(), "b".to_string());

        let cwd = std::env::current_dir().unwrap();
        let result = expand_unquoted_positional_with_prefix_suffix(
            &state,
            false,
            "=",
            "=",
            &cwd,
            true, // noglob
            false,
            false,
            false,
        )
        .unwrap();
        // "=a" and "b=" -> ["=a", "b="]
        assert_eq!(result.values, vec!["=a", "b="]);
    }

    #[test]
    fn test_expand_unquoted_positional_with_prefix_suffix_no_params() {
        let state = InterpreterState::default();
        let cwd = std::env::current_dir().unwrap();
        let result = expand_unquoted_positional_with_prefix_suffix(
            &state,
            false,
            "pre",
            "suf",
            &cwd,
            true,
            false,
            false,
            false,
        )
        .unwrap();
        // No params - just return prefix+suffix
        assert_eq!(result.values, vec!["presuf"]);
    }

    #[test]
    fn test_expand_unquoted_positional_with_prefix_suffix_empty() {
        let state = InterpreterState::default();
        let cwd = std::env::current_dir().unwrap();
        let result = expand_unquoted_positional_with_prefix_suffix(
            &state,
            false,
            "",
            "",
            &cwd,
            true,
            false,
            false,
            false,
        )
        .unwrap();
        assert_eq!(result.values, Vec::<String>::new());
    }

    #[test]
    fn test_apply_glob_expansion_noglob() {
        let cwd = std::env::current_dir().unwrap();
        let values = vec!["*.txt".to_string(), "hello".to_string()];
        let result = apply_glob_expansion(&values, &cwd, true, false, false, false).unwrap();
        // noglob - values unchanged
        assert_eq!(result, vec!["*.txt", "hello"]);
    }

    #[test]
    fn test_apply_glob_expansion_no_pattern() {
        let cwd = std::env::current_dir().unwrap();
        let values = vec!["hello".to_string(), "world".to_string()];
        let result = apply_glob_expansion(&values, &cwd, false, false, false, false).unwrap();
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_array_pattern_replacement() {
        let state = make_state_with_array("arr", &["hello", "world"]);
        let result = expand_unquoted_array_pattern_replacement(
            &state,
            "arr",
            false,
            "o",
            "0",
            false,
            false,
            false,
        );
        // Replace first 'o' with '0': "hello" -> "hell0", "world" -> "w0rld"
        assert_eq!(result.values, vec!["hell0", "w0rld"]);
    }

    #[test]
    fn test_expand_unquoted_array_pattern_replacement_all() {
        let state = make_state_with_array("arr", &["hello", "hello"]);
        let result = expand_unquoted_array_pattern_replacement(
            &state,
            "arr",
            false,
            "l",
            "L",
            true, // replace_all
            false,
            false,
        );
        // Replace all 'l' with 'L': "hello" -> "heLLo"
        assert_eq!(result.values, vec!["heLLo", "heLLo"]);
    }

    #[test]
    fn test_expand_unquoted_array_pattern_replacement_anchor_start() {
        let state = make_state_with_array("arr", &["hello", "world"]);
        let result = expand_unquoted_array_pattern_replacement(
            &state,
            "arr",
            false,
            "h",
            "H",
            false,
            true, // anchor_start
            false,
        );
        // Anchor start: "hello" -> "Hello", "world" (no h at start) -> "world"
        assert_eq!(result.values, vec!["Hello", "world"]);
    }

    #[test]
    fn test_expand_unquoted_array_pattern_replacement_star() {
        let mut state = InterpreterState::default();
        // Create array with elements containing spaces
        state.env.insert("arr_0".to_string(), "a b".to_string());
        state.env.insert("arr_1".to_string(), "c d".to_string());

        let result = expand_unquoted_array_pattern_replacement(
            &state,
            "arr",
            true, // is_star
            "a",
            "x",
            false,
            false,
            false,
        );
        // "a b" -> "x b", "c d" -> "c d"
        // Join with space: "x b c d", then split: ["x", "b", "c", "d"]
        assert_eq!(result.values, vec!["x", "b", "c", "d"]);
    }
}
