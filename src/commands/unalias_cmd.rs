use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

const ALIAS_PREFIX: &str = "BASH_ALIAS_";

pub struct UnaliasCommand;

#[async_trait]
impl Command for UnaliasCommand {
    fn name(&self) -> &'static str { "unalias" }

    async fn execute(&self, mut ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "unalias - remove alias definitions\n\nUsage: unalias [-a] name [name ...]\n".to_string()
            );
        }

        if ctx.args.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "unalias: usage: unalias [-a] name [name ...]\n".to_string(),
                1,
            );
        }

        if ctx.args.first().map(|s| s.as_str()) == Some("-a") {
            let keys_to_remove: Vec<_> = ctx.env.keys()
                .filter(|k| k.starts_with(ALIAS_PREFIX))
                .cloned()
                .collect();
            for key in keys_to_remove {
                ctx.env.remove(&key);
            }
            return CommandResult::success(String::new());
        }

        let args: Vec<_> = if ctx.args.first().map(|s| s.as_str()) == Some("--") {
            ctx.args[1..].to_vec()
        } else {
            ctx.args.clone()
        };

        let mut stderr = String::new();
        let mut any_error = false;

        for name in args {
            let key = format!("{}{}", ALIAS_PREFIX, name);
            if ctx.env.contains_key(&key) {
                ctx.env.remove(&key);
            } else {
                stderr.push_str(&format!("unalias: {}: not found\n", name));
                any_error = true;
            }
        }

        CommandResult::with_exit_code(String::new(), stderr, if any_error { 1 } else { 0 })
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

    fn create_ctx_with_env(args: Vec<&str>, env: HashMap<String, String>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env,
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_unalias_help() {
        let cmd = UnaliasCommand;
        let result = cmd.execute(create_ctx(vec!["--help"])).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("unalias"));
    }

    #[tokio::test]
    async fn test_unalias_no_args() {
        let cmd = UnaliasCommand;
        let result = cmd.execute(create_ctx(vec![])).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("usage"));
    }

    #[tokio::test]
    async fn test_unalias_remove_all() {
        let cmd = UnaliasCommand;
        let mut env = HashMap::new();
        env.insert("BASH_ALIAS_ll".to_string(), "ls -la".to_string());
        env.insert("BASH_ALIAS_la".to_string(), "ls -a".to_string());
        let result = cmd.execute(create_ctx_with_env(vec!["-a"], env)).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_unalias_not_found() {
        let cmd = UnaliasCommand;
        let result = cmd.execute(create_ctx(vec!["nonexistent"])).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not found"));
    }
}
