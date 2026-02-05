//! Builtin Command Dispatch
//!
//! Handles dispatch of built-in shell commands like export, unset, cd, etc.
//! Separated from interpreter.rs for modularity.

use std::collections::HashMap;
use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::{OK, failure, test_result};
use crate::interpreter::helpers::shell_constants::SHELL_BUILTINS;

/// Type for the function that runs a command recursively
pub type RunCommandFn<'a> = &'a dyn Fn(
    &str,           // command_name
    &[String],      // args
    &[bool],        // quoted_args
    &str,           // stdin
    bool,           // skip_functions
    bool,           // use_default_path
    i32,            // stdin_source_fd
) -> ExecResult;

/// Type for the function that builds exported environment
pub type BuildExportedEnvFn<'a> = &'a dyn Fn() -> HashMap<String, String>;

/// Type for the function that executes user scripts
pub type ExecuteUserScriptFn<'a> = &'a dyn Fn(&str, &[String], Option<&str>) -> ExecResult;

/// Dispatch context containing dependencies needed for builtin dispatch
pub struct BuiltinDispatchContext<'a> {
    pub state: &'a mut InterpreterState,
    pub run_command: RunCommandFn<'a>,
    pub build_exported_env: BuildExportedEnvFn<'a>,
    pub execute_user_script: ExecuteUserScriptFn<'a>,
}

/// Dispatch a command to the appropriate builtin handler or external command.
/// Returns None if the command should be handled by external command resolution.
pub fn dispatch_builtin(
    dispatch_ctx: &mut BuiltinDispatchContext,
    command_name: &str,
    args: &[String],
    _quoted_args: &[bool],
    stdin: &str,
    skip_functions: bool,
    _use_default_path: bool,
    _stdin_source_fd: i32,
) -> Option<ExecResult> {
    // Built-in commands (special builtins that cannot be overridden by functions)
    match command_name {
        "export" => {
            return Some(handle_export_stub(dispatch_ctx.state, args));
        }
        "exit" => {
            return Some(handle_exit_stub(dispatch_ctx.state, args));
        }
        "set" => {
            return Some(handle_set_stub(dispatch_ctx.state, args));
        }
        "break" => {
            return Some(handle_break_stub(dispatch_ctx.state, args));
        }
        "continue" => {
            return Some(handle_continue_stub(dispatch_ctx.state, args));
        }
        "return" => {
            return Some(handle_return_stub(dispatch_ctx.state, args));
        }
        "shift" => {
            return Some(handle_shift_stub(dispatch_ctx.state, args));
        }
        "shopt" => {
            return Some(handle_shopt_stub(dispatch_ctx.state, args));
        }
        "help" => {
            return Some(handle_help_stub(dispatch_ctx.state, args));
        }
        _ => {}
    }

    // User-defined functions override most builtins (except special ones above)
    if !skip_functions {
        if dispatch_ctx.state.functions.contains_key(command_name) {
            // Would call function here
            return Some(OK);
        }
    }

    // Simple builtins (can be overridden by functions)
    match command_name {
        ":" | "true" => {
            return Some(OK);
        }
        "false" => {
            return Some(test_result(false));
        }
        "command" => {
            return Some(handle_command_builtin(dispatch_ctx, args, stdin));
        }
        "builtin" => {
            return Some(handle_builtin_builtin(dispatch_ctx, args, stdin));
        }
        "exec" => {
            if args.is_empty() {
                return Some(OK);
            }
            let cmd = &args[0];
            let rest: Vec<String> = args[1..].to_vec();
            return Some((dispatch_ctx.run_command)(cmd, &rest, &[], stdin, false, false, -1));
        }
        "wait" => {
            return Some(OK);
        }
        "[" | "test" => {
            let mut test_args = args.to_vec();
            if command_name == "[" {
                if test_args.last().map(|s| s.as_str()) != Some("]") {
                    return Some(failure("[: missing `]'\n"));
                }
                test_args.pop();
            }
            return Some(handle_test_stub(dispatch_ctx.state, &test_args));
        }
        _ => {}
    }

    // Return None to indicate command should be handled by external resolution
    None
}

/// Handle the 'command' builtin
fn handle_command_builtin(
    dispatch_ctx: &mut BuiltinDispatchContext,
    args: &[String],
    stdin: &str,
) -> ExecResult {
    if args.is_empty() {
        return OK;
    }

    // Parse options
    let mut use_default_path = false;
    let mut verbose_describe = false;
    let mut show_path = false;
    let mut cmd_args = args.to_vec();

    while !cmd_args.is_empty() && cmd_args[0].starts_with('-') {
        let opt = &cmd_args[0];
        if opt == "--" {
            cmd_args.remove(0);
            break;
        }
        for ch in opt[1..].chars() {
            match ch {
                'p' => use_default_path = true,
                'V' => verbose_describe = true,
                'v' => show_path = true,
                _ => {}
            }
        }
        cmd_args.remove(0);
    }

    if cmd_args.is_empty() {
        return OK;
    }

    // Handle -v and -V: describe commands without executing
    if show_path || verbose_describe {
        return handle_command_v_stub(dispatch_ctx.state, &cmd_args, show_path, verbose_describe);
    }

    // Run command without checking functions
    let cmd = &cmd_args[0];
    let rest: Vec<String> = cmd_args[1..].to_vec();
    (dispatch_ctx.run_command)(cmd, &rest, &[], stdin, true, use_default_path, -1)
}

/// Handle the 'builtin' builtin
fn handle_builtin_builtin(
    dispatch_ctx: &mut BuiltinDispatchContext,
    args: &[String],
    stdin: &str,
) -> ExecResult {
    if args.is_empty() {
        return OK;
    }

    let mut cmd_args = args.to_vec();
    if cmd_args[0] == "--" {
        cmd_args.remove(0);
        if cmd_args.is_empty() {
            return OK;
        }
    }

    let cmd = &cmd_args[0];

    if !SHELL_BUILTINS.contains(cmd.as_str()) {
        return failure(format!("bash: builtin: {}: not a shell builtin\n", cmd));
    }

    let rest: Vec<String> = cmd_args[1..].to_vec();
    (dispatch_ctx.run_command)(cmd, &rest, &[], stdin, true, false, -1)
}

// ============================================================================
// Stub functions for builtins not yet migrated
// ============================================================================

fn handle_export_stub(state: &mut InterpreterState, args: &[String]) -> ExecResult {
    // Simplified export implementation
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            state.env.insert(name.to_string(), value.to_string());
            if state.exported_vars.is_none() {
                state.exported_vars = Some(std::collections::HashSet::new());
            }
            if let Some(ref mut exported) = state.exported_vars {
                exported.insert(name.to_string());
            }
        } else {
            // Just mark as exported
            if state.exported_vars.is_none() {
                state.exported_vars = Some(std::collections::HashSet::new());
            }
            if let Some(ref mut exported) = state.exported_vars {
                exported.insert(arg.clone());
            }
        }
    }
    OK
}

fn handle_exit_stub(_state: &InterpreterState, args: &[String]) -> ExecResult {
    let exit_code = args.first()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    ExecResult::new(String::new(), String::new(), exit_code)
}

fn handle_set_stub(state: &mut InterpreterState, args: &[String]) -> ExecResult {
    for arg in args {
        match arg.as_str() {
            "-e" => state.options.errexit = true,
            "+e" => state.options.errexit = false,
            "-x" => state.options.xtrace = true,
            "+x" => state.options.xtrace = false,
            "-u" => state.options.nounset = true,
            "+u" => state.options.nounset = false,
            "-o" => {}
            _ => {}
        }
    }
    OK
}

fn handle_break_stub(state: &InterpreterState, args: &[String]) -> ExecResult {
    let n = args.first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    if state.loop_depth == 0 {
        return failure("bash: break: only meaningful in a `for', `while', or `until' loop\n");
    }
    OK
}

fn handle_continue_stub(state: &InterpreterState, args: &[String]) -> ExecResult {
    let n = args.first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    if state.loop_depth == 0 {
        return failure("bash: continue: only meaningful in a `for', `while', or `until' loop\n");
    }
    OK
}

fn handle_return_stub(state: &InterpreterState, args: &[String]) -> ExecResult {
    let exit_code = args.first()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(state.last_exit_code);
    if state.call_depth == 0 && state.source_depth == 0 {
        return failure("bash: return: can only `return' from a function or sourced script\n");
    }
    ExecResult::new(String::new(), String::new(), exit_code)
}

fn handle_shift_stub(state: &mut InterpreterState, args: &[String]) -> ExecResult {
    let n = args.first()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    let argc: usize = state.env.get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if n > argc {
        return failure("bash: shift: shift count out of range\n");
    }

    // Shift positional parameters
    for i in 1..=(argc - n) {
        let src_key = format!("{}", i + n);
        let dst_key = format!("{}", i);
        if let Some(val) = state.env.get(&src_key).cloned() {
            state.env.insert(dst_key, val);
        }
    }

    // Remove old parameters
    for i in (argc - n + 1)..=argc {
        state.env.remove(&format!("{}", i));
    }

    state.env.insert("#".to_string(), (argc - n).to_string());
    OK
}

fn handle_shopt_stub(state: &mut InterpreterState, args: &[String]) -> ExecResult {
    let mut set_mode = false;
    let mut unset_mode = false;
    let mut options: Vec<&str> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-s" => set_mode = true,
            "-u" => unset_mode = true,
            _ => options.push(arg),
        }
    }

    for opt in options {
        match opt {
            "extglob" => {
                if set_mode { state.shopt_options.extglob = true; }
                if unset_mode { state.shopt_options.extglob = false; }
            }
            "nullglob" => {
                if set_mode { state.shopt_options.nullglob = true; }
                if unset_mode { state.shopt_options.nullglob = false; }
            }
            "dotglob" => {
                if set_mode { state.shopt_options.dotglob = true; }
                if unset_mode { state.shopt_options.dotglob = false; }
            }
            "globstar" => {
                if set_mode { state.shopt_options.globstar = true; }
                if unset_mode { state.shopt_options.globstar = false; }
            }
            _ => {}
        }
    }
    OK
}

fn handle_help_stub(_state: &InterpreterState, _args: &[String]) -> ExecResult {
    let help_text = "GNU bash, version 5.2\n\
        These shell commands are defined internally.\n\
        Type `help name' to find out more about the function `name'.\n";
    ExecResult::new(help_text.to_string(), String::new(), 0)
}

fn handle_test_stub(_state: &InterpreterState, args: &[String]) -> ExecResult {
    // Simplified test implementation
    if args.is_empty() {
        return test_result(false);
    }

    if args.len() == 1 {
        // [ string ] - true if string is non-empty
        return test_result(!args[0].is_empty());
    }

    if args.len() == 2 {
        match args[0].as_str() {
            "-n" => return test_result(!args[1].is_empty()),
            "-z" => return test_result(args[1].is_empty()),
            "!" => return test_result(args[1].is_empty()),
            _ => {}
        }
    }

    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];

        match op.as_str() {
            "=" | "==" => return test_result(left == right),
            "!=" => return test_result(left != right),
            "-eq" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l == r);
            }
            "-ne" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l != r);
            }
            "-lt" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l < r);
            }
            "-le" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l <= r);
            }
            "-gt" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l > r);
            }
            "-ge" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return test_result(l >= r);
            }
            _ => {}
        }
    }

    test_result(false)
}

fn handle_command_v_stub(
    state: &InterpreterState,
    names: &[String],
    show_path: bool,
    verbose_describe: bool,
) -> ExecResult {
    let mut stdout = String::new();
    let mut exit_code = 0;

    for name in names {
        if SHELL_BUILTINS.contains(name.as_str()) {
            if verbose_describe {
                stdout.push_str(&format!("{} is a shell builtin\n", name));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        } else if state.functions.contains_key(name) {
            if verbose_describe {
                stdout.push_str(&format!("{} is a function\n", name));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        } else {
            exit_code = 1;
        }
    }

    ExecResult::new(stdout, String::new(), exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_export_stub() {
        let mut state = InterpreterState::default();
        let result = handle_export_stub(&mut state, &["FOO=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_handle_test_stub_string() {
        let state = InterpreterState::default();
        let result = handle_test_stub(&state, &["hello".to_string()]);
        assert_eq!(result.exit_code, 0);

        let result = handle_test_stub(&state, &["".to_string()]);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_handle_test_stub_comparison() {
        let state = InterpreterState::default();
        let result = handle_test_stub(&state, &["a".to_string(), "=".to_string(), "a".to_string()]);
        assert_eq!(result.exit_code, 0);

        let result = handle_test_stub(&state, &["a".to_string(), "!=".to_string(), "b".to_string()]);
        assert_eq!(result.exit_code, 0);
    }
}
