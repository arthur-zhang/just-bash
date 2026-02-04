//! Condition execution helper for the interpreter.
//!
//! Handles executing condition statements with proper inCondition state management.
//! Used by if, while, and until loops.

/// Result of executing a condition.
#[derive(Debug, Clone, Default)]
pub struct ConditionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ConditionResult {
    /// Create a new condition result.
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }

    /// Create a successful condition result (exit code 0).
    pub fn success() -> Self {
        Self::default()
    }

    /// Create a failed condition result (exit code 1).
    pub fn failure() -> Self {
        Self { exit_code: 1, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_result_new() {
        let result = ConditionResult::new("out".to_string(), "err".to_string(), 42);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    fn test_condition_result_success() {
        let result = ConditionResult::success();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_condition_result_failure() {
        let result = ConditionResult::failure();
        assert_eq!(result.exit_code, 1);
    }
}
