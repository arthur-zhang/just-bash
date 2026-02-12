use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TrueCommand;

#[async_trait]
impl Command for TrueCommand {
    fn name(&self) -> &'static str {
        "true"
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::success(String::new())
    }
}

pub struct FalseCommand;

#[async_trait]
impl Command for FalseCommand {
    fn name(&self) -> &'static str {
        "false"
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::with_exit_code(String::new(), String::new(), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    fn create_ctx() -> CommandContext {
        CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_true_returns_zero() {
        let cmd = TrueCommand;
        let result = cmd.execute(create_ctx()).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_false_returns_one() {
        let cmd = FalseCommand;
        let result = cmd.execute(create_ctx()).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
}
