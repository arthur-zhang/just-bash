//! complete - Set and display programmable completion specifications
//!
//! Usage:
//!   complete                        - List all completion specs
//!   complete -p                     - Print all completion specs in reusable format
//!   complete -p cmd                 - Print completion spec for specific command
//!   complete -W 'word1 word2' cmd   - Set word list completion for cmd
//!   complete -F func cmd            - Set function completion for cmd
//!   complete -r cmd                 - Remove completion spec for cmd
//!   complete -r                     - Remove all completion specs
//!   complete -D ...                 - Set default completion (for commands with no specific spec)
//!   complete -o opt cmd             - Set completion options (nospace, filenames, default, etc.)

use std::collections::HashMap;
use crate::interpreter::types::{CompletionSpec, InterpreterState};
use super::break_cmd::BuiltinResult;

/// Valid completion options for -o flag
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

/// Handle the complete builtin command.
pub fn handle_complete(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Initialize completionSpecs if not present
    if state.completion_specs.is_none() {
        state.completion_specs = Some(HashMap::new());
    }

    // Parse options
    let mut print_mode = false;
    let mut remove_mode = false;
    let mut is_default = false;
    let mut wordlist: Option<String> = None;
    let mut func_name: Option<String> = None;
    let mut command_str: Option<String> = None;
    let mut options: Vec<String> = Vec::new();
    let mut actions: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-p" {
            print_mode = true;
        } else if arg == "-r" {
            remove_mode = true;
        } else if arg == "-D" {
            is_default = true;
        } else if arg == "-W" {
            // Word list
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("complete: -W: option requires an argument\n", 2);
            }
            wordlist = Some(args[i].clone());
        } else if arg == "-F" {
            // Function name
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("complete: -F: option requires an argument\n", 2);
            }
            func_name = Some(args[i].clone());
        } else if arg == "-o" {
            // Completion option
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("complete: -o: option requires an argument\n", 2);
            }
            let opt = &args[i];
            if !VALID_OPTIONS.contains(&opt.as_str()) {
                return BuiltinResult::failure(&format!("complete: {}: invalid option name\n", opt), 2);
            }
            options.push(opt.clone());
        } else if arg == "-A" {
            // Action
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("complete: -A: option requires an argument\n", 2);
            }
            actions.push(args[i].clone());
        } else if arg == "-C" {
            // Command to run for completion
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure("complete: -C: option requires an argument\n", 2);
            }
            command_str = Some(args[i].clone());
        } else if arg == "-G" || arg == "-P" || arg == "-S" || arg == "-X" {
            // Skip these options (not fully implemented)
            i += 1;
            if i >= args.len() {
                return BuiltinResult::failure(&format!("complete: {}: option requires an argument\n", arg), 2);
            }
        } else if arg == "--" {
            // End of options
            commands.extend(args[i + 1..].iter().cloned());
            break;
        } else if !arg.starts_with('-') {
            commands.push(arg.clone());
        }
        i += 1;
    }

    let specs = state.completion_specs.as_mut().unwrap();

    // Handle remove mode (-r)
    if remove_mode {
        if commands.is_empty() {
            // Remove all completion specs
            specs.clear();
            return BuiltinResult::ok();
        }
        // Remove specific completion specs
        for cmd in &commands {
            specs.remove(cmd);
        }
        return BuiltinResult::ok();
    }

    // Handle print mode (-p)
    if print_mode {
        if commands.is_empty() {
            // Print all completion specs
            return print_completion_specs(specs, None);
        }
        // Print specific completion specs
        return print_completion_specs(specs, Some(&commands));
    }

    // If no options provided and no commands, just print all specs
    if args.is_empty()
        || (commands.is_empty()
            && wordlist.is_none()
            && func_name.is_none()
            && command_str.is_none()
            && options.is_empty()
            && actions.is_empty()
            && !is_default)
    {
        return print_completion_specs(specs, None);
    }

    // Check for usage errors
    // -F requires a command name (unless -D is specified)
    if func_name.is_some() && commands.is_empty() && !is_default {
        return BuiltinResult::failure("complete: -F: option requires a command name\n", 2);
    }

    // Set completion specs for commands
    if is_default {
        // Set default completion
        let spec = CompletionSpec {
            wordlist,
            function: func_name,
            command: command_str,
            options: if options.is_empty() { None } else { Some(options) },
            actions: if actions.is_empty() { None } else { Some(actions) },
            is_default: Some(true),
        };
        specs.insert("__default__".to_string(), spec);
        return BuiltinResult::ok();
    }

    for cmd in &commands {
        let spec = CompletionSpec {
            wordlist: wordlist.clone(),
            function: func_name.clone(),
            command: command_str.clone(),
            options: if options.is_empty() { None } else { Some(options.clone()) },
            actions: if actions.is_empty() { None } else { Some(actions.clone()) },
            is_default: None,
        };
        specs.insert(cmd.clone(), spec);
    }

    BuiltinResult::ok()
}

/// Print completion specs in reusable format
fn print_completion_specs(
    specs: &HashMap<String, CompletionSpec>,
    commands: Option<&[String]>,
) -> BuiltinResult {
    if specs.is_empty() {
        if let Some(cmds) = commands {
            if !cmds.is_empty() {
                let mut stderr = String::new();
                for cmd in cmds {
                    stderr.push_str(&format!("complete: {}: no completion specification\n", cmd));
                }
                return BuiltinResult {
                    stdout: String::new(),
                    stderr,
                    exit_code: 1,
                };
            }
        }
        return BuiltinResult::ok();
    }

    let mut output: Vec<String> = Vec::new();
    let target_commands: Vec<&String> = match commands {
        Some(cmds) => cmds.iter().collect(),
        None => specs.keys().collect(),
    };

    for cmd in target_commands {
        if cmd == "__default__" {
            continue; // Skip internal default key when listing all
        }

        let spec = match specs.get(cmd) {
            Some(s) => s,
            None => {
                if commands.is_some() {
                    // Specifically requested this command but it doesn't exist
                    return BuiltinResult {
                        stdout: if output.is_empty() {
                            String::new()
                        } else {
                            format!("{}\n", output.join("\n"))
                        },
                        stderr: format!("complete: {}: no completion specification\n", cmd),
                        exit_code: 1,
                    };
                }
                continue;
            }
        };

        let mut line = "complete".to_string();

        // Add options
        if let Some(ref opts) = spec.options {
            for opt in opts {
                line.push_str(&format!(" -o {}", opt));
            }
        }

        // Add actions
        if let Some(ref acts) = spec.actions {
            for action in acts {
                line.push_str(&format!(" -A {}", action));
            }
        }

        // Add wordlist
        if let Some(ref wl) = spec.wordlist {
            // Quote the wordlist if it contains spaces
            if wl.contains(' ') || wl.contains('\'') {
                line.push_str(&format!(" -W '{}'", wl));
            } else {
                line.push_str(&format!(" -W {}", wl));
            }
        }

        // Add function
        if let Some(ref func) = spec.function {
            line.push_str(&format!(" -F {}", func));
        }

        // Add default flag
        if spec.is_default == Some(true) {
            line.push_str(" -D");
        }

        // Add command name
        line.push_str(&format!(" {}", cmd));

        output.push(line);
    }

    if output.is_empty() {
        return BuiltinResult::ok();
    }

    BuiltinResult {
        stdout: format!("{}\n", output.join("\n")),
        stderr: String::new(),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_set_wordlist() {
        let mut state = InterpreterState::default();
        let result = handle_complete(&mut state, &[
            "-W".to_string(),
            "foo bar baz".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("mycommand").unwrap();
        assert_eq!(spec.wordlist, Some("foo bar baz".to_string()));
    }

    #[test]
    fn test_complete_set_function() {
        let mut state = InterpreterState::default();
        let result = handle_complete(&mut state, &[
            "-F".to_string(),
            "_my_completion".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("mycommand").unwrap();
        assert_eq!(spec.function, Some("_my_completion".to_string()));
    }

    #[test]
    fn test_complete_remove() {
        let mut state = InterpreterState::default();
        state.completion_specs = Some(HashMap::new());
        state.completion_specs.as_mut().unwrap().insert(
            "mycommand".to_string(),
            CompletionSpec::default(),
        );

        let result = handle_complete(&mut state, &["-r".to_string(), "mycommand".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(!state.completion_specs.unwrap().contains_key("mycommand"));
    }

    #[test]
    fn test_complete_invalid_option() {
        let mut state = InterpreterState::default();
        let result = handle_complete(&mut state, &[
            "-o".to_string(),
            "invalid".to_string(),
            "mycommand".to_string(),
        ]);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("invalid option name"));
    }

    #[test]
    fn test_complete_default() {
        let mut state = InterpreterState::default();
        let result = handle_complete(&mut state, &[
            "-D".to_string(),
            "-W".to_string(),
            "default words".to_string(),
        ]);
        assert_eq!(result.exit_code, 0);

        let specs = state.completion_specs.unwrap();
        let spec = specs.get("__default__").unwrap();
        assert_eq!(spec.is_default, Some(true));
    }
}
