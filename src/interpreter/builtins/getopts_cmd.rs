//! getopts - Parse positional parameters as options
//!
//! getopts optstring name [arg...]
//!
//! Parses options from positional parameters (or provided args).
//! - optstring: string of valid option characters
//! - If a character is followed by ':', it requires an argument
//! - If optstring starts with ':', silent error reporting mode
//! - name: variable to store the current option
//! - OPTARG: set to the option argument (if any)
//! - OPTIND: index of next argument to process (starts at 1)
//!
//! Returns 0 if option found, 1 if end of options or error.

use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// Check if a string is a valid variable name.
fn is_valid_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Handle the getopts builtin command.
///
/// # Arguments
/// * `state` - The interpreter state (mutable for modifying env)
/// * `args` - Command arguments
///
/// # Returns
/// BuiltinResult with exit code 0 if option found, 1 if end of options
pub fn handle_getopts(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Need at least optstring and name
    if args.len() < 2 {
        return BuiltinResult::failure("bash: getopts: usage: getopts optstring name [arg ...]\n", 1);
    }

    let optstring = &args[0];
    let var_name = &args[1];

    // Check if variable name is valid
    let invalid_var_name = !is_valid_var_name(var_name);

    // Determine if silent mode (optstring starts with ':')
    let silent_mode = optstring.starts_with(':');
    let actual_optstring = if silent_mode { &optstring[1..] } else { optstring.as_str() };

    // Get arguments to parse - either explicit args or positional parameters
    let args_to_process: Vec<String> = if args.len() > 2 {
        // Explicit arguments provided
        args[2..].to_vec()
    } else {
        // Use positional parameters
        let param_count: i32 = state.env.get("#")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (1..=param_count)
            .map(|i| state.env.get(&i.to_string()).cloned().unwrap_or_default())
            .collect()
    };

    // Get current OPTIND (1-based, default 1)
    let mut optind: i32 = state.env.get("OPTIND")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    if optind < 1 {
        optind = 1;
    }

    // Get the "char index" within the current argument for combined options like -abc
    let char_index: usize = state.env.get("__GETOPTS_CHARINDEX")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Clear OPTARG
    state.env.insert("OPTARG".to_string(), String::new());

    // Check if we've exhausted all arguments
    if optind as usize > args_to_process.len() {
        if !invalid_var_name {
            state.env.insert(var_name.clone(), "?".to_string());
        }
        state.env.insert("OPTIND".to_string(), (args_to_process.len() + 1).to_string());
        state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if invalid_var_name { 2 } else { 1 },
        };
    }

    // Get current argument (0-indexed in array, but OPTIND is 1-based)
    let current_arg = &args_to_process[(optind - 1) as usize];

    // Check if this is an option argument (starts with -)
    if current_arg.is_empty() || current_arg == "-" || !current_arg.starts_with('-') {
        // Not an option - end of options
        if !invalid_var_name {
            state.env.insert(var_name.clone(), "?".to_string());
        }
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if invalid_var_name { 2 } else { 1 },
        };
    }

    // Check for -- (end of options marker)
    if current_arg == "--" {
        state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
        state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        if !invalid_var_name {
            state.env.insert(var_name.clone(), "?".to_string());
        }
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if invalid_var_name { 2 } else { 1 },
        };
    }

    // Get the option character to process
    // char_index 0 means we're starting a new argument, so skip the leading '-'
    let start_index = if char_index == 0 { 1 } else { char_index };
    let opt_char = current_arg.chars().nth(start_index);

    let opt_char = match opt_char {
        Some(c) => c,
        None => {
            // No more characters in this argument, move to next
            state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
            // Recursively call to process next argument
            return handle_getopts(state, args);
        }
    };

    // Check if this option is valid
    let opt_index = actual_optstring.find(opt_char);
    if opt_index.is_none() {
        // Invalid option
        let stderr_msg = if !silent_mode {
            format!("bash: illegal option -- {}\n", opt_char)
        } else {
            state.env.insert("OPTARG".to_string(), opt_char.to_string());
            String::new()
        };
        if !invalid_var_name {
            state.env.insert(var_name.clone(), "?".to_string());
        }

        // Move to next character or next argument
        if start_index + 1 < current_arg.len() {
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), (start_index + 1).to_string());
            state.env.insert("OPTIND".to_string(), optind.to_string());
        } else {
            state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        }

        return BuiltinResult {
            stdout: String::new(),
            stderr: stderr_msg,
            exit_code: if invalid_var_name { 2 } else { 0 },
        };
    }

    let opt_index = opt_index.unwrap();

    // Check if this option requires an argument
    let requires_arg = actual_optstring.chars().nth(opt_index + 1) == Some(':');

    if requires_arg {
        // Option requires an argument
        // Check if there are more characters in the current arg (e.g., -cVALUE)
        if start_index + 1 < current_arg.len() {
            // Rest of current arg is the argument
            state.env.insert("OPTARG".to_string(), current_arg[start_index + 1..].to_string());
            state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        } else {
            // Next argument is the option argument
            if optind as usize >= args_to_process.len() {
                // No argument provided
                let stderr_msg = if !silent_mode {
                    if !invalid_var_name {
                        state.env.insert(var_name.clone(), "?".to_string());
                    }
                    format!("bash: option requires an argument -- {}\n", opt_char)
                } else {
                    state.env.insert("OPTARG".to_string(), opt_char.to_string());
                    if !invalid_var_name {
                        state.env.insert(var_name.clone(), ":".to_string());
                    }
                    String::new()
                };
                state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
                state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: stderr_msg,
                    exit_code: if invalid_var_name { 2 } else { 0 },
                };
            }
            state.env.insert("OPTARG".to_string(), args_to_process[optind as usize].clone());
            state.env.insert("OPTIND".to_string(), (optind + 2).to_string());
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        }
    } else {
        // Option doesn't require an argument
        // Move to next character or next argument
        if start_index + 1 < current_arg.len() {
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), (start_index + 1).to_string());
            state.env.insert("OPTIND".to_string(), optind.to_string());
        } else {
            state.env.insert("OPTIND".to_string(), (optind + 1).to_string());
            state.env.insert("__GETOPTS_CHARINDEX".to_string(), "0".to_string());
        }
    }

    // Set the variable to the option character (if valid variable name)
    if !invalid_var_name {
        state.env.insert(var_name.clone(), opt_char.to_string());
    }

    BuiltinResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: if invalid_var_name { 2 } else { 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getopts_simple_option() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            "ab".to_string(),
            "opt".to_string(),
            "-a".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "a");
        assert_eq!(state.env.get("OPTIND").unwrap(), "2");
    }

    #[test]
    fn test_getopts_option_with_argument() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            "a:".to_string(),
            "opt".to_string(),
            "-a".to_string(),
            "value".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "a");
        assert_eq!(state.env.get("OPTARG").unwrap(), "value");
        assert_eq!(state.env.get("OPTIND").unwrap(), "3");
    }

    #[test]
    fn test_getopts_combined_options() {
        let mut state = InterpreterState::default();

        // First call: -ab should return 'a'
        let result = handle_getopts(&mut state, &[
            "ab".to_string(),
            "opt".to_string(),
            "-ab".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "a");

        // Second call: should return 'b'
        let result = handle_getopts(&mut state, &[
            "ab".to_string(),
            "opt".to_string(),
            "-ab".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "b");
    }

    #[test]
    fn test_getopts_invalid_option() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            "ab".to_string(),
            "opt".to_string(),
            "-c".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "?");
        assert!(result.stderr.contains("illegal option"));
    }

    #[test]
    fn test_getopts_silent_mode() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            ":ab".to_string(),
            "opt".to_string(),
            "-c".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "?");
        assert_eq!(state.env.get("OPTARG").unwrap(), "c");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_getopts_end_of_options() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            "ab".to_string(),
            "opt".to_string(),
            "--".to_string(),
            "-a".to_string(),
        ]);
        assert_eq!(result.exit_code, 1);
        assert_eq!(state.env.get("opt").unwrap(), "?");
    }

    #[test]
    fn test_getopts_missing_argument() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &[
            "a:".to_string(),
            "opt".to_string(),
            "-a".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("opt").unwrap(), "?");
        assert!(result.stderr.contains("requires an argument"));
    }

    #[test]
    fn test_getopts_usage_error() {
        let mut state = InterpreterState::default();
        let result = handle_getopts(&mut state, &["ab".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("usage"));
    }
}
