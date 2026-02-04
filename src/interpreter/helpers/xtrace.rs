//! xtrace (set -x) helper functions
//!
//! Handles trace output generation when xtrace option is enabled.
//! PS4 variable controls the prefix (default "+ ").
//! PS4 is expanded (variable substitution) before each trace line.

use std::collections::HashMap;
use crate::interpreter::types::ShellOptions;

/// Default PS4 value when not set
pub const DEFAULT_PS4: &str = "+ ";

/// Get the xtrace prefix from PS4 variable.
/// Note: Full PS4 expansion requires the expansion module.
/// This is a simplified version that returns the literal PS4 value.
pub fn get_xtrace_prefix(env: &HashMap<String, String>) -> String {
    match env.get("PS4") {
        None => DEFAULT_PS4.to_string(),
        Some(ps4) if ps4.is_empty() => String::new(),
        Some(ps4) => ps4.clone(),
    }
}

/// Get the xtrace prefix with PS4 variable expansion.
///
/// This version accepts an expander function that can expand variables
/// in the PS4 string (e.g., $VAR, ${VAR}, $?, $LINENO).
///
/// # Arguments
/// * `env` - Environment variables
/// * `expander` - Function to expand the PS4 string
///
/// # Returns
/// The expanded PS4 prefix, or the literal PS4 if expansion fails.
pub fn get_xtrace_prefix_expanded<F>(
    env: &HashMap<String, String>,
    expander: F,
) -> String
where
    F: FnOnce(&str) -> Result<String, String>,
{
    match env.get("PS4") {
        None => DEFAULT_PS4.to_string(),
        Some(ps4) if ps4.is_empty() => String::new(),
        Some(ps4) => {
            match expander(ps4) {
                Ok(expanded) => expanded,
                Err(_) => ps4.clone(), // Fallback to literal on expansion error
            }
        }
    }
}

/// Get the xtrace prefix with PS4 variable expansion and error reporting.
///
/// This version also reports expansion errors to stderr.
///
/// # Arguments
/// * `env` - Environment variables
/// * `expander` - Function to expand the PS4 string
///
/// # Returns
/// A tuple of (prefix, stderr) where stderr contains any error messages.
pub fn get_xtrace_prefix_with_error<F>(
    env: &HashMap<String, String>,
    expander: F,
) -> (String, Option<String>)
where
    F: FnOnce(&str) -> Result<String, String>,
{
    match env.get("PS4") {
        None => (DEFAULT_PS4.to_string(), None),
        Some(ps4) if ps4.is_empty() => (String::new(), None),
        Some(ps4) => {
            match expander(ps4) {
                Ok(expanded) => (expanded, None),
                Err(err_msg) => {
                    let stderr = format!("bash: {}: bad substitution\n", ps4);
                    // Return literal PS4 on error, like bash does
                    (ps4.clone(), Some(format!("{}{}", stderr, err_msg)))
                }
            }
        }
    }
}

/// Quote a value for trace output if needed.
/// Follows bash conventions for xtrace output quoting.
pub fn quote_for_trace(value: &str) -> String {
    // Empty string needs quotes
    if value.is_empty() {
        return "''".to_string();
    }

    // Check if quoting is needed
    // Need to quote if contains: whitespace, quotes, special chars, newlines
    let needs_quoting = value.chars().any(|c| {
        matches!(c, ' ' | '\t' | '\n' | '\'' | '"' | '\\' | '$' | '`' | '!' |
                    '*' | '?' | '[' | ']' | '{' | '}' | '|' | '&' | ';' |
                    '<' | '>' | '(' | ')' | '~' | '#')
    });

    if !needs_quoting {
        return value.to_string();
    }

    // Check for special characters that need $'...' quoting
    let has_control_chars = value.chars().any(|c| {
        let code = c as u32;
        code < 32 || code == 127
    });
    let has_newline = value.contains('\n');
    let has_tab = value.contains('\t');
    let has_backslash = value.contains('\\');
    let has_single_quote = value.contains('\'');

    // Use $'...' quoting for control characters, newlines, tabs
    if has_control_chars || has_newline || has_tab || has_backslash {
        let mut escaped = String::new();
        for c in value.chars() {
            let code = c as u32;
            match c {
                '\n' => escaped.push_str("\\n"),
                '\t' => escaped.push_str("\\t"),
                '\\' => escaped.push_str("\\\\"),
                '\'' => escaped.push('\''),
                '"' => escaped.push('"'),
                _ if code < 32 || code == 127 => {
                    // Control character - use \xNN
                    escaped.push_str(&format!("\\x{:02x}", code));
                }
                _ => escaped.push(c),
            }
        }
        return format!("$'{}'", escaped);
    }

    // Use single quotes if possible (no single quotes in value)
    if !has_single_quote {
        return format!("'{}'", value);
    }

    // Use double quotes for values with single quotes
    // Need to escape $ ` \ " in double quotes
    let escaped: String = value.chars().map(|c| {
        match c {
            '\\' | '$' | '`' | '"' => format!("\\{}", c),
            _ => c.to_string(),
        }
    }).collect();
    format!("\"{}\"", escaped)
}

/// Format a trace line for output.
/// Quotes arguments that need quoting for shell safety.
pub fn format_trace_line(parts: &[&str]) -> String {
    parts.iter()
        .map(|part| quote_for_trace(part))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generate xtrace output for a simple command.
/// Returns the trace line to be added to stderr.
pub fn trace_simple_command(
    options: &ShellOptions,
    env: &HashMap<String, String>,
    command_name: &str,
    args: &[&str],
) -> String {
    if !options.xtrace {
        return String::new();
    }

    let prefix = get_xtrace_prefix(env);
    let mut parts = vec![command_name];
    parts.extend(args);
    let trace_line = format_trace_line(&parts);

    format!("{}{}\n", prefix, trace_line)
}

/// Generate xtrace output for an assignment.
/// Returns the trace line to be added to stderr.
pub fn trace_assignment(
    options: &ShellOptions,
    env: &HashMap<String, String>,
    name: &str,
    value: &str,
) -> String {
    if !options.xtrace {
        return String::new();
    }

    let prefix = get_xtrace_prefix(env);
    // Don't quote the assignment value - show raw name=value
    format!("{}{}={}\n", prefix, name, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn make_options() -> ShellOptions {
        ShellOptions::default()
    }

    #[test]
    fn test_get_xtrace_prefix_default() {
        let env = make_env();
        assert_eq!(get_xtrace_prefix(&env), "+ ");
    }

    #[test]
    fn test_get_xtrace_prefix_custom() {
        let mut env = make_env();
        env.insert("PS4".to_string(), ">> ".to_string());
        assert_eq!(get_xtrace_prefix(&env), ">> ");
    }

    #[test]
    fn test_get_xtrace_prefix_empty() {
        let mut env = make_env();
        env.insert("PS4".to_string(), String::new());
        assert_eq!(get_xtrace_prefix(&env), "");
    }

    #[test]
    fn test_quote_for_trace_simple() {
        assert_eq!(quote_for_trace("hello"), "hello");
        assert_eq!(quote_for_trace("world123"), "world123");
    }

    #[test]
    fn test_quote_for_trace_empty() {
        assert_eq!(quote_for_trace(""), "''");
    }

    #[test]
    fn test_quote_for_trace_spaces() {
        assert_eq!(quote_for_trace("hello world"), "'hello world'");
    }

    #[test]
    fn test_quote_for_trace_single_quote() {
        assert_eq!(quote_for_trace("it's"), "\"it's\"");
    }

    #[test]
    fn test_quote_for_trace_newline() {
        assert_eq!(quote_for_trace("line1\nline2"), "$'line1\\nline2'");
    }

    #[test]
    fn test_quote_for_trace_tab() {
        assert_eq!(quote_for_trace("col1\tcol2"), "$'col1\\tcol2'");
    }

    #[test]
    fn test_quote_for_trace_backslash() {
        assert_eq!(quote_for_trace("path\\file"), "$'path\\\\file'");
    }

    #[test]
    fn test_format_trace_line() {
        let parts = vec!["echo", "hello", "world"];
        assert_eq!(format_trace_line(&parts), "echo hello world");
    }

    #[test]
    fn test_format_trace_line_with_spaces() {
        let parts = vec!["echo", "hello world"];
        assert_eq!(format_trace_line(&parts), "echo 'hello world'");
    }

    #[test]
    fn test_trace_simple_command_disabled() {
        let options = make_options();
        let env = make_env();
        let result = trace_simple_command(&options, &env, "echo", &["hello"]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_trace_simple_command_enabled() {
        let mut options = make_options();
        options.xtrace = true;
        let env = make_env();
        let result = trace_simple_command(&options, &env, "echo", &["hello"]);
        assert_eq!(result, "+ echo hello\n");
    }

    #[test]
    fn test_trace_assignment_disabled() {
        let options = make_options();
        let env = make_env();
        let result = trace_assignment(&options, &env, "FOO", "bar");
        assert_eq!(result, "");
    }

    #[test]
    fn test_trace_assignment_enabled() {
        let mut options = make_options();
        options.xtrace = true;
        let env = make_env();
        let result = trace_assignment(&options, &env, "FOO", "bar");
        assert_eq!(result, "+ FOO=bar\n");
    }
}
