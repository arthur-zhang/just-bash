use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct BashCommand;

#[async_trait]
impl Command for BashCommand {
    fn name(&self) -> &'static str { "bash" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "bash - execute shell commands or scripts\n\nUsage: bash [OPTIONS] [SCRIPT_FILE] [ARGUMENTS...]\n\nOptions:\n  -c COMMAND  execute COMMAND string\n".to_string()
            );
        }

        let exec_fn = match &ctx.exec_fn {
            Some(f) => f.clone(),
            None => return CommandResult::error("bash: internal error: exec function not available\n".to_string()),
        };

        if ctx.args.len() >= 2 && ctx.args[0] == "-c" {
            let command = &ctx.args[1];
            let script_name = ctx.args.get(2).cloned().unwrap_or_else(|| "bash".to_string());
            let script_args: Vec<_> = ctx.args.iter().skip(3).cloned().collect();
            return execute_script(command, &script_name, &script_args, &ctx, exec_fn).await;
        }

        if ctx.args.is_empty() {
            if !ctx.stdin.trim().is_empty() {
                return execute_script(&ctx.stdin, "bash", &[], &ctx, exec_fn).await;
            }
            return CommandResult::success(String::new());
        }

        let script_path = &ctx.args[0];
        let script_args: Vec<_> = ctx.args.iter().skip(1).cloned().collect();
        let full_path = ctx.fs.resolve_path(&ctx.cwd, script_path);

        match ctx.fs.read_file(&full_path).await {
            Ok(content) => execute_script(&content, script_path, &script_args, &ctx, exec_fn).await,
            Err(_) => CommandResult::with_exit_code(
                String::new(),
                format!("bash: {}: No such file or directory\n", script_path),
                127,
            ),
        }
    }
}

async fn execute_script(
    script: &str,
    script_name: &str,
    script_args: &[String],
    ctx: &CommandContext,
    exec_fn: crate::commands::types::ExecFn,
) -> CommandResult {
    let mut env = ctx.env.clone();
    env.insert("0".to_string(), script_name.to_string());
    env.insert("#".to_string(), script_args.len().to_string());
    env.insert("@".to_string(), script_args.join(" "));
    env.insert("*".to_string(), script_args.join(" "));
    for (i, arg) in script_args.iter().enumerate() {
        env.insert((i + 1).to_string(), arg.clone());
    }

    let mut script_to_run = script.to_string();
    if script_to_run.starts_with("#!") {
        if let Some(idx) = script_to_run.find('\n') {
            script_to_run = script_to_run[idx + 1..].to_string();
        }
    }

    exec_fn(script_to_run, ctx.stdin.clone(), ctx.cwd.clone(), env, ctx.fs.clone()).await
}

pub struct ShCommand;

#[async_trait]
impl Command for ShCommand {
    fn name(&self) -> &'static str { "sh" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "sh - execute shell commands or scripts (POSIX shell)\n\nUsage: sh [OPTIONS] [SCRIPT_FILE] [ARGUMENTS...]\n\nOptions:\n  -c COMMAND  execute COMMAND string\n".to_string()
            );
        }

        let exec_fn = match &ctx.exec_fn {
            Some(f) => f.clone(),
            None => return CommandResult::error("sh: internal error: exec function not available\n".to_string()),
        };

        if ctx.args.len() >= 2 && ctx.args[0] == "-c" {
            let command = &ctx.args[1];
            let script_name = ctx.args.get(2).cloned().unwrap_or_else(|| "sh".to_string());
            let script_args: Vec<_> = ctx.args.iter().skip(3).cloned().collect();
            return execute_script(command, &script_name, &script_args, &ctx, exec_fn).await;
        }

        if ctx.args.is_empty() {
            if !ctx.stdin.trim().is_empty() {
                return execute_script(&ctx.stdin, "sh", &[], &ctx, exec_fn).await;
            }
            return CommandResult::success(String::new());
        }

        let script_path = &ctx.args[0];
        let script_args: Vec<_> = ctx.args.iter().skip(1).cloned().collect();
        let full_path = ctx.fs.resolve_path(&ctx.cwd, script_path);

        match ctx.fs.read_file(&full_path).await {
            Ok(content) => execute_script(&content, script_path, &script_args, &ctx, exec_fn).await,
            Err(_) => CommandResult::with_exit_code(
                String::new(),
                format!("sh: {}: No such file or directory\n", script_path),
                127,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    fn create_ctx(args: Vec<&str>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_bash_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = BashCommand.execute(ctx).await;
        assert!(result.stdout.contains("bash"));
        assert!(result.stdout.contains("-c"));
    }

    #[tokio::test]
    async fn test_sh_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = ShCommand.execute(ctx).await;
        assert!(result.stdout.contains("sh"));
        assert!(result.stdout.contains("-c"));
    }

    #[tokio::test]
    async fn test_bash_no_exec_fn() {
        let ctx = create_ctx(vec!["-c", "echo hello"]);
        let result = BashCommand.execute(ctx).await;
        assert!(result.stderr.contains("internal error"));
    }

    #[tokio::test]
    async fn test_sh_no_exec_fn() {
        let ctx = create_ctx(vec!["-c", "echo hello"]);
        let result = ShCommand.execute(ctx).await;
        assert!(result.stderr.contains("internal error"));
    }

    #[tokio::test]
    async fn test_bash_empty_no_stdin() {
        let mut ctx = create_ctx(vec![]);
        ctx.exec_fn = Some(Arc::new(|_, _, _, _, _| {
            Box::pin(async { CommandResult::success("ok".to_string()) })
        }));
        let result = BashCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_bash_file_not_found() {
        let mut ctx = create_ctx(vec!["nonexistent.sh"]);
        ctx.exec_fn = Some(Arc::new(|_, _, _, _, _| {
            Box::pin(async { CommandResult::success("ok".to_string()) })
        }));
        let result = BashCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 127);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_sh_file_not_found() {
        let mut ctx = create_ctx(vec!["nonexistent.sh"]);
        ctx.exec_fn = Some(Arc::new(|_, _, _, _, _| {
            Box::pin(async { CommandResult::success("ok".to_string()) })
        }));
        let result = ShCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 127);
        assert!(result.stderr.contains("No such file or directory"));
    }
}
