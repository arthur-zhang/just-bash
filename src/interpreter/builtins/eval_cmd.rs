//! eval - Execute arguments as a shell command
//!
//! Concatenates all arguments and executes them as a shell command
//! in the current environment (variables persist after eval).
//!
//! Note: This module provides the argument parsing and validation logic.
//! The actual script execution requires runtime dependencies (parser, interpreter)
//! that must be provided by the runtime environment.

use crate::interpreter::types::{ExecResult, InterpreterState};

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Parsed eval command ready for execution.
#[derive(Debug, Clone)]
pub struct EvalCommand {
    /// The concatenated command string to execute
    pub command: String,
    /// The stdin to pass to the command (if any)
    pub stdin: Option<String>,
}

/// Parse and validate eval arguments.
///
/// Returns Ok(Some(EvalCommand)) if there's a command to execute,
/// Ok(None) if eval should return success with no action,
/// Err(BuiltinResult) if there's an error.
pub fn parse_eval_args(args: &[String]) -> Result<Option<EvalCommand>, BuiltinResult> {
    // Handle options like bash does:
    // -- ends option processing
    // - alone is a plain argument
    // -x (any other option) is invalid
    let mut eval_args = args;

    if !eval_args.is_empty() {
        let first = &eval_args[0];
        if first == "--" {
            eval_args = &eval_args[1..];
        } else if first.starts_with('-') && first != "-" && first.len() > 1 {
            // Invalid option like -z, -x, etc.
            return Err((
                String::new(),
                format!("bash: eval: {}: invalid option\neval: usage: eval [arg ...]\n", first),
                2,
            ));
        }
    }

    if eval_args.is_empty() {
        return Ok(None);
    }

    // Concatenate all arguments with spaces (like bash does)
    let command = eval_args.join(" ");

    if command.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(EvalCommand {
        command,
        stdin: None,
    }))
}

/// Handle the `eval` builtin command.
///
/// This function parses and validates the arguments. The actual execution
/// requires runtime dependencies and should be handled by the runtime.
///
/// Returns:
/// - Ok(None) if eval should return success with no action
/// - Ok(Some(EvalCommand)) if there's a command to execute
/// - Err(BuiltinResult) if there's an error
pub fn handle_eval_parse(args: &[String]) -> Result<Option<EvalCommand>, BuiltinResult> {
    parse_eval_args(args)
}

/// Prepare state for eval execution.
///
/// Saves the current groupStdin and sets up the effective stdin.
/// Returns the saved groupStdin value for later restoration.
pub fn prepare_eval_stdin(
    state: &mut InterpreterState,
    stdin: Option<&str>,
) -> Option<String> {
    let saved_group_stdin = state.group_stdin.clone();
    let effective_stdin = stdin.map(|s| s.to_string()).or_else(|| state.group_stdin.clone());
    if effective_stdin.is_some() {
        state.group_stdin = effective_stdin;
    }
    saved_group_stdin
}

/// Restore state after eval execution.
pub fn restore_eval_stdin(state: &mut InterpreterState, saved_group_stdin: Option<String>) {
    state.group_stdin = saved_group_stdin;
}

/// Create an ExecResult from a parse error message.
pub fn eval_parse_error(message: &str) -> ExecResult {
    ExecResult::failure(format!("bash: eval: {}\n", message))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eval_args_empty() {
        let result = parse_eval_args(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_parse_eval_args_simple() {
        let args = vec!["echo".to_string(), "hello".to_string()];
        let result = parse_eval_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap().unwrap();
        assert_eq!(cmd.command, "echo hello");
    }

    #[test]
    fn test_parse_eval_args_with_double_dash() {
        let args = vec!["--".to_string(), "echo".to_string(), "hello".to_string()];
        let result = parse_eval_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap().unwrap();
        assert_eq!(cmd.command, "echo hello");
    }

    #[test]
    fn test_parse_eval_args_single_dash() {
        let args = vec!["-".to_string()];
        let result = parse_eval_args(&args);
        assert!(result.is_ok());
        let cmd = result.unwrap().unwrap();
        assert_eq!(cmd.command, "-");
    }

    #[test]
    fn test_parse_eval_args_invalid_option() {
        let args = vec!["-x".to_string()];
        let result = parse_eval_args(&args);
        assert!(result.is_err());
        let (_, stderr, code) = result.unwrap_err();
        assert_eq!(code, 2);
        assert!(stderr.contains("invalid option"));
    }

    #[test]
    fn test_parse_eval_args_whitespace_only() {
        let args = vec!["   ".to_string()];
        let result = parse_eval_args(&args);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_prepare_and_restore_stdin() {
        let mut state = InterpreterState::default();
        state.group_stdin = Some("original".to_string());

        let saved = prepare_eval_stdin(&mut state, Some("new stdin"));
        assert_eq!(saved, Some("original".to_string()));
        assert_eq!(state.group_stdin, Some("new stdin".to_string()));

        restore_eval_stdin(&mut state, saved);
        assert_eq!(state.group_stdin, Some("original".to_string()));
    }

    #[test]
    fn test_prepare_stdin_uses_group_stdin_when_no_stdin() {
        let mut state = InterpreterState::default();
        state.group_stdin = Some("group".to_string());

        let saved = prepare_eval_stdin(&mut state, None);
        assert_eq!(saved, Some("group".to_string()));
        assert_eq!(state.group_stdin, Some("group".to_string()));
    }
}
