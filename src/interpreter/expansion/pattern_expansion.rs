//! Pattern Expansion
//!
//! Functions for expanding variables within glob/extglob patterns.
//! Handles command substitution, variable expansion, and quoting within patterns.

use crate::interpreter::expansion::escape_glob_chars;
use crate::interpreter::InterpreterState;

/// Check if a pattern string contains command substitution $(...)
pub fn pattern_has_command_substitution(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Skip escaped characters
        if c == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        // Skip single-quoted strings
        if c == '\'' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_idx) = rest.find('\'') {
                i = i + 1 + close_idx + 1;
                continue;
            }
        }
        // Check for $( which indicates command substitution
        if c == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            return true;
        }
        // Check for backtick command substitution
        if c == '`' {
            return true;
        }
        i += 1;
    }
    false
}

/// Find the matching closing parenthesis for a command substitution.
/// Handles nested parentheses, quotes, and escapes.
/// Returns the index of the closing ), or None if not found.
pub fn find_command_substitution_end(pattern: &str, start_idx: usize) -> Option<usize> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut depth = 1;
    let mut i = start_idx;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < chars.len() && depth > 0 {
        let c = chars[i];

        // Handle escapes (only outside single quotes)
        if c == '\\' && !in_single_quote && i + 1 < chars.len() {
            i += 2;
            continue;
        }

        // Handle single quotes (only outside double quotes)
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        // Handle double quotes (only outside single quotes)
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        // Handle parentheses (only outside quotes)
        if !in_single_quote && !in_double_quote {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }

        i += 1;
    }

    None
}

/// Expand variables within a double-quoted string inside a pattern.
/// Handles $var and ${var} but not nested quotes.
fn expand_variables_in_double_quoted_pattern(state: &InterpreterState, content: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle backslash escapes
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // In double quotes, only $, `, \, ", and newline are special after \
            if next == '$' || next == '`' || next == '\\' || next == '"' {
                result.push(next);
                i += 2;
                continue;
            }
            // Other escapes pass through as-is
            result.push(c);
            i += 1;
            continue;
        }

        // Handle variable references: $var or ${var}
        if c == '$' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' {
                // ${var} form - find matching }
                let rest: String = chars[i + 2..].iter().collect();
                if let Some(close_idx) = rest.find('}') {
                    let var_name: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                    result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                    i = i + 2 + close_idx + 1;
                    continue;
                }
            } else if next.is_ascii_alphabetic() || next == '_' {
                // $var form - read variable name
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                let var_name: String = chars[i + 1..end].iter().collect();
                result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                i = end;
                continue;
            }
        }

        // All other characters pass through unchanged
        result.push(c);
        i += 1;
    }

    result
}

/// Expand variables within a glob/extglob pattern string.
/// This handles patterns like @($var|$other) where variables need expansion.
/// Also handles quoted strings inside patterns (e.g., @(foo|'bar'|"$baz")).
/// Preserves pattern metacharacters while expanding $var and ${var} references.
pub fn expand_variables_in_pattern(state: &InterpreterState, pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle single-quoted strings - content is literal, strip quotes, escape glob chars
        if c == '\'' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_idx) = rest.find('\'') {
                let content: String = chars[i + 1..i + 1 + close_idx].iter().collect();
                // Escape glob metacharacters so they match literally
                result.push_str(&escape_glob_chars(&content));
                i = i + 1 + close_idx + 1;
                continue;
            }
        }

        // Handle double-quoted strings - expand variables inside, strip quotes, escape glob chars
        if c == '"' {
            // Find matching close quote, handling escapes
            let mut close_idx = None;
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '\\' {
                    j += 2; // Skip escaped char
                    continue;
                }
                if chars[j] == '"' {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(close) = close_idx {
                let content: String = chars[i + 1..close].iter().collect();
                // Recursively expand variables in the double-quoted content
                // but without the quote handling (pass through all other chars)
                let expanded = expand_variables_in_double_quoted_pattern(state, &content);
                // Escape glob metacharacters so they match literally
                result.push_str(&escape_glob_chars(&expanded));
                i = close + 1;
                continue;
            }
        }

        // Handle variable references: $var or ${var}
        if c == '$' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' {
                // ${var} form - find matching }
                let rest: String = chars[i + 2..].iter().collect();
                if let Some(close_idx) = rest.find('}') {
                    let var_name: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                    // Simple variable expansion (no complex operations)
                    result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                    i = i + 2 + close_idx + 1;
                    continue;
                }
            } else if next.is_ascii_alphabetic() || next == '_' {
                // $var form - read variable name
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                let var_name: String = chars[i + 1..end].iter().collect();
                result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                i = end;
                continue;
            }
        }

        // Handle backslash escapes - preserve them
        if c == '\\' && i + 1 < chars.len() {
            result.push(c);
            result.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // All other characters pass through unchanged
        result.push(c);
        i += 1;
    }

    result
}

/// Result of pattern expansion with command substitution.
#[derive(Debug, Clone, Default)]
pub struct PatternExpansionResult {
    pub value: String,
    pub stderr: String,
}

/// Expand variables within a double-quoted string inside a pattern with command substitution support.
fn expand_variables_in_double_quoted_pattern_with_exec<F>(
    state: &InterpreterState,
    content: &str,
    exec_fn: &Option<F>,
) -> PatternExpansionResult
where
    F: Fn(&str) -> (String, String, i32), // (stdout, stderr, exit_code)
{
    let mut result = String::new();
    let mut stderr = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle backslash escapes
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // In double quotes, only $, `, \, ", and newline are special after \
            if next == '$' || next == '`' || next == '\\' || next == '"' {
                result.push(next);
                i += 2;
                continue;
            }
            // Other escapes pass through as-is
            result.push(c);
            i += 1;
            continue;
        }

        // Handle command substitution: $(...)
        if c == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_idx) = find_command_substitution_end(&rest, 0) {
                let cmd_str: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                if let Some(ref exec) = exec_fn {
                    let (stdout, cmd_stderr, _) = exec(&cmd_str);
                    result.push_str(stdout.trim_end_matches('\n'));
                    if !cmd_stderr.is_empty() {
                        stderr.push_str(&cmd_stderr);
                    }
                } else {
                    // Keep as-is if no exec function
                    let segment: String = chars[i..i + 2 + close_idx + 1].iter().collect();
                    result.push_str(&segment);
                }
                i = i + 2 + close_idx + 1;
                continue;
            }
        }

        // Handle backtick command substitution: `...`
        if c == '`' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_idx) = rest.find('`') {
                let cmd_str: String = chars[i + 1..i + 1 + close_idx].iter().collect();
                if let Some(ref exec) = exec_fn {
                    let (stdout, cmd_stderr, _) = exec(&cmd_str);
                    result.push_str(stdout.trim_end_matches('\n'));
                    if !cmd_stderr.is_empty() {
                        stderr.push_str(&cmd_stderr);
                    }
                } else {
                    // Keep as-is if no exec function
                    let segment: String = chars[i..i + 1 + close_idx + 1].iter().collect();
                    result.push_str(&segment);
                }
                i = i + 1 + close_idx + 1;
                continue;
            }
        }

        // Handle variable references: $var or ${var}
        if c == '$' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' {
                // ${var} form - find matching }
                let rest: String = chars[i + 2..].iter().collect();
                if let Some(close_idx) = rest.find('}') {
                    let var_name: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                    result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                    i = i + 2 + close_idx + 1;
                    continue;
                }
            } else if next.is_ascii_alphabetic() || next == '_' {
                // $var form - read variable name
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                let var_name: String = chars[i + 1..end].iter().collect();
                result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                i = end;
                continue;
            }
        }

        // All other characters pass through unchanged
        result.push(c);
        i += 1;
    }

    PatternExpansionResult { value: result, stderr }
}

/// Expand variables within a glob/extglob pattern string with command substitution support.
/// This handles patterns like @($var|$(echo foo)) where command substitutions need expansion.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `pattern` - The pattern string to expand
/// * `exec_fn` - Optional function to execute command substitutions
///
/// # Returns
/// The expanded pattern and any stderr output.
pub fn expand_variables_in_pattern_with_exec<F>(
    state: &InterpreterState,
    pattern: &str,
    exec_fn: Option<F>,
) -> PatternExpansionResult
where
    F: Fn(&str) -> (String, String, i32), // (stdout, stderr, exit_code)
{
    let mut result = String::new();
    let mut stderr = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle single-quoted strings - content is literal, strip quotes, escape glob chars
        if c == '\'' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_idx) = rest.find('\'') {
                let content: String = chars[i + 1..i + 1 + close_idx].iter().collect();
                // Escape glob metacharacters so they match literally
                result.push_str(&escape_glob_chars(&content));
                i = i + 1 + close_idx + 1;
                continue;
            }
        }

        // Handle double-quoted strings - expand variables inside, strip quotes, escape glob chars
        if c == '"' {
            // Find matching close quote, handling escapes
            let mut close_idx = None;
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '\\' {
                    j += 2; // Skip escaped char
                    continue;
                }
                if chars[j] == '"' {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(close) = close_idx {
                let content: String = chars[i + 1..close].iter().collect();
                // Recursively expand (including command substitutions) in the double-quoted content
                let expanded = expand_variables_in_double_quoted_pattern_with_exec(state, &content, &exec_fn);
                stderr.push_str(&expanded.stderr);
                // Escape glob metacharacters so they match literally
                result.push_str(&escape_glob_chars(&expanded.value));
                i = close + 1;
                continue;
            }
        }

        // Handle command substitution: $(...)
        if c == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_idx) = find_command_substitution_end(&rest, 0) {
                let cmd_str: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                if let Some(ref exec) = exec_fn {
                    let (stdout, cmd_stderr, _) = exec(&cmd_str);
                    result.push_str(stdout.trim_end_matches('\n'));
                    if !cmd_stderr.is_empty() {
                        stderr.push_str(&cmd_stderr);
                    }
                } else {
                    // Keep as-is if no exec function
                    let segment: String = chars[i..i + 2 + close_idx + 1].iter().collect();
                    result.push_str(&segment);
                }
                i = i + 2 + close_idx + 1;
                continue;
            }
        }

        // Handle backtick command substitution: `...`
        if c == '`' {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close_idx) = rest.find('`') {
                let cmd_str: String = chars[i + 1..i + 1 + close_idx].iter().collect();
                if let Some(ref exec) = exec_fn {
                    let (stdout, cmd_stderr, _) = exec(&cmd_str);
                    result.push_str(stdout.trim_end_matches('\n'));
                    if !cmd_stderr.is_empty() {
                        stderr.push_str(&cmd_stderr);
                    }
                } else {
                    // Keep as-is if no exec function
                    let segment: String = chars[i..i + 1 + close_idx + 1].iter().collect();
                    result.push_str(&segment);
                }
                i = i + 1 + close_idx + 1;
                continue;
            }
        }

        // Handle variable references: $var or ${var}
        if c == '$' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' {
                // ${var} form - find matching }
                let rest: String = chars[i + 2..].iter().collect();
                if let Some(close_idx) = rest.find('}') {
                    let var_name: String = chars[i + 2..i + 2 + close_idx].iter().collect();
                    // Simple variable expansion (no complex operations)
                    result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                    i = i + 2 + close_idx + 1;
                    continue;
                }
            } else if next.is_ascii_alphabetic() || next == '_' {
                // $var form - read variable name
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                let var_name: String = chars[i + 1..end].iter().collect();
                result.push_str(state.env.get(&var_name).map(|s| s.as_str()).unwrap_or(""));
                i = end;
                continue;
            }
        }

        // Handle backslash escapes - preserve them
        if c == '\\' && i + 1 < chars.len() {
            result.push(c);
            result.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // All other characters pass through unchanged
        result.push(c);
        i += 1;
    }

    PatternExpansionResult { value: result, stderr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("foo".to_string(), "bar".to_string());
        env.insert("x".to_string(), "123".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_pattern_has_command_substitution() {
        assert!(pattern_has_command_substitution("$(echo foo)"));
        assert!(pattern_has_command_substitution("`echo foo`"));
        assert!(pattern_has_command_substitution("prefix$(cmd)suffix"));
        assert!(!pattern_has_command_substitution("$var"));
        assert!(!pattern_has_command_substitution("plain"));
        assert!(!pattern_has_command_substitution("'$(not a cmd)'"));
    }

    #[test]
    fn test_expand_variables_in_pattern() {
        let state = make_state();
        assert_eq!(expand_variables_in_pattern(&state, "$foo"), "bar");
        assert_eq!(expand_variables_in_pattern(&state, "${foo}"), "bar");
        assert_eq!(expand_variables_in_pattern(&state, "@($foo|$x)"), "@(bar|123)");
        // Single-quoted content is literal, $ is not a glob metachar so not escaped
        assert_eq!(expand_variables_in_pattern(&state, "'$foo'"), "$foo");
    }

    #[test]
    fn test_find_command_substitution_end() {
        // Note: start_idx should be the position AFTER the opening paren
        // For "$(echo foo)", if $ is at 0, ( is at 1, then start_idx should be 2
        assert_eq!(find_command_substitution_end("echo foo)", 0), Some(8));
        assert_eq!(find_command_substitution_end("echo (nested))", 0), Some(13));
        assert_eq!(find_command_substitution_end("unclosed", 0), None);
    }

    #[test]
    fn test_expand_variables_in_pattern_with_exec() {
        let state = make_state();
        // With exec function that returns "hello"
        let exec_fn = |_cmd: &str| ("hello\n".to_string(), String::new(), 0);
        let result = expand_variables_in_pattern_with_exec(&state, "$(echo hello)", Some(exec_fn));
        assert_eq!(result.value, "hello");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_expand_variables_in_pattern_with_exec_backtick() {
        let state = make_state();
        // With exec function for backtick
        let exec_fn = |_cmd: &str| ("world\n".to_string(), String::new(), 0);
        let result = expand_variables_in_pattern_with_exec(&state, "`echo world`", Some(exec_fn));
        assert_eq!(result.value, "world");
    }

    #[test]
    fn test_expand_variables_in_pattern_without_exec() {
        let state = make_state();
        // Without exec function - command substitution kept as-is
        let result = expand_variables_in_pattern_with_exec::<fn(&str) -> (String, String, i32)>(
            &state,
            "$(echo hello)",
            None,
        );
        assert_eq!(result.value, "$(echo hello)");
    }

    #[test]
    fn test_expand_variables_in_pattern_mixed() {
        let state = make_state();
        // Mix of variables and command substitution
        let exec_fn = |_cmd: &str| ("cmd_output\n".to_string(), String::new(), 0);
        let result = expand_variables_in_pattern_with_exec(&state, "@($foo|$(cmd))", Some(exec_fn));
        assert_eq!(result.value, "@(bar|cmd_output)");
    }
}
