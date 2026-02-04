//! export - Set environment variables builtin
//!
//! Usage:
//!   export              - List all exported variables
//!   export -p           - List all exported variables (same as no args)
//!   export NAME=value   - Set and export variable
//!   export NAME+=value  - Append value and export variable
//!   export NAME         - Export existing variable (or create empty)
//!   export -n NAME      - Un-export variable (remove from env)

use crate::interpreter::helpers::readonly::{mark_exported, unmark_exported};
use crate::interpreter::helpers::tilde::expand_tildes_in_value;
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// Check if a string is a valid variable name.
fn is_valid_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    // First char must be letter or underscore
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    // Rest must be alphanumeric or underscore
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse append syntax: NAME+=value
/// Returns (name, value) if it matches, None otherwise.
fn parse_append_syntax(arg: &str) -> Option<(&str, &str)> {
    if let Some(plus_eq_idx) = arg.find("+=") {
        let name = &arg[..plus_eq_idx];
        if is_valid_var_name(name) {
            let value = &arg[plus_eq_idx + 2..];
            return Some((name, value));
        }
    }
    None
}

/// Handle the export builtin command.
///
/// # Arguments
/// * `state` - The interpreter state (mutable for modifying env)
/// * `args` - Command arguments
///
/// # Returns
/// BuiltinResult with stdout/stderr and exit code
pub fn handle_export(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Handle -n flag for un-export
    let mut unexport = false;
    let mut processed_args: Vec<&str> = Vec::new();

    for arg in args {
        if arg == "-n" {
            unexport = true;
        } else if arg == "-p" {
            // -p flag is handled implicitly (list mode)
        } else if arg == "--" {
            // End of options
        } else {
            processed_args.push(arg);
        }
    }

    // No args or just -p: list all exported variables
    if processed_args.is_empty() && !unexport {
        let mut stdout = String::new();
        // Only list variables that are actually exported
        if let Some(ref exported_vars) = state.exported_vars {
            let mut sorted_names: Vec<&String> = exported_vars.iter().collect();
            sorted_names.sort();

            for name in sorted_names {
                if let Some(value) = state.env.get(name) {
                    // Quote the value with double quotes, escaping backslashes and double quotes
                    let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
                    stdout.push_str(&format!("declare -x {}=\"{}\"\n", name, escaped_value));
                }
            }
        }
        return BuiltinResult {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }

    // Handle un-export: remove export attribute but keep variable value
    // In bash, `export -n name=value` sets the value AND removes export attribute
    if unexport {
        for arg in processed_args {
            let name: &str;
            let value: Option<String>;

            if let Some(eq_idx) = arg.find('=') {
                name = &arg[..eq_idx];
                value = Some(expand_tildes_in_value(&state.env, &arg[eq_idx + 1..]));
            } else {
                name = arg;
                value = None;
            }

            // Set the value if provided
            if let Some(v) = value {
                state.env.insert(name.to_string(), v);
            }

            // Remove export attribute without deleting the variable
            unmark_exported(state, name);
        }
        return BuiltinResult::ok();
    }

    // Process each argument
    let mut stderr = String::new();
    let mut exit_code = 0;

    for arg in processed_args {
        let name: String;
        let value: Option<String>;
        let is_append: bool;

        // Check for += append syntax: export NAME+=value
        if let Some((n, v)) = parse_append_syntax(arg) {
            name = n.to_string();
            value = Some(expand_tildes_in_value(&state.env, v));
            is_append = true;
        } else if let Some(eq_idx) = arg.find('=') {
            // export NAME=value
            name = arg[..eq_idx].to_string();
            value = Some(expand_tildes_in_value(&state.env, &arg[eq_idx + 1..]));
            is_append = false;
        } else {
            // export NAME (without value)
            name = arg.to_string();
            value = None;
            is_append = false;
        }

        // Validate variable name: must start with letter/underscore, contain only alphanumeric/_
        if !is_valid_var_name(&name) {
            stderr.push_str(&format!("bash: export: `{}': not a valid identifier\n", arg));
            exit_code = 1;
            continue;
        }

        if let Some(v) = value {
            if is_append {
                // Append to existing value (or set if not defined)
                let existing = state.env.get(&name).cloned().unwrap_or_default();
                state.env.insert(name.clone(), existing + &v);
            } else {
                state.env.insert(name.clone(), v);
            }
        } else {
            // If variable doesn't exist, create it as empty
            if !state.env.contains_key(&name) {
                state.env.insert(name.clone(), String::new());
            }
        }

        // Mark the variable as exported
        mark_exported(state, &name);
    }

    BuiltinResult {
        stdout: String::new(),
        stderr,
        exit_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_set_and_export() {
        let mut state = InterpreterState::default();
        let result = handle_export(&mut state, &["FOO=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("FOO").unwrap(), "bar");
        assert!(state.exported_vars.as_ref().unwrap().contains("FOO"));
    }

    #[test]
    fn test_export_existing_variable() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "existing".to_string());

        let result = handle_export(&mut state, &["FOO".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("FOO").unwrap(), "existing");
        assert!(state.exported_vars.as_ref().unwrap().contains("FOO"));
    }

    #[test]
    fn test_export_new_variable_without_value() {
        let mut state = InterpreterState::default();
        let result = handle_export(&mut state, &["NEW_VAR".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("NEW_VAR").unwrap(), "");
        assert!(state.exported_vars.as_ref().unwrap().contains("NEW_VAR"));
    }

    #[test]
    fn test_export_append() {
        let mut state = InterpreterState::default();
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());

        let result = handle_export(&mut state, &["PATH+=/home/bin".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("PATH").unwrap(), "/usr/bin/home/bin");
    }

    #[test]
    fn test_export_unexport() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        mark_exported(&mut state, "FOO");

        let result = handle_export(&mut state, &["-n".to_string(), "FOO".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("FOO").unwrap(), "bar"); // Value preserved
        assert!(!state.exported_vars.as_ref().unwrap().contains("FOO")); // No longer exported
    }

    #[test]
    fn test_export_invalid_name() {
        let mut state = InterpreterState::default();
        let result = handle_export(&mut state, &["123invalid=value".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not a valid identifier"));
    }

    #[test]
    fn test_export_list() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.env.insert("BAZ".to_string(), "qux".to_string());
        mark_exported(&mut state, "FOO");
        mark_exported(&mut state, "BAZ");

        let result = handle_export(&mut state, &[]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("declare -x BAZ=\"qux\""));
        assert!(result.stdout.contains("declare -x FOO=\"bar\""));
    }

    #[test]
    fn test_export_tilde_expansion() {
        let mut state = InterpreterState::default();
        state.env.insert("HOME".to_string(), "/home/user".to_string());

        let result = handle_export(&mut state, &["PATH=~/bin:/usr/bin".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("PATH").unwrap(), "/home/user/bin:/usr/bin");
    }

    #[test]
    fn test_is_valid_var_name() {
        assert!(is_valid_var_name("FOO"));
        assert!(is_valid_var_name("_foo"));
        assert!(is_valid_var_name("foo123"));
        assert!(is_valid_var_name("_123"));
        assert!(!is_valid_var_name("123foo"));
        assert!(!is_valid_var_name(""));
        assert!(!is_valid_var_name("foo-bar"));
    }
}
