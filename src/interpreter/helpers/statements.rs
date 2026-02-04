//! Statement execution helpers for the interpreter.
//!
//! Consolidates the common pattern of executing a list of statements
//! and accumulating their output.

use crate::interpreter::errors::{
    BreakError, ContinueError, ErrexitError, ExecutionLimitError,
    ExitError, ReturnError, SubshellExitError,
};
use crate::interpreter::types::ExecResult;

/// Check if an error is a scope exit error (break, continue, return).
pub fn is_scope_exit_error<E: std::error::Error>(error: &E) -> bool {
    let msg = error.to_string();
    msg.contains("break") || msg.contains("continue") || msg.contains("return")
}

/// Trait for errors that can have output prepended.
pub trait PrependOutput {
    fn prepend_output(&mut self, stdout: &str, stderr: &str);
}

impl PrependOutput for BreakError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for ContinueError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for ReturnError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for ErrexitError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for ExitError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for ExecutionLimitError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

impl PrependOutput for SubshellExitError {
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        self.stdout = format!("{}{}", stdout, self.stdout);
        self.stderr = format!("{}{}", stderr, self.stderr);
    }
}

/// Accumulated result from executing multiple statements.
#[derive(Debug, Clone, Default)]
pub struct StatementsResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl StatementsResult {
    /// Create a new statements result.
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }

    /// Create from an ExecResult.
    pub fn from_exec_result(result: &ExecResult) -> Self {
        Self {
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
            exit_code: result.exit_code,
        }
    }

    /// Append another result's output.
    pub fn append(&mut self, result: &ExecResult) {
        self.stdout.push_str(&result.stdout);
        self.stderr.push_str(&result.stderr);
        self.exit_code = result.exit_code;
    }

    /// Convert to ExecResult.
    pub fn to_exec_result(&self) -> ExecResult {
        ExecResult {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
            env: None,
        }
    }
}

/// Error type for statement execution errors.
#[derive(Debug)]
pub enum StatementError {
    Break(BreakError),
    Continue(ContinueError),
    Return(ReturnError),
    Errexit(ErrexitError),
    Exit(ExitError),
    ExecutionLimit(ExecutionLimitError),
    SubshellExit(SubshellExitError),
    Other { message: String, stdout: String, stderr: String },
}

impl StatementError {
    /// Prepend output to the error.
    pub fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        match self {
            StatementError::Break(e) => e.prepend_output(stdout, stderr),
            StatementError::Continue(e) => e.prepend_output(stdout, stderr),
            StatementError::Return(e) => e.prepend_output(stdout, stderr),
            StatementError::Errexit(e) => e.prepend_output(stdout, stderr),
            StatementError::Exit(e) => e.prepend_output(stdout, stderr),
            StatementError::ExecutionLimit(e) => e.prepend_output(stdout, stderr),
            StatementError::SubshellExit(e) => e.prepend_output(stdout, stderr),
            StatementError::Other { stdout: s, stderr: e, .. } => {
                *s = format!("{}{}", stdout, s);
                *e = format!("{}{}", stderr, e);
            }
        }
    }

    /// Check if this is a scope exit error (break, continue, return).
    pub fn is_scope_exit(&self) -> bool {
        matches!(
            self,
            StatementError::Break(_) | StatementError::Continue(_) | StatementError::Return(_)
        )
    }
}

/// Result type for statement execution.
pub type StatementResult<T> = Result<T, StatementError>;

/// Execute a list of statements and accumulate their output.
///
/// This is a generic version that accepts an executor function.
/// The executor function should execute a single statement and return its result.
///
/// # Arguments
/// * `statements` - Iterator of statements to execute
/// * `executor` - Function to execute a single statement
/// * `initial_stdout` - Initial stdout to prepend
/// * `initial_stderr` - Initial stderr to prepend
///
/// # Returns
/// Accumulated stdout, stderr, and final exit code, or an error.
pub fn execute_statements<S, F, E>(
    statements: impl IntoIterator<Item = S>,
    mut executor: F,
    initial_stdout: &str,
    initial_stderr: &str,
) -> StatementResult<StatementsResult>
where
    F: FnMut(S) -> Result<ExecResult, E>,
    E: Into<StatementError>,
{
    let mut result = StatementsResult::new(
        initial_stdout.to_string(),
        initial_stderr.to_string(),
        0,
    );

    for stmt in statements {
        match executor(stmt) {
            Ok(exec_result) => {
                result.append(&exec_result);
            }
            Err(error) => {
                let mut stmt_error = error.into();
                stmt_error.prepend_output(&result.stdout, &result.stderr);
                return Err(stmt_error);
            }
        }
    }

    Ok(result)
}

/// Execute statements with error handling that converts unknown errors to results.
///
/// Unlike `execute_statements`, this function catches unknown errors and returns
/// them as a result with exit code 1, rather than propagating them.
pub fn execute_statements_with_catch<S, F, E>(
    statements: impl IntoIterator<Item = S>,
    mut executor: F,
    initial_stdout: &str,
    initial_stderr: &str,
    get_error_message: impl Fn(&E) -> String,
) -> StatementResult<StatementsResult>
where
    F: FnMut(S) -> Result<ExecResult, E>,
{
    let mut result = StatementsResult::new(
        initial_stdout.to_string(),
        initial_stderr.to_string(),
        0,
    );

    for stmt in statements {
        match executor(stmt) {
            Ok(exec_result) => {
                result.append(&exec_result);
            }
            Err(error) => {
                // For unknown errors, append error message to stderr and return
                let error_msg = get_error_message(&error);
                result.stderr.push_str(&error_msg);
                result.stderr.push('\n');
                result.exit_code = 1;
                return Ok(result);
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statements_result_new() {
        let result = StatementsResult::new("out".to_string(), "err".to_string(), 0);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_statements_result_append() {
        let mut result = StatementsResult::new("a".to_string(), "b".to_string(), 0);
        let exec = ExecResult {
            stdout: "c".to_string(),
            stderr: "d".to_string(),
            exit_code: 1,
            env: None,
        };
        result.append(&exec);
        assert_eq!(result.stdout, "ac");
        assert_eq!(result.stderr, "bd");
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_prepend_output_break() {
        let mut err = BreakError::new(1, "out".to_string(), "err".to_string());
        err.prepend_output("pre_", "pre_");
        assert_eq!(err.stdout, "pre_out");
        assert_eq!(err.stderr, "pre_err");
    }

    #[test]
    fn test_prepend_output_continue() {
        let mut err = ContinueError::new(1, "out".to_string(), "err".to_string());
        err.prepend_output("pre_", "pre_");
        assert_eq!(err.stdout, "pre_out");
        assert_eq!(err.stderr, "pre_err");
    }

    #[test]
    fn test_prepend_output_return() {
        let mut err = ReturnError::new(0, "out".to_string(), "err".to_string());
        err.prepend_output("pre_", "pre_");
        assert_eq!(err.stdout, "pre_out");
        assert_eq!(err.stderr, "pre_err");
    }
}
