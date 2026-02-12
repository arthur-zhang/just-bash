use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct ClearCommand;

#[async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "clear - clear the terminal screen\n\nUsage: clear [OPTIONS]\n\nOptions:\n    --help display this help and exit\n".to_string()
            );
        }

        // ANSI escape sequence to clear screen and move cursor to top-left
        CommandResult::success("\x1B[2J\x1B[H".to_string())
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
    async fn test_clear_outputs_ansi_sequence() {
        let cmd = ClearCommand;
        let result = cmd.execute(create_ctx(vec![])).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "\x1B[2J\x1B[H");
    }

    #[tokio::test]
    async fn test_clear_help() {
        let cmd = ClearCommand;
        let result = cmd.execute(create_ctx(vec!["--help"])).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("clear"));
    }
}
