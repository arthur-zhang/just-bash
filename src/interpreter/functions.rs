//! Function Handling
//!
//! Handles shell function definition and invocation:
//! - Function definition (adding to function table)
//! - Function calls (with positional parameters and local scopes)

use std::collections::HashMap;
use crate::FunctionDefNode;
use crate::interpreter::errors::{ExitError, InterpreterError, ExecutionLimitError, LimitType, ReturnError};
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::shell_constants::POSIX_SPECIAL_BUILTINS;

/// Execute a function definition (add to function table).
/// Returns Ok(ExecResult) on success, or Err with ExitError if the function
/// name conflicts with a POSIX special builtin in POSIX mode.
pub fn execute_function_def(
    state: &mut InterpreterState,
    node: &FunctionDefNode,
    current_source: Option<&str>,
) -> Result<ExecResult, ExitError> {
    // In POSIX mode, special built-ins cannot be redefined as functions
    // This is a fatal error that exits the script
    if state.options.posix && POSIX_SPECIAL_BUILTINS.contains(node.name.as_str()) {
        let stderr = format!(
            "bash: line {}: `{}': is a special builtin\n",
            state.current_line,
            node.name
        );
        return Err(ExitError::new(2, String::new(), stderr));
    }

    // Store the source file where this function is defined (for BASH_SOURCE)
    let mut func_with_source = node.clone();
    if func_with_source.source_file.is_none() {
        func_with_source.source_file = current_source.map(|s| s.to_string());
    }
    if func_with_source.source_file.is_none() {
        func_with_source.source_file = Some("main".to_string());
    }

    // Add to function table
    state.functions.insert(node.name.clone(), func_with_source);

    Ok(ExecResult::ok())
}

/// Check if a function is defined
pub fn is_function_defined(state: &InterpreterState, name: &str) -> bool {
    state.functions.contains_key(name)
}

/// Get a function definition by name
pub fn get_function<'a>(state: &'a InterpreterState, name: &str) -> Option<&'a FunctionDefNode> {
    state.functions.get(name)
}

/// Remove a function definition
pub fn unset_function(state: &mut InterpreterState, name: &str) -> bool {
    state.functions.remove(name).is_some()
}

/// Get all function names
pub fn get_function_names(state: &InterpreterState) -> Vec<String> {
    state.functions.keys().cloned().collect()
}

/// Prepare for function call by setting up local scope and positional parameters.
/// Returns a FunctionCallContext that should be used to cleanup after the call.
pub struct FunctionCallContext {
    /// Saved positional parameters
    pub saved_positional: HashMap<String, Option<String>>,
    /// The scope index for this call
    pub scope_index: usize,
}

/// Set up the environment for a function call.
/// This pushes a new local scope and sets up positional parameters.
pub fn setup_function_call(
    state: &mut InterpreterState,
    func: &FunctionDefNode,
    args: &[String],
    call_line: Option<u32>,
    max_call_depth: u32,
) -> Result<FunctionCallContext, InterpreterError> {
    // Increment call depth
    state.call_depth += 1;

    // Check recursion depth limit
    if state.call_depth > max_call_depth {
        state.call_depth -= 1;
        return Err(InterpreterError::ExecutionLimit(ExecutionLimitError::simple(
            format!(
                "{}: maximum recursion depth ({}) exceeded",
                func.name, max_call_depth
            ),
            LimitType::Recursion,
        )));
    }

    // Initialize stacks if not present
    if state.func_name_stack.is_none() {
        state.func_name_stack = Some(Vec::new());
    }
    if state.call_line_stack.is_none() {
        state.call_line_stack = Some(Vec::new());
    }
    if state.source_stack.is_none() {
        state.source_stack = Some(Vec::new());
    }

    // Push the function name and the line where it was called from
    state.func_name_stack.as_mut().unwrap().insert(0, func.name.clone());
    state.call_line_stack.as_mut().unwrap().insert(0, call_line.unwrap_or(state.current_line));
    state.source_stack.as_mut().unwrap().insert(0, func.source_file.clone().unwrap_or_else(|| "main".to_string()));

    // Push a new local scope
    state.local_scopes.push(HashMap::new());
    let scope_index = state.local_scopes.len() - 1;

    // Push a new set for tracking exports made in this scope
    if state.local_exported_vars.is_none() {
        state.local_exported_vars = Some(Vec::new());
    }
    state.local_exported_vars.as_mut().unwrap().push(std::collections::HashSet::new());

    // Save and set positional parameters
    let mut saved_positional = HashMap::new();
    for (i, arg) in args.iter().enumerate() {
        let key = (i + 1).to_string();
        saved_positional.insert(key.clone(), state.env.get(&key).cloned());
        state.env.insert(key, arg.clone());
    }
    saved_positional.insert("@".to_string(), state.env.get("@").cloned());
    saved_positional.insert("#".to_string(), state.env.get("#").cloned());
    state.env.insert("@".to_string(), args.join(" "));
    state.env.insert("#".to_string(), args.len().to_string());

    Ok(FunctionCallContext {
        saved_positional,
        scope_index,
    })
}

/// Clean up after a function call.
/// This pops the local scope and restores positional parameters.
pub fn cleanup_function_call(
    state: &mut InterpreterState,
    ctx: FunctionCallContext,
) {
    // Get the scope index before popping (for localVarStack cleanup)
    let scope_index = ctx.scope_index;

    // Pop local scope and restore variables
    if let Some(local_scope) = state.local_scopes.pop() {
        for (var_name, original_value) in local_scope {
            match original_value {
                Some(value) => { state.env.insert(var_name, value); }
                None => { state.env.remove(&var_name); }
            }
        }
    }

    // Clear any localVarStack entries for this scope
    clear_local_var_stack_for_scope(state, scope_index);

    // Clear fullyUnsetLocals entries for this scope only
    if let Some(ref mut fully_unset_locals) = state.fully_unset_locals {
        fully_unset_locals.retain(|_, entry_scope| *entry_scope != scope_index);
    }

    // Pop local export tracking and restore export state
    if let Some(ref mut local_exported_vars) = state.local_exported_vars {
        if let Some(local_exports) = local_exported_vars.pop() {
            if let Some(ref mut exported_vars) = state.exported_vars {
                for name in local_exports {
                    exported_vars.remove(&name);
                }
            }
        }
    }

    // Restore positional parameters
    for (key, value) in ctx.saved_positional {
        match value {
            Some(v) => { state.env.insert(key, v); }
            None => { state.env.remove(&key); }
        }
    }

    // Pop from call stack tracking
    if let Some(ref mut stack) = state.func_name_stack {
        if !stack.is_empty() {
            stack.remove(0);
        }
    }
    if let Some(ref mut stack) = state.call_line_stack {
        if !stack.is_empty() {
            stack.remove(0);
        }
    }
    if let Some(ref mut stack) = state.source_stack {
        if !stack.is_empty() {
            stack.remove(0);
        }
    }

    // Decrement call depth
    state.call_depth -= 1;
}

/// Clear local variable stack entries for a specific scope.
/// This is called during function cleanup to remove entries that belong to the exiting scope.
pub fn clear_local_var_stack_for_scope(state: &mut InterpreterState, scope_index: usize) {
    if let Some(ref mut local_var_stack) = state.local_var_stack {
        // Collect names to remove (to avoid borrowing issues)
        let mut names_to_remove = Vec::new();

        for (name, stack) in local_var_stack.iter_mut() {
            // Remove entries from the top of the stack that belong to this scope
            while !stack.is_empty() && stack.last().map(|e| e.scope_index) == Some(scope_index) {
                stack.pop();
            }
            // Mark empty entries for cleanup
            if stack.is_empty() {
                names_to_remove.push(name.clone());
            }
        }

        // Clean up empty entries
        for name in names_to_remove {
            local_var_stack.remove(&name);
        }
    }
}

/// Handle a return error from a function call.
/// Converts the ReturnError to a normal ExecResult.
pub fn handle_return_error(error: ReturnError) -> ExecResult {
    ExecResult {
        stdout: error.stdout,
        stderr: error.stderr,
        exit_code: error.exit_code,
        env: None,
    }
}

/// Execute a function call with full setup, execution, and cleanup.
/// This is the main entry point for function invocation.
///
/// The `execute_body` callback is used to execute the function body,
/// allowing the caller to provide the actual command execution logic.
pub fn call_function<F>(
    state: &mut InterpreterState,
    func: &FunctionDefNode,
    args: &[String],
    stdin: &str,
    call_line: Option<u32>,
    max_call_depth: u32,
    execute_body: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnOnce(&mut InterpreterState, &str) -> Result<ExecResult, InterpreterError>,
{
    let ctx = setup_function_call(state, func, args, call_line, max_call_depth)?;

    let result = match execute_body(state, stdin) {
        Ok(res) => res,
        Err(InterpreterError::Return(ret_err)) => {
            // Convert ReturnError to normal result
            ExecResult {
                stdout: ret_err.stdout,
                stderr: ret_err.stderr,
                exit_code: ret_err.exit_code,
                env: None,
            }
        }
        Err(e) => {
            cleanup_function_call(state, ctx);
            return Err(e);
        }
    };

    cleanup_function_call(state, ctx);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompoundCommandNode, GroupNode};

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    fn make_function(name: &str) -> FunctionDefNode {
        FunctionDefNode {
            name: name.to_string(),
            body: Box::new(CompoundCommandNode::Group(GroupNode {
                body: vec![],
                redirections: vec![],
            })),
            redirections: vec![],
            source_file: None,
        }
    }

    #[test]
    fn test_execute_function_def() {
        let mut state = make_state();
        let func = make_function("myfunc");

        let result = execute_function_def(&mut state, &func, None);
        assert!(result.is_ok());
        assert!(is_function_defined(&state, "myfunc"));
    }

    #[test]
    fn test_execute_function_def_posix_special_builtin() {
        let mut state = make_state();
        state.options.posix = true;
        let func = make_function("break"); // break is a POSIX special builtin

        let result = execute_function_def(&mut state, &func, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(err.stderr.contains("special builtin"));
    }

    #[test]
    fn test_get_function() {
        let mut state = make_state();
        let func = make_function("myfunc");
        execute_function_def(&mut state, &func, None).unwrap();

        let retrieved = get_function(&state, "myfunc");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "myfunc");

        assert!(get_function(&state, "nonexistent").is_none());
    }

    #[test]
    fn test_unset_function() {
        let mut state = make_state();
        let func = make_function("myfunc");
        execute_function_def(&mut state, &func, None).unwrap();

        assert!(is_function_defined(&state, "myfunc"));
        assert!(unset_function(&mut state, "myfunc"));
        assert!(!is_function_defined(&state, "myfunc"));
        assert!(!unset_function(&mut state, "myfunc")); // Already removed
    }

    #[test]
    fn test_get_function_names() {
        let mut state = make_state();
        execute_function_def(&mut state, &make_function("func1"), None).unwrap();
        execute_function_def(&mut state, &make_function("func2"), None).unwrap();

        let names = get_function_names(&state);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"func1".to_string()));
        assert!(names.contains(&"func2".to_string()));
    }

    #[test]
    fn test_setup_and_cleanup_function_call() {
        let mut state = make_state();
        let func = make_function("myfunc");
        let args = vec!["arg1".to_string(), "arg2".to_string()];

        // Setup
        let ctx = setup_function_call(&mut state, &func, &args, Some(10), 1000).unwrap();

        // Check positional parameters are set
        assert_eq!(state.env.get("1"), Some(&"arg1".to_string()));
        assert_eq!(state.env.get("2"), Some(&"arg2".to_string()));
        assert_eq!(state.env.get("#"), Some(&"2".to_string()));
        assert_eq!(state.call_depth, 1);

        // Cleanup
        cleanup_function_call(&mut state, ctx);

        // Check positional parameters are restored
        assert!(state.env.get("1").is_none());
        assert!(state.env.get("2").is_none());
        assert_eq!(state.call_depth, 0);
    }

    #[test]
    fn test_recursion_depth_limit() {
        let mut state = make_state();
        let func = make_function("recursive");

        // Simulate deep recursion up to the limit
        for _ in 0..100 {
            let ctx = setup_function_call(&mut state, &func, &[], None, 1000).unwrap();
            // Don't cleanup - simulate nested calls
            std::mem::forget(ctx);
        }

        // Should fail at depth 101 when limit is 100
        let result = setup_function_call(&mut state, &func, &[], None, 100);
        assert!(result.is_err());

        // Verify it's an ExecutionLimit error
        if let Err(InterpreterError::ExecutionLimit(e)) = result {
            assert!(e.message.contains("maximum recursion depth"));
            assert!(e.message.contains("recursive"));
        } else {
            panic!("Expected ExecutionLimit error");
        }
    }

    #[test]
    fn test_recursion_depth_limit_exact_boundary() {
        let mut state = make_state();
        let func = make_function("test_func");

        // Should succeed at exactly the limit
        let ctx = setup_function_call(&mut state, &func, &[], None, 1).unwrap();
        assert_eq!(state.call_depth, 1);
        cleanup_function_call(&mut state, ctx);

        // Setup again to depth 1
        let _ctx = setup_function_call(&mut state, &func, &[], None, 1).unwrap();

        // Should fail at depth 2 when limit is 1
        let result = setup_function_call(&mut state, &func, &[], None, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_call_function_success() {
        let mut state = make_state();
        let func = make_function("myfunc");
        let args = vec!["arg1".to_string()];

        let result = call_function(
            &mut state,
            &func,
            &args,
            "",
            None,
            1000,
            |_state, _stdin| {
                Ok(ExecResult {
                    stdout: "hello\n".to_string(),
                    stderr: String::new(),
                    exit_code: 0,
                    env: None,
                })
            },
        );

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.stdout, "hello\n");
        assert_eq!(res.exit_code, 0);
        // State should be cleaned up
        assert_eq!(state.call_depth, 0);
    }

    #[test]
    fn test_call_function_handles_return() {
        use crate::interpreter::errors::ReturnError;

        let mut state = make_state();
        let func = make_function("myfunc");

        let result = call_function(
            &mut state,
            &func,
            &[],
            "",
            None,
            1000,
            |_state, _stdin| {
                Err(InterpreterError::Return(ReturnError::new(
                    42,
                    "output\n".to_string(),
                    String::new(),
                )))
            },
        );

        // Return should be converted to normal result
        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.exit_code, 42);
        assert_eq!(res.stdout, "output\n");
        // State should be cleaned up
        assert_eq!(state.call_depth, 0);
    }

    #[test]
    fn test_call_function_propagates_other_errors() {
        use crate::interpreter::errors::ExitError;

        let mut state = make_state();
        let func = make_function("myfunc");

        let result = call_function(
            &mut state,
            &func,
            &[],
            "",
            None,
            1000,
            |_state, _stdin| {
                Err(InterpreterError::Exit(ExitError::new(
                    1,
                    String::new(),
                    "error\n".to_string(),
                )))
            },
        );

        // Other errors should be propagated
        assert!(result.is_err());
        assert!(matches!(result, Err(InterpreterError::Exit(_))));
        // State should still be cleaned up
        assert_eq!(state.call_depth, 0);
    }

    #[test]
    fn test_call_function_recursion_limit() {
        let mut state = make_state();
        let func = make_function("recursive");

        // Set call_depth to simulate being at the limit
        state.call_depth = 100;

        let result = call_function(
            &mut state,
            &func,
            &[],
            "",
            None,
            100, // limit is 100, we're already at 100
            |_state, _stdin| {
                Ok(ExecResult::ok())
            },
        );

        // Should fail due to recursion limit
        assert!(result.is_err());
        assert!(matches!(result, Err(InterpreterError::ExecutionLimit(_))));
    }
}
