// src/commands/uniq/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct UniqCommand;

#[async_trait]
impl Command for UniqCommand {
    fn name(&self) -> &'static str {
        "uniq"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: uniq [OPTION]... [INPUT [OUTPUT]]\n\n\
                 Filter adjacent matching lines from INPUT (or stdin).\n\n\
                 Options:\n\
                   -c, --count          prefix lines by the number of occurrences\n\
                   -d, --repeated       only print duplicate lines, one for each group\n\
                   -u, --unique         only print unique lines\n\
                   -i, --ignore-case    ignore differences in case when comparing\n\
                       --help           display this help and exit\n"
                    .to_string(),
            );
        }

        let mut count = false;
        let mut repeated = false;
        let mut unique = false;
        let mut ignore_case = false;
        let mut files: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-c" | "--count" => count = true,
                "-d" | "--repeated" => repeated = true,
                "-u" | "--unique" => unique = true,
                "-i" | "--ignore-case" => ignore_case = true,
                _ if !arg.starts_with('-') => files.push(arg.clone()),
                _ => {}
            }
        }

        let input = if files.is_empty() || files[0] == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[0]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => {
                    return CommandResult::error(format!(
                        "uniq: {}: No such file or directory\n",
                        files[0]
                    ));
                }
            }
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return CommandResult::success(String::new());
        }

        // Group adjacent identical lines
        let mut groups: Vec<(usize, &str)> = Vec::new();
        for line in &lines {
            if let Some(last) = groups.last_mut() {
                let matches = if ignore_case {
                    last.1.eq_ignore_ascii_case(line)
                } else {
                    last.1 == *line
                };
                if matches {
                    last.0 += 1;
                    continue;
                }
            }
            groups.push((1, line));
        }

        let mut output = String::new();
        for (cnt, line) in &groups {
            let include = if repeated {
                *cnt > 1
            } else if unique {
                *cnt == 1
            } else {
                true
            };

            if include {
                if count {
                    output.push_str(&format!("{:>7} {}\n", cnt, line));
                } else {
                    output.push_str(line);
                    output.push('\n');
                }
            }
        }

        CommandResult::success(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use crate::fs::types::FileSystem;
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn make_ctx(
        args: Vec<&str>,
        stdin: &str,
        files: Vec<(&str, &str)>,
    ) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content.as_bytes()).await.unwrap();
        }
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_remove_adjacent_duplicates() {
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "aaa\nbbb\nccc\n");
    }

    #[tokio::test]
    async fn test_count_with_c() {
        let ctx = make_ctx(
            vec!["-c", "/test.txt"],
            "",
            vec![("/test.txt", "aaa\naaa\naaa\nbbb\nccc\nccc\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("3 aaa"));
        assert!(result.stdout.contains("1 bbb"));
        assert!(result.stdout.contains("2 ccc"));
    }

    #[tokio::test]
    async fn test_duplicates_only_with_d() {
        let ctx = make_ctx(
            vec!["-d", "/test.txt"],
            "",
            vec![("/test.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "aaa\nccc\n");
    }

    #[tokio::test]
    async fn test_unique_only_with_u() {
        let ctx = make_ctx(
            vec!["-u", "/test.txt"],
            "",
            vec![("/test.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "bbb\n");
    }

    #[tokio::test]
    async fn test_only_adjacent_duplicates() {
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", "aaa\nbbb\naaa\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "aaa\nbbb\naaa\n");
    }

    #[tokio::test]
    async fn test_stdin() {
        let ctx = make_ctx(vec![], "hello\nhello\nworld\n", vec![]).await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello\nworld\n");
    }

    #[tokio::test]
    async fn test_case_insensitive_with_i() {
        let ctx = make_ctx(
            vec!["-i", "/test.txt"],
            "",
            vec![("/test.txt", "Hello\nhello\nWorld\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Hello\nWorld\n");
    }

    #[tokio::test]
    async fn test_no_duplicates() {
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", "a\nb\nc\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_all_same() {
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", "x\nx\nx\n")],
        )
        .await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "x\n");
    }

    #[tokio::test]
    async fn test_empty_input() {
        let ctx = make_ctx(vec![], "", vec![]).await;
        let result = UniqCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let ctx = make_ctx(vec!["/nonexistent.txt"], "", vec![]).await;
        let result = UniqCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }
}