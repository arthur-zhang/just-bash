use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct HostnameCommand;

#[async_trait]
impl Command for HostnameCommand {
    fn name(&self) -> &'static str {
        "hostname"
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        // In sandboxed environment, always return "localhost"
        CommandResult::success("localhost\n".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_hostname() {
        let cmd = HostnameCommand;
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
        assert_eq!(result.stdout, "localhost\n");
    }
}
