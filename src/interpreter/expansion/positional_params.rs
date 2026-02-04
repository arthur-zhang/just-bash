//! Positional Parameter Expansion Handlers
//!
//! Handles $@ and $* expansion with various operations:
//! - "${@:offset}" and "${*:offset}" - slicing
//! - "${@/pattern/replacement}" - pattern replacement
//! - "${@#pattern}" - pattern removal (strip)
//! - "$@" and "$*" with adjacent text

use crate::interpreter::expansion::{apply_pattern_removal, pattern_to_regex, PatternRemovalSide};
use crate::interpreter::helpers::get_ifs_separator;
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Result type for positional parameter expansion handlers.
#[derive(Debug, Clone)]
pub struct PositionalExpansionResult {
    pub values: Vec<String>,
    pub quoted: bool,
}

/// Get positional parameters from state
pub fn get_positional_params(state: &InterpreterState) -> Vec<String> {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }
    params
}

/// Apply positional parameter slicing.
/// offset and length should be pre-evaluated (arithmetic evaluation requires async).
pub fn apply_positional_slicing(
    state: &InterpreterState,
    is_star: bool,
    prefix: &str,
    suffix: &str,
    offset: i64,
    length: Option<i64>,
) -> PositionalExpansionResult {
    // Get positional parameters
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut all_params = Vec::new();
    for i in 1..=num_params {
        all_params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    let shell_name = state.env.get("0").cloned().unwrap_or_else(|| "bash".to_string());

    // Build sliced params array
    let sliced_params: Vec<String> = if offset <= 0 {
        // offset 0: include $0 at position 0
        let mut with_zero = vec![shell_name];
        with_zero.extend(all_params.clone());

        let computed_idx = with_zero.len() as i64 + offset;
        // If negative offset goes beyond array bounds, return empty
        if computed_idx < 0 {
            vec![]
        } else {
            let start_idx = if offset < 0 {
                computed_idx as usize
            } else {
                0
            };
            if let Some(len) = length {
                let end_idx = if len < 0 {
                    (with_zero.len() as i64 + len) as usize
                } else {
                    start_idx + len as usize
                };
                with_zero[start_idx..end_idx.max(start_idx).min(with_zero.len())].to_vec()
            } else {
                with_zero[start_idx..].to_vec()
            }
        }
    } else {
        // offset > 0: start from $<offset>
        let start_idx = (offset - 1) as usize;
        if start_idx >= all_params.len() {
            vec![]
        } else if let Some(len) = length {
            let end_idx = if len < 0 {
                (all_params.len() as i64 + len) as usize
            } else {
                start_idx + len as usize
            };
            all_params[start_idx..end_idx.max(start_idx).min(all_params.len())].to_vec()
        } else {
            all_params[start_idx..].to_vec()
        }
    };

    if sliced_params.is_empty() {
        // No params after slicing -> prefix + suffix as one word
        let combined = format!("{}{}", prefix, suffix);
        return PositionalExpansionResult {
            values: if combined.is_empty() {
                vec![]
            } else {
                vec![combined]
            },
            quoted: true,
        };
    }

    if is_star {
        // "${*:offset}" - join all sliced params with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, sliced_params.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${@:offset}" - each sliced param is a separate word
    if sliced_params.len() == 1 {
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, sliced_params[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(sliced_params.len());
    result.push(format!("{}{}", prefix, sliced_params[0]));
    for p in &sliced_params[1..sliced_params.len() - 1] {
        result.push(p.clone());
    }
    result.push(format!("{}{}", sliced_params[sliced_params.len() - 1], suffix));

    PositionalExpansionResult {
        values: result,
        quoted: true,
    }
}

/// Apply pattern replacement to positional parameters.
/// regex_pattern and replacement should be pre-expanded.
pub fn apply_positional_pattern_replacement(
    state: &InterpreterState,
    is_star: bool,
    prefix: &str,
    suffix: &str,
    regex_pattern: &str,
    replacement: &str,
    replace_all: bool,
    anchor_start: bool,
    anchor_end: bool,
) -> PositionalExpansionResult {
    let params = get_positional_params(state);

    if params.is_empty() {
        let combined = format!("{}{}", prefix, suffix);
        return PositionalExpansionResult {
            values: if combined.is_empty() {
                vec![]
            } else {
                vec![combined]
            },
            quoted: true,
        };
    }

    // Apply anchor modifiers
    let final_pattern = if anchor_start {
        format!("^{}", regex_pattern)
    } else if anchor_end {
        format!("{}$", regex_pattern)
    } else {
        regex_pattern.to_string()
    };

    // Apply replacement to each param
    let replaced_params: Vec<String> = match Regex::new(&final_pattern) {
        Ok(re) => params
            .iter()
            .map(|param| {
                if replace_all {
                    re.replace_all(param, replacement).to_string()
                } else {
                    re.replace(param, replacement).to_string()
                }
            })
            .collect(),
        Err(_) => params,
    };

    if is_star {
        // "${*/...}" - join all params with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, replaced_params.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${@/...}" - each param is a separate word
    if replaced_params.len() == 1 {
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, replaced_params[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(replaced_params.len());
    result.push(format!("{}{}", prefix, replaced_params[0]));
    for p in &replaced_params[1..replaced_params.len() - 1] {
        result.push(p.clone());
    }
    result.push(format!(
        "{}{}",
        replaced_params[replaced_params.len() - 1],
        suffix
    ));

    PositionalExpansionResult {
        values: result,
        quoted: true,
    }
}

/// Apply pattern removal to positional parameters.
/// regex_str should be pre-expanded.
pub fn apply_positional_pattern_removal(
    state: &InterpreterState,
    is_star: bool,
    prefix: &str,
    suffix: &str,
    regex_str: &str,
    side: PatternRemovalSide,
    greedy: bool,
) -> PositionalExpansionResult {
    let params = get_positional_params(state);

    if params.is_empty() {
        let combined = format!("{}{}", prefix, suffix);
        return PositionalExpansionResult {
            values: if combined.is_empty() {
                vec![]
            } else {
                vec![combined]
            },
            quoted: true,
        };
    }

    // Apply pattern removal to each param
    let stripped_params: Vec<String> = params
        .iter()
        .map(|param| apply_pattern_removal(param, regex_str, side, greedy))
        .collect();

    if is_star {
        // "${*#...}" - join all params with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, stripped_params.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "${@#...}" - each param is a separate word
    if stripped_params.len() == 1 {
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, stripped_params[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(stripped_params.len());
    result.push(format!("{}{}", prefix, stripped_params[0]));
    for p in &stripped_params[1..stripped_params.len() - 1] {
        result.push(p.clone());
    }
    result.push(format!(
        "{}{}",
        stripped_params[stripped_params.len() - 1],
        suffix
    ));

    PositionalExpansionResult {
        values: result,
        quoted: true,
    }
}

/// Handle simple "$@" and "$*" expansion with prefix/suffix.
/// "$@": Each positional parameter becomes a separate word, with prefix joined to first
///       and suffix joined to last. If no params, produces nothing (or just prefix+suffix if present)
/// "$*": All params joined with IFS as ONE word. If no params, produces one empty word.
pub fn apply_simple_positional_expansion(
    state: &InterpreterState,
    is_star: bool,
    prefix: &str,
    suffix: &str,
) -> PositionalExpansionResult {
    let num_params: i32 = state
        .env
        .get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if num_params == 0 {
        if is_star {
            // "$*" with no params -> one empty word (prefix + suffix)
            return PositionalExpansionResult {
                values: vec![format!("{}{}", prefix, suffix)],
                quoted: true,
            };
        }
        // "$@" with no params -> no words (unless there's prefix/suffix)
        let combined = format!("{}{}", prefix, suffix);
        return PositionalExpansionResult {
            values: if combined.is_empty() {
                vec![]
            } else {
                vec![combined]
            },
            quoted: true,
        };
    }

    // Get individual positional parameters
    let mut params = Vec::new();
    for i in 1..=num_params {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    if is_star {
        // "$*" - join all params with IFS into one word
        let ifs_sep = get_ifs_separator(&state.env);
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, params.join(ifs_sep), suffix)],
            quoted: true,
        };
    }

    // "$@" - each param is a separate word
    // Join prefix with first, suffix with last
    if params.len() == 1 {
        return PositionalExpansionResult {
            values: vec![format!("{}{}{}", prefix, params[0], suffix)],
            quoted: true,
        };
    }

    let mut result = Vec::with_capacity(params.len());
    result.push(format!("{}{}", prefix, params[0]));
    for p in &params[1..params.len() - 1] {
        result.push(p.clone());
    }
    result.push(format!("{}{}", params[params.len() - 1], suffix));

    PositionalExpansionResult {
        values: result,
        quoted: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state_with_params(params: &[&str]) -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("#".to_string(), params.len().to_string());
        env.insert("0".to_string(), "bash".to_string());
        for (i, p) in params.iter().enumerate() {
            env.insert((i + 1).to_string(), p.to_string());
        }
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_get_positional_params() {
        let state = make_state_with_params(&["a", "b", "c"]);
        let params = get_positional_params(&state);
        assert_eq!(params, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_simple_at_expansion() {
        let state = make_state_with_params(&["hello", "world"]);
        let result = apply_simple_positional_expansion(&state, false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-hello", "world-suf"]);
    }

    #[test]
    fn test_simple_star_expansion() {
        let state = make_state_with_params(&["hello", "world"]);
        let result = apply_simple_positional_expansion(&state, true, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-hello world-suf"]);
    }

    #[test]
    fn test_simple_at_expansion_empty() {
        let state = make_state_with_params(&[]);
        let result = apply_simple_positional_expansion(&state, false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre--suf"]);
    }

    #[test]
    fn test_simple_star_expansion_empty() {
        let state = make_state_with_params(&[]);
        let result = apply_simple_positional_expansion(&state, true, "", "");
        assert_eq!(result.values, vec![""]);
    }

    #[test]
    fn test_slicing_offset_positive() {
        let state = make_state_with_params(&["a", "b", "c", "d"]);
        let result = apply_positional_slicing(&state, false, "", "", 2, None);
        // offset 2 means start from $2, which is "b"
        assert_eq!(result.values, vec!["b", "c", "d"]);
    }

    #[test]
    fn test_slicing_offset_with_length() {
        let state = make_state_with_params(&["a", "b", "c", "d"]);
        let result = apply_positional_slicing(&state, false, "", "", 2, Some(2));
        assert_eq!(result.values, vec!["b", "c"]);
    }

    #[test]
    fn test_slicing_offset_zero() {
        let state = make_state_with_params(&["a", "b", "c"]);
        let result = apply_positional_slicing(&state, false, "", "", 0, Some(2));
        // offset 0 includes $0 (bash), so result is ["bash", "a"]
        assert_eq!(result.values, vec!["bash", "a"]);
    }

    #[test]
    fn test_pattern_replacement() {
        let state = make_state_with_params(&["hello", "world", "help"]);
        let regex = pattern_to_regex("hel", true, false);
        let result = apply_positional_pattern_replacement(
            &state, false, "", "", &regex, "HEL", false, false, false,
        );
        assert_eq!(result.values, vec!["HELlo", "world", "HELp"]);
    }

    #[test]
    fn test_pattern_removal() {
        let state = make_state_with_params(&["hello", "world", "help"]);
        let regex = pattern_to_regex("hel", false, false);
        let result = apply_positional_pattern_removal(
            &state,
            false,
            "",
            "",
            &regex,
            PatternRemovalSide::Prefix,
            false,
        );
        assert_eq!(result.values, vec!["lo", "world", "p"]);
    }

    #[test]
    fn test_single_param_with_prefix_suffix() {
        let state = make_state_with_params(&["only"]);
        let result = apply_simple_positional_expansion(&state, false, "pre-", "-suf");
        assert_eq!(result.values, vec!["pre-only-suf"]);
    }
}
