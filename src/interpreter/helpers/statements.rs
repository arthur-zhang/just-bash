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
