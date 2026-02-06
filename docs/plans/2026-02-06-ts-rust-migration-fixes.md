# TypeScript to Rust Migration Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete the partial migrations in arithmetic.rs, functions.rs, pipeline_execution.rs, subshell_group.rs, and help_cmd.rs to achieve 100% feature parity with TypeScript.

**Architecture:** Each module follows a callback-based design where execution logic is provided by the runtime. Rust modules provide state management, helper functions, and type definitions. The runtime implements the actual command execution through callbacks/traits.

**Tech Stack:** Rust, std collections, inline unit tests

---

## Task 1: Fix arithmetic.rs CommandSubst Support (98% → 100%)

**Files:**
- Modify: `src/interpreter/arithmetic.rs:1-400`
- Reference: `/Users/arthur/PycharmProjects/just-bash/src/interpreter/arithmetic.ts`

### Step 1.1: Add ExecFn callback type

Add a callback type for command substitution execution:

```rust
// Add after line 10 in arithmetic.rs
/// Callback type for executing command substitutions in arithmetic expressions.
/// Returns (stdout, stderr, exit_code).
pub type ArithExecFn = Box<dyn Fn(&crate::CommandNode) -> (String, String, i32)>;
```

### Step 1.2: Update evaluate_arithmetic signature

Modify the function to accept an optional exec callback:

```rust
pub fn evaluate_arithmetic(
    ctx: &mut InterpreterContext,
    expr: &ArithExpr,
    exec_fn: Option<&ArithExecFn>,
) -> Result<i64, ArithmeticError>
```

### Step 1.3: Implement CommandSubst evaluation

Replace the placeholder in the CommandSubst match arm:

```rust
ArithExpr::CommandSubst(node) => {
    if let Some(exec) = exec_fn {
        let (stdout, stderr, _exit_code) = exec(node);
        // Append stderr to expansion_stderr if needed
        if !stderr.is_empty() {
            if let Some(ref mut exp_stderr) = ctx.state.expansion_stderr {
                exp_stderr.push_str(&stderr);
            } else {
                ctx.state.expansion_stderr = Some(stderr);
            }
        }
        let output = stdout.trim();
        Ok(output.parse::<i64>().unwrap_or(0))
    } else {
        Ok(0)
    }
}
```

### Step 1.4: Add :? error handling in expand_braced_content

Change the return type and implement error throwing:

```rust
// In expand_braced_content function
":?" | "?" => {
    let should_error = is_unset || (check_empty && is_empty);
    if should_error {
        let msg = if default_value.is_empty() {
            format!("{}: parameter null or not set", var_name)
        } else {
            default_value.to_string()
        };
        return Err(ArithmeticError::ParameterError(msg));
    }
    Ok(value.cloned().unwrap_or_default())
}
```

### Step 1.5: Add special variable support in get_variable

```rust
fn get_variable(ctx: &InterpreterContext, name: &str) -> String {
    match name {
        "?" => ctx.state.last_exit_code.to_string(),
        "$" => ctx.state.shell_pid.to_string(),
        "!" => ctx.state.last_background_pid.map(|p| p.to_string()).unwrap_or_default(),
        "#" => ctx.state.env.get("#").cloned().unwrap_or_else(|| "0".to_string()),
        "@" | "*" => ctx.state.env.get(name).cloned().unwrap_or_default(),
        _ => ctx.state.env.get(name).cloned().unwrap_or_default(),
    }
}
```

### Step 1.6: Add associative array empty key handling

```rust
// In ArithAssignment handling
let expanded_key = get_arith_variable(ctx, &var_node.name);
// When variable expands to empty, use backslash as the key (OSH behavior)
let env_key = if expanded_key.is_empty() {
    format!("{}_{}", node.variable, "\\")
} else {
    format!("{}_{}", node.variable, expanded_key)
};
```

### Step 1.7: Run tests

```bash
cd /Users/arthur/conductor/workspaces/just-bash-v1/muscat-v1
cargo test arithmetic --lib
```

### Step 1.8: Commit

```bash
git add src/interpreter/arithmetic.rs
git commit -m "feat(arithmetic): add CommandSubst, special vars, :? error handling"
```

---

## Task 2: Fix functions.rs Execution Logic (60% → 100%)

**Files:**
- Modify: `src/interpreter/functions.rs:1-350`
- Reference: `/Users/arthur/PycharmProjects/just-bash/src/interpreter/functions.ts`

### Step 2.1: Add recursion depth check

```rust
// Add to setup_function_call, after incrementing call_depth
pub fn setup_function_call(
    state: &mut InterpreterState,
    func: &FunctionDefNode,
    args: &[String],
    call_line: Option<u32>,
    max_call_depth: u32,
) -> Result<FunctionCallContext, InterpreterError> {
    state.call_depth += 1;

    if state.call_depth > max_call_depth {
        state.call_depth -= 1;
        return Err(InterpreterError::ExecutionLimit(format!(
            "{}: maximum recursion depth ({}) exceeded",
            func.name, max_call_depth
        )));
    }
    // ... rest of function
}
```

### Step 2.2: Add ExecuteCommandFn callback type

```rust
/// Callback type for executing commands within function body.
pub type ExecuteCommandFn = Box<dyn Fn(&crate::CompoundCommandNode, &str) -> ExecResult>;
```

### Step 2.3: Add call_function main function

```rust
/// Execute a function call with full setup, execution, and cleanup.
/// This is the main entry point for function invocation.
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
```

### Step 2.4: Add tests for recursion limit

```rust
#[test]
fn test_recursion_depth_limit() {
    let mut state = make_state();
    let func = make_function("recursive");

    // Simulate deep recursion
    for _ in 0..100 {
        let _ = setup_function_call(&mut state, &func, &[], None, 1000);
    }

    // Should fail at depth 101
    let result = setup_function_call(&mut state, &func, &[], None, 100);
    assert!(result.is_err());
}
```

### Step 2.5: Run tests

```bash
cargo test functions --lib
```

### Step 2.6: Commit

```bash
git add src/interpreter/functions.rs
git commit -m "feat(functions): add call_function with recursion limit"
```

---

## Task 3: Fix pipeline_execution.rs Main Loop (70% → 100%)

**Files:**
- Modify: `src/interpreter/pipeline_execution.rs:1-400`
- Reference: `/Users/arthur/PycharmProjects/just-bash/src/interpreter/pipeline-execution.ts`

### Step 3.1: Add ExecuteCommandFn type

```rust
/// Callback for executing a single pipeline command.
/// Returns (stdout, stderr, exit_code).
pub type PipelineExecuteFn<'a> = &'a mut dyn FnMut(
    &crate::CommandNode,
    &str, // stdin
) -> Result<ExecResult, InterpreterError>;
```

### Step 3.2: Add execute_pipeline main function

```rust
/// Execute a pipeline of commands.
pub fn execute_pipeline<F>(
    state: &mut PipelineState,
    commands: &[crate::CommandNode],
    pipe_stderr: &[bool],
    options: &PipelineOptions,
    mut execute_command: F,
) -> Result<PipelineResult, InterpreterError>
where
    F: FnMut(&crate::CommandNode, &str) -> Result<ExecResult, InterpreterError>,
{
    let start_time = std::time::Instant::now();
    let is_multi_command = commands.len() > 1;

    // Save lastArg for multi-command pipelines
    let saved_last_arg = if is_multi_command {
        Some(state.saved_last_arg.clone())
    } else {
        None
    };

    // Save environment for subshell context
    let saved_env = if options.runs_in_subshell {
        Some(state.saved_env.clone())
    } else {
        None
    };

    let mut results = Vec::new();
    let mut current_stdin = String::new();

    for (i, cmd) in commands.iter().enumerate() {
        // Clear lastArg at start of each pipeline command
        if is_multi_command {
            state.saved_last_arg = None;
        }

        let result = match execute_command(cmd, &current_stdin) {
            Ok(res) => res,
            Err(InterpreterError::BadSubstitution(msg)) => {
                ExecResult {
                    stdout: String::new(),
                    stderr: msg,
                    exit_code: 1,
                    env: None,
                }
            }
            Err(InterpreterError::Exit(e)) if is_multi_command => {
                ExecResult {
                    stdout: e.stdout,
                    stderr: e.stderr,
                    exit_code: e.exit_code,
                    env: None,
                }
            }
            Err(e) => return Err(e),
        };

        // Set up stdin for next command
        if i < commands.len() - 1 {
            current_stdin = if pipe_stderr.get(i).copied().unwrap_or(false) {
                format!("{}{}", result.stdout, result.stderr)
            } else {
                result.stdout.clone()
            };
        }

        results.push(result);

        // Restore env after each subshell command
        if let Some(ref env) = saved_env {
            state.saved_env = env.clone();
        }
    }

    // Calculate exit code based on pipefail
    let exit_code = if options.pipefail {
        results.iter().map(|r| r.exit_code).filter(|&c| c != 0).last().unwrap_or(0)
    } else {
        results.last().map(|r| r.exit_code).unwrap_or(0)
    };

    // Restore lastArg if not lastpipe
    if let Some(last_arg) = saved_last_arg {
        if !options.lastpipe {
            state.saved_last_arg = last_arg;
        }
    }

    let elapsed = start_time.elapsed();

    Ok(PipelineResult {
        exit_codes: results.iter().map(|r| r.exit_code).collect(),
        final_exit_code: exit_code,
        stdout: results.iter().map(|r| r.stdout.as_str()).collect::<Vec<_>>().join(""),
        stderr: results.iter().map(|r| r.stderr.as_str()).collect::<Vec<_>>().join(""),
        elapsed_time: Some(elapsed),
    })
}
```

### Step 3.3: Add PipelineOptions struct

```rust
/// Options for pipeline execution.
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    pub pipefail: bool,
    pub lastpipe: bool,
    pub runs_in_subshell: bool,
    pub time_pipeline: bool,
    pub time_posix_format: bool,
}
```

### Step 3.4: Add PipelineResult struct

```rust
/// Result of pipeline execution.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub exit_codes: Vec<i32>,
    pub final_exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_time: Option<std::time::Duration>,
}
```

### Step 3.5: Add pipefail tests

```rust
#[test]
fn test_pipefail_rightmost_failure() {
    // Test that pipefail returns rightmost non-zero exit code
    let exit_codes = vec![0, 1, 0, 2, 0];
    let result = calculate_pipefail_exit_code(&exit_codes, true);
    assert_eq!(result, 2);
}

#[test]
fn test_without_pipefail() {
    // Without pipefail, return last command's exit code
    let exit_codes = vec![1, 2, 0];
    let result = calculate_pipefail_exit_code(&exit_codes, false);
    assert_eq!(result, 0);
}
```

### Step 3.6: Run tests

```bash
cargo test pipeline --lib
```

### Step 3.7: Commit

```bash
git add src/interpreter/pipeline_execution.rs
git commit -m "feat(pipeline): add execute_pipeline with pipefail support"
```

---

## Task 4: Fix subshell_group.rs Execution Logic (55% → 100%)

**Files:**
- Modify: `src/interpreter/subshell_group.rs:1-400`
- Reference: `/Users/arthur/PycharmProjects/just-bash/src/interpreter/subshell-group.ts`

### Step 4.1: Add ExecuteStatementFn type

```rust
/// Callback for executing statements within subshell/group.
pub type ExecuteStatementFn<'a> = &'a mut dyn FnMut(
    &crate::StatementNode,
) -> Result<ExecResult, InterpreterError>;
```

### Step 4.2: Add execute_subshell function

```rust
/// Execute a subshell node (...).
/// Creates an isolated execution environment that doesn't affect the parent.
pub fn execute_subshell<F>(
    state: &mut InterpreterState,
    body: &[crate::StatementNode],
    stdin: Option<&str>,
    mut execute_statement: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnMut(&crate::StatementNode) -> Result<ExecResult, InterpreterError>,
{
    let saved = prepare_subshell(state, stdin);

    let mut result = CompoundResult::new();

    for stmt in body {
        match execute_statement(stmt) {
            Ok(res) => result.append(&res),
            Err(InterpreterError::ExecutionLimit(msg)) => {
                saved.restore(state);
                return Err(InterpreterError::ExecutionLimit(msg));
            }
            Err(InterpreterError::SubshellExit(e)) => {
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                saved.restore(state);
                return Ok(ExecResult {
                    stdout: result.stdout,
                    stderr: result.stderr,
                    exit_code: 0,
                    env: None,
                });
            }
            Err(InterpreterError::Break(e)) | Err(InterpreterError::Continue(e)) => {
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Exit(e)) => {
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Return(e)) => {
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(InterpreterError::Errexit(e)) => {
                result.stdout.push_str(&e.stdout);
                result.stderr.push_str(&e.stderr);
                result.exit_code = e.exit_code;
                saved.restore(state);
                return Ok(result.to_exec_result());
            }
            Err(e) => {
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
```

### Step 4.3: Add execute_group function

```rust
/// Execute a group node { ...; }.
/// Runs commands in the current execution environment.
pub fn execute_group<F>(
    state: &mut InterpreterState,
    body: &[crate::StatementNode],
    stdin: Option<&str>,
    mut execute_statement: F,
) -> Result<ExecResult, InterpreterError>
where
    F: FnMut(&crate::StatementNode) -> Result<ExecResult, InterpreterError>,
{
    let saved = prepare_group(state, stdin);

    let mut result = CompoundResult::new();

    for stmt in body {
        match execute_statement(stmt) {
            Ok(res) => result.append(&res),
            Err(InterpreterError::ExecutionLimit(msg)) => {
                saved.restore(state);
                return Err(InterpreterError::ExecutionLimit(msg));
            }
            Err(InterpreterError::Break(mut e)) |
            Err(InterpreterError::Continue(mut e)) |
            Err(InterpreterError::Errexit(mut e)) |
            Err(InterpreterError::Exit(mut e)) => {
                e.stdout = format!("{}{}", result.stdout, e.stdout);
                e.stderr = format!("{}{}", result.stderr, e.stderr);
                saved.restore(state);
                // Re-throw with prepended output
                return Err(match e {
                    _ if matches!(e, InterpreterError::Break(_)) => InterpreterError::Break(e),
                    _ => InterpreterError::Exit(e.into()),
                });
            }
            Err(e) => {
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
```

### Step 4.4: Add execute_user_script function

```rust
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
    let script_content = skip_shebang(content);

    let saved = prepare_script(state, script_path, args, stdin);

    let result = match execute_script(state) {
        Ok(res) => res,
        Err(InterpreterError::Exit(e)) => {
            saved.restore(state);
            return Err(InterpreterError::Exit(e));
        }
        Err(InterpreterError::ExecutionLimit(msg)) => {
            saved.restore(state);
            return Err(InterpreterError::ExecutionLimit(msg));
        }
        Err(e) => {
            saved.restore(state);
            return Err(e);
        }
    };

    saved.restore(state);
    Ok(result)
}
```

### Step 4.5: Add subshell isolation tests

```rust
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
```

### Step 4.6: Run tests

```bash
cargo test subshell --lib
```

### Step 4.7: Commit

```bash
git add src/interpreter/subshell_group.rs
git commit -m "feat(subshell): add execute_subshell, execute_group, execute_user_script"
```

---

## Task 5: Fix help_cmd.rs Missing Builtins (95% → 100%)

**Files:**
- Modify: `src/interpreter/builtins/help_cmd.rs`
- Reference: `/Users/arthur/PycharmProjects/just-bash/src/interpreter/builtins/help.ts`

### Step 5.1: Add missing builtin help entries (Part 1)

Add to `BUILTIN_HELP` array:

```rust
BuiltinHelp {
    name: "bg",
    synopsis: "bg [job_spec ...]",
    description: "Move jobs to the background.",
},
BuiltinHelp {
    name: "builtin",
    synopsis: "builtin [shell-builtin [arg ...]]",
    description: "Execute shell builtins.",
},
BuiltinHelp {
    name: "caller",
    synopsis: "caller [expr]",
    description: "Return the context of the current subroutine call.",
},
BuiltinHelp {
    name: "compgen",
    synopsis: "compgen [-abcdefgjksuv] [-o option] [-A action] [-G globpat] [-W wordlist] [-F function] [-C command] [-X filterpat] [-P prefix] [-S suffix] [word]",
    description: "Display possible completions depending on the options.",
},
BuiltinHelp {
    name: "complete",
    synopsis: "complete [-abcdefgjksuv] [-pr] [-DEI] [-o option] [-A action] [-G globpat] [-W wordlist] [-F function] [-C command] [-X filterpat] [-P prefix] [-S suffix] [name ...]",
    description: "Specify how arguments are to be completed by Readline.",
},
BuiltinHelp {
    name: "dirs",
    synopsis: "dirs [-clpv] [+N] [-N]",
    description: "Display directory stack.",
},
```

### Step 5.2: Add missing builtin help entries (Part 2)

```rust
BuiltinHelp {
    name: "disown",
    synopsis: "disown [-h] [-ar] [jobspec ... | pid ...]",
    description: "Remove jobs from current shell.",
},
BuiltinHelp {
    name: "enable",
    synopsis: "enable [-a] [-dnps] [-f filename] [name ...]",
    description: "Enable and disable shell builtins.",
},
BuiltinHelp {
    name: "exec",
    synopsis: "exec [-cl] [-a name] [command [arguments ...]] [redirection ...]",
    description: "Replace the shell with the given command.",
},
BuiltinHelp {
    name: "fc",
    synopsis: "fc [-e ename] [-lnr] [first] [last] or fc -s [pat=rep] [command]",
    description: "Display or execute commands from the history list.",
},
BuiltinHelp {
    name: "fg",
    synopsis: "fg [job_spec]",
    description: "Move job to the foreground.",
},
BuiltinHelp {
    name: "history",
    synopsis: "history [-c] [-d offset] [n] or history -anrw [filename] or history -ps arg [arg...]",
    description: "Display or manipulate the history list.",
},
```

### Step 5.3: Add missing builtin help entries (Part 3)

```rust
BuiltinHelp {
    name: "jobs",
    synopsis: "jobs [-lnprs] [jobspec ...] or jobs -x command [args]",
    description: "Display status of jobs.",
},
BuiltinHelp {
    name: "kill",
    synopsis: "kill [-s sigspec | -n signum | -sigspec] pid | jobspec ... or kill -l [sigspec]",
    description: "Send a signal to a job.",
},
BuiltinHelp {
    name: "logout",
    synopsis: "logout [n]",
    description: "Exit a login shell.",
},
BuiltinHelp {
    name: "popd",
    synopsis: "popd [-n] [+N | -N]",
    description: "Remove directories from stack.",
},
BuiltinHelp {
    name: "pushd",
    synopsis: "pushd [-n] [+N | -N | dir]",
    description: "Add directories to stack.",
},
BuiltinHelp {
    name: "readarray",
    synopsis: "readarray [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]",
    description: "Read lines from a file into an array variable.",
},
```

### Step 5.4: Add missing builtin help entries (Part 4)

```rust
BuiltinHelp {
    name: "suspend",
    synopsis: "suspend [-f]",
    description: "Suspend shell execution.",
},
BuiltinHelp {
    name: "times",
    synopsis: "times",
    description: "Display process times.",
},
BuiltinHelp {
    name: "trap",
    synopsis: "trap [-lp] [[arg] signal_spec ...]",
    description: "Trap signals and other events.",
},
BuiltinHelp {
    name: "typeset",
    synopsis: "typeset [-aAfFgilnrtux] [-p] name[=value] ...",
    description: "Set variable values and attributes.",
},
BuiltinHelp {
    name: "ulimit",
    synopsis: "ulimit [-SHabcdefiklmnpqrstuvxPT] [limit]",
    description: "Modify shell resource limits.",
},
BuiltinHelp {
    name: "umask",
    synopsis: "umask [-p] [-S] [mode]",
    description: "Display or set file mode mask.",
},
BuiltinHelp {
    name: "unalias",
    synopsis: "unalias [-a] name [name ...]",
    description: "Remove each NAME from the list of defined aliases.",
},
```

### Step 5.5: Add missing tests

```rust
#[test]
fn test_help_common_builtins() {
    let result = handle_help(&[]);
    assert!(result.stdout.contains("cd"));
    assert!(result.stdout.contains("export"));
    assert!(result.stdout.contains("echo"));
}

#[test]
fn test_help_cd() {
    let result = handle_help(&["cd".to_string()]);
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("cd:"));
    assert!(result.stdout.to_lowercase().contains("change"));
}

#[test]
fn test_help_double_dash() {
    let result = handle_help(&["--".to_string(), "help".to_string()]);
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("help"));
}
```

### Step 5.6: Run tests

```bash
cargo test help --lib
```

### Step 5.7: Commit

```bash
git add src/interpreter/builtins/help_cmd.rs
git commit -m "feat(help): add 24 missing builtin help entries"
```

---

## Task 6: Add Missing Tests for All Modules

**Files:**
- Modify: All above files

### Step 6.1: Add arithmetic integration tests

Create comprehensive tests for arithmetic operations based on TypeScript test file.

### Step 6.2: Add function call tests

Test recursion limits, return handling, and redirection.

### Step 6.3: Add pipeline tests

Test pipefail, lastpipe, and error handling.

### Step 6.4: Add subshell tests

Test variable isolation, error propagation, and script execution.

### Step 6.5: Run all tests

```bash
cargo test --lib
```

### Step 6.6: Final commit

```bash
git add -A
git commit -m "test: add comprehensive tests for migrated modules"
```

---

## Summary

| Module | Before | After | Key Changes |
|--------|--------|-------|-------------|
| arithmetic.rs | 98% | 100% | CommandSubst, special vars, :? errors |
| functions.rs | 60% | 100% | call_function, recursion limit |
| pipeline_execution.rs | 70% | 100% | execute_pipeline, pipefail |
| subshell_group.rs | 55% | 100% | execute_subshell/group/script |
| help_cmd.rs | 95% | 100% | 24 missing builtin entries |

**Total estimated tasks:** 6 major tasks, ~35 steps
