//! continue - Skip to next loop iteration builtin

use crate::interpreter::errors::{ContinueError, ExitError, SubshellExitError, InterpreterError};
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// Handle the continue builtin command.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `args` - Command arguments
///
/// # Returns
/// Ok(BuiltinResult) for success, Err(InterpreterError) for control flow
pub fn handle_continue(state: &InterpreterState, args: &[String]) -> Result<BuiltinResult, InterpreterError> {
    // Check if we're in a loop
    if state.loop_depth == 0 {
        // If we're in a subshell spawned from a loop context, exit the subshell
        if state.parent_has_loop_context.unwrap_or(false) {
            return Err(SubshellExitError::default().into());
        }
        // Otherwise, continue silently does nothing (returns 0)
        return Ok(BuiltinResult::ok());
    }

    // bash: too many arguments is an error (exit code 1)
    if args.len() > 1 {
        return Err(ExitError::new(1, String::new(), "bash: continue: too many arguments\n".to_string()).into());
    }

    let mut levels = 1u32;
    if !args.is_empty() {
        match args[0].parse::<i32>() {
            Ok(n) if n >= 1 => {
                levels = n as u32;
            }
            _ => {
                return Err(ExitError::new(
                    1,
                    String::new(),
                    format!("bash: continue: {}: numeric argument required\n", args[0]),
                ).into());
            }
        }
    }

    Err(ContinueError::new(levels, String::new(), String::new()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_continue_outside_loop() {
        let state = make_state();
        let result = handle_continue(&state, &[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code, 0);
    }

    #[test]
    fn test_continue_in_loop() {
        let mut state = make_state();
        state.loop_depth = 1;
        let result = handle_continue(&state, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Continue(e) => assert_eq!(e.levels, 1),
            _ => panic!("Expected ContinueError"),
        }
    }

    #[test]
    fn test_continue_with_levels() {
        let mut state = make_state();
        state.loop_depth = 3;
        let result = handle_continue(&state, &["2".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Continue(e) => assert_eq!(e.levels, 2),
            _ => panic!("Expected ContinueError"),
        }
    }
}
