//! ExecResult factory functions for cleaner code.
//!
//! These helpers reduce verbosity and improve readability when
//! constructing ExecResult objects throughout the interpreter.

use crate::interpreter::types::ExecResult;
use crate::interpreter::errors::{ExecutionLimitError, LimitType};

/// A successful result with no output.
/// Use this for commands that succeed silently.
pub const OK: ExecResult = ExecResult {
    stdout: String::new(),
    stderr: String::new(),
    exit_code: 0,
    env: None,
};

/// Create a successful result with optional stdout.
pub fn success(stdout: impl Into<String>) -> ExecResult {
    ExecResult::new(stdout.into(), String::new(), 0)
}

/// Create a failure result with stderr message.
pub fn failure(stderr: impl Into<String>) -> ExecResult {
    ExecResult::new(String::new(), stderr.into(), 1)
}

/// Create a failure result with stderr message and custom exit code.
pub fn failure_with_code(stderr: impl Into<String>, exit_code: i32) -> ExecResult {
    ExecResult::new(String::new(), stderr.into(), exit_code)
}

/// Create a result with all fields specified.
pub fn result(stdout: impl Into<String>, stderr: impl Into<String>, exit_code: i32) -> ExecResult {
    ExecResult::new(stdout.into(), stderr.into(), exit_code)
}

/// Convert a boolean test result to an ExecResult.
/// Useful for test/conditional commands where true = exit 0, false = exit 1.
pub fn test_result(passed: bool) -> ExecResult {
    ExecResult::new(String::new(), String::new(), if passed { 0 } else { 1 })
}

/// Throw an ExecutionLimitError for execution limits (recursion, iterations, commands).
///
/// # Panics
/// This function always panics with an ExecutionLimitError.
pub fn throw_execution_limit(
    message: impl Into<String>,
    limit_type: LimitType,
    stdout: impl Into<String>,
    stderr: impl Into<String>,
) -> ! {
    panic!("{}", ExecutionLimitError::new(
        message.into(),
        limit_type,
        stdout.into(),
        stderr.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok() {
        assert_eq!(OK.exit_code, 0);
        assert!(OK.stdout.is_empty());
        assert!(OK.stderr.is_empty());
    }

    #[test]
    fn test_success() {
        let r = success("hello");
        assert_eq!(r.exit_code, 0);
        assert_eq!(r.stdout, "hello");
        assert!(r.stderr.is_empty());
    }

    #[test]
    fn test_failure() {
        let r = failure("error");
        assert_eq!(r.exit_code, 1);
        assert!(r.stdout.is_empty());
        assert_eq!(r.stderr, "error");
    }

    #[test]
    fn test_failure_with_code() {
        let r = failure_with_code("not found", 127);
        assert_eq!(r.exit_code, 127);
        assert_eq!(r.stderr, "not found");
    }

    #[test]
    fn test_result() {
        let r = result("out", "err", 42);
        assert_eq!(r.stdout, "out");
        assert_eq!(r.stderr, "err");
        assert_eq!(r.exit_code, 42);
    }

    #[test]
    fn test_test_result_fn() {
        assert_eq!(super::test_result(true).exit_code, 0);
        assert_eq!(super::test_result(false).exit_code, 1);
    }
}
