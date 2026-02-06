// src/commands/head/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::utils::{parse_head_tail_args, process_head_tail_files, get_head, HeadTailParseResult};

pub struct HeadCommand;

#[async_trait]
impl Command for HeadCommand {
    fn name(&self) -> &'static str {
        "head"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: head [OPTION]... [FILE]...\n\n\
                 Print the first 10 lines of each FILE to standard output.\n\n\
                 Options:\n\
                   -c, --bytes=NUM    print the first NUM bytes\n\
                   -n, --lines=NUM    print the first NUM lines (default 10)\n\
                   -q, --quiet        never print headers giving file names\n\
                   -v, --verbose      always print headers giving file names\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let opts = match parse_head_tail_args(&ctx.args, "head") {
            HeadTailParseResult::Ok(o) => o,
            HeadTailParseResult::Err(e) => return e,
        };

        let lines = opts.lines;
        let bytes = opts.bytes;

        process_head_tail_files(&ctx, &opts, "head", |content| {
            get_head(content, lines, bytes)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx_with_files(args: Vec<&str>, files: Vec<(&str, &str)>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content.as_bytes()).await.unwrap();
        }
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    #[tokio::test]
    async fn test_head_default() {
        let content = (1..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        let expected = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_head_n5() {
        let content = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "5", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        let expected = (1..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_head_bytes() {
        let ctx = make_ctx_with_files(vec!["-c", "5", "/test.txt"], vec![("/test.txt", "hello world\n")]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello");
    }

    #[tokio::test]
    async fn test_head_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\n")],
        ).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("==> /a.txt <=="));
        assert!(result.stdout.contains("==> /b.txt <=="));
        assert!(result.stdout.contains("aaa"));
        assert!(result.stdout.contains("bbb"));
    }
}
