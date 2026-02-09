// src/commands/basename/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct BasenameCommand;

#[async_trait]
impl Command for BasenameCommand {
    fn name(&self) -> &'static str {
        "basename"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        // 检查 --help
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: basename NAME [SUFFIX]\n       basename OPTION... NAME...\n\n\
                 Strip directory and suffix from filenames.\n\n\
                 Options:\n\
                   -a, --multiple   support multiple arguments\n\
                   -s, --suffix=SUFFIX  remove a trailing SUFFIX\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut multiple = false;
        let mut suffix = String::new();
        let mut names: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-a" || arg == "--multiple" {
                multiple = true;
            } else if arg == "-s" && i + 1 < args.len() {
                i += 1;
                suffix = args[i].clone();
                multiple = true;
            } else if let Some(s) = arg.strip_prefix("--suffix=") {
                suffix = s.to_string();
                multiple = true;
            } else if !arg.starts_with('-') {
                names.push(arg.clone());
            }
            i += 1;
        }

        if names.is_empty() {
            return CommandResult::error("basename: missing operand\n".to_string());
        }

        // 如果不是 multiple 模式，第二个参数是 suffix
        if !multiple && names.len() >= 2 {
            suffix = names.pop().unwrap();
        }

        let results: Vec<String> = names
            .iter()
            .map(|name| {
                // 移除尾部斜杠
                let clean_name = name.trim_end_matches('/');
                let mut base = clean_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(clean_name)
                    .to_string();

                // 移除后缀
                if !suffix.is_empty() && base.ends_with(&suffix) && base.len() > suffix.len() {
                    base = base[..base.len() - suffix.len()].to_string();
                }
                base
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
    async fn test_basename_simple() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/sort"])).await;
        assert_eq!(result.stdout, "sort\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_with_suffix() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["include/stdio.h", ".h"])).await;
        assert_eq!(result.stdout, "stdio\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_trailing_slash() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/"])).await;
        assert_eq!(result.stdout, "usr\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_missing_operand() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec![])).await;
        assert_eq!(result.stderr, "basename: missing operand\n");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_basename_multiple() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["-a", "/usr/bin/sort", "/usr/bin/ls"])).await;
        assert_eq!(result.stdout, "sort\nls\n");
        assert_eq!(result.exit_code, 0);
    }
}
