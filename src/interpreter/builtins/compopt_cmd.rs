//! compopt - Modify completion options
//!
//! Usage:
//!   compopt [-o option] [+o option] [name ...]
//!   compopt -D [-o option] [+o option]
//!   compopt -E [-o option] [+o option]
//!
//! Modifies completion options for the specified commands (names) or the
//! currently executing completion when no names are provided.
//!
//! Options:
//!   -o option  Enable completion option
//!   +o option  Disable completion option
//!   -D         Apply to default completion
//!   -E         Apply to empty-line completion
//!
//! Valid completion options:
//!   bashdefault, default, dirnames, filenames, noquote, nosort, nospace, plusdirs
//!
//! Returns:
//!   0 on success
//!   1 if not in a completion function and no command name is given
//!   2 if an invalid option is specified

use std::collections::{HashMap, HashSet};
use crate::interpreter::types::{CompletionSpec, InterpreterState};
use super::break_cmd::BuiltinResult;

/// Valid completion options for -o/+o flags
const VALID_OPTIONS: &[&str] = &[
    "bashdefault",
    "default",
    "dirnames",
    "filenames",
    "noquote",
    "nosort",
    "nospace",
    "plusdirs",
];

/// Handle the compopt builtin command.
pub fn handle_compopt(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Initialize completionSpecs if not present
    if state.completion_specs.is_none() {
        state.completion_specs = Some(HashMap::new());
    }

    // Parse options
    let mut is_default = false;
    let mut is_empty_line = false;
    let mut enable_options: Vec<String> = Vec::new();
    let mut disable_options: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-D" {
            is_default = true;
        } else if arg == "-E" {
            is_empty_line = true;
        } else if arg == "-o" {
            // Enable completion option
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("compopt: -o: option requires an argument\n", 2);
            }
            let opt = &args[i];
            if !VALID_OPTIONS.contains(&opt.as_str()) {
                return BuiltinResult::failure(&format!("compopt: {}: invalid option name\n", opt), 2);
            }
            enable_options.push(opt.clone());
        } else if arg == "+o" {
            // Disable completion option
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("compopt: +o: option requires an argument\n", 2);
            }
            let opt = &args[i];
            if !VALID_OPTIONS.contains(&opt.as_str()) {
                return BuiltinResult::failure(&format!("compopt: {}: invalid option name\n", opt), 2);
            }
            disable_options.push(opt.clone());
        } else if arg == "--" {
            // End of options
            commands.extend(args[i + 1..].iter().cloned());
            break;
        } else if !arg.starts_with('-') && !arg.starts_with('+') {
            commands.push(arg.clone());
        }
        i += 1;
    }

    let specs = state.completion_specs.as_mut().unwrap();

    // If -D flag is set, modify default completion
    if is_default {
        let spec = specs.entry("__default__".to_string()).or_insert_with(|| {
            CompletionSpec {
                is_default: Some(true),
                ..Default::default()
            }
        });

        let mut current_options: HashSet<String> = spec.options
            .as_ref()
            .map(|o| o.iter().cloned().collect())
            .unwrap_or_default();

        // Enable options
        for opt in &enable_options {
            current_options.insert(opt.clone());
        }

        // Disable options
        for opt in &disable_options {
            current_options.remove(opt);
        }

        spec.options = if current_options.is_empty() {
            None
        } else {
            Some(current_options.into_iter().collect())
        };

        return BuiltinResult::ok();
    }

    // If -E flag is set, modify empty-line completion
    if is_empty_line {
        let spec = specs.entry("__empty__".to_string()).or_insert_with(CompletionSpec::default);

        let mut current_options: HashSet<String> = spec.options
            .as_ref()
            .map(|o| o.iter().cloned().collect())
            .unwrap_or_default();

        // Enable options
        for opt in &enable_options {
            current_options.insert(opt.clone());
        }

        // Disable options
        for opt in &disable_options {
            current_options.remove(opt);
        }

        spec.options = if current_options.is_empty() {
            None
        } else {
            Some(current_options.into_iter().collect())
        };

        return BuiltinResult::ok();
    }

    // If command names are provided, modify their completion specs
    if !commands.is_empty() {
        for cmd in &commands {
            let spec = specs.entry(cmd.clone()).or_insert_with(CompletionSpec::default);

            let mut current_options: HashSet<String> = spec.options
                .as_ref()
                .map(|o| o.iter().cloned().collect())
                .unwrap_or_default();

            // Enable options
            for opt in &enable_options {
                current_options.insert(opt.clone());
            }

            // Disable options
            for opt in &disable_options {
                current_options.remove(opt);
            }

            spec.options = if current_options.is_empty() {
                None
            } else {
                Some(current_options.into_iter().collect())
            };
        }
        return BuiltinResult::ok();
    }

    // No command name and not -D/-E: we need to be in a completion function
    // In bash, compopt modifies the current completion context when called
    // from within a completion function. Since we don't have a completion
    // context indicator, we fail if no command name is given.
    BuiltinResult::failure("compopt: not currently executing completion function\n", 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compopt_enable_option() {
        let mut state = InterpreterState::default();
        state.completion_specs = Some(HashMap::new());
        state.completion_specs.as_mut().unwrap().insert(
            "mycommand".to_string(),
            CompletionSpec::default(),
        );

        let result = handle_compopt(&mut state, &[
            "-o".to_string(),
            "nospace".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("mycommand").unwrap();
        assert!(spec.options.as_ref().unwrap().contains(&"nospace".to_string()));
    }

    #[test]
    fn test_compopt_disable_option() {
        let mut state = InterpreterState::default();
        state.completion_specs = Some(HashMap::new());
        state.completion_specs.as_mut().unwrap().insert(
            "mycommand".to_string(),
            CompletionSpec {
                options: Some(vec!["nospace".to_string(), "filenames".to_string()]),
                ..Default::default()
            },
        );

        let result = handle_compopt(&mut state, &[
            "+o".to_string(),
            "nospace".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("mycommand").unwrap();
        assert!(!spec.options.as_ref().unwrap().contains(&"nospace".to_string()));
        assert!(spec.options.as_ref().unwrap().contains(&"filenames".to_string()));
    }

    #[test]
    fn test_compopt_default() {
        let mut state = InterpreterState::default();

        let result = handle_compopt(&mut state, &[
            "-D".to_string(),
            "-o".to_string(),
            "nospace".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("__default__").unwrap();
        assert!(spec.options.as_ref().unwrap().contains(&"nospace".to_string()));
    }

    #[test]
    fn test_compopt_invalid_option() {
        let mut state = InterpreterState::default();

        let result = handle_compopt(&mut state, &[
            "-o".to_string(),
            "invalid".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("invalid option name"));
    }

    #[test]
    fn test_compopt_no_command() {
        let mut state = InterpreterState::default();

        let result = handle_compopt(&mut state, &[
            "-o".to_string(),
            "nospace".to_string(),
        ]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not currently executing completion function"));
    }
}
