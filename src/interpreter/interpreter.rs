//! Interpreter - AST Execution Engine
//!
//! Main interpreter traits and types for executing bash AST nodes.
//! This module defines the interfaces that a runtime must implement
//! to execute bash scripts.
//!
//! The actual execution logic requires runtime dependencies:
//! - File system access
//! - Command execution
//! - Network access (optional)
//! - Async operations
//!
//! Delegates to specialized modules for:
//! - Word expansion (expansion/)
//! - Arithmetic evaluation (arithmetic.rs)
//! - Conditional evaluation (conditionals.rs)
//! - Built-in commands (builtins/)
//! - Redirections (redirections.rs)

use std::collections::HashMap;
use crate::ast::types::{
    CommandNode, PipelineNode, ScriptNode, SimpleCommandNode, StatementNode,
};
use crate::interpreter::types::{ExecResult, ExecutionLimits, InterpreterState};

/// Options for creating an interpreter instance.
#[derive(Debug, Clone)]
pub struct InterpreterOptions {
    /// Execution limits (max recursion, max commands, etc.)
    pub limits: ExecutionLimits,
}

impl Default for InterpreterOptions {
    fn default() -> Self {
        Self {
            limits: ExecutionLimits::default(),
        }
    }
}

/// File system interface for the interpreter.
///
/// This trait must be implemented by the runtime to provide
/// file system access to the interpreter.
pub trait FileSystem: Send + Sync {
    /// Read a file's contents.
    fn read_file(&self, path: &str) -> Result<String, std::io::Error>;

    /// Write contents to a file.
    fn write_file(&self, path: &str, contents: &str) -> Result<(), std::io::Error>;

    /// Append contents to a file.
    fn append_file(&self, path: &str, contents: &str) -> Result<(), std::io::Error>;

    /// Check if a path exists.
    fn exists(&self, path: &str) -> bool;

    /// Check if a path is a directory.
    fn is_dir(&self, path: &str) -> bool;

    /// Check if a path is a file.
    fn is_file(&self, path: &str) -> bool;

    /// Resolve a path relative to a base directory.
    fn resolve_path(&self, base: &str, path: &str) -> String;

    /// Get file metadata.
    fn stat(&self, path: &str) -> Result<FileStat, std::io::Error>;

    /// List directory contents.
    fn read_dir(&self, path: &str) -> Result<Vec<String>, std::io::Error>;

    /// Expand glob patterns.
    fn glob(&self, pattern: &str, cwd: &str) -> Result<Vec<String>, std::io::Error>;
}

/// File metadata.
#[derive(Debug, Clone)]
pub struct FileStat {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime: u64,
}

/// Command execution interface.
///
/// This trait must be implemented by the runtime to provide
/// external command execution.
pub trait CommandExecutor: Send + Sync {
    /// Execute an external command.
    fn execute(
        &self,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        cwd: &str,
        stdin: &str,
    ) -> ExecResult;
}

/// Script execution callback type.
///
/// Used for eval, source, and other commands that need to
/// execute parsed scripts.
pub type ExecuteScriptFn = Box<dyn Fn(&ScriptNode, &mut InterpreterState) -> ExecResult + Send + Sync>;

/// Statement execution callback type.
pub type ExecuteStatementFn = Box<dyn Fn(&StatementNode, &mut InterpreterState, &str) -> ExecResult + Send + Sync>;

/// Command execution callback type.
pub type ExecuteCommandFn = Box<dyn Fn(&CommandNode, &mut InterpreterState, &str) -> ExecResult + Send + Sync>;

/// Interpreter context passed to execution functions.
///
/// Contains all the dependencies needed for script execution.
pub struct InterpreterContext<'a> {
    /// Mutable interpreter state
    pub state: &'a mut InterpreterState,
    /// Execution limits
    pub limits: &'a ExecutionLimits,
    /// File system interface (optional, provided by runtime)
    pub fs: Option<&'a dyn FileSystem>,
    /// Command executor (optional, provided by runtime)
    pub executor: Option<&'a dyn CommandExecutor>,
}

impl<'a> InterpreterContext<'a> {
    /// Create a new interpreter context with minimal dependencies.
    pub fn new(state: &'a mut InterpreterState, limits: &'a ExecutionLimits) -> Self {
        Self {
            state,
            limits,
            fs: None,
            executor: None,
        }
    }

    /// Create a context with file system access.
    pub fn with_fs(mut self, fs: &'a dyn FileSystem) -> Self {
        self.fs = Some(fs);
        self
    }

    /// Create a context with command executor.
    pub fn with_executor(mut self, executor: &'a dyn CommandExecutor) -> Self {
        self.executor = Some(executor);
        self
    }
}

/// Build environment record containing only exported variables.
///
/// In bash, only exported variables are passed to child processes.
/// This includes both permanently exported variables (via export/declare -x)
/// and temporarily exported variables (prefix assignments like FOO=bar cmd).
pub fn build_exported_env(state: &InterpreterState) -> HashMap<String, String> {
    let mut all_exported = std::collections::HashSet::new();

    // Add permanently exported variables
    if let Some(ref exported_vars) = state.exported_vars {
        for name in exported_vars {
            all_exported.insert(name.clone());
        }
    }

    // Add temporarily exported variables
    if let Some(ref temp_exported_vars) = state.temp_exported_vars {
        for name in temp_exported_vars {
            all_exported.insert(name.clone());
        }
    }

    if all_exported.is_empty() {
        return HashMap::new();
    }

    let mut env = HashMap::new();
    for name in all_exported {
        if let Some(value) = state.env.get(&name) {
            env.insert(name, value.clone());
        }
    }
    env
}

/// Check if errexit should trigger for the given exit code and context.
///
/// Errexit (set -e) should NOT trigger when:
/// - Command was in a && or || list and wasn't the final command (short-circuit)
/// - Command was negated with !
/// - Command is part of a condition in if/while/until
/// - Exit code came from a compound command where inner execution was errexit-safe
pub fn should_trigger_errexit(
    state: &InterpreterState,
    exit_code: i32,
    was_short_circuited: bool,
    was_negated: bool,
) -> bool {
    state.options.errexit
        && exit_code != 0
        && !was_short_circuited
        && !was_negated
        && !state.in_condition
        && !state.errexit_safe.unwrap_or(false)
}

/// Update the exit code in state after command execution.
pub fn update_exit_code(state: &mut InterpreterState, exit_code: i32) {
    state.last_exit_code = exit_code;
    state.env.insert("?".to_string(), exit_code.to_string());
}

/// Increment command count and check execution limits.
///
/// Returns an error message if the limit is exceeded.
pub fn check_command_limit(state: &mut InterpreterState, limits: &ExecutionLimits) -> Option<String> {
    state.command_count += 1;
    if state.command_count > limits.max_command_count {
        Some(format!(
            "too many commands executed (>{}), increase executionLimits.maxCommandCount",
            limits.max_command_count
        ))
    } else {
        None
    }
}

/// Check recursion depth limit.
///
/// Returns an error message if the limit is exceeded.
pub fn check_recursion_limit(state: &InterpreterState, limits: &ExecutionLimits) -> Option<String> {
    if state.call_depth > limits.max_recursion_depth {
        Some(format!(
            "maximum recursion depth exceeded (>{})",
            limits.max_recursion_depth
        ))
    } else {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_exported_env_empty() {
        let state = InterpreterState::default();
        let env = build_exported_env(&state);
        assert!(env.is_empty());
    }

    #[test]
    fn test_build_exported_env_with_exports() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.env.insert("BAZ".to_string(), "qux".to_string());
        state.exported_vars = Some(["FOO".to_string()].into_iter().collect());

        let env = build_exported_env(&state);
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(env.get("BAZ"), None); // Not exported
    }

    #[test]
    fn test_build_exported_env_with_temp_exports() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.temp_exported_vars = Some(["FOO".to_string()].into_iter().collect());

        let env = build_exported_env(&state);
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_should_trigger_errexit() {
        let mut state = InterpreterState::default();
        state.options.errexit = true;

        // Should trigger for non-zero exit code
        assert!(should_trigger_errexit(&state, 1, false, false));

        // Should not trigger for zero exit code
        assert!(!should_trigger_errexit(&state, 0, false, false));

        // Should not trigger when short-circuited
        assert!(!should_trigger_errexit(&state, 1, true, false));

        // Should not trigger when negated
        assert!(!should_trigger_errexit(&state, 1, false, true));

        // Should not trigger in condition
        state.in_condition = true;
        assert!(!should_trigger_errexit(&state, 1, false, false));
    }

    #[test]
    fn test_update_exit_code() {
        let mut state = InterpreterState::default();
        update_exit_code(&mut state, 42);
        assert_eq!(state.last_exit_code, 42);
        assert_eq!(state.env.get("?"), Some(&"42".to_string()));
    }

    #[test]
    fn test_check_command_limit() {
        let mut state = InterpreterState::default();
        let limits = ExecutionLimits {
            max_command_count: 10,
            ..Default::default()
        };

        // Should not exceed limit
        for _ in 0..10 {
            assert!(check_command_limit(&mut state, &limits).is_none());
        }

        // Should exceed limit
        assert!(check_command_limit(&mut state, &limits).is_some());
    }

    #[test]
    fn test_check_recursion_limit() {
        let mut state = InterpreterState::default();
        let limits = ExecutionLimits {
            max_recursion_depth: 5,
            ..Default::default()
        };

        state.call_depth = 5;
        assert!(check_recursion_limit(&state, &limits).is_none());

        state.call_depth = 6;
        assert!(check_recursion_limit(&state, &limits).is_some());
    }
}
