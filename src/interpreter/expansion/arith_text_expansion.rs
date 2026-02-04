//! Arithmetic Text Expansion
//!
//! Functions for expanding variables within arithmetic expression text.
//! This handles the bash behavior where $(( $x * 3 )) with x='1 + 2' should
//! expand to $(( 1 + 2 * 3 )) = 7, not $(( (1+2) * 3 )) = 9.

use crate::interpreter::expansion::get_variable;
use crate::interpreter::InterpreterState;

/// Expand $var patterns in arithmetic expression text for text substitution.
/// Only expands simple $var patterns, not ${...}, $(()), $(), etc.
pub fn expand_dollar_vars_in_arith_text(state: &InterpreterState, text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // Check for ${...} - don't expand, keep as-is for arithmetic parser
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                // Find matching }
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let segment: String = chars[i..j].iter().collect();
                result.push_str(&segment);
                i = j;
                continue;
            }
            // Check for $((, $( - don't expand
            if i + 1 < chars.len() && chars[i + 1] == '(' {
                // Find matching ) or ))
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '(' {
                        depth += 1;
                    } else if chars[j] == ')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let segment: String = chars[i..j].iter().collect();
                result.push_str(&segment);
                i = j;
                continue;
            }
            // Check for $var pattern
            if i + 1 < chars.len() && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let var_name: String = chars[i + 1..j].iter().collect();
                let value = get_variable(state, &var_name);
                result.push_str(&value);
                i = j;
                continue;
            }
            // Check for $1, $2, etc. (positional parameters)
            if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let var_name: String = chars[i + 1..j].iter().collect();
                let value = get_variable(state, &var_name);
                result.push_str(&value);
                i = j;
                continue;
            }
            // Check for special vars: $*, $@, $#, $?, etc.
            if i + 1 < chars.len() && "*@#?-!$".contains(chars[i + 1]) {
                let var_name = chars[i + 1].to_string();
                let value = get_variable(state, &var_name);
                result.push_str(&value);
                i += 2;
                continue;
            }
        }
        // Check for double quotes - expand variables inside but keep the quotes
        // (arithmetic preprocessor will strip them)
        if chars[i] == '"' {
            result.push('"');
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '$'
                    && i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
                {
                    // Expand $var inside quotes
                    let mut j = i + 1;
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    let var_name: String = chars[i + 1..j].iter().collect();
                    let value = get_variable(state, &var_name);
                    result.push_str(&value);
                    i = j;
                } else if chars[i] == '\\' {
                    // Keep escape sequences
                    result.push(chars[i]);
                    i += 1;
                    if i < chars.len() {
                        result.push(chars[i]);
                        i += 1;
                    }
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            if i < chars.len() {
                result.push('"');
                i += 1;
            }
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Expand variable references and command substitutions in an array subscript.
/// e.g., "${array[@]}" -> "1 2 3", "$(echo 1)" -> "1"
/// This is needed for associative array subscripts like assoc["${array[@]}"]
/// where the subscript may contain variable or array expansions.
///
/// Note: This is a simplified synchronous version that doesn't handle command
/// substitution. For full async command substitution, use the interpreter's
/// expand function.
pub fn expand_subscript_for_assoc_array(state: &InterpreterState, subscript: &str) -> String {
    // Remove surrounding quotes if present
    let inner: &str;
    let has_double_quotes = subscript.starts_with('"') && subscript.ends_with('"');
    let has_single_quotes = subscript.starts_with('\'') && subscript.ends_with('\'');

    if has_double_quotes || has_single_quotes {
        inner = &subscript[1..subscript.len() - 1];
    } else {
        inner = subscript;
    }

    // For single-quoted strings, no expansion
    if has_single_quotes {
        return inner.to_string();
    }

    // Expand $var references in the string
    let mut result = String::new();
    let chars: Vec<char> = inner.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // Check for $(...) command substitution - not supported in sync version
            if i + 1 < chars.len() && chars[i + 1] == '(' {
                // Find matching closing paren
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '(' {
                        depth += 1;
                    } else if chars[j] == ')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                // Keep as-is since we can't execute commands synchronously
                let segment: String = chars[i..j].iter().collect();
                result.push_str(&segment);
                i = j;
            } else if i + 1 < chars.len() && chars[i + 1] == '{' {
                // Check for ${...} - find matching }
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let var_expr: String = chars[i + 2..j - 1].iter().collect();
                // Use get_variable to properly handle array expansions
                let value = get_variable(state, &var_expr);
                result.push_str(&value);
                i = j;
            } else if i + 1 < chars.len()
                && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
            {
                // $name - find end of name
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let var_name: String = chars[i + 1..j].iter().collect();
                let value = get_variable(state, &var_name);
                result.push_str(&value);
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else if chars[i] == '`' {
            // Legacy backtick command substitution - not supported in sync version
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            // Keep as-is since we can't execute commands synchronously
            let segment: String = chars[i..j + 1].iter().collect();
            result.push_str(&segment);
            i = j + 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Result of command substitution expansion.
#[derive(Debug, Clone, Default)]
pub struct SubscriptExpansionResult {
    pub value: String,
    pub stderr: String,
}

/// Expand variable references and command substitutions in an array subscript
/// with command execution support.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `subscript` - The subscript string to expand
/// * `exec_fn` - Optional function to execute command substitutions
///
/// # Returns
/// The expanded subscript value and any stderr output.
pub fn expand_subscript_for_assoc_array_with_exec<F>(
    state: &InterpreterState,
    subscript: &str,
    exec_fn: Option<F>,
) -> SubscriptExpansionResult
where
    F: Fn(&str) -> (String, String, i32), // (stdout, stderr, exit_code)
{
    // Remove surrounding quotes if present
    let inner: &str;
    let has_double_quotes = subscript.starts_with('"') && subscript.ends_with('"');
    let has_single_quotes = subscript.starts_with('\'') && subscript.ends_with('\'');

    if has_double_quotes || has_single_quotes {
        inner = &subscript[1..subscript.len() - 1];
    } else {
        inner = subscript;
    }

    // For single-quoted strings, no expansion
    if has_single_quotes {
        return SubscriptExpansionResult {
            value: inner.to_string(),
            stderr: String::new(),
        };
    }

    // Expand $var references and command substitutions in the string
    let mut result = String::new();
    let mut stderr = String::new();
    let chars: Vec<char> = inner.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // Check for $(...) command substitution
            if i + 1 < chars.len() && chars[i + 1] == '(' {
                // Find matching closing paren
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '(' && j > 0 && chars[j - 1] == '$' {
                        depth += 1;
                    } else if chars[j] == '(' {
                        depth += 1;
                    } else if chars[j] == ')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                // Extract and execute the command
                let cmd_str: String = chars[i + 2..j - 1].iter().collect();
                if let Some(ref exec) = exec_fn {
                    let (stdout, cmd_stderr, _) = exec(&cmd_str);
                    // Strip trailing newlines like command substitution does
                    result.push_str(stdout.trim_end_matches('\n'));
                    if !cmd_stderr.is_empty() {
                        stderr.push_str(&cmd_stderr);
                    }
                } else {
                    // Keep as-is if no exec function
                    let segment: String = chars[i..j].iter().collect();
                    result.push_str(&segment);
                }
                i = j;
            } else if i + 1 < chars.len() && chars[i + 1] == '{' {
                // Check for ${...} - find matching }
                let mut depth = 1;
                let mut j = i + 2;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let var_expr: String = chars[i + 2..j - 1].iter().collect();
                // Use get_variable to properly handle array expansions
                let value = get_variable(state, &var_expr);
                result.push_str(&value);
                i = j;
            } else if i + 1 < chars.len()
                && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
            {
                // $name - find end of name
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let var_name: String = chars[i + 1..j].iter().collect();
                let value = get_variable(state, &var_name);
                result.push_str(&value);
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else if chars[i] == '`' {
            // Legacy backtick command substitution
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            let cmd_str: String = chars[i + 1..j].iter().collect();
            if let Some(ref exec) = exec_fn {
                let (stdout, cmd_stderr, _) = exec(&cmd_str);
                result.push_str(stdout.trim_end_matches('\n'));
                if !cmd_stderr.is_empty() {
                    stderr.push_str(&cmd_stderr);
                }
            } else {
                // Keep as-is if no exec function
                let segment: String = chars[i..j + 1].iter().collect();
                result.push_str(&segment);
            }
            i = j + 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    SubscriptExpansionResult { value: result, stderr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("x".to_string(), "1 + 2".to_string());
        env.insert("y".to_string(), "5".to_string());
        env.insert("foo".to_string(), "bar".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_expand_dollar_vars_simple() {
        let state = make_state();
        assert_eq!(expand_dollar_vars_in_arith_text(&state, "$y"), "5");
        assert_eq!(expand_dollar_vars_in_arith_text(&state, "$x"), "1 + 2");
        assert_eq!(expand_dollar_vars_in_arith_text(&state, "$x * 3"), "1 + 2 * 3");
    }

    #[test]
    fn test_expand_dollar_vars_preserves_complex() {
        let state = make_state();
        // ${...} should be preserved for arithmetic parser
        assert_eq!(expand_dollar_vars_in_arith_text(&state, "${x}"), "${x}");
        // $((...)) should be preserved
        assert_eq!(expand_dollar_vars_in_arith_text(&state, "$((1+2))"), "$((1+2))");
    }

    #[test]
    fn test_expand_subscript_simple() {
        let state = make_state();
        assert_eq!(expand_subscript_for_assoc_array(&state, "$foo"), "bar");
        assert_eq!(expand_subscript_for_assoc_array(&state, "${foo}"), "bar");
    }

    #[test]
    fn test_expand_subscript_quoted() {
        let state = make_state();
        // Double-quoted - should expand
        assert_eq!(expand_subscript_for_assoc_array(&state, "\"$foo\""), "bar");
        // Single-quoted - no expansion
        assert_eq!(expand_subscript_for_assoc_array(&state, "'$foo'"), "$foo");
    }

    #[test]
    fn test_expand_subscript_with_exec() {
        let state = make_state();
        // With exec function that returns "hello"
        let exec_fn = |_cmd: &str| ("hello\n".to_string(), String::new(), 0);
        let result = expand_subscript_for_assoc_array_with_exec(&state, "$(echo hello)", Some(exec_fn));
        assert_eq!(result.value, "hello");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_expand_subscript_with_exec_backtick() {
        let state = make_state();
        // With exec function for backtick
        let exec_fn = |_cmd: &str| ("world\n".to_string(), String::new(), 0);
        let result = expand_subscript_for_assoc_array_with_exec(&state, "`echo world`", Some(exec_fn));
        assert_eq!(result.value, "world");
    }

    #[test]
    fn test_expand_subscript_without_exec() {
        let state = make_state();
        // Without exec function - command substitution kept as-is
        let result = expand_subscript_for_assoc_array_with_exec::<fn(&str) -> (String, String, i32)>(
            &state,
            "$(echo hello)",
            None,
        );
        assert_eq!(result.value, "$(echo hello)");
    }
}
