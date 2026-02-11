// src/commands/pwd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct PwdCommand;

#[async_trait]
impl Command for PwdCommand {
    fn name(&self) -> &'static str {
        "pwd"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        // Parse options
        let mut use_physical = false;

        for arg in args {
            match arg.as_str() {
                "-P" => use_physical = true,
                "-L" => use_physical = false,
                "--" => break,
                _ if arg.starts_with('-') => {
                    // Ignore unknown options (bash behavior)
                }
                _ => {}
            }
        }

        let mut pwd = ctx.cwd.clone();

        if use_physical {
            // -P: resolve all symlinks to get physical path
            if let Ok(real) = ctx.fs.realpath(&ctx.cwd).await {
                pwd = real;
            }
            // If realpath fails, fall back to current cwd (bash behavior)
        }

        CommandResult::success(format!("{}\n", pwd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, cwd: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: cwd.to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_pwd_default() {
        let ctx = make_ctx(vec![], "/home/user");
        let cmd = PwdCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/home/user\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_pwd_root() {
        let ctx = make_ctx(vec![], "/");
        let cmd = PwdCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_pwd_ignore_args() {
        let ctx = make_ctx(vec!["ignored", "args"], "/test");
        let cmd = PwdCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/test\n");
    }
}
