//! Directory Stack Builtins: pushd, popd, dirs
//!
//! pushd [dir] - Push directory onto stack and cd to it
//! popd - Pop directory from stack and cd to previous
//! dirs [-clpv] - Display directory stack

use crate::interpreter::types::InterpreterState;

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Get the directory stack, initializing if needed
fn get_stack(state: &mut InterpreterState) -> &mut Vec<String> {
    if state.directory_stack.is_none() {
        state.directory_stack = Some(Vec::new());
    }
    state.directory_stack.as_mut().unwrap()
}

/// Format a path, replacing HOME prefix with ~
fn format_path(path: &str, home: &str) -> String {
    if !home.is_empty() && path == home {
        return "~".to_string();
    }
    if !home.is_empty() && path.starts_with(&format!("{}/", home)) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_string()
}

/// Normalize a path by resolving . and ..
fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let mut result: Vec<&str> = Vec::new();

    for part in parts {
        if part == ".." {
            result.pop();
        } else {
            result.push(part);
        }
    }

    format!("/{}", result.join("/"))
}

/// Handle the `pushd` builtin command.
///
/// pushd [dir] - Push current dir, cd to dir
///
/// Note: This implementation does not verify directory existence (requires fs access).
/// The runtime should verify the directory exists before calling this.
pub fn handle_pushd(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    let mut target_dir: Option<String> = None;

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            if i + 1 < args.len() {
                if target_dir.is_some() {
                    return (String::new(), "bash: pushd: too many arguments\n".to_string(), 2);
                }
                target_dir = Some(args[i + 1].clone());
                i += 1;
            }
        } else if arg.starts_with('-') && arg != "-" {
            return (String::new(), format!("bash: pushd: {}: invalid option\n", arg), 2);
        } else {
            if target_dir.is_some() {
                return (String::new(), "bash: pushd: too many arguments\n".to_string(), 2);
            }
            target_dir = Some(arg.clone());
        }
        i += 1;
    }

    let stack = get_stack(state);

    if target_dir.is_none() {
        // No dir specified - swap top two entries if possible
        if stack.len() < 2 {
            return (String::new(), "bash: pushd: no other directory\n".to_string(), 1);
        }
        stack.swap(0, 1);
        target_dir = Some(stack[0].clone());
    }

    let target = target_dir.unwrap();

    // Resolve the target directory
    let resolved_dir = if target.starts_with('/') {
        target.clone()
    } else if target == ".." {
        let parts: Vec<&str> = state.cwd.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts[..parts.len().saturating_sub(1)].join("/"))
        }
    } else if target == "." {
        state.cwd.clone()
    } else if target.starts_with('~') {
        let home = state.env.get("HOME").cloned().unwrap_or_else(|| "/".to_string());
        format!("{}{}", home, &target[1..])
    } else {
        format!("{}/{}", state.cwd, target)
    };

    // Normalize the path
    let resolved_dir = normalize_path(&resolved_dir);

    // Note: Directory existence check should be done by the runtime
    // For now, we assume the directory exists

    // Push current directory onto stack
    let cwd_clone = state.cwd.clone();
    let stack = get_stack(state);
    stack.insert(0, cwd_clone);

    // Change to new directory
    state.previous_dir = state.cwd.clone();
    state.cwd = resolved_dir.clone();
    state.env.insert("PWD".to_string(), resolved_dir.clone());
    state.env.insert("OLDPWD".to_string(), state.previous_dir.clone());

    // Output the stack (pushd DOES do tilde substitution)
    let home = state.env.get("HOME").cloned().unwrap_or_default();
    let stack = get_stack(state);
    let mut output_parts = vec![format_path(&resolved_dir, &home)];
    for dir in stack.iter() {
        output_parts.push(format_path(dir, &home));
    }
    let output = format!("{}\n", output_parts.join(" "));

    (output, String::new(), 0)
}

/// Handle the `popd` builtin command.
///
/// popd - Pop directory from stack and cd to it
pub fn handle_popd(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse arguments
    for arg in args {
        if arg == "--" {
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            return (String::new(), format!("bash: popd: {}: invalid option\n", arg), 2);
        }
        // popd doesn't take positional arguments
        return (String::new(), "bash: popd: too many arguments\n".to_string(), 2);
    }

    let stack = get_stack(state);

    if stack.is_empty() {
        return (String::new(), "bash: popd: directory stack empty\n".to_string(), 1);
    }

    // Pop the top entry and cd to it
    let new_dir = stack.remove(0);

    // Change to the popped directory
    state.previous_dir = state.cwd.clone();
    state.cwd = new_dir.clone();
    state.env.insert("PWD".to_string(), new_dir.clone());
    state.env.insert("OLDPWD".to_string(), state.previous_dir.clone());

    // Output the stack (popd DOES do tilde substitution)
    let home = state.env.get("HOME").cloned().unwrap_or_default();
    let stack = get_stack(state);
    let mut output_parts = vec![format_path(&new_dir, &home)];
    for dir in stack.iter() {
        output_parts.push(format_path(dir, &home));
    }
    let output = format!("{}\n", output_parts.join(" "));

    (output, String::new(), 0)
}

/// Handle the `dirs` builtin command.
///
/// dirs [-clpv]
///   -c: Clear the stack
///   -l: Long format (no tilde substitution)
///   -p: One entry per line
///   -v: One entry per line with index numbers
pub fn handle_dirs(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    let mut clear_stack = false;
    let mut long_format = false;
    let mut per_line = false;
    let mut with_numbers = false;

    // Parse arguments
    for arg in args {
        if arg == "--" {
            continue;
        }
        if arg.starts_with('-') {
            for flag in arg[1..].chars() {
                match flag {
                    'c' => clear_stack = true,
                    'l' => long_format = true,
                    'p' => per_line = true,
                    'v' => {
                        per_line = true;
                        with_numbers = true;
                    }
                    _ => {
                        return (String::new(), format!("bash: dirs: -{}: invalid option\n", flag), 2);
                    }
                }
            }
        } else {
            // dirs doesn't take positional arguments
            return (String::new(), "bash: dirs: too many arguments\n".to_string(), 1);
        }
    }

    if clear_stack {
        state.directory_stack = Some(Vec::new());
        return (String::new(), String::new(), 0);
    }

    // Build the stack display (current dir + stack)
    let cwd_clone = state.cwd.clone();
    let stack = get_stack(state);
    let mut full_stack = vec![cwd_clone];
    full_stack.extend(stack.iter().cloned());

    let home = state.env.get("HOME").cloned().unwrap_or_default();

    let output = if with_numbers {
        let lines: Vec<String> = full_stack
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let path = if long_format { p.clone() } else { format_path(p, &home) };
                format!(" {}  {}", i, path)
            })
            .collect();
        format!("{}\n", lines.join("\n"))
    } else if per_line {
        let lines: Vec<String> = full_stack
            .iter()
            .map(|p| if long_format { p.clone() } else { format_path(p, &home) })
            .collect();
        format!("{}\n", lines.join("\n"))
    } else {
        let parts: Vec<String> = full_stack
            .iter()
            .map(|p| if long_format { p.clone() } else { format_path(p, &home) })
            .collect();
        format!("{}\n", parts.join(" "))
    };

    (output, String::new(), 0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_path_home() {
        assert_eq!(format_path("/home/user", "/home/user"), "~");
        assert_eq!(format_path("/home/user/docs", "/home/user"), "~/docs");
        assert_eq!(format_path("/other/path", "/home/user"), "/other/path");
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/../bar"), "/bar");
        assert_eq!(normalize_path("/foo/./bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/bar/.."), "/foo");
    }

    #[test]
    fn test_handle_dirs_empty() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        let (stdout, stderr, code) = handle_dirs(&mut state, &[]);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(stdout, "/home/user\n");
    }

    #[test]
    fn test_handle_dirs_with_tilde() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.env.insert("HOME".to_string(), "/home/user".to_string());
        let (stdout, _, code) = handle_dirs(&mut state, &[]);
        assert_eq!(code, 0);
        assert_eq!(stdout, "~\n");
    }

    #[test]
    fn test_handle_dirs_long_format() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.env.insert("HOME".to_string(), "/home/user".to_string());
        let (stdout, _, code) = handle_dirs(&mut state, &["-l".to_string()]);
        assert_eq!(code, 0);
        assert_eq!(stdout, "/home/user\n");
    }

    #[test]
    fn test_handle_dirs_per_line() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.directory_stack = Some(vec!["/tmp".to_string()]);
        let (stdout, _, code) = handle_dirs(&mut state, &["-p".to_string()]);
        assert_eq!(code, 0);
        assert_eq!(stdout, "/home/user\n/tmp\n");
    }

    #[test]
    fn test_handle_dirs_with_numbers() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.directory_stack = Some(vec!["/tmp".to_string()]);
        let (stdout, _, code) = handle_dirs(&mut state, &["-v".to_string()]);
        assert_eq!(code, 0);
        assert!(stdout.contains(" 0  /home/user"));
        assert!(stdout.contains(" 1  /tmp"));
    }

    #[test]
    fn test_handle_dirs_clear() {
        let mut state = InterpreterState::default();
        state.directory_stack = Some(vec!["/tmp".to_string(), "/var".to_string()]);
        let (stdout, stderr, code) = handle_dirs(&mut state, &["-c".to_string()]);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        assert_eq!(state.directory_stack, Some(Vec::new()));
    }

    #[test]
    fn test_handle_popd_empty_stack() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        let (_, stderr, code) = handle_popd(&mut state, &[]);
        assert_eq!(code, 1);
        assert!(stderr.contains("directory stack empty"));
    }

    #[test]
    fn test_handle_popd_success() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.directory_stack = Some(vec!["/tmp".to_string()]);
        let (stdout, stderr, code) = handle_popd(&mut state, &[]);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(state.cwd, "/tmp");
        assert_eq!(state.directory_stack, Some(Vec::new()));
        assert!(stdout.contains("/tmp"));
    }

    #[test]
    fn test_handle_pushd_absolute_path() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        let (stdout, stderr, code) = handle_pushd(&mut state, &["/tmp".to_string()]);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(state.cwd, "/tmp");
        assert_eq!(state.directory_stack, Some(vec!["/home/user".to_string()]));
        assert!(stdout.contains("/tmp"));
    }

    #[test]
    fn test_handle_pushd_no_args_swap() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.directory_stack = Some(vec!["/tmp".to_string(), "/var".to_string()]);
        let (_, _, code) = handle_pushd(&mut state, &[]);
        assert_eq!(code, 0);
        // After swap and push: stack[0] and stack[1] are swapped, then cwd is pushed
        // Original stack: ["/tmp", "/var"], after swap: ["/var", "/tmp"]
        // Then cwd "/home/user" is pushed, and we cd to "/var"
        assert_eq!(state.cwd, "/var");
        let stack = state.directory_stack.unwrap();
        assert_eq!(stack[0], "/home/user");
        assert_eq!(stack[1], "/var");
        assert_eq!(stack[2], "/tmp");
    }

    #[test]
    fn test_handle_pushd_no_other_directory() {
        let mut state = InterpreterState::default();
        state.cwd = "/home/user".to_string();
        state.directory_stack = Some(vec!["/tmp".to_string()]);
        let (_, stderr, code) = handle_pushd(&mut state, &[]);
        assert_eq!(code, 1);
        assert!(stderr.contains("no other directory"));
    }
}
