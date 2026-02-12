use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct WhoamiCommand;

#[async_trait]
impl Command for WhoamiCommand {
    fn name(&self) -> &'static str {
        "whoami"
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        // In sandboxed environment, always return "user"
        CommandResult::success("user\n".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_whoami() {
        let cmd = WhoamiCommand;
        let ctx = CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "user\n");
    }
}
