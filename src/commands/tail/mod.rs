// src/commands/tail/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::utils::{parse_head_tail_args, process_head_tail_files, get_tail, HeadTailParseResult};

pub struct TailCommand;

#[async_trait]
impl Command for TailCommand {
    fn name(&self) -> &'static str {
        "tail"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: tail [OPTION]... [FILE]...\n\n\
                 Print the last 10 lines of each FILE to standard output.\n\n\
                 Options:\n\
                   -c, --bytes=NUM    print the last NUM bytes\n\
                   -n, --lines=NUM    print the last NUM lines (default 10)\n\
                   -n +NUM            print starting from line NUM\n\
                   -q, --quiet        never print headers giving file names\n\
                   -v, --verbose      always print headers giving file names\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let opts = match parse_head_tail_args(&ctx.args, "tail") {
            HeadTailParseResult::Ok(o) => o,
            HeadTailParseResult::Err(e) => return e,
        };

        let lines = opts.lines;
        let bytes = opts.bytes;
        let from_line = opts.from_line;

        process_head_tail_files(&ctx, &opts, "tail", |content| {
            get_tail(content, lines, bytes, from_line)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use crate::fs::types::FileSystem;
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
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_tail_default() {
        let content = (1..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (6..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_n3() {
        let content = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "3", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (8..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_from_line() {
        let content = (1..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "+3", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (3..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_bytes() {
        let ctx = make_ctx_with_files(vec!["-c", "5", "/test.txt"], vec![("/test.txt", "hello world\n")]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "orld\n");
    }
}
