//! Loop Error Handling Helpers
//!
//! Consolidates the repeated error handling logic used in all loop constructs
//! (for, c-style for, while, until).

use crate::interpreter::errors::{
    InterpreterError, ControlFlowError,
};

#[cfg(test)]
use crate::interpreter::errors::{BreakError, ContinueError, ReturnError};

/// Action to take after handling a loop error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    /// Break out of the current loop
    Break,
    /// Continue to the next iteration
    Continue,
    /// Rethrow the error to outer scope
    Rethrow,
    /// Return an error result
    Error,
}

/// Result of handling a loop error.
#[derive(Debug, Clone)]
pub struct LoopErrorResult {
    pub action: LoopAction,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub error: Option<InterpreterError>,
}

impl LoopErrorResult {
    pub fn break_loop(stdout: String, stderr: String) -> Self {
        Self {
            action: LoopAction::Break,
            stdout,
            stderr,
            exit_code: None,
            error: None,
        }
    }

    pub fn continue_loop(stdout: String, stderr: String) -> Self {
        Self {
            action: LoopAction::Continue,
            stdout,
            stderr,
            exit_code: None,
            error: None,
        }
    }

    pub fn rethrow(stdout: String, stderr: String, error: InterpreterError) -> Self {
        Self {
            action: LoopAction::Rethrow,
            stdout,
            stderr,
            exit_code: None,
            error: Some(error),
        }
    }

    pub fn error(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            action: LoopAction::Error,
            stdout,
            stderr,
            exit_code: Some(exit_code),
            error: None,
        }
    }
}

/// Handle errors thrown during loop body execution.
///
/// # Arguments
/// * `error` - The caught error
/// * `stdout` - Current accumulated stdout
/// * `stderr` - Current accumulated stderr
/// * `loop_depth` - Current loop nesting depth from ctx.state.loop_depth
///
/// # Returns
/// Result indicating what action the loop should take
pub fn handle_loop_error(
    error: InterpreterError,
    mut stdout: String,
    mut stderr: String,
    loop_depth: u32,
) -> LoopErrorResult {
    match error {
        InterpreterError::Break(mut e) => {
            stdout.push_str(&e.stdout);
            stderr.push_str(&e.stderr);
            // Only propagate if levels > 1 AND we're not at the outermost loop
            // Per bash docs: "If n is greater than the number of enclosing loops,
            // the last enclosing loop is exited"
            if e.levels > 1 && loop_depth > 1 {
                e.levels -= 1;
                e.stdout = stdout.clone();
                e.stderr = stderr.clone();
                return LoopErrorResult::rethrow(stdout, stderr, InterpreterError::Break(e));
            }
            LoopErrorResult::break_loop(stdout, stderr)
        }

        InterpreterError::Continue(mut e) => {
            stdout.push_str(&e.stdout);
            stderr.push_str(&e.stderr);
            // Only propagate if levels > 1 AND we're not at the outermost loop
            // Per bash docs: "If n is greater than the number of enclosing loops,
            // the last enclosing loop is resumed"
            if e.levels > 1 && loop_depth > 1 {
                e.levels -= 1;
                e.stdout = stdout.clone();
                e.stderr = stderr.clone();
                return LoopErrorResult::rethrow(stdout, stderr, InterpreterError::Continue(e));
            }
            LoopErrorResult::continue_loop(stdout, stderr)
        }

        InterpreterError::Return(mut e) => {
            e.prepend_output(&stdout, &stderr);
            LoopErrorResult::rethrow(stdout, stderr, InterpreterError::Return(e))
        }

        InterpreterError::Errexit(mut e) => {
            e.prepend_output(&stdout, &stderr);
            LoopErrorResult::rethrow(stdout, stderr, InterpreterError::Errexit(e))
        }

        InterpreterError::Exit(mut e) => {
            e.prepend_output(&stdout, &stderr);
            LoopErrorResult::rethrow(stdout, stderr, InterpreterError::Exit(e))
        }

        InterpreterError::ExecutionLimit(mut e) => {
            e.prepend_output(&stdout, &stderr);
            LoopErrorResult::rethrow(stdout, stderr, InterpreterError::ExecutionLimit(e))
        }

        // Other errors - return error result
        other => {
            let message = other.to_string();
            stderr.push_str(&message);
            stderr.push('\n');
            LoopErrorResult::error(stdout, stderr, 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_break_single_level() {
        let error = InterpreterError::Break(BreakError::new(1, String::new(), String::new()));
        let result = handle_loop_error(error, String::new(), String::new(), 1);
        assert_eq!(result.action, LoopAction::Break);
    }

    #[test]
    fn test_handle_break_multi_level() {
        let error = InterpreterError::Break(BreakError::new(2, String::new(), String::new()));
        let result = handle_loop_error(error, String::new(), String::new(), 2);
        assert_eq!(result.action, LoopAction::Rethrow);
        if let Some(InterpreterError::Break(e)) = result.error {
            assert_eq!(e.levels, 1);
        } else {
            panic!("Expected BreakError");
        }
    }

    #[test]
    fn test_handle_continue_single_level() {
        let error = InterpreterError::Continue(ContinueError::new(1, String::new(), String::new()));
        let result = handle_loop_error(error, String::new(), String::new(), 1);
        assert_eq!(result.action, LoopAction::Continue);
    }

    #[test]
    fn test_handle_return() {
        let error = InterpreterError::Return(ReturnError::new(0, String::new(), String::new()));
        let result = handle_loop_error(error, "out".to_string(), "err".to_string(), 1);
        assert_eq!(result.action, LoopAction::Rethrow);
    }
}
