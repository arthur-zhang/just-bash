// src/commands/env/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct EnvCommand;

#[async_trait]
impl Command for EnvCommand {
    fn name(&self) -> &'static str {
        "env"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: env [OPTION]... [NAME=VALUE]... [COMMAND [ARG]...]\n\n\
                 Run a program in a modified environment.\n\n\
                 Options:\n\
                   -i, --ignore-environment  start with an empty environment\n\
                   -u NAME, --unset=NAME     remove NAME from the environment\n\
                       --help                display this help and exit\n".to_string()
            );
        }

        let mut ignore_env = false;
        let mut unset_vars: Vec<String> = Vec::new();
        let mut set_vars: Vec<(String, String)> = Vec::new();
        let mut command_start: i32 = -1;

        // Parse arguments
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];

            if arg == "-i" || arg == "--ignore-environment" {
                ignore_env = true;
            } else if arg == "-u" && i + 1 < args.len() {
                i += 1;
                unset_vars.push(args[i].clone());
            } else if arg.starts_with("-u") && arg.len() > 2 {
                unset_vars.push(arg[2..].to_string());
            } else if arg.starts_with("--unset=") {
                unset_vars.push(arg[8..].to_string());
            } else if arg.starts_with("--") && arg != "--" {
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("env: unrecognized option '{}'\n", arg),
                    1,
                );
            } else if arg.starts_with('-') && arg != "-" {
                // Check for unknown single-char options
                for c in arg[1..].chars() {
                    if c != 'i' && c != 'u' {
                        return CommandResult::with_exit_code(
                            String::new(),
                            format!("env: invalid option -- '{}'\n", c),
                            1,
                        );
                    }
                }
                if arg.contains('i') {
                    ignore_env = true;
                }
            } else if arg.contains('=') && command_start == -1 {
                // NAME=VALUE assignment
                if let Some(eq_idx) = arg.find('=') {
                    let name = &arg[..eq_idx];
                    let value = &arg[eq_idx + 1..];
                    set_vars.push((name.to_string(), value.to_string()));
                }
            } else {
                // Start of command
                command_start = i as i32;
                break;
            }
            i += 1;
        }

        // Build the new environment
        let mut new_env = if ignore_env {
            std::collections::HashMap::new()
        } else {
            ctx.env.clone()
        };

        // Unset variables
        for name in &unset_vars {
            new_env.remove(name);
        }

        // Set new variables
        for (name, value) in &set_vars {
            new_env.insert(name.clone(), value.clone());
        }

        // If no command, just print environment
        if command_start == -1 {
            let mut lines: Vec<String> = Vec::new();
            for (key, value) in &new_env {
                lines.push(format!("{}={}", key, value));
            }
            let output = if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            };
            return CommandResult::success(output);
        }

        // Execute command with modified environment
        if ctx.exec_fn.is_none() {
            return CommandResult::with_exit_code(
                String::new(),
                "env: command execution not supported in this context\n".to_string(),
                1,
            );
        }

        let exec_fn = ctx.exec_fn.as_ref().unwrap();
        let cmd_args = &args[command_start as usize..];
        let cmd_name = &cmd_args[0];
        let cmd_rest: Vec<&str> = cmd_args[1..].iter().map(|s| s.as_str()).collect();

        // Quote arguments that contain spaces or special characters
        let quoted_args: Vec<String> = cmd_rest.iter().map(|arg| {
            if arg.contains(|c: char| c.is_whitespace() || "\"'\\$`!*?[]{}|&;<>()".contains(c)) {
                format!("'{}'", arg.replace('\'', "'\\''"))
            } else {
                arg.to_string()
            }
        }).collect();

        let mut parts = vec!["command".to_string(), cmd_name.clone()];
        parts.extend(quoted_args);
        let command = parts.join(" ");

        let env_prefix: String = set_vars
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(" ");

        let full_command = if env_prefix.is_empty() {
            command
        } else {
            format!("{} {}", env_prefix, command)
        };

        let result = exec_fn(
            full_command,
            String::new(),
            ctx.cwd.clone(),
            new_env,
            ctx.fs.clone(),
        ).await;

        result
    }
}

pub struct PrintenvCommand;

#[async_trait]
impl Command for PrintenvCommand {
    fn name(&self) -> &'static str {
        "printenv"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: printenv [OPTION]... [VARIABLE]...\n\n\
                 Print all or part of environment.\n\n\
                 Options:\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let vars: Vec<&str> = args.iter()
            .filter(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .collect();

        if vars.is_empty() {
            // Print all
            let mut lines: Vec<String> = Vec::new();
            for (key, value) in &ctx.env {
                lines.push(format!("{}={}", key, value));
            }
            let output = if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            };
            return CommandResult::success(output);
        }

        // Print specific variables
        let mut lines: Vec<String> = Vec::new();
        let mut exit_code = 0;
        for var_name in &vars {
            if let Some(value) = ctx.env.get(*var_name) {
                lines.push(value.clone());
            } else {
                exit_code = 1;
            }
        }

        let output = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        CommandResult::with_exit_code(output, String::new(), exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, env: HashMap<String, String>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env,
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_env_print_all() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "qux".to_string());
        let ctx = make_ctx(vec![], env);
        let cmd = EnvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("FOO=bar"));
        assert!(result.stdout.contains("BAZ=qux"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_env_help() {
        let ctx = make_ctx(vec!["--help"], HashMap::new());
        let cmd = EnvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("env"));
        assert!(result.stdout.contains("environment"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_printenv_all() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let ctx = make_ctx(vec![], env);
        let cmd = PrintenvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("FOO=bar"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_printenv_specific() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "qux".to_string());
        let ctx = make_ctx(vec!["FOO"], env);
        let cmd = PrintenvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "bar\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_printenv_multiple() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "qux".to_string());
        let ctx = make_ctx(vec!["FOO", "BAZ"], env);
        let cmd = PrintenvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "bar\nqux\n");
    }

    #[tokio::test]
    async fn test_printenv_missing() {
        let ctx = make_ctx(vec!["NONEXISTENT"], HashMap::new());
        let cmd = PrintenvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_printenv_help() {
        let ctx = make_ctx(vec!["--help"], HashMap::new());
        let cmd = PrintenvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("printenv"));
        assert_eq!(result.exit_code, 0);
    }
}
