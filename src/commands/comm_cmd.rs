use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct CommCommand;

const HELP: &str = "comm - compare two sorted files line by line

Usage: comm [OPTION]... FILE1 FILE2

Options:
  -1             suppress column 1 (lines unique to FILE1)
  -2             suppress column 2 (lines unique to FILE2)
  -3             suppress column 3 (lines that appear in both files)
  --help         display this help and exit";

#[async_trait]
impl Command for CommCommand {
    fn name(&self) -> &'static str {
        "comm"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut suppress1 = false;
        let mut suppress2 = false;
        let mut suppress3 = false;
        let mut files = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-1" => suppress1 = true,
                "-2" => suppress2 = true,
                "-3" => suppress3 = true,
                "-12" | "-21" => { suppress1 = true; suppress2 = true; }
                "-13" | "-31" => { suppress1 = true; suppress3 = true; }
                "-23" | "-32" => { suppress2 = true; suppress3 = true; }
                "-123" | "-132" | "-213" | "-231" | "-312" | "-321" => {
                    suppress1 = true; suppress2 = true; suppress3 = true;
                }
                s if s.starts_with('-') && s != "-" => {
                    return CommandResult::error(format!("comm: invalid option -- '{}'\n", &s[1..]));
                }
                _ => files.push(arg.clone()),
            }
        }

        if files.len() != 2 {
            return CommandResult::error(
                "comm: missing operand\nTry 'comm --help' for more information.\n".to_string()
            );
        }

        let content1 = if files[0] == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[0]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => {
                    return CommandResult::error(format!("comm: {}: No such file or directory\n", files[0]));
                }
            }
        };

        let content2 = if files[1] == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[1]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => {
                    return CommandResult::error(format!("comm: {}: No such file or directory\n", files[1]));
                }
            }
        };

        let mut lines1: Vec<&str> = content1.split('\n').collect();
        let mut lines2: Vec<&str> = content2.split('\n').collect();

        if !lines1.is_empty() && lines1.last() == Some(&"") { lines1.pop(); }
        if !lines2.is_empty() && lines2.last() == Some(&"") { lines2.pop(); }

        let col2_prefix = if suppress1 { "" } else { "\t" };
        let col3_prefix = format!("{}{}", if suppress1 { "" } else { "\t" }, if suppress2 { "" } else { "\t" });

        let mut output = String::new();
        let mut i = 0;
        let mut j = 0;

        while i < lines1.len() || j < lines2.len() {
            if i >= lines1.len() {
                if !suppress2 {
                    output.push_str(&format!("{}{}\n", col2_prefix, lines2[j]));
                }
                j += 1;
            } else if j >= lines2.len() {
                if !suppress1 {
                    output.push_str(&format!("{}\n", lines1[i]));
                }
                i += 1;
            } else if lines1[i] < lines2[j] {
                if !suppress1 {
                    output.push_str(&format!("{}\n", lines1[i]));
                }
                i += 1;
            } else if lines1[i] > lines2[j] {
                if !suppress2 {
                    output.push_str(&format!("{}{}\n", col2_prefix, lines2[j]));
                }
                j += 1;
            } else {
                if !suppress3 {
                    output.push_str(&format!("{}{}\n", col3_prefix, lines1[i]));
                }
                i += 1;
                j += 1;
            }
        }

        CommandResult::success(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::{InMemoryFs, FileSystem};

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
    async fn test_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = CommCommand.execute(ctx).await;
        assert!(result.stdout.contains("comm"));
        assert!(result.stdout.contains("-1"));
    }

    #[tokio::test]
    async fn test_missing_operand() {
        let ctx = create_ctx(vec![]);
        let result = CommCommand.execute(ctx).await;
        assert!(result.stderr.contains("missing operand"));
    }

    #[tokio::test]
    async fn test_compare_files() {
        let mut ctx = create_ctx(vec!["/a.txt", "/b.txt"]);
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"a\nb\nc\n").await.unwrap();
        fs.write_file("/b.txt", b"b\nc\nd\n").await.unwrap();
        ctx.fs = fs;
        let result = CommCommand.execute(ctx).await;
        assert!(result.stdout.contains("a"));
        assert!(result.stdout.contains("d"));
    }

    #[tokio::test]
    async fn test_suppress_col1() {
        let mut ctx = create_ctx(vec!["-1", "/a.txt", "/b.txt"]);
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"a\nb\n").await.unwrap();
        fs.write_file("/b.txt", b"b\nc\n").await.unwrap();
        ctx.fs = fs;
        let result = CommCommand.execute(ctx).await;
        assert!(!result.stdout.starts_with("a"));
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let ctx = create_ctx(vec!["/nonexistent", "/b.txt"]);
        let result = CommCommand.execute(ctx).await;
        assert!(result.stderr.contains("No such file"));
    }
}
