//! Type Command Implementation
//!
//! Implements the `type` builtin command and related functionality:
//! - type [-afptP] name...
//! - command -v/-V name...

use std::collections::HashMap;
use crate::interpreter::helpers::shell_constants::{SHELL_BUILTINS, SHELL_KEYWORDS};
use crate::interpreter::types::{ExecResult, InterpreterState};

/// Context needed for type command operations
pub struct TypeCommandContext<'a> {
    pub state: &'a InterpreterState,
}

/// Handle the `type` builtin command.
/// type [-afptP] name...
pub fn handle_type<F, G>(
    ctx: &TypeCommandContext,
    args: &[String],
    find_first_in_path: F,
    find_all_in_path: G,
) -> ExecResult
where
    F: Fn(&str) -> Option<String>,
    G: Fn(&str) -> Vec<String>,
{
    // Parse options
    let mut type_only = false;      // -t flag
    let mut path_only = false;      // -p flag
    let mut force_path_search = false; // -P flag
    let mut show_all = false;       // -a flag
    let mut suppress_functions = false; // -f flag
    let mut names: Vec<&str> = Vec::new();

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    't' => type_only = true,
                    'p' => path_only = true,
                    'P' => force_path_search = true,
                    'a' => show_all = true,
                    'f' => suppress_functions = true,
                    _ => {}
                }
            }
        } else {
            names.push(arg);
        }
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut any_not_found = false;

    for name in names {
        let mut found_any = false;

        // -P flag: force PATH search
        if force_path_search {
            if show_all {
                let all_paths = find_all_in_path(name);
                if !all_paths.is_empty() {
                    for p in all_paths {
                        stdout.push_str(&format!("{}\n", p));
                    }
                    found_any = true;
                }
            } else if let Some(path_result) = find_first_in_path(name) {
                stdout.push_str(&format!("{}\n", path_result));
                found_any = true;
            }
            if !found_any {
                any_not_found = true;
            }
            continue;
        }

        // Check functions (unless -f suppresses them)
        let has_function = !suppress_functions && ctx.state.functions.contains_key(name);
        if has_function && (show_all || !found_any) {
            if !path_only {
                if type_only {
                    stdout.push_str("function\n");
                } else {
                    stdout.push_str(&format!("{} is a function\n", name));
                }
            }
            found_any = true;
            if !show_all {
                continue;
            }
        }

        // Check aliases
        if let Some(ref aliases) = ctx.state.aliases {
            if let Some(alias_value) = aliases.get(name) {
                if !path_only {
                    if type_only {
                        stdout.push_str("alias\n");
                    } else {
                        stdout.push_str(&format!("{} is aliased to `{}'\n", name, alias_value));
                    }
                }
                found_any = true;
                if !show_all {
                    continue;
                }
            }
        }

        // Check keywords
        if SHELL_KEYWORDS.contains(name) && (show_all || !found_any) {
            if !path_only {
                if type_only {
                    stdout.push_str("keyword\n");
                } else {
                    stdout.push_str(&format!("{} is a shell keyword\n", name));
                }
            }
            found_any = true;
            if !show_all {
                continue;
            }
        }

        // Check builtins
        if SHELL_BUILTINS.contains(name) && (show_all || !found_any) {
            if !path_only {
                if type_only {
                    stdout.push_str("builtin\n");
                } else {
                    stdout.push_str(&format!("{} is a shell builtin\n", name));
                }
            }
            found_any = true;
            if !show_all {
                continue;
            }
        }

        // Check PATH for external command(s)
        if show_all {
            let all_paths = find_all_in_path(name);
            for path_result in all_paths {
                if path_only {
                    stdout.push_str(&format!("{}\n", path_result));
                } else if type_only {
                    stdout.push_str("file\n");
                } else {
                    stdout.push_str(&format!("{} is {}\n", name, path_result));
                }
                found_any = true;
            }
        } else if !found_any {
            if let Some(path_result) = find_first_in_path(name) {
                if path_only {
                    stdout.push_str(&format!("{}\n", path_result));
                } else if type_only {
                    stdout.push_str("file\n");
                } else {
                    stdout.push_str(&format!("{} is {}\n", name, path_result));
                }
                found_any = true;
            }
        }

        if !found_any {
            any_not_found = true;
            if !type_only && !path_only {
                stderr.push_str(&format!("bash: type: {}: not found\n", name));
            }
        }
    }

    let exit_code = if any_not_found { 1 } else { 0 };
    ExecResult::new(stdout, stderr, exit_code)
}

/// Handle `command -v` and `command -V` flags
pub fn handle_command_v(
    ctx: &TypeCommandContext,
    names: &[String],
    _show_path: bool,
    verbose_describe: bool,
) -> ExecResult {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    for name in names {
        if name.is_empty() {
            exit_code = 1;
            continue;
        }

        // Check aliases first
        if let Some(ref aliases) = ctx.state.aliases {
            if let Some(alias_value) = aliases.get(name) {
                if verbose_describe {
                    stdout.push_str(&format!("{} is an alias for \"{}\"\n", name, alias_value));
                } else {
                    stdout.push_str(&format!("alias {}='{}'\n", name, alias_value));
                }
                continue;
            }
        }

        if SHELL_KEYWORDS.contains(name.as_str()) {
            if verbose_describe {
                stdout.push_str(&format!("{} is a shell keyword\n", name));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        } else if SHELL_BUILTINS.contains(name.as_str()) {
            if verbose_describe {
                stdout.push_str(&format!("{} is a shell builtin\n", name));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        } else if ctx.state.functions.contains_key(name) {
            if verbose_describe {
                stdout.push_str(&format!("{} is a function\n", name));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        } else {
            if verbose_describe {
                stderr.push_str(&format!("{}: not found\n", name));
            }
            exit_code = 1;
        }
    }

    ExecResult::new(stdout, stderr, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_type_builtin() {
        let state = InterpreterState::default();
        let ctx = TypeCommandContext { state: &state };

        let result = handle_type(
            &ctx,
            &["echo".to_string()],
            |_| None,
            |_| vec![],
        );

        assert!(result.stdout.contains("echo is a shell builtin"));
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_handle_type_keyword() {
        let state = InterpreterState::default();
        let ctx = TypeCommandContext { state: &state };

        let result = handle_type(
            &ctx,
            &["if".to_string()],
            |_| None,
            |_| vec![],
        );

        assert!(result.stdout.contains("if is a shell keyword"));
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_handle_type_not_found() {
        let state = InterpreterState::default();
        let ctx = TypeCommandContext { state: &state };

        let result = handle_type(
            &ctx,
            &["nonexistent_command".to_string()],
            |_| None,
            |_| vec![],
        );

        assert!(result.stderr.contains("not found"));
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_handle_type_t_flag() {
        let state = InterpreterState::default();
        let ctx = TypeCommandContext { state: &state };

        let result = handle_type(
            &ctx,
            &["-t".to_string(), "echo".to_string()],
            |_| None,
            |_| vec![],
        );

        assert_eq!(result.stdout.trim(), "builtin");
        assert_eq!(result.exit_code, 0);
    }
}
