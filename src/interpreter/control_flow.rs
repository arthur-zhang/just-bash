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
use crate::interpreter::types::{InterpreterState, ExecResult};
use crate::interpreter::helpers::condition::{execute_condition, ConditionResult};
use crate::interpreter::helpers::loop_helpers::{handle_loop_error, LoopAction};
use crate::interpreter::errors::{InterpreterError, ExecutionLimitError, LimitType};

/// Validate that a variable name is a valid identifier.
/// Returns true if valid, false otherwise.
pub fn is_valid_identifier(name: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    re.is_match(name)
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

// ============================================================================
// Control Flow Execution Functions
// ============================================================================

/// Result of executing an if statement.
#[derive(Debug, Clone)]
pub struct IfResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl IfResult {
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }
}

/// Execute an if/elif/else statement.
///
/// # Arguments
/// * `state` - Interpreter state
/// * `clauses` - List of (condition_statements, body_statements) pairs
/// * `else_body` - Optional else body statements
/// * `condition_executor` - Function to execute condition statements
/// * `body_executor` - Function to execute body statements
///
/// # Returns
/// Result with accumulated stdout, stderr, and exit code
pub fn execute_if<C, B, F1, F2, E>(
    state: &mut InterpreterState,
    clauses: &[(Vec<C>, Vec<B>)],
    else_body: Option<&[B]>,
    mut condition_executor: F1,
    mut body_executor: F2,
) -> Result<IfResult, E>
where
    F1: FnMut(&mut InterpreterState, &C) -> Result<ConditionResult, E>,
    F2: FnMut(&mut InterpreterState, &B) -> Result<ExecResult, E>,
{
    let mut stdout = String::new();
    let mut stderr = String::new();

    for (condition, body) in clauses {
        // Execute condition with in_condition flag set
        let cond_result = execute_condition(state, condition, &mut condition_executor)?;
        stdout.push_str(&cond_result.stdout);
        stderr.push_str(&cond_result.stderr);

        if cond_result.exit_code == 0 {
            // Condition is true, execute body
            let mut exit_code = 0;
            for stmt in body {
                let res = body_executor(state, stmt)?;
                stdout.push_str(&res.stdout);
                stderr.push_str(&res.stderr);
                exit_code = res.exit_code;
            }
            return Ok(IfResult::new(stdout, stderr, exit_code));
        }
    }

    // No condition matched, execute else body if present
    if let Some(else_stmts) = else_body {
        let mut exit_code = 0;
        for stmt in else_stmts {
            let res = body_executor(state, stmt)?;
            stdout.push_str(&res.stdout);
            stderr.push_str(&res.stderr);
            exit_code = res.exit_code;
        }
        return Ok(IfResult::new(stdout, stderr, exit_code));
    }

    Ok(IfResult::new(stdout, stderr, 0))
}

/// Result of executing a for loop.
#[derive(Debug, Clone)]
pub struct ForResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ForResult {
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }
}

/// Execute a for loop.
///
/// # Arguments
/// * `state` - Interpreter state
/// * `variable` - Loop variable name
/// * `words` - List of words to iterate over
/// * `body` - Body statements
/// * `max_iterations` - Maximum allowed iterations
/// * `executor` - Function to execute body statements
///
/// # Returns
/// Result with accumulated stdout, stderr, and exit code
pub fn execute_for<B, F>(
    state: &mut InterpreterState,
    variable: &str,
    words: &[String],
    body: &[B],
    max_iterations: u64,
    mut executor: F,
) -> Result<ForResult, InterpreterError>
where
    F: FnMut(&mut InterpreterState, &B) -> Result<ExecResult, InterpreterError>,
{
    // Validate variable name
    if !is_valid_identifier(variable) {
        return Ok(ForResult::new(
            String::new(),
            format!("bash: `{}': not a valid identifier\n", variable),
            1,
        ));
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;
    let mut iterations = 0u64;

    state.loop_depth += 1;

    let result = (|| {
        for value in words {
            iterations += 1;
            if iterations > max_iterations {
                return Err(InterpreterError::ExecutionLimit(ExecutionLimitError::new(
                    format!("for loop: too many iterations ({})", max_iterations),
                    LimitType::Iterations,
                    stdout.clone(),
                    stderr.clone(),
                )));
            }

            state.env.insert(variable.to_string(), value.clone());

            for stmt in body {
                match executor(state, stmt) {
                    Ok(res) => {
                        stdout.push_str(&res.stdout);
                        stderr.push_str(&res.stderr);
                        exit_code = res.exit_code;
                    }
                    Err(error) => {
                        let loop_result = handle_loop_error(
                            error,
                            stdout.clone(),
                            stderr.clone(),
                            state.loop_depth,
                        );
                        stdout = loop_result.stdout;
                        stderr = loop_result.stderr;
                        match loop_result.action {
                            LoopAction::Break => return Ok(ForResult::new(stdout, stderr, exit_code)),
                            LoopAction::Continue => break,
                            LoopAction::Error => {
                                return Ok(ForResult::new(stdout, stderr, loop_result.exit_code.unwrap_or(1)));
                            }
                            LoopAction::Rethrow => {
                                return Err(loop_result.error.unwrap());
                            }
                        }
                    }
                }
            }
        }
        Ok(ForResult::new(stdout, stderr, exit_code))
    })();

    state.loop_depth -= 1;
    result
}

/// Execute a while loop.
///
/// # Arguments
/// * `state` - Interpreter state
/// * `condition` - Condition statements
/// * `body` - Body statements
/// * `max_iterations` - Maximum allowed iterations
/// * `condition_executor` - Function to execute condition statements
/// * `body_executor` - Function to execute body statements
///
/// # Returns
/// Result with accumulated stdout, stderr, and exit code
pub fn execute_while<C, B, F1, F2>(
    state: &mut InterpreterState,
    condition: &[C],
    body: &[B],
    max_iterations: u64,
    mut condition_executor: F1,
    mut body_executor: F2,
) -> Result<ForResult, InterpreterError>
where
    F1: FnMut(&mut InterpreterState, &C) -> Result<ConditionResult, InterpreterError>,
    F2: FnMut(&mut InterpreterState, &B) -> Result<ExecResult, InterpreterError>,
{
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;
    let mut iterations = 0u64;

    state.loop_depth += 1;

    let result = (|| {
        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(InterpreterError::ExecutionLimit(ExecutionLimitError::new(
                    format!("while loop: too many iterations ({})", max_iterations),
                    LimitType::Iterations,
                    stdout.clone(),
                    stderr.clone(),
                )));
            }

            // Execute condition
            let cond_result = execute_condition(state, condition, &mut condition_executor)?;
            stdout.push_str(&cond_result.stdout);
            stderr.push_str(&cond_result.stderr);

            if cond_result.exit_code != 0 {
                break;
            }

            // Execute body
            for stmt in body {
                match body_executor(state, stmt) {
                    Ok(res) => {
                        stdout.push_str(&res.stdout);
                        stderr.push_str(&res.stderr);
                        exit_code = res.exit_code;
                    }
                    Err(error) => {
                        let loop_result = handle_loop_error(
                            error,
                            stdout.clone(),
                            stderr.clone(),
                            state.loop_depth,
                        );
                        stdout = loop_result.stdout;
                        stderr = loop_result.stderr;
                        match loop_result.action {
                            LoopAction::Break => return Ok(ForResult::new(stdout, stderr, exit_code)),
                            LoopAction::Continue => break,
                            LoopAction::Error => {
                                return Ok(ForResult::new(stdout, stderr, loop_result.exit_code.unwrap_or(1)));
                            }
                            LoopAction::Rethrow => {
                                return Err(loop_result.error.unwrap());
                            }
                        }
                    }
                }
            }
        }
        Ok(ForResult::new(stdout, stderr, exit_code))
    })();

    state.loop_depth -= 1;
    result
}

/// Execute an until loop.
///
/// # Arguments
/// * `state` - Interpreter state
/// * `condition` - Condition statements
/// * `body` - Body statements
/// * `max_iterations` - Maximum allowed iterations
/// * `condition_executor` - Function to execute condition statements
/// * `body_executor` - Function to execute body statements
///
/// # Returns
/// Result with accumulated stdout, stderr, and exit code
pub fn execute_until<C, B, F1, F2>(
    state: &mut InterpreterState,
    condition: &[C],
    body: &[B],
    max_iterations: u64,
    mut condition_executor: F1,
    mut body_executor: F2,
) -> Result<ForResult, InterpreterError>
where
    F1: FnMut(&mut InterpreterState, &C) -> Result<ConditionResult, InterpreterError>,
    F2: FnMut(&mut InterpreterState, &B) -> Result<ExecResult, InterpreterError>,
{
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;
    let mut iterations = 0u64;

    state.loop_depth += 1;

    let result = (|| {
        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(InterpreterError::ExecutionLimit(ExecutionLimitError::new(
                    format!("until loop: too many iterations ({})", max_iterations),
                    LimitType::Iterations,
                    stdout.clone(),
                    stderr.clone(),
                )));
            }

            // Execute condition
            let cond_result = execute_condition(state, condition, &mut condition_executor)?;
            stdout.push_str(&cond_result.stdout);
            stderr.push_str(&cond_result.stderr);

            // Until loop exits when condition becomes true (exit code 0)
            if cond_result.exit_code == 0 {
                break;
            }

            // Execute body
            for stmt in body {
                match body_executor(state, stmt) {
                    Ok(res) => {
                        stdout.push_str(&res.stdout);
                        stderr.push_str(&res.stderr);
                        exit_code = res.exit_code;
                    }
                    Err(error) => {
                        let loop_result = handle_loop_error(
                            error,
                            stdout.clone(),
                            stderr.clone(),
                            state.loop_depth,
                        );
                        stdout = loop_result.stdout;
                        stderr = loop_result.stderr;
                        match loop_result.action {
                            LoopAction::Break => return Ok(ForResult::new(stdout, stderr, exit_code)),
                            LoopAction::Continue => break,
                            LoopAction::Error => {
                                return Ok(ForResult::new(stdout, stderr, loop_result.exit_code.unwrap_or(1)));
                            }
                            LoopAction::Rethrow => {
                                return Err(loop_result.error.unwrap());
                            }
                        }
                    }
                }
            }
        }
        Ok(ForResult::new(stdout, stderr, exit_code))
    })();

    state.loop_depth -= 1;
    result
}

/// Case item with pattern and body.
pub struct CaseItem<'a, P, B> {
    pub patterns: &'a [P],
    pub body: &'a [B],
    pub terminator: CaseTerminator,
}

/// Execute a case statement.
///
/// # Arguments
/// * `state` - Interpreter state
/// * `value` - The value to match against patterns
/// * `items` - List of case items (patterns, body, terminator)
/// * `pattern_matcher` - Function to check if value matches a pattern
/// * `body_executor` - Function to execute body statements
///
/// # Returns
/// Result with accumulated stdout, stderr, and exit code
pub fn execute_case<P, B, F1, F2, E>(
    state: &mut InterpreterState,
    value: &str,
    items: &[CaseItem<P, B>],
    mut pattern_matcher: F1,
    mut body_executor: F2,
) -> Result<ForResult, E>
where
    F1: FnMut(&InterpreterState, &str, &P) -> Result<bool, E>,
    F2: FnMut(&mut InterpreterState, &B) -> Result<ExecResult, E>,
{
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;
    let mut fall_through = false;

    for item in items {
        let mut matched = fall_through;

        if !fall_through {
            // Normal pattern matching
            for pattern in item.patterns {
                if pattern_matcher(state, value, pattern)? {
                    matched = true;
                    break;
                }
            }
        }

        if matched {
            // Execute body
            for stmt in item.body {
                let res = body_executor(state, stmt)?;
                stdout.push_str(&res.stdout);
                stderr.push_str(&res.stderr);
                exit_code = res.exit_code;
            }

            // Handle terminator
            match item.terminator {
                CaseTerminator::Break => break,
                CaseTerminator::FallThrough => fall_through = true,
                CaseTerminator::ContinueMatching => fall_through = false,
            }
        } else {
            fall_through = false;
        }
    }

    Ok(ForResult::new(stdout, stderr, exit_code))
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
}
