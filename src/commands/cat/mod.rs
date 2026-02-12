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
            exec_fn: None,
            fetch_fn: None,
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
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "from stdin\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_file_with_newline() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello world\n");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_three_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt", "/c.txt"],
            vec![("/a.txt", "A"), ("/b.txt", "B"), ("/c.txt", "C")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ABC");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_line_numbers_padded() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/test.txt"],
            vec![("/test.txt", "a\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "     1\ta\n");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_continue_after_missing_file() {
        let ctx = make_ctx_with_files(
            vec!["/missing.txt", "/exists.txt"],
            vec![("/exists.txt", "content")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "content");
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_cat_empty_file() {
        let ctx = make_ctx_with_files(
            vec!["/empty.txt"],
            vec![("/empty.txt", "")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_special_characters() {
        let ctx = make_ctx_with_files(
            vec!["/special.txt"],
            vec![("/special.txt", "tab:\there\nnewline above")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "tab:\there\nnewline above");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_relative_path() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/home", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.mkdir("/home/user", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/home/user/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["file.txt".to_string()],
            stdin: String::new(),
            cwd: "/home/user".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "content");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_stdin_with_file() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file.txt", b"from file\n").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-".to_string(), "/file.txt".to_string()],
            stdin: "from stdin\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "from stdin\nfrom file\n");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_file_with_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file.txt", b"from file\n").await.unwrap();
        let ctx = CommandContext {
            args: vec!["/file.txt".to_string(), "-".to_string()],
            stdin: "from stdin\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "from file\nfrom stdin\n");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_stdin_with_line_numbers() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file.txt", b"line1\n").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-n".to_string(), "/file.txt".to_string(), "-".to_string()],
            stdin: "line2\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "     1\tline1\n     2\tline2\n");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_number_flag_long() {
        let ctx = make_ctx_with_files(
            vec!["--number", "/test.txt"],
            vec![("/test.txt", "line1\nline2\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "     1\tline1\n     2\tline2\n");
        assert_eq!(result.exit_code, 0);
    }
}
