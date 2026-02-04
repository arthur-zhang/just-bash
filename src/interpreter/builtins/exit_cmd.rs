//! exit - Exit shell builtin

use crate::interpreter::errors::{ExitError, InterpreterError};
use crate::interpreter::types::InterpreterState;
use std::convert::Infallible;

/// Handle the exit builtin command.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `args` - Command arguments
///
/// # Returns
/// Always returns Err(InterpreterError::Exit) to terminate execution
pub fn handle_exit(state: &InterpreterState, args: &[String]) -> Result<Infallible, InterpreterError> {
    let (exit_code, stderr) = if args.is_empty() {
        // Use last command's exit code when no argument given
        (state.last_exit_code, String::new())
    } else {
        let arg = &args[0];
        // Empty string or non-numeric is an error
        if arg.is_empty() || !arg.chars().all(|c| c.is_ascii_digit() || c == '-') {
            (2, format!("bash: exit: {}: numeric argument required\n", arg))
        } else {
            match arg.parse::<i32>() {
                Ok(parsed) => {
                    // Exit codes are modulo 256 (wrap around)
                    let code = ((parsed % 256) + 256) % 256;
                    (code, String::new())
                }
                Err(_) => {
                    (2, format!("bash: exit: {}: numeric argument required\n", arg))
                }
            }
        }
    };

    Err(ExitError::new(exit_code, String::new(), stderr).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_exit_no_args() {
        let mut state = make_state();
        state.last_exit_code = 42;
        let result = handle_exit(&state, &[]);
        match result {
            Err(InterpreterError::Exit(e)) => {
                assert_eq!(e.exit_code, 42);
                assert!(e.stderr.is_empty());
            }
            _ => panic!("Expected ExitError"),
        }
    }

    #[test]
    fn test_exit_with_code() {
        let state = make_state();
        let result = handle_exit(&state, &["5".to_string()]);
        match result {
            Err(InterpreterError::Exit(e)) => {
                assert_eq!(e.exit_code, 5);
                assert!(e.stderr.is_empty());
            }
            _ => panic!("Expected ExitError"),
        }
    }

    #[test]
    fn test_exit_with_large_code() {
        let state = make_state();
        let result = handle_exit(&state, &["300".to_string()]);
        match result {
            Err(InterpreterError::Exit(e)) => {
                assert_eq!(e.exit_code, 44); // 300 % 256 = 44
            }
            _ => panic!("Expected ExitError"),
        }
    }

    #[test]
    fn test_exit_with_negative_code() {
        let state = make_state();
        let result = handle_exit(&state, &["-1".to_string()]);
        match result {
            Err(InterpreterError::Exit(e)) => {
                assert_eq!(e.exit_code, 255);
            }
            _ => panic!("Expected ExitError"),
        }
    }

    #[test]
    fn test_exit_invalid_arg() {
        let state = make_state();
        let result = handle_exit(&state, &["abc".to_string()]);
        match result {
            Err(InterpreterError::Exit(e)) => {
                assert_eq!(e.exit_code, 2);
                assert!(e.stderr.contains("numeric argument required"));
            }
            _ => panic!("Expected ExitError"),
        }
    }
}
