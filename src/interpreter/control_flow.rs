//! Control Flow Execution
//!
//! Handles control flow constructs:
//! - if/elif/else
//! - for loops
//! - C-style for loops
//! - while loops
//! - until loops
//! - case statements
//! - break/continue

use regex_lite::Regex;
use crate::interpreter::errors::{BreakError, ContinueError};
use crate::interpreter::types::ExecResult;

/// Validate that a variable name is a valid identifier.
/// Returns true if valid, false otherwise.
pub fn is_valid_identifier(name: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    re.is_match(name)
}

/// Result of handling a loop error (break/continue).
#[derive(Debug)]
pub enum LoopAction {
    /// Break out of the loop
    Break,
    /// Continue to next iteration
    Continue,
    /// Return with an error result
    Error { exit_code: i32 },
    /// Re-throw the error
    Rethrow,
}

/// Handle break/continue errors in a loop context.
/// Returns the action to take and updated stdout/stderr.
pub fn handle_loop_control_error(
    error: &mut dyn std::any::Any,
    stdout: &mut String,
    stderr: &mut String,
    loop_depth: u32,
) -> LoopAction {
    if let Some(break_err) = error.downcast_mut::<BreakError>() {
        stdout.push_str(&break_err.stdout);
        stderr.push_str(&break_err.stderr);

        if break_err.levels > 1 && loop_depth > 1 {
            break_err.levels -= 1;
            break_err.stdout = stdout.clone();
            break_err.stderr = stderr.clone();
            return LoopAction::Rethrow;
        }
        return LoopAction::Break;
    }

    if let Some(continue_err) = error.downcast_mut::<ContinueError>() {
        stdout.push_str(&continue_err.stdout);
        stderr.push_str(&continue_err.stderr);

        if continue_err.levels > 1 && loop_depth > 1 {
            continue_err.levels -= 1;
            continue_err.stdout = stdout.clone();
            continue_err.stderr = stderr.clone();
            return LoopAction::Rethrow;
        }
        return LoopAction::Continue;
    }

    LoopAction::Rethrow
}

/// Case statement terminator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseTerminator {
    /// ;; - stop, no fall-through
    Break,
    /// ;& - unconditional fall-through (execute next body without pattern check)
    FallThrough,
    /// ;;& - continue pattern matching (check next case patterns)
    ContinueMatching,
}

impl CaseTerminator {
    /// Parse a terminator string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            ";;" => Some(CaseTerminator::Break),
            ";&" => Some(CaseTerminator::FallThrough),
            ";;&" => Some(CaseTerminator::ContinueMatching),
            _ => None,
        }
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            CaseTerminator::Break => ";;",
            CaseTerminator::FallThrough => ";&",
            CaseTerminator::ContinueMatching => ";;&",
        }
    }
}

/// Loop iteration result for tracking state across iterations.
#[derive(Debug, Default)]
pub struct LoopState {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub iterations: u32,
}

impl LoopState {
    /// Create a new loop state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append output from an execution result.
    pub fn append_result(&mut self, result: &ExecResult) {
        self.stdout.push_str(&result.stdout);
        self.stderr.push_str(&result.stderr);
        self.exit_code = result.exit_code;
    }

    /// Convert to an ExecResult.
    pub fn to_result(&self) -> ExecResult {
        ExecResult {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
            env: None,
        }
    }

    /// Check if iteration limit is exceeded.
    pub fn check_iteration_limit(&self, max_iterations: u32, loop_type: &str) -> Option<String> {
        if self.iterations > max_iterations {
            Some(format!(
                "{} loop: too many iterations ({}), increase executionLimits.maxLoopIterations",
                loop_type, max_iterations
            ))
        } else {
            None
        }
    }
}

/// Condition evaluation result.
#[derive(Debug)]
pub struct ConditionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// Whether to break out of the enclosing loop
    pub should_break: bool,
    /// Whether to continue to next iteration
    pub should_continue: bool,
}

impl ConditionResult {
    /// Create a successful condition result.
    pub fn success(stdout: String, stderr: String) -> Self {
        Self {
            stdout,
            stderr,
            exit_code: 0,
            should_break: false,
            should_continue: false,
        }
    }

    /// Create a failed condition result.
    pub fn failure(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            should_break: false,
            should_continue: false,
        }
    }

    /// Check if the condition passed (exit code 0).
    pub fn passed(&self) -> bool {
        self.exit_code == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("foo123"));
        assert!(is_valid_identifier("_123"));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
        assert!(!is_valid_identifier("foo bar"));
        assert!(!is_valid_identifier(""));
    }

    #[test]
    fn test_case_terminator() {
        assert_eq!(CaseTerminator::from_str(";;"), Some(CaseTerminator::Break));
        assert_eq!(CaseTerminator::from_str(";&"), Some(CaseTerminator::FallThrough));
        assert_eq!(CaseTerminator::from_str(";;&"), Some(CaseTerminator::ContinueMatching));
        assert_eq!(CaseTerminator::from_str("invalid"), None);

        assert_eq!(CaseTerminator::Break.as_str(), ";;");
        assert_eq!(CaseTerminator::FallThrough.as_str(), ";&");
        assert_eq!(CaseTerminator::ContinueMatching.as_str(), ";;&");
    }

    #[test]
    fn test_loop_state() {
        let mut state = LoopState::new();
        assert_eq!(state.iterations, 0);
        assert_eq!(state.exit_code, 0);

        state.iterations = 5;
        state.stdout = "output".to_string();
        state.stderr = "error".to_string();
        state.exit_code = 1;

        let result = state.to_result();
        assert_eq!(result.stdout, "output");
        assert_eq!(result.stderr, "error");
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_loop_state_iteration_limit() {
        let mut state = LoopState::new();
        state.iterations = 100;

        assert!(state.check_iteration_limit(50, "for").is_some());
        assert!(state.check_iteration_limit(100, "for").is_none());
        assert!(state.check_iteration_limit(200, "for").is_none());
    }

    #[test]
    fn test_condition_result() {
        let success = ConditionResult::success("out".to_string(), "err".to_string());
        assert!(success.passed());
        assert!(!success.should_break);
        assert!(!success.should_continue);

        let failure = ConditionResult::failure("out".to_string(), "err".to_string(), 1);
        assert!(!failure.passed());
    }
}
