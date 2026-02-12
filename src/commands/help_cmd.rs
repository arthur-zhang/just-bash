use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct HelpCommand;

const CATEGORIES: &[(&str, &[&str])] = &[
    ("File operations", &["ls", "cat", "head", "tail", "wc", "touch", "mkdir", "rm", "cp", "mv", "ln", "chmod", "stat", "readlink"]),
    ("Text processing", &["grep", "sed", "awk", "sort", "uniq", "cut", "tr", "tee", "diff"]),
    ("Search", &["find"]),
    ("Navigation & paths", &["pwd", "basename", "dirname", "tree", "du"]),
    ("Environment & shell", &["echo", "printf", "env", "printenv", "export", "alias", "unalias", "history", "clear", "true", "false", "bash", "sh"]),
    ("Data processing", &["xargs", "jq", "base64", "date"]),
    ("Network", &["curl"]),
];

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &'static str { "help" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help" || a == "-h") {
            return CommandResult::success(
                "help - display available commands\n\nUsage: help [command]\n\nOptions:\n  -h, --help    Show this help message\n".to_string()
            );
        }

        if !ctx.args.is_empty() {
            if let Some(exec_fn) = &ctx.exec_fn {
                let cmd_name = &ctx.args[0];
                return exec_fn(
                    format!("{} --help", cmd_name),
                    String::new(),
                    ctx.cwd.clone(),
                    ctx.env.clone(),
                    ctx.fs.clone(),
                ).await;
            }
        }

        let mut stdout = String::from("Available commands:\n\n");

        for (category, cmds) in CATEGORIES {
            stdout.push_str(&format!("  {}:\n", category));
            stdout.push_str(&format!("    {}\n\n", cmds.join(", ")));
        }

        stdout.push_str("Use '<command> --help' for details on a specific command.\n");

        CommandResult::success(stdout)
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
    async fn test_help_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = HelpCommand.execute(ctx).await;
        assert!(result.stdout.contains("help"));
        assert!(result.stdout.contains("Usage"));
    }

    #[tokio::test]
    async fn test_list_commands() {
        let ctx = create_ctx(vec![]);
        let result = HelpCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Available commands"));
        assert!(result.stdout.contains("File operations"));
        assert!(result.stdout.contains("ls"));
        assert!(result.stdout.contains("grep"));
    }

    #[tokio::test]
    async fn test_categories() {
        let ctx = create_ctx(vec![]);
        let result = HelpCommand.execute(ctx).await;
        assert!(result.stdout.contains("Text processing"));
        assert!(result.stdout.contains("Network"));
        assert!(result.stdout.contains("Data processing"));
    }
}
