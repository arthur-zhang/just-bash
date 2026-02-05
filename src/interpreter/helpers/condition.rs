//! Condition execution helper for the interpreter.
//!
//! Handles executing condition statements with proper inCondition state management.
//! Used by if, while, and until loops.

use crate::interpreter::types::InterpreterState;

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

/// Execute condition statements with in_condition flag set.
/// This prevents errexit from triggering during condition evaluation.
///
/// # Arguments
/// * `state` - Interpreter state (in_condition will be temporarily set to true)
/// * `statements` - Condition statements to execute
/// * `executor` - Function to execute a single statement
///
/// # Returns
/// Accumulated stdout, stderr, and final exit code, or an error.
pub fn execute_condition<S, F, E>(
    state: &mut InterpreterState,
    statements: &[S],
    mut executor: F,
) -> Result<ConditionResult, E>
where
    F: FnMut(&mut InterpreterState, &S) -> Result<ConditionResult, E>,
{
    let saved_in_condition = state.in_condition;
    state.in_condition = true;

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    let result = (|| {
        for stmt in statements {
            let res = executor(state, stmt)?;
            stdout.push_str(&res.stdout);
            stderr.push_str(&res.stderr);
            exit_code = res.exit_code;
        }
        Ok(ConditionResult::new(stdout, stderr, exit_code))
    })();

    state.in_condition = saved_in_condition;
    result
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

    #[test]
    fn test_execute_condition_sets_in_condition_flag() {
        let mut state = InterpreterState::default();
        assert!(!state.in_condition);

        let statements: Vec<i32> = vec![1];
        let mut was_in_condition = false;

        let result: Result<ConditionResult, ()> = execute_condition(
            &mut state,
            &statements,
            |s, _| {
                was_in_condition = s.in_condition;
                Ok(ConditionResult::success())
            },
        );

        assert!(result.is_ok());
        assert!(was_in_condition);
        assert!(!state.in_condition);
    }

    #[test]
    fn test_execute_condition_restores_flag_on_error() {
        let mut state = InterpreterState::default();
        state.in_condition = false;

        let statements: Vec<i32> = vec![1];

        let result: Result<ConditionResult, &str> = execute_condition(
            &mut state,
            &statements,
            |_, _| Err("test error"),
        );

        assert!(result.is_err());
        assert!(!state.in_condition);
    }

    #[test]
    fn test_execute_condition_accumulates_output() {
        let mut state = InterpreterState::default();

        let statements: Vec<i32> = vec![1, 2, 3];

        let result: Result<ConditionResult, ()> = execute_condition(
            &mut state,
            &statements,
            |_, stmt| {
                Ok(ConditionResult::new(
                    format!("out{}", stmt),
                    format!("err{}", stmt),
                    *stmt,
                ))
            },
        );

        let res = result.unwrap();
        assert_eq!(res.stdout, "out1out2out3");
        assert_eq!(res.stderr, "err1err2err3");
        assert_eq!(res.exit_code, 3);
    }

    #[test]
    fn test_execute_condition_preserves_existing_in_condition() {
        let mut state = InterpreterState::default();
        state.in_condition = true;

        let statements: Vec<i32> = vec![1];

        let _: Result<ConditionResult, ()> = execute_condition(
            &mut state,
            &statements,
            |_, _| Ok(ConditionResult::success()),
        );

        assert!(state.in_condition);
    }

    #[test]
    fn test_execute_condition_empty_statements() {
        let mut state = InterpreterState::default();

        let statements: Vec<i32> = vec![];

        let result: Result<ConditionResult, ()> = execute_condition(
            &mut state,
            &statements,
            |_, _| Ok(ConditionResult::success()),
        );

        let res = result.unwrap();
        assert!(res.stdout.is_empty());
        assert!(res.stderr.is_empty());
        assert_eq!(res.exit_code, 0);
    }
}
