//! source/. - Execute commands from a file in current environment builtin
//!
//! Note: This module provides the argument parsing and state management logic.
//! The actual file reading and script execution require runtime dependencies
//! (filesystem access, parser, interpreter) that must be provided by the runtime.

use crate::interpreter::types::{ExecResult, InterpreterState};
use std::collections::HashMap;

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Parsed source command ready for execution.
#[derive(Debug, Clone)]
pub struct SourceCommand {
    /// The filename to source
    pub filename: String,
    /// Additional arguments to pass as positional parameters
    pub script_args: Vec<String>,
}

/// Saved state for restoring after source execution.
#[derive(Debug)]
pub struct SourceSavedState {
    /// Saved positional parameters
    pub positional: HashMap<String, Option<String>>,
    /// Saved current source context
    pub current_source: Option<String>,
    /// Whether positional parameters were changed
    pub changed_positional: bool,
}

/// Parse and validate source arguments.
///
/// Returns Ok(SourceCommand) if there's a file to source,
/// Err(BuiltinResult) if there's an error.
pub fn parse_source_args(args: &[String]) -> Result<SourceCommand, BuiltinResult> {
    // Handle -- to end options (ignored like bash does)
    let mut source_args = args;
    if !source_args.is_empty() && source_args[0] == "--" {
        source_args = &source_args[1..];
    }

    if source_args.is_empty() {
        return Err((
            String::new(),
            "bash: source: filename argument required\n".to_string(),
            2,
        ));
    }

    let filename = source_args[0].clone();
    let script_args = source_args[1..].to_vec();

    Ok(SourceCommand {
        filename,
        script_args,
    })
}

/// Handle the `source` builtin command parsing.
///
/// This function parses and validates the arguments. The actual execution
/// requires runtime dependencies and should be handled by the runtime.
pub fn handle_source_parse(args: &[String]) -> Result<SourceCommand, BuiltinResult> {
    parse_source_args(args)
}

/// Prepare state for source execution.
///
/// Saves current positional parameters and source context,
/// sets up new positional parameters if provided.
pub fn prepare_source_state(
    state: &mut InterpreterState,
    cmd: &SourceCommand,
) -> SourceSavedState {
    let mut saved = SourceSavedState {
        positional: HashMap::new(),
        current_source: state.current_source.clone(),
        changed_positional: !cmd.script_args.is_empty(),
    };

    if !cmd.script_args.is_empty() {
        // Save current positional parameters
        for i in 1..=9 {
            let key = i.to_string();
            saved.positional.insert(key.clone(), state.env.get(&key).cloned());
        }
        saved.positional.insert("#".to_string(), state.env.get("#").cloned());
        saved.positional.insert("@".to_string(), state.env.get("@").cloned());

        // Set new positional parameters
        state.env.insert("#".to_string(), cmd.script_args.len().to_string());
        state.env.insert("@".to_string(), cmd.script_args.join(" "));
        for (i, arg) in cmd.script_args.iter().enumerate() {
            if i < 9 {
                state.env.insert((i + 1).to_string(), arg.clone());
            }
        }
        // Clear any remaining positional parameters
        for i in (cmd.script_args.len() + 1)..=9 {
            state.env.remove(&i.to_string());
        }
    }

    // Update source tracking
    state.source_depth += 1;
    state.current_source = Some(cmd.filename.clone());

    saved
}

/// Restore state after source execution.
pub fn restore_source_state(state: &mut InterpreterState, saved: SourceSavedState) {
    state.source_depth -= 1;
    state.current_source = saved.current_source;

    // Restore positional parameters if we changed them
    if saved.changed_positional {
        for (key, value) in saved.positional {
            match value {
                Some(v) => {
                    state.env.insert(key, v);
                }
                None => {
                    state.env.remove(&key);
                }
            }
        }
    }
}

/// Create an ExecResult for file not found error.
pub fn source_file_not_found(filename: &str) -> ExecResult {
    ExecResult::failure(format!("bash: {}: No such file or directory\n", filename))
}

/// Create an ExecResult for parse error.
pub fn source_parse_error(filename: &str, message: &str) -> ExecResult {
    ExecResult::failure(format!("bash: {}: {}\n", filename, message))
}

/// Resolve a source filename to a path.
///
/// If filename contains '/', use it directly (relative or absolute path).
/// Otherwise, search in PATH first, then current directory.
///
/// Returns a list of candidate paths to try, in order.
pub fn resolve_source_paths(
    cwd: &str,
    filename: &str,
    path_env: Option<&str>,
) -> Vec<String> {
    let mut candidates = Vec::new();

    if filename.contains('/') {
        // Use directly (relative or absolute path)
        let path = if filename.starts_with('/') {
            filename.to_string()
        } else {
            format!("{}/{}", cwd, filename)
        };
        candidates.push(normalize_path(&path));
    } else {
        // Search in PATH first
        if let Some(path_env) = path_env {
            for dir in path_env.split(':').filter(|d| !d.is_empty()) {
                let candidate = if dir.starts_with('/') {
                    format!("{}/{}", dir, filename)
                } else {
                    format!("{}/{}/{}", cwd, dir, filename)
                };
                candidates.push(normalize_path(&candidate));
            }
        }

        // Then try current directory
        candidates.push(format!("{}/{}", cwd, filename));
    }

    candidates
}

/// Normalize a path by resolving . and .. components.
fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            c => {
                components.push(c);
            }
        }
    }

    if path.starts_with('/') {
        format!("/{}", components.join("/"))
    } else {
        components.join("/")
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_source_args_empty() {
        let result = parse_source_args(&[]);
        assert!(result.is_err());
        let (_, stderr, code) = result.unwrap_err();
        assert_eq!(code, 2);
        assert!(stderr.contains("filename argument required"));
    }

    #[test]
    fn test_parse_source_args_simple() {
        let args = vec!["script.sh".to_string()];
        let result = parse_source_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.filename, "script.sh");
        assert!(cmd.script_args.is_empty());
    }

    #[test]
    fn test_parse_source_args_with_args() {
        let args = vec!["script.sh".to_string(), "arg1".to_string(), "arg2".to_string()];
        let result = parse_source_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.filename, "script.sh");
        assert_eq!(cmd.script_args, vec!["arg1", "arg2"]);
    }

    #[test]
    fn test_parse_source_args_with_double_dash() {
        let args = vec!["--".to_string(), "script.sh".to_string()];
        let result = parse_source_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.filename, "script.sh");
    }

    #[test]
    fn test_resolve_source_paths_absolute() {
        let paths = resolve_source_paths("/home/user", "/etc/script.sh", None);
        assert_eq!(paths, vec!["/etc/script.sh"]);
    }

    #[test]
    fn test_resolve_source_paths_relative_with_slash() {
        let paths = resolve_source_paths("/home/user", "./script.sh", None);
        assert_eq!(paths, vec!["/home/user/script.sh"]);
    }

    #[test]
    fn test_resolve_source_paths_no_slash() {
        let paths = resolve_source_paths("/home/user", "script.sh", Some("/bin:/usr/bin"));
        assert_eq!(paths, vec![
            "/bin/script.sh",
            "/usr/bin/script.sh",
            "/home/user/script.sh",
        ]);
    }

    #[test]
    fn test_resolve_source_paths_no_path() {
        let paths = resolve_source_paths("/home/user", "script.sh", None);
        assert_eq!(paths, vec!["/home/user/script.sh"]);
    }

    #[test]
    fn test_prepare_and_restore_state() {
        let mut state = InterpreterState::default();
        state.env.insert("1".to_string(), "old1".to_string());
        state.env.insert("#".to_string(), "1".to_string());
        state.source_depth = 0;

        let cmd = SourceCommand {
            filename: "test.sh".to_string(),
            script_args: vec!["new1".to_string(), "new2".to_string()],
        };

        let saved = prepare_source_state(&mut state, &cmd);

        assert_eq!(state.source_depth, 1);
        assert_eq!(state.current_source, Some("test.sh".to_string()));
        assert_eq!(state.env.get("1"), Some(&"new1".to_string()));
        assert_eq!(state.env.get("2"), Some(&"new2".to_string()));
        assert_eq!(state.env.get("#"), Some(&"2".to_string()));

        restore_source_state(&mut state, saved);

        assert_eq!(state.source_depth, 0);
        assert_eq!(state.env.get("1"), Some(&"old1".to_string()));
        assert_eq!(state.env.get("2"), None);
        assert_eq!(state.env.get("#"), Some(&"1".to_string()));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/../bar"), "/bar");
        assert_eq!(normalize_path("/foo/./bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/bar/.."), "/foo");
        assert_eq!(normalize_path("/foo/bar/../baz"), "/foo/baz");
    }
}
