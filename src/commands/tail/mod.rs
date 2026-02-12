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

    #[tokio::test]
    async fn test_tail_n_attached() {
        let content = "a\nb\nc\nd\ne\n";
        let ctx = make_ctx_with_files(vec!["-n2", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "d\ne\n");
    }

    #[tokio::test]
    async fn test_tail_dash_num() {
        let content = "a\nb\nc\nd\ne\n";
        let ctx = make_ctx_with_files(vec!["-3", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "c\nd\ne\n");
    }

    #[tokio::test]
    async fn test_tail_fewer_lines_than_requested() {
        let content = "a\nb\n";
        let ctx = make_ctx_with_files(vec!["-n", "10", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_tail_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\n")],
        ).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("==> /a.txt <=="));
        assert!(result.stdout.contains("==> /b.txt <=="));
        assert!(result.stdout.contains("aaa"));
        assert!(result.stdout.contains("bbb"));
    }

    #[tokio::test]
    async fn test_tail_missing_file() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["/missing.txt".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_tail_from_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-n".to_string(), "2".to_string()],
            stdin: "a\nb\nc\nd\ne\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "d\ne\n");
    }

    #[tokio::test]
    async fn test_tail_empty_file() {
        let ctx = make_ctx_with_files(vec!["/empty.txt"], vec![("/empty.txt", "")]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_tail_n1_single_line() {
        let content = "only line\n";
        let ctx = make_ctx_with_files(vec!["-n", "1", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "only line\n");
    }

    #[tokio::test]
    async fn test_tail_show_last_line_only() {
        let content = "first\nsecond\nthird\n";
        let ctx = make_ctx_with_files(vec!["-n", "1", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "third\n");
    }

    #[tokio::test]
    async fn test_tail_20_lines_default_10() {
        let lines: Vec<String> = (1..=20).map(|i| format!("line{}", i)).collect();
        let content = lines.join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected: Vec<String> = (11..=20).map(|i| format!("line{}", i)).collect();
        let expected = expected.join("\n") + "\n";
        assert_eq!(result.stdout, expected);
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tail_from_line_plus1() {
        let content = "line1\nline2\nline3\n";
        let ctx = make_ctx_with_files(vec!["-n", "+1", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "line1\nline2\nline3\n");
    }

    #[tokio::test]
    async fn test_tail_from_line_plus2() {
        let content = "line1\nline2\nline3\n";
        let ctx = make_ctx_with_files(vec!["-n", "+2", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "line2\nline3\n");
    }

    #[tokio::test]
    async fn test_tail_from_line_beyond_file() {
        let content = "line1\nline2\n";
        let ctx = make_ctx_with_files(vec!["-n", "+10", "/test.txt"], vec![("/test.txt", content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tail_from_line_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-n".to_string(), "+3".to_string()],
            stdin: "a\nb\nc\nd\ne\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "c\nd\ne\n");
    }
}
