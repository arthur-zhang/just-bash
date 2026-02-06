//! Subshell, Group, and Script Execution
//!
//! Handles execution of subshells (...), groups { ...; }, and user scripts

use std::collections::HashMap;
use crate::interpreter::types::{ExecResult, InterpreterState, LocalVarStackEntry, ShellOptions};
use crate::interpreter::errors::{InterpreterError, ControlFlowError};

/// Saved state for subshell execution.
/// Used to restore the parent environment after subshell completes.
#[derive(Debug, Clone)]
pub struct SubshellSavedState {
    pub env: HashMap<String, String>,
    pub cwd: String,
    pub options: ShellOptions,
    pub loop_depth: u32,
    pub parent_has_loop_context: Option<bool>,
    pub last_arg: String,
    pub bash_pid: u32,
    pub group_stdin: Option<String>,
    pub current_source: Option<String>,
    // Local variable scoping state - subshell gets a copy, changes don't affect parent
    pub local_scopes: Vec<HashMap<String, Option<String>>>,
    pub local_var_stack: Option<HashMap<String, Vec<LocalVarStackEntry>>>,
    pub local_var_depth: Option<HashMap<String, u32>>,
    pub fully_unset_locals: Option<HashMap<String, usize>>,
}

impl SubshellSavedState {
    /// Save the current state for subshell execution.
    pub fn save(state: &InterpreterState) -> Self {
        Self {
            env: state.env.clone(),
            cwd: state.cwd.clone(),
            options: state.options.clone(),
            loop_depth: state.loop_depth,
            parent_has_loop_context: state.parent_has_loop_context,
            last_arg: state.last_arg.clone(),
            bash_pid: state.bash_pid,
            group_stdin: state.group_stdin.clone(),
            current_source: state.current_source.clone(),
            // Save local scoping state
            local_scopes: state.local_scopes.clone(),
            local_var_stack: state.local_var_stack.clone(),
            local_var_depth: state.local_var_depth.clone(),
            fully_unset_locals: state.fully_unset_locals.clone(),
        }
    }

    /// Restore the saved state.
    pub fn restore(self, state: &mut InterpreterState) {
        state.env = self.env;
        state.cwd = self.cwd;
        state.options = self.options;
        state.loop_depth = self.loop_depth;
        state.parent_has_loop_context = self.parent_has_loop_context;
        state.last_arg = self.last_arg;
        state.bash_pid = self.bash_pid;
        state.group_stdin = self.group_stdin;
        state.current_source = self.current_source;
        // Restore local scoping state
        state.local_scopes = self.local_scopes;
        state.local_var_stack = self.local_var_stack;
        state.local_var_depth = self.local_var_depth;
        state.fully_unset_locals = self.fully_unset_locals;
    }
}

/// Prepare state for subshell execution.
/// Returns the saved state that should be restored after execution.
pub fn prepare_subshell(state: &mut InterpreterState, stdin: Option<&str>) -> SubshellSavedState {
    let saved = SubshellSavedState::save(state);

    // Deep copy the local scoping structures for the subshell
    // Subshell gets a copy of these, but changes don't affect parent
    // Note: state.local_scopes is already cloned in save(), but we need to ensure
    // the subshell has its own independent copy (which clone() provides for Vec<HashMap>)

    // For local_var_stack, we need a deep copy where each entry is also cloned
    // The clone() on Option<HashMap<String, Vec<LocalVarStackEntry>>> already does this
    // since LocalVarStackEntry derives Clone

    // Reset loopDepth in subshell - break/continue should not affect parent loops
    state.parent_has_loop_context = Some(saved.loop_depth > 0);
    state.loop_depth = 0;

    // Subshells get a new BASHPID (unlike $$ which stays the same)
    state.bash_pid = state.next_virtual_pid;
    state.next_virtual_pid += 1;

    // Set stdin if provided
    if let Some(s) = stdin {
        if !s.is_empty() {
            state.group_stdin = Some(s.to_string());
        }
    }

    saved
}

/// Saved state for group execution.
/// Groups run in the current environment but may have their own stdin.
#[derive(Debug, Clone)]
pub struct GroupSavedState {
    pub group_stdin: Option<String>,
}

impl GroupSavedState {
    /// Save the current state for group execution.
    pub fn save(state: &InterpreterState) -> Self {
        Self {
            group_stdin: state.group_stdin.clone(),
        }
    }

    /// Restore the saved state.
    pub fn restore(self, state: &mut InterpreterState) {
        state.group_stdin = self.group_stdin;
    }
}

/// Prepare state for group execution.
/// Returns the saved state that should be restored after execution.
pub fn prepare_group(state: &mut InterpreterState, stdin: Option<&str>) -> GroupSavedState {
    let saved = GroupSavedState::save(state);

    // Set stdin if provided
    if let Some(s) = stdin {
        if !s.is_empty() {
            state.group_stdin = Some(s.to_string());
        }
    }

    saved
}

/// Saved state for user script execution.
/// Scripts run in a subshell-like environment with their own positional parameters.
#[derive(Debug, Clone)]
pub struct ScriptSavedState {
    pub env: HashMap<String, String>,
    pub cwd: String,
    pub options: ShellOptions,
    pub loop_depth: u32,
    pub parent_has_loop_context: Option<bool>,
    pub last_arg: String,
    pub bash_pid: u32,
    pub group_stdin: Option<String>,
    pub current_source: Option<String>,
}

impl ScriptSavedState {
    /// Save the current state for script execution.
    pub fn save(state: &InterpreterState) -> Self {
        Self {
            env: state.env.clone(),
            cwd: state.cwd.clone(),
            options: state.options.clone(),
            loop_depth: state.loop_depth,
            parent_has_loop_context: state.parent_has_loop_context,
            last_arg: state.last_arg.clone(),
            bash_pid: state.bash_pid,
            group_stdin: state.group_stdin.clone(),
            current_source: state.current_source.clone(),
        }
    }

    /// Restore the saved state.
    pub fn restore(self, state: &mut InterpreterState) {
        state.env = self.env;
        state.cwd = self.cwd;
        state.options = self.options;
        state.loop_depth = self.loop_depth;
        state.parent_has_loop_context = self.parent_has_loop_context;
        state.last_arg = self.last_arg;
        state.bash_pid = self.bash_pid;
        state.group_stdin = self.group_stdin;
        state.current_source = self.current_source;
    }
}

/// Prepare state for user script execution.
/// Returns the saved state that should be restored after execution.
pub fn prepare_script(
    state: &mut InterpreterState,
    script_path: &str,
    args: &[String],
    stdin: Option<&str>,
) -> ScriptSavedState {
    let saved = ScriptSavedState::save(state);

    // Set up subshell-like environment
    state.parent_has_loop_context = Some(saved.loop_depth > 0);
    state.loop_depth = 0;
    state.bash_pid = state.next_virtual_pid;
    state.next_virtual_pid += 1;

    if let Some(s) = stdin {
        if !s.is_empty() {
            state.group_stdin = Some(s.to_string());
        }
    }

    state.current_source = Some(script_path.to_string());

    // Set positional parameters ($1, $2, etc.) from args
    // $0 should be the script path
    state.env.insert("0".to_string(), script_path.to_string());
    state.env.insert("#".to_string(), args.len().to_string());
    state.env.insert("@".to_string(), args.join(" "));
    state.env.insert("*".to_string(), args.join(" "));

    for (i, arg) in args.iter().enumerate().take(9) {
        state.env.insert((i + 1).to_string(), arg.clone());
    }

    // Clear any remaining positional parameters
    for i in (args.len() + 1)..=9 {
        state.env.remove(&i.to_string());
    }

    saved
}

/// Check if a script content starts with a shebang and extract the interpreter.
pub fn parse_shebang(content: &str) -> Option<&str> {
    if content.starts_with("#!") {
        let first_line = content.lines().next()?;
        let interpreter = first_line.strip_prefix("#!")?;
        Some(interpreter.trim())
    } else {
        None
    }
}

/// Skip the shebang line from script content.
pub fn skip_shebang(content: &str) -> &str {
    if content.starts_with("#!") {
        if let Some(newline_pos) = content.find('\n') {
            return &content[newline_pos + 1..];
        }
    }
    content
}

/// Execution result accumulator for compound commands.
#[derive(Debug, Default)]
pub struct CompoundResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CompoundResult {
    /// Create a new compound result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append output from an execution result.
    pub fn append(&mut self, result: &ExecResult) {
        self.stdout.push_str(&result.stdout);
        self.stderr.push_str(&result.stderr);
        self.exit_code = result.exit_code;
    }

    /// Convert to an ExecResult.
    pub fn to_exec_result(&self) -> ExecResult {
        ExecResult {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
            env: None,
        }
    }
}

/// Execute a subshell node (...).
/// Creates an isolated execution environment that doesn't affect the parent.
pub fn execute_subshell<F>(
    state: &mut InterpreterState,
    body: &[crate::StatementNode],
    stdin: Option<&str>,
    mut execute_statement: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnMut(&mut InterpreterState, &crate::StatementNode) -> Result<ExecResult, InterpreterError>,
{
    let saved = prepare_subshell(state, stdin);

    let mut result = CompoundResult::new();

    for stmt in body {
        match execute_statement(state, stmt) {
            Ok(res) => result.append(&res),
            Err(InterpreterError::ExecutionLimit(e)) => {
                // ExecutionLimitError must always propagate - these are safety limits
                saved.restore(state);
                return Err(InterpreterError::ExecutionLimit(e));
            }
            Err(InterpreterError::SubshellExit(e)) => {
                // SubshellExitError means break/continue was called when parent had loop context
                // This exits the subshell cleanly with exit code 0
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Break(e)) => {
                // BreakError should NOT propagate out of subshell
                // They only affect loops within the subshell
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Continue(e)) => {
                // ContinueError should NOT propagate out of subshell
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Exit(e)) => {
                // ExitError in subshell should NOT propagate - just return the exit code
                // (subshells are like separate processes)
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Return(e)) => {
                // ReturnError in subshell (e.g., f() ( return 42; )) should also just exit
                // with the given code, since subshells are like separate processes
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Errexit(e)) => {
                // Errexit in subshell - return with the exit code
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(e) => {
                // Other errors - convert to result with error message
                result.stderr.push_str(&format!("{}\n", e));
                result.exit_code = 1;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
        }
    }

    saved.restore(state);
    Ok(result.to_exec_result())
}

/// Execute a group node { ...; }.
/// Runs commands in the current execution environment.
pub fn execute_group<F>(
    state: &mut InterpreterState,
    body: &[crate::StatementNode],
    stdin: Option<&str>,
    mut execute_statement: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnMut(&mut InterpreterState, &crate::StatementNode) -> Result<ExecResult, InterpreterError>,
{
    let saved = prepare_group(state, stdin);

    let mut result = CompoundResult::new();

    for stmt in body {
        match execute_statement(state, stmt) {
            Ok(res) => result.append(&res),
            Err(InterpreterError::ExecutionLimit(e)) => {
                // ExecutionLimitError must always propagate - these are safety limits
                saved.restore(state);
                return Err(InterpreterError::ExecutionLimit(e));
            }
            Err(InterpreterError::Break(mut e)) => {
                // Groups propagate errors with prepended output
                e.prepend_output(&result.stdout, &result.stderr);
                saved.restore(state);
                return Err(InterpreterError::Break(e));
            }
            Err(InterpreterError::Continue(mut e)) => {
                e.prepend_output(&result.stdout, &result.stderr);
                saved.restore(state);
                return Err(InterpreterError::Continue(e));
            }
            Err(InterpreterError::Return(mut e)) => {
                e.prepend_output(&result.stdout, &result.stderr);
                saved.restore(state);
                return Err(InterpreterError::Return(e));
            }
            Err(InterpreterError::Errexit(mut e)) => {
                e.prepend_output(&result.stdout, &result.stderr);
                saved.restore(state);
                return Err(InterpreterError::Errexit(e));
            }
            Err(InterpreterError::Exit(mut e)) => {
                e.prepend_output(&result.stdout, &result.stderr);
                saved.restore(state);
                return Err(InterpreterError::Exit(e));
            }
            Err(e) => {
                // Other errors - convert to result with error message
                result.stderr.push_str(&format!("{}\n", e));
                result.exit_code = 1;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
        }
    }

    saved.restore(state);
    Ok(result.to_exec_result())
}

/// Execute a user script file.
pub fn execute_user_script<F>(
    state: &mut InterpreterState,
    script_path: &str,
    content: &str,
    args: &[String],
    stdin: Option<&str>,
    execute_script: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnOnce(&mut InterpreterState) -> Result<ExecResult, InterpreterError>,
{
    // Skip shebang if present
    let _script_content = skip_shebang(content);

    let saved = prepare_script(state, script_path, args, stdin);

    let result = match execute_script(state) {
        Ok(res) => res,
        Err(InterpreterError::Exit(e)) => {
            // ExitError propagates up (but with output from this script)
            saved.restore(state);
            return Err(InterpreterError::Exit(e));
        }
        Err(InterpreterError::ExecutionLimit(e)) => {
            // ExecutionLimitError must always propagate
            saved.restore(state);
            return Err(InterpreterError::ExecutionLimit(e));
        }
        Err(e) => {
            saved.restore(state);
            return Err(e);
        }
    };

    saved.restore(state);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_subshell_saved_state() {
        let mut state = make_state();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.cwd = "/home/user".to_string();
        state.loop_depth = 2;

        let saved = SubshellSavedState::save(&state);

        // Modify state
        state.env.insert("FOO".to_string(), "changed".to_string());
        state.cwd = "/tmp".to_string();
        state.loop_depth = 0;

        // Restore
        saved.restore(&mut state);

        assert_eq!(state.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(state.cwd, "/home/user");
        assert_eq!(state.loop_depth, 2);
    }

    #[test]
    fn test_prepare_subshell() {
        let mut state = make_state();
        state.loop_depth = 2;
        state.next_virtual_pid = 100;

        let saved = prepare_subshell(&mut state, Some("input"));

        assert_eq!(state.loop_depth, 0);
        assert_eq!(state.parent_has_loop_context, Some(true));
        assert_eq!(state.bash_pid, 100);
        assert_eq!(state.next_virtual_pid, 101);
        assert_eq!(state.group_stdin, Some("input".to_string()));

        saved.restore(&mut state);
        assert_eq!(state.loop_depth, 2);
    }

    #[test]
    fn test_prepare_script() {
        let mut state = make_state();
        let args = vec!["arg1".to_string(), "arg2".to_string()];

        let _saved = prepare_script(&mut state, "/path/to/script.sh", &args, None);

        assert_eq!(state.env.get("0"), Some(&"/path/to/script.sh".to_string()));
        assert_eq!(state.env.get("#"), Some(&"2".to_string()));
        assert_eq!(state.env.get("1"), Some(&"arg1".to_string()));
        assert_eq!(state.env.get("2"), Some(&"arg2".to_string()));
        assert_eq!(state.current_source, Some("/path/to/script.sh".to_string()));
    }

    #[test]
    fn test_parse_shebang() {
        assert_eq!(parse_shebang("#!/bin/bash\necho hello"), Some("/bin/bash"));
        assert_eq!(parse_shebang("#!/usr/bin/env bash\necho hello"), Some("/usr/bin/env bash"));
        assert_eq!(parse_shebang("echo hello"), None);
    }

    #[test]
    fn test_skip_shebang() {
        assert_eq!(skip_shebang("#!/bin/bash\necho hello"), "echo hello");
        assert_eq!(skip_shebang("echo hello"), "echo hello");
    }

    #[test]
    fn test_compound_result() {
        let mut result = CompoundResult::new();

        let exec1 = ExecResult {
            stdout: "out1".to_string(),
            stderr: "err1".to_string(),
            exit_code: 0,
            env: None,
        };
        result.append(&exec1);

        let exec2 = ExecResult {
            stdout: "out2".to_string(),
            stderr: "err2".to_string(),
            exit_code: 1,
            env: None,
        };
        result.append(&exec2);

        assert_eq!(result.stdout, "out1out2");
        assert_eq!(result.stderr, "err1err2");
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_subshell_variable_isolation() {
        let mut state = make_state();
        state.env.insert("FOO".to_string(), "original".to_string());

        let saved = prepare_subshell(&mut state, None);

        // Modify in subshell
        state.env.insert("FOO".to_string(), "modified".to_string());
        state.env.insert("NEW".to_string(), "value".to_string());

        // Restore
        saved.restore(&mut state);

        // Parent should be unchanged
        assert_eq!(state.env.get("FOO"), Some(&"original".to_string()));
        assert!(state.env.get("NEW").is_none());
    }

    #[test]
    fn test_execute_subshell_basic() {
        let mut state = make_state();

        // Execute subshell with a simple callback that returns success
        let result = execute_subshell(&mut state, &[], None, |_state, _stmt| {
            Ok(ExecResult {
                stdout: "hello".to_string(),
                stderr: String::new(),
                exit_code: 0,
                env: None,
            })
        });

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.exit_code, 0);
    }

    #[test]
    fn test_execute_group_basic() {
        let mut state = make_state();

        // Execute group with a simple callback that returns success
        let result = execute_group(&mut state, &[], None, |_state, _stmt| {
            Ok(ExecResult {
                stdout: "hello".to_string(),
                stderr: String::new(),
                exit_code: 0,
                env: None,
            })
        });

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.exit_code, 0);
    }

    #[test]
    fn test_group_stdin_handling() {
        let mut state = make_state();
        state.group_stdin = Some("original_stdin".to_string());

        let saved = prepare_group(&mut state, Some("new_stdin"));

        assert_eq!(state.group_stdin, Some("new_stdin".to_string()));

        saved.restore(&mut state);

        assert_eq!(state.group_stdin, Some("original_stdin".to_string()));
    }
}
