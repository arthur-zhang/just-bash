// src/commands/cat/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct CatCommand;

#[async_trait]
impl Command for CatCommand {
    fn name(&self) -> &'static str {
        "cat"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: cat [OPTION]... [FILE]...\n\n\
                 Concatenate FILE(s) to standard output.\n\n\
                 Options:\n\
                   -n, --number     number all output lines\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut show_line_numbers = false;
        let mut files: Vec<String> = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-n" | "--number" => show_line_numbers = true,
                _ if !arg.starts_with('-') || arg == "-" => files.push(arg.clone()),
                _ => {}
            }
        }

        // 如果没有文件，从 stdin 读取
        if files.is_empty() {
            files.push("-".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let mut line_number = 1;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("cat: {}: No such file or directory\n", file));
                        exit_code = 1;
                        continue;
                    }
                }
            };

            if show_line_numbers {
                let (numbered, next_line) = add_line_numbers(&content, line_number);
                stdout.push_str(&numbered);
                line_number = next_line;
            } else {
                stdout.push_str(&content);
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

fn add_line_numbers(content: &str, start_line: usize) -> (String, usize) {
    let lines: Vec<&str> = content.split('\n').collect();
    let has_trailing_newline = content.ends_with('\n');
    let lines_to_number = if has_trailing_newline {
        &lines[..lines.len() - 1]
    } else {
        &lines[..]
    };

    let numbered: Vec<String> = lines_to_number
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{}", start_line + i, line))
        .collect();

    let result = if has_trailing_newline {
        format!("{}\n", numbered.join("\n"))
    } else {
        numbered.join("\n")
    };

    (result, start_line + lines_to_number.len())
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
    async fn test_cat_single_file() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello world\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "aaa\nbbb\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_with_line_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/test.txt"],
            vec![("/test.txt", "line1\nline2\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "     1\tline1\n     2\tline2\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_file_not_found() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt"], vec![]).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_cat_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-".to_string()],
            stdin: "from stdin\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "from stdin\n");
        assert_eq!(result.exit_code, 0);
    }
}
