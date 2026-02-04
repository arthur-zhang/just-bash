//! Control Flow Errors
//!
//! Error types used to implement shell control flow:
//! - break: Exit loops
//! - continue: Skip to next iteration
//! - return: Exit functions
//! - errexit: Exit on error (set -e)
//! - nounset: Error on unset variables (set -u)
//!
//! All control flow errors carry stdout/stderr to accumulate output
//! as they propagate through the execution stack.

use std::fmt;

/// Base trait for control flow errors that carry stdout/stderr.
pub trait ControlFlowError: std::error::Error {
    fn stdout(&self) -> &str;
    fn stderr(&self) -> &str;
    fn stdout_mut(&mut self) -> &mut String;
    fn stderr_mut(&mut self) -> &mut String;

    /// Prepend output from the current context before re-throwing.
    fn prepend_output(&mut self, stdout: &str, stderr: &str) {
        let new_stdout = format!("{}{}", stdout, self.stdout());
        let new_stderr = format!("{}{}", stderr, self.stderr());
        *self.stdout_mut() = new_stdout;
        *self.stderr_mut() = new_stderr;
    }
}

/// Error thrown when break is called to exit loops.
#[derive(Debug, Clone)]
pub struct BreakError {
    pub levels: u32,
    pub stdout: String,
    pub stderr: String,
}

impl BreakError {
    pub fn new(levels: u32, stdout: String, stderr: String) -> Self {
        Self { levels, stdout, stderr }
    }
}

impl Default for BreakError {
    fn default() -> Self {
        Self { levels: 1, stdout: String::new(), stderr: String::new() }
    }
}

impl fmt::Display for BreakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "break")
    }
}

impl std::error::Error for BreakError {}

impl ControlFlowError for BreakError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when continue is called to skip to next iteration.
#[derive(Debug, Clone)]
pub struct ContinueError {
    pub levels: u32,
    pub stdout: String,
    pub stderr: String,
}

impl ContinueError {
    pub fn new(levels: u32, stdout: String, stderr: String) -> Self {
        Self { levels, stdout, stderr }
    }
}

impl Default for ContinueError {
    fn default() -> Self {
        Self { levels: 1, stdout: String::new(), stderr: String::new() }
    }
}

impl fmt::Display for ContinueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "continue")
    }
}

impl std::error::Error for ContinueError {}

impl ControlFlowError for ContinueError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when return is called to exit a function.
#[derive(Debug, Clone)]
pub struct ReturnError {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ReturnError {
    pub fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self { exit_code, stdout, stderr }
    }
}

impl Default for ReturnError {
    fn default() -> Self {
        Self { exit_code: 0, stdout: String::new(), stderr: String::new() }
    }
}

impl fmt::Display for ReturnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "return")
    }
}

impl std::error::Error for ReturnError {}

impl ControlFlowError for ReturnError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when set -e (errexit) is enabled and a command fails.
#[derive(Debug, Clone)]
pub struct ErrexitError {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ErrexitError {
    pub fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self { exit_code, stdout, stderr }
    }
}

impl fmt::Display for ErrexitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "errexit: command exited with status {}", self.exit_code)
    }
}

impl std::error::Error for ErrexitError {}

impl ControlFlowError for ErrexitError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when set -u (nounset) is enabled and an unset variable is referenced.
#[derive(Debug, Clone)]
pub struct NounsetError {
    pub var_name: String,
    pub stdout: String,
    pub stderr: String,
}

impl NounsetError {
    pub fn new(var_name: String, stdout: String) -> Self {
        let stderr = format!("bash: {}: unbound variable\n", var_name);
        Self { var_name, stdout, stderr }
    }
}

impl fmt::Display for NounsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: unbound variable", self.var_name)
    }
}

impl std::error::Error for NounsetError {}

impl ControlFlowError for NounsetError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when exit builtin is called to terminate the script.
#[derive(Debug, Clone)]
pub struct ExitError {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ExitError {
    pub fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self { exit_code, stdout, stderr }
    }
}

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit")
    }
}

impl std::error::Error for ExitError {}

impl ControlFlowError for ExitError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown for arithmetic expression errors (e.g., floating point, invalid syntax).
/// Returns exit code 1 instead of 2 (syntax error).
#[derive(Debug, Clone)]
pub struct ArithmeticError {
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    /// If true, this error should abort script execution (like missing operand after binary operator).
    /// If false, the error is recoverable and execution can continue.
    pub fatal: bool,
}

impl ArithmeticError {
    pub fn new(message: String, stdout: String, stderr: String, fatal: bool) -> Self {
        let stderr = if stderr.is_empty() {
            format!("bash: {}\n", message)
        } else {
            stderr
        };
        Self { message, stdout, stderr, fatal }
    }

    pub fn simple(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::new(msg.clone(), String::new(), format!("bash: {}\n", msg), false)
    }
}

impl fmt::Display for ArithmeticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArithmeticError {}

impl ControlFlowError for ArithmeticError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown for bad substitution errors (e.g., ${#var:1:3}).
/// Returns exit code 1.
#[derive(Debug, Clone)]
pub struct BadSubstitutionError {
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

impl BadSubstitutionError {
    pub fn new(message: String, stdout: String, stderr: String) -> Self {
        let stderr = if stderr.is_empty() {
            format!("bash: {}: bad substitution\n", message)
        } else {
            stderr
        };
        Self { message, stdout, stderr }
    }

    pub fn simple(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::new(msg.clone(), String::new(), format!("bash: {}: bad substitution\n", msg))
    }
}

impl fmt::Display for BadSubstitutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BadSubstitutionError {}

impl ControlFlowError for BadSubstitutionError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when failglob is enabled and a glob pattern has no matches.
/// Returns exit code 1.
#[derive(Debug, Clone)]
pub struct GlobError {
    pub pattern: String,
    pub stdout: String,
    pub stderr: String,
}

impl GlobError {
    pub fn new(pattern: String, stdout: String, stderr: String) -> Self {
        let stderr = if stderr.is_empty() {
            format!("bash: no match: {}\n", pattern)
        } else {
            stderr
        };
        Self { pattern, stdout, stderr }
    }

    pub fn simple(pattern: impl Into<String>) -> Self {
        let pat = pattern.into();
        Self::new(pat.clone(), String::new(), format!("bash: no match: {}\n", pat))
    }
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no match: {}", self.pattern)
    }
}

impl std::error::Error for GlobError {}

impl ControlFlowError for GlobError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown for invalid brace expansions (e.g., mixed case character ranges like {z..A}).
/// Returns exit code 1 (matching bash behavior).
#[derive(Debug, Clone)]
pub struct BraceExpansionError {
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

impl BraceExpansionError {
    pub fn new(message: String, stdout: String, stderr: String) -> Self {
        let stderr = if stderr.is_empty() {
            format!("bash: {}\n", message)
        } else {
            stderr
        };
        Self { message, stdout, stderr }
    }

    pub fn simple(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::new(msg.clone(), String::new(), format!("bash: {}\n", msg))
    }
}

impl fmt::Display for BraceExpansionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BraceExpansionError {}

impl ControlFlowError for BraceExpansionError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// The type of execution limit that was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitType {
    Recursion,
    Commands,
    Iterations,
}

impl fmt::Display for LimitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LimitType::Recursion => write!(f, "recursion"),
            LimitType::Commands => write!(f, "commands"),
            LimitType::Iterations => write!(f, "iterations"),
        }
    }
}

/// Error thrown when execution limits are exceeded (recursion depth, command count, loop iterations).
/// This should ALWAYS be thrown before Rust's native stack overflow kicks in.
/// Exit code 126 indicates a limit was exceeded.
#[derive(Debug, Clone)]
pub struct ExecutionLimitError {
    pub message: String,
    pub limit_type: LimitType,
    pub stdout: String,
    pub stderr: String,
}

impl ExecutionLimitError {
    pub const EXIT_CODE: i32 = 126;

    pub fn new(message: String, limit_type: LimitType, stdout: String, stderr: String) -> Self {
        let stderr = if stderr.is_empty() {
            format!("bash: {}\n", message)
        } else {
            stderr
        };
        Self { message, limit_type, stdout, stderr }
    }

    pub fn simple(message: impl Into<String>, limit_type: LimitType) -> Self {
        let msg = message.into();
        Self::new(msg.clone(), limit_type, String::new(), format!("bash: {}\n", msg))
    }
}

impl fmt::Display for ExecutionLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExecutionLimitError {}

impl ControlFlowError for ExecutionLimitError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when break/continue is called in a subshell that was
/// spawned from within a loop context. Causes the subshell to exit cleanly.
#[derive(Debug, Clone)]
pub struct SubshellExitError {
    pub stdout: String,
    pub stderr: String,
}

impl SubshellExitError {
    pub fn new(stdout: String, stderr: String) -> Self {
        Self { stdout, stderr }
    }
}

impl Default for SubshellExitError {
    fn default() -> Self {
        Self { stdout: String::new(), stderr: String::new() }
    }
}

impl fmt::Display for SubshellExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "subshell exit")
    }
}

impl std::error::Error for SubshellExitError {}

impl ControlFlowError for SubshellExitError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Error thrown when a POSIX special builtin fails in POSIX mode.
/// In POSIX mode (set -o posix), errors in special builtins like
/// shift, set, readonly, export, etc. cause the entire script to exit.
///
/// Per POSIX 2.8.1 - Consequences of Shell Errors:
/// "A special built-in utility causes an interactive or non-interactive shell
/// to exit when an error occurs."
#[derive(Debug, Clone)]
pub struct PosixFatalError {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl PosixFatalError {
    pub fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self { exit_code, stdout, stderr }
    }
}

impl fmt::Display for PosixFatalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "posix fatal error")
    }
}

impl std::error::Error for PosixFatalError {}

impl ControlFlowError for PosixFatalError {
    fn stdout(&self) -> &str { &self.stdout }
    fn stderr(&self) -> &str { &self.stderr }
    fn stdout_mut(&mut self) -> &mut String { &mut self.stdout }
    fn stderr_mut(&mut self) -> &mut String { &mut self.stderr }
}

/// Unified error enum for all interpreter errors.
#[derive(Debug, Clone)]
pub enum InterpreterError {
    Break(BreakError),
    Continue(ContinueError),
    Return(ReturnError),
    Errexit(ErrexitError),
    Nounset(NounsetError),
    Exit(ExitError),
    Arithmetic(ArithmeticError),
    BadSubstitution(BadSubstitutionError),
    Glob(GlobError),
    BraceExpansion(BraceExpansionError),
    ExecutionLimit(ExecutionLimitError),
    SubshellExit(SubshellExitError),
    PosixFatal(PosixFatalError),
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterpreterError::Break(e) => write!(f, "{}", e),
            InterpreterError::Continue(e) => write!(f, "{}", e),
            InterpreterError::Return(e) => write!(f, "{}", e),
            InterpreterError::Errexit(e) => write!(f, "{}", e),
            InterpreterError::Nounset(e) => write!(f, "{}", e),
            InterpreterError::Exit(e) => write!(f, "{}", e),
            InterpreterError::Arithmetic(e) => write!(f, "{}", e),
            InterpreterError::BadSubstitution(e) => write!(f, "{}", e),
            InterpreterError::Glob(e) => write!(f, "{}", e),
            InterpreterError::BraceExpansion(e) => write!(f, "{}", e),
            InterpreterError::ExecutionLimit(e) => write!(f, "{}", e),
            InterpreterError::SubshellExit(e) => write!(f, "{}", e),
            InterpreterError::PosixFatal(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for InterpreterError {}

/// Check if an error is a scope exit error (return, break, continue).
/// These need special handling vs errexit/nounset which terminate execution.
pub fn is_scope_exit_error(error: &InterpreterError) -> bool {
    matches!(
        error,
        InterpreterError::Break(_) | InterpreterError::Continue(_) | InterpreterError::Return(_)
    )
}

// Implement From for each error type
impl From<BreakError> for InterpreterError {
    fn from(e: BreakError) -> Self { InterpreterError::Break(e) }
}

impl From<ContinueError> for InterpreterError {
    fn from(e: ContinueError) -> Self { InterpreterError::Continue(e) }
}

impl From<ReturnError> for InterpreterError {
    fn from(e: ReturnError) -> Self { InterpreterError::Return(e) }
}

impl From<ErrexitError> for InterpreterError {
    fn from(e: ErrexitError) -> Self { InterpreterError::Errexit(e) }
}

impl From<NounsetError> for InterpreterError {
    fn from(e: NounsetError) -> Self { InterpreterError::Nounset(e) }
}

impl From<ExitError> for InterpreterError {
    fn from(e: ExitError) -> Self { InterpreterError::Exit(e) }
}

impl From<ArithmeticError> for InterpreterError {
    fn from(e: ArithmeticError) -> Self { InterpreterError::Arithmetic(e) }
}

impl From<BadSubstitutionError> for InterpreterError {
    fn from(e: BadSubstitutionError) -> Self { InterpreterError::BadSubstitution(e) }
}

impl From<GlobError> for InterpreterError {
    fn from(e: GlobError) -> Self { InterpreterError::Glob(e) }
}

impl From<BraceExpansionError> for InterpreterError {
    fn from(e: BraceExpansionError) -> Self { InterpreterError::BraceExpansion(e) }
}

impl From<ExecutionLimitError> for InterpreterError {
    fn from(e: ExecutionLimitError) -> Self { InterpreterError::ExecutionLimit(e) }
}

impl From<SubshellExitError> for InterpreterError {
    fn from(e: SubshellExitError) -> Self { InterpreterError::SubshellExit(e) }
}

impl From<PosixFatalError> for InterpreterError {
    fn from(e: PosixFatalError) -> Self { InterpreterError::PosixFatal(e) }
}
