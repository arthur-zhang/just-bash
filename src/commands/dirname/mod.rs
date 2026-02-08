// src/commands/dirname/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct DirnameCommand;

#[async_trait]
impl Command for DirnameCommand {
    fn name(&self) -> &'static str {
        "dirname"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: dirname [OPTION] NAME...\n\n\
                 Strip last component from file name.\n\n\
                 Options:\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let names: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();

        if names.is_empty() {
            return CommandResult::error("dirname: missing operand\n".to_string());
        }

        let results: Vec<String> = names
            .iter()
            .map(|name| {
                // 移除尾部斜杠
                let clean_name = name.trim_end_matches('/');
                match clean_name.rfind('/') {
                    None => ".".to_string(),
                    Some(0) => "/".to_string(),
                    Some(pos) => clean_name[..pos].to_string(),
                }
            })
            .collect();

        CommandResult::success(format!("{}\n", results.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>) -> CommandContext {
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
    async fn test_dirname_simple() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/sort"])).await;
        assert_eq!(result.stdout, "/usr/bin\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_no_slash() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["stdio.h"])).await;
        assert_eq!(result.stdout, ".\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_root() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr"])).await;
        assert_eq!(result.stdout, "/\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_trailing_slash() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/"])).await;
        assert_eq!(result.stdout, "/usr\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_missing_operand() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec![])).await;
        assert_eq!(result.stderr, "dirname: missing operand\n");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_dirname_multiple() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/a/b", "/c/d"])).await;
        assert_eq!(result.stdout, "/a\n/c\n");
        assert_eq!(result.exit_code, 0);
    }
}
