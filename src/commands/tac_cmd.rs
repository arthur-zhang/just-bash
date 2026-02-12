use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TacCommand;

#[async_trait]
impl Command for TacCommand {
    fn name(&self) -> &'static str {
        "tac"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let content = if !ctx.args.is_empty() && ctx.args[0] != "-" {
            let file_path = if ctx.args[0].starts_with('/') {
                ctx.args[0].clone()
            } else {
                format!("{}/{}", ctx.cwd, ctx.args[0])
            };

            match ctx.fs.read_file(&file_path).await {
                Ok(content) => content,
                Err(_) => {
                    return CommandResult::error(format!(
                        "tac: {}: No such file or directory\n",
                        ctx.args[0]
                    ));
                }
            }
        } else {
            ctx.stdin.clone()
        };

        let mut lines: Vec<&str> = content.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        lines.reverse();

        let output = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        CommandResult::success(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_tac_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: "line1\nline2\nline3\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TacCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "line3\nline2\nline1\n");
    }

    #[tokio::test]
    async fn test_tac_empty() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TacCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_tac_file_not_found() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["nonexistent.txt".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TacCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file"));
    }
}
