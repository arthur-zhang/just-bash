//! set - Set/unset shell options builtin
//!
//! In POSIX mode (set -o posix), errors from set (like invalid options)
//! cause the script to exit immediately.

use crate::interpreter::errors::{InterpreterError, PosixFatalError};
use crate::interpreter::helpers::array::{get_array_indices, get_assoc_array_keys};
use crate::interpreter::helpers::quoting::{quote_value, quote_array_value};
use crate::interpreter::helpers::shellopts::update_shellopts;
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

const SET_USAGE: &str = r#"set: usage: set [-eux] [+eux] [-o option] [+o option]
Options:
  -e            Exit immediately if a command exits with non-zero status
  +e            Disable -e
  -u            Treat unset variables as an error when substituting
  +u            Disable -u
  -x            Print commands and their arguments as they are executed
  +x            Disable -x
  -o errexit    Same as -e
  +o errexit    Disable errexit
  -o nounset    Same as -u
  +o nounset    Disable nounset
  -o pipefail   Return status of last failing command in pipeline
  +o pipefail   Disable pipefail
  -o xtrace     Same as -x
  +o xtrace     Disable xtrace
"#;

/// Map short options to their corresponding shell option
fn get_short_option(flag: char) -> Option<&'static str> {
    match flag {
        'e' => Some("errexit"),
        'u' => Some("nounset"),
        'x' => Some("xtrace"),
        'v' => Some("verbose"),
        'f' => Some("noglob"),
        'C' => Some("noclobber"),
        'a' => Some("allexport"),
        'n' => Some("noexec"),
        // No-ops (accepted for compatibility)
        'h' | 'b' | 'm' | 'B' | 'H' | 'P' | 'T' | 'E' | 'p' => Some(""),
        _ => None,
    }
}

/// Map long options to their corresponding shell option
fn get_long_option(name: &str) -> Option<&'static str> {
    match name {
        "errexit" => Some("errexit"),
        "pipefail" => Some("pipefail"),
        "nounset" => Some("nounset"),
        "xtrace" => Some("xtrace"),
        "verbose" => Some("verbose"),
        "noclobber" => Some("noclobber"),
        "noglob" => Some("noglob"),
        "allexport" => Some("allexport"),
        "noexec" => Some("noexec"),
        "posix" => Some("posix"),
        "vi" => Some("vi"),
        "emacs" => Some("emacs"),
        // No-ops (accepted for compatibility)
        "notify" | "monitor" | "braceexpand" | "histexpand" | "physical" |
        "functrace" | "errtrace" | "privileged" | "hashall" | "ignoreeof" |
        "interactive-comments" | "keyword" | "onecmd" => Some(""),
        _ => None,
    }
}

/// List of implemented options to display
const DISPLAY_OPTIONS: &[&str] = &[
    "allexport", "emacs", "errexit", "noclobber", "noexec", "noglob",
    "nounset", "pipefail", "posix", "verbose", "vi", "xtrace",
];

/// List of no-op options to display (always off)
const NOOP_DISPLAY_OPTIONS: &[&str] = &[
    "braceexpand", "errtrace", "functrace", "hashall", "histexpand",
    "history", "ignoreeof", "interactive-comments", "keyword", "monitor",
    "nolog", "notify", "onecmd", "physical", "privileged",
];

/// Set a shell option value
fn set_shell_option(state: &mut InterpreterState, option: &str, value: bool) {
    if option.is_empty() {
        return; // No-op option
    }

    // Handle mutual exclusivity of vi and emacs
    if value {
        if option == "vi" {
            state.options.emacs = false;
        } else if option == "emacs" {
            state.options.vi = false;
        }
    }

    match option {
        "errexit" => state.options.errexit = value,
        "pipefail" => state.options.pipefail = value,
        "nounset" => state.options.nounset = value,
        "xtrace" => state.options.xtrace = value,
        "verbose" => state.options.verbose = value,
        "noclobber" => state.options.noclobber = value,
        "noglob" => state.options.noglob = value,
        "allexport" => state.options.allexport = value,
        "noexec" => state.options.noexec = value,
        "posix" => state.options.posix = value,
        "vi" => state.options.vi = value,
        "emacs" => state.options.emacs = value,
        _ => {}
    }

    update_shellopts(&mut state.env, &state.options);
}

/// Get a shell option value
fn get_shell_option(state: &InterpreterState, option: &str) -> bool {
    match option {
        "errexit" => state.options.errexit,
        "pipefail" => state.options.pipefail,
        "nounset" => state.options.nounset,
        "xtrace" => state.options.xtrace,
        "verbose" => state.options.verbose,
        "noclobber" => state.options.noclobber,
        "noglob" => state.options.noglob,
        "allexport" => state.options.allexport,
        "noexec" => state.options.noexec,
        "posix" => state.options.posix,
        "vi" => state.options.vi,
        "emacs" => state.options.emacs,
        _ => false,
    }
}

/// Check if the next argument exists and is not an option flag
fn has_non_option_arg(args: &[String], i: usize) -> bool {
    i + 1 < args.len()
        && !args[i + 1].starts_with('-')
        && !args[i + 1].starts_with('+')
}

/// Quote a key for associative array output
fn quote_assoc_key(key: &str) -> String {
    // If key contains no special chars, return as-is
    if key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return key.to_string();
    }
    // Use double quotes for keys with spaces or shell metacharacters
    let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

/// Format an array variable for set output
fn format_array_output(state: &InterpreterState, array_name: &str) -> String {
    let indices = get_array_indices(&state.env, array_name);
    if indices.is_empty() {
        return format!("{}=()", array_name);
    }

    let elements: Vec<String> = indices
        .iter()
        .map(|i| {
            let key = format!("{}_{}", array_name, i);
            let value = state.env.get(&key).map(|s| s.as_str()).unwrap_or("");
            format!("[{}]={}", i, quote_array_value(value))
        })
        .collect();

    format!("{}=({})", array_name, elements.join(" "))
}

/// Format an associative array variable for set output
fn format_assoc_array_output(state: &InterpreterState, array_name: &str) -> String {
    let keys = get_assoc_array_keys(&state.env, array_name);
    if keys.is_empty() {
        return format!("{}=()", array_name);
    }

    let elements: Vec<String> = keys
        .iter()
        .map(|k| {
            let env_key = format!("{}_{}", array_name, k);
            let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
            format!("[{}]={}", quote_assoc_key(k), quote_array_value(value))
        })
        .collect();

    // Note: bash has a trailing space before the closing paren for assoc arrays
    format!("{}=({} )", array_name, elements.join(" "))
}

/// Get all indexed array names from the environment
fn get_indexed_array_names(state: &InterpreterState) -> Vec<String> {
    let assoc_arrays = state.associative_arrays.as_ref();
    let mut array_names: Vec<String> = Vec::new();

    for key in state.env.keys() {
        // Match array element pattern: name_index where index is numeric
        if let Some(underscore_pos) = key.rfind('_') {
            let name = &key[..underscore_pos];
            let index_str = &key[underscore_pos + 1..];

            // Check if it's a numeric index
            if index_str.parse::<i64>().is_ok() {
                // Exclude associative arrays
                if assoc_arrays.map_or(true, |a| !a.contains(name)) {
                    if !array_names.contains(&name.to_string()) {
                        array_names.push(name.to_string());
                    }
                }
            }
        }
    }

    array_names.sort();
    array_names
}

/// Handle the set builtin command.
pub fn handle_set(state: &mut InterpreterState, args: &[String]) -> Result<BuiltinResult, InterpreterError> {
    if args.iter().any(|a| a == "--help") {
        return Ok(BuiltinResult {
            stdout: SET_USAGE.to_string(),
            stderr: String::new(),
            exit_code: 0,
        });
    }

    // With no arguments, print all shell variables
    if args.is_empty() {
        let indexed_array_names = get_indexed_array_names(state);
        let assoc_array_names: Vec<String> = state.associative_arrays
            .as_ref()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();

        // Collect scalar variables
        let mut lines: Vec<String> = Vec::new();

        for (key, value) in &state.env {
            // Only valid variable names
            if !key.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_') {
                continue;
            }
            if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }

            // Skip if this is an indexed array
            if indexed_array_names.contains(key) {
                continue;
            }
            // Skip if this is an associative array
            if assoc_array_names.contains(key) {
                continue;
            }
            // Skip array element variables
            if let Some(underscore_pos) = key.rfind('_') {
                let name = &key[..underscore_pos];
                let suffix = &key[underscore_pos + 1..];
                if indexed_array_names.contains(&name.to_string()) && suffix.parse::<i64>().is_ok() {
                    continue;
                }
                if suffix == "_length" && (indexed_array_names.contains(&name.to_string()) || assoc_array_names.contains(&name.to_string())) {
                    continue;
                }
            }
            // Skip associative array elements
            let is_assoc_element = assoc_array_names.iter().any(|arr| {
                let prefix = format!("{}_", arr);
                key.starts_with(&prefix) && !key.ends_with("__length")
            });
            if is_assoc_element {
                continue;
            }

            lines.push(format!("{}={}", key, quote_value(value)));
        }

        // Add indexed arrays
        for array_name in &indexed_array_names {
            lines.push(format_array_output(state, array_name));
        }

        // Add associative arrays
        for array_name in &assoc_array_names {
            lines.push(format_assoc_array_output(state, array_name));
        }

        // Sort all lines
        lines.sort();

        let stdout = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        return Ok(BuiltinResult {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        });
    }

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Handle -o / +o with option name
        if (arg == "-o" || arg == "+o") && has_non_option_arg(args, i) {
            let opt_name = &args[i + 1];
            match get_long_option(opt_name) {
                Some(option) => {
                    set_shell_option(state, option, arg == "-o");
                }
                None => {
                    let error_msg = format!("bash: set: {}: invalid option name\n{}", opt_name, SET_USAGE);
                    if state.options.posix {
                        return Err(PosixFatalError::new(1, String::new(), error_msg).into());
                    }
                    return Ok(BuiltinResult::failure(&error_msg, 1));
                }
            }
            i += 2;
            continue;
        }

        // Handle -o alone (print current settings)
        if arg == "-o" {
            let mut all_options: Vec<String> = Vec::new();

            for opt in DISPLAY_OPTIONS {
                let value = get_shell_option(state, opt);
                all_options.push(format!("{:16}{}", opt, if value { "on" } else { "off" }));
            }
            for opt in NOOP_DISPLAY_OPTIONS {
                all_options.push(format!("{:16}off", opt));
            }

            all_options.sort();
            return Ok(BuiltinResult {
                stdout: format!("{}\n", all_options.join("\n")),
                stderr: String::new(),
                exit_code: 0,
            });
        }

        // Handle +o alone (print commands to recreate settings)
        if arg == "+o" {
            let mut all_options: Vec<String> = Vec::new();

            for opt in DISPLAY_OPTIONS {
                let value = get_shell_option(state, opt);
                all_options.push(format!("set {} {}", if value { "-o" } else { "+o" }, opt));
            }
            for opt in NOOP_DISPLAY_OPTIONS {
                all_options.push(format!("set +o {}", opt));
            }

            all_options.sort();
            return Ok(BuiltinResult {
                stdout: format!("{}\n", all_options.join("\n")),
                stderr: String::new(),
                exit_code: 0,
            });
        }

        // Handle combined short flags like -eu or +eu
        if arg.len() > 1 && (arg.starts_with('-') || arg.starts_with('+')) && !arg.starts_with("--") {
            let enable = arg.starts_with('-');
            for flag in arg[1..].chars() {
                match get_short_option(flag) {
                    Some(option) => {
                        set_shell_option(state, option, enable);
                    }
                    None => {
                        let error_msg = format!("bash: set: {}{}: invalid option\n{}", if enable { '-' } else { '+' }, flag, SET_USAGE);
                        if state.options.posix {
                            return Err(PosixFatalError::new(1, String::new(), error_msg).into());
                        }
                        return Ok(BuiltinResult::failure(&error_msg, 1));
                    }
                }
            }
            i += 1;
            continue;
        }

        // Handle -- (end of options)
        if arg == "--" {
            set_positional_parameters(state, &args[i + 1..]);
            return Ok(BuiltinResult::ok());
        }

        // Handle - (disable xtrace and verbose, end of options)
        if arg == "-" {
            state.options.xtrace = false;
            state.options.verbose = false;
            update_shellopts(&mut state.env, &state.options);
            if i + 1 < args.len() {
                set_positional_parameters(state, &args[i + 1..]);
                return Ok(BuiltinResult::ok());
            }
            i += 1;
            continue;
        }

        // Handle + (single + is ignored)
        if arg == "+" {
            i += 1;
            continue;
        }

        // Invalid option
        if arg.starts_with('-') || arg.starts_with('+') {
            let error_msg = format!("bash: set: {}: invalid option\n{}", arg, SET_USAGE);
            if state.options.posix {
                return Err(PosixFatalError::new(1, String::new(), error_msg).into());
            }
            return Ok(BuiltinResult::failure(&error_msg, 1));
        }

        // Non-option arguments are positional parameters
        set_positional_parameters(state, &args[i..]);
        return Ok(BuiltinResult::ok());
    }

    Ok(BuiltinResult::ok())
}

/// Set positional parameters ($1, $2, etc.) and update $@, $*, $#
fn set_positional_parameters(state: &mut InterpreterState, params: &[String]) {
    // Clear existing positional parameters
    let mut i = 1;
    while state.env.contains_key(&i.to_string()) {
        state.env.remove(&i.to_string());
        i += 1;
    }

    // Set new positional parameters
    for (j, param) in params.iter().enumerate() {
        state.env.insert((j + 1).to_string(), param.clone());
    }

    // Update $# (number of parameters)
    state.env.insert("#".to_string(), params.len().to_string());

    // Update $@ and $* (all parameters)
    let all_params = params.join(" ");
    state.env.insert("@".to_string(), all_params.clone());
    state.env.insert("*".to_string(), all_params);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_errexit() {
        let mut state = InterpreterState::default();
        assert!(!state.options.errexit);

        let result = handle_set(&mut state, &["-e".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(state.options.errexit);
    }

    #[test]
    fn test_set_unset_errexit() {
        let mut state = InterpreterState::default();
        state.options.errexit = true;

        let result = handle_set(&mut state, &["+e".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!state.options.errexit);
    }

    #[test]
    fn test_set_o_errexit() {
        let mut state = InterpreterState::default();

        let result = handle_set(&mut state, &["-o".to_string(), "errexit".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(state.options.errexit);
    }

    #[test]
    fn test_set_combined_flags() {
        let mut state = InterpreterState::default();

        let result = handle_set(&mut state, &["-eux".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(state.options.errexit);
        assert!(state.options.nounset);
        assert!(state.options.xtrace);
    }

    #[test]
    fn test_set_positional_params() {
        let mut state = InterpreterState::default();

        let result = handle_set(&mut state, &["--".to_string(), "a".to_string(), "b".to_string(), "c".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("1").unwrap(), "a");
        assert_eq!(state.env.get("2").unwrap(), "b");
        assert_eq!(state.env.get("3").unwrap(), "c");
        assert_eq!(state.env.get("#").unwrap(), "3");
    }

    #[test]
    fn test_set_invalid_option() {
        let mut state = InterpreterState::default();

        let result = handle_set(&mut state, &["-z".to_string()]).unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("invalid option"));
    }

    #[test]
    fn test_set_o_print() {
        let mut state = InterpreterState::default();
        state.options.errexit = true;

        let result = handle_set(&mut state, &["-o".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("errexit"));
        assert!(result.stdout.contains("on"));
    }

    #[test]
    fn test_set_plus_o_print() {
        let mut state = InterpreterState::default();
        state.options.errexit = true;

        let result = handle_set(&mut state, &["+o".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("set -o errexit"));
    }
}
