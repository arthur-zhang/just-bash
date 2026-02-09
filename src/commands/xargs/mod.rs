// src/commands/xargs/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct XargsCommand;

/// Quote an argument if it contains shell metacharacters.
fn quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if arg.contains(|c: char| c.is_whitespace() || "\"'$`\\!#&|;(){}".contains(c)) {
        format!(
            "\"{}\"",
            arg.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
                .replace('`', "\\`")
        )
    } else {
        arg.to_string()
    }
}

/// Parse a delimiter string, handling escape sequences.
fn parse_delimiter(delim: &str) -> String {
    delim
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r")
        .replace("\\0", "\0")
        .replace("\\\\", "\\")
}

/// Parse input into items based on the splitting mode.
fn parse_input(stdin: &str, null_separator: bool, delimiter: &Option<String>) -> Vec<String> {
    if null_separator {
        stdin
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    } else if let Some(ref delim) = delimiter {
        // Strip trailing newline before splitting (echo adds trailing newlines)
        let input = stdin.strip_suffix('\n').unwrap_or(stdin);
        input
            .split(delim.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    } else {
        // Default: split on whitespace
        stdin
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }
}

#[async_trait]
impl Command for XargsCommand {
    fn name(&self) -> &'static str {
        "xargs"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: xargs [OPTION]... [COMMAND [INITIAL-ARGS]]\n\n\
                 Build and execute command lines from standard input.\n\n\
                 Options:\n\
                   -I REPLACE   replace occurrences of REPLACE with input\n\
                   -d DELIM     use DELIM as input delimiter\n\
                   -n NUM       use at most NUM arguments per command line\n\
                   -P NUM       run at most NUM processes at a time\n\
                   -0, --null   items are separated by null, not whitespace\n\
                   -t, --verbose  print commands before executing\n\
                   -r, --no-run-if-empty  do not run command if input is empty\n\
                       --help   display this help and exit\n"
                    .to_string(),
            );
        }

        let mut replace_str: Option<String> = None;
        let mut delimiter: Option<String> = None;
        let mut max_args: Option<usize> = None;
        let mut _max_procs: Option<usize> = None;
        let mut null_separator = false;
        let mut verbose = false;
        let mut no_run_if_empty = false;
        let mut command_start: usize = 0;

        // Parse xargs options
        let args = &ctx.args;
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-I" && i + 1 < args.len() {
                i += 1;
                replace_str = Some(args[i].clone());
                command_start = i + 1;
            } else if arg == "-d" && i + 1 < args.len() {
                i += 1;
                delimiter = Some(parse_delimiter(&args[i]));
                command_start = i + 1;
            } else if arg == "-n" && i + 1 < args.len() {
                i += 1;
                match args[i].parse::<usize>() {
                    Ok(n) => max_args = Some(n),
                    Err(_) => {
                        return CommandResult::error(format!(
                            "xargs: invalid number for -n: '{}'\n",
                            args[i]
                        ));
                    }
                }
                command_start = i + 1;
            } else if arg == "-P" && i + 1 < args.len() {
                i += 1;
                match args[i].parse::<usize>() {
                    Ok(n) => _max_procs = Some(n),
                    Err(_) => {
                        return CommandResult::error(format!(
                            "xargs: invalid number for -P: '{}'\n",
                            args[i]
                        ));
                    }
                }
                command_start = i + 1;
            } else if arg == "-0" || arg == "--null" {
                null_separator = true;
                command_start = i + 1;
            } else if arg == "-t" || arg == "--verbose" {
                verbose = true;
                command_start = i + 1;
            } else if arg == "-r" || arg == "--no-run-if-empty" {
                no_run_if_empty = true;
                command_start = i + 1;
            } else if arg.starts_with("--") {
                return CommandResult::error(format!(
                    "xargs: unknown option '{}'\n",
                    arg
                ));
            } else if arg.starts_with('-') && arg.len() > 1 {
                // Check for combined short boolean options
                for c in arg[1..].chars() {
                    if !"0tr".contains(c) {
                        return CommandResult::error(format!(
                            "xargs: unknown option '-{}'\n",
                            c
                        ));
                    }
                }
                if arg.contains('0') {
                    null_separator = true;
                }
                if arg.contains('t') {
                    verbose = true;
                }
                if arg.contains('r') {
                    no_run_if_empty = true;
                }
                command_start = i + 1;
            } else if !arg.starts_with('-') {
                command_start = i;
                break;
            }
            i += 1;
        }

        // Get command and initial args
        let mut command: Vec<String> = args[command_start..].to_vec();
        if command.is_empty() {
            command.push("echo".to_string());
        }

        // Parse input
        let items = parse_input(&ctx.stdin, null_separator, &delimiter);

        if items.is_empty() {
            if no_run_if_empty {
                return CommandResult::success(String::new());
            }
            // With no -r flag, still run the command with no args
            return CommandResult::success(String::new());
        }

        // Execute commands
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code: i32 = 0;

        // Build list of command invocations
        let cmd_args_list: Vec<Vec<String>> = if replace_str.is_some() {
            let rs = replace_str.as_ref().unwrap();
            items
                .iter()
                .map(|item| {
                    command
                        .iter()
                        .map(|c| c.replace(rs.as_str(), item))
                        .collect()
                })
                .collect()
        } else if let Some(n) = max_args {
            items
                .chunks(n)
                .map(|batch| {
                    let mut cmd = command.clone();
                    cmd.extend(batch.iter().cloned());
                    cmd
                })
                .collect()
        } else {
            let mut cmd = command.clone();
            cmd.extend(items.iter().cloned());
            vec![cmd]
        };

        // Execute each command invocation
        for cmd_args in &cmd_args_list {
            let cmd_line = cmd_args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" ");

            if verbose {
                stderr.push_str(&format!("{}\n", cmd_line));
            }

            if let Some(ref exec_fn) = ctx.exec_fn {
                let result = exec_fn(
                    cmd_line,
                    String::new(),
                    ctx.cwd.clone(),
                    ctx.env.clone(),
                    ctx.fs.clone(),
                )
                .await;
                stdout.push_str(&result.stdout);
                stderr.push_str(&result.stderr);
                if result.exit_code != 0 {
                    exit_code = result.exit_code;
                }
            } else {
                // Fallback: output what would be run
                stdout.push_str(&format!("{}\n", cmd_line));
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::pin::Pin;
    use std::future::Future;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    fn make_ctx_with_exec(args: Vec<&str>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        let exec_fn: crate::commands::types::ExecFn = Arc::new(|cmd, _stdin, _cwd, _env, _fs| {
            Box::pin(async move {
                CommandResult::success(format!("EXEC: {}\n", cmd))
            }) as Pin<Box<dyn Future<Output = CommandResult> + Send>>
        });
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: Some(exec_fn),
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_xargs_basic_echo_default() {
        // Basic: items passed to echo (default command)
        let ctx = make_ctx(vec![], "hello world\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo hello world\n");
    }

    #[tokio::test]
    async fn test_xargs_replace_mode() {
        // Replace mode: -I {} echo {}
        let ctx = make_ctx(vec!["-I", "{}", "echo", "{}"], "foo\nbar\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo foo\necho bar\n");
    }

    #[tokio::test]
    async fn test_xargs_batch_mode() {
        // Batch mode: -n 2 groups items into pairs
        let ctx = make_ctx(vec!["-n", "2"], "a b c d e\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo a b\necho c d\necho e\n");
    }

    #[tokio::test]
    async fn test_xargs_null_separator() {
        // Null separator: -0
        let ctx = make_ctx(vec!["-0"], "foo\0bar\0baz\0");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo foo bar baz\n");
    }

    #[tokio::test]
    async fn test_xargs_custom_delimiter() {
        // Custom delimiter: -d ,
        let ctx = make_ctx(vec!["-d", ","], "a,b,c\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo a b c\n");
    }

    #[tokio::test]
    async fn test_xargs_delimiter_escape_newline() {
        // Delimiter escape sequences: -d '\n'
        let ctx = make_ctx(vec!["-d", "\\n"], "line1\nline2\nline3\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo line1 line2 line3\n");
    }

    #[tokio::test]
    async fn test_xargs_verbose_mode() {
        // Verbose mode: -t prints commands to stderr
        let ctx = make_ctx(vec!["-t"], "hello world\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo hello world\n");
        assert_eq!(result.stderr, "echo hello world\n");
    }

    #[tokio::test]
    async fn test_xargs_no_run_if_empty() {
        // No-run-if-empty: -r with empty input -> no output
        let ctx = make_ctx(vec!["-r"], "");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_xargs_multiple_items_default() {
        // Multiple items default mode (all on one command)
        let ctx = make_ctx(vec![], "one two three four\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo one two three four\n");
    }

    #[tokio::test]
    async fn test_xargs_empty_input_without_r() {
        // Empty input without -r (still returns empty - no args to pass)
        let ctx = make_ctx(vec![], "");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_xargs_argument_quoting() {
        // Argument quoting (items with spaces)
        let ctx = make_ctx(vec!["-d", ","], "hello world,foo bar\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "echo \"hello world\" \"foo bar\"\n");
    }

    #[tokio::test]
    async fn test_xargs_without_exec_fn() {
        // Without exec_fn: returns formatted command strings
        let ctx = make_ctx(vec!["grep", "-l", "pattern"], "file1 file2 file3\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "grep -l pattern file1 file2 file3\n");
    }

    #[tokio::test]
    async fn test_xargs_with_exec_fn() {
        // With exec_fn: executes and returns combined output
        let ctx = make_ctx_with_exec(vec![], "hello world\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "EXEC: echo hello world\n");
    }

    #[tokio::test]
    async fn test_xargs_replace_multiple_occurrences() {
        // Replace mode with multiple occurrences of replace string
        let ctx = make_ctx(vec!["-I", "{}", "cp", "{}", "{}.bak"], "file1\nfile2\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "cp file1 file1.bak\ncp file2 file2.bak\n");
    }

    #[tokio::test]
    async fn test_xargs_command_with_arguments() {
        // Command with arguments: xargs grep -l pattern
        let ctx = make_ctx(vec!["grep", "-l", "pattern"], "a.txt b.txt\n");
        let result = XargsCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "grep -l pattern a.txt b.txt\n");
    }
}
