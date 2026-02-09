// src/commands/paste/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct PasteCommand;

#[async_trait]
impl Command for PasteCommand {
    fn name(&self) -> &'static str {
        "paste"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: paste [OPTION]... [FILE]...\n\n\
                 Merge lines of files side-by-side.\n\n\
                 Options:\n\
                   -d, --delimiters=LIST  use characters from LIST instead of TABs\n\
                   -s, --serial           paste one file at a time instead of in parallel\n\
                       --help             display this help and exit\n"
                    .to_string(),
            );
        }

        let mut delimiters: Vec<char> = vec!['\t'];
        let mut serial = false;
        let mut files: Vec<String> = Vec::new();
        let mut i = 0;
        let args = &ctx.args;

        while i < args.len() {
            let arg = &args[i];
            if arg == "-s" || arg == "--serial" {
                serial = true;
            } else if arg == "-d" {
                // -d LIST (separate argument)
                i += 1;
                if i < args.len() {
                    delimiters = args[i].chars().collect();
                    if delimiters.is_empty() {
                        delimiters = vec!['\t'];
                    }
                }
            } else if arg.starts_with("-d") {
                // -dLIST (combined)
                let list = &arg[2..];
                delimiters = list.chars().collect();
                if delimiters.is_empty() {
                    delimiters = vec!['\t'];
                }
            } else if arg.starts_with("--delimiters=") {
                let list = &arg["--delimiters=".len()..];
                delimiters = list.chars().collect();
                if delimiters.is_empty() {
                    delimiters = vec!['\t'];
                }
            } else if !arg.starts_with('-') || arg == "-" {
                files.push(arg.clone());
            }
            i += 1;
        }

        if files.is_empty() {
            return CommandResult::error(
                "paste: missing operand\n".to_string(),
            );
        }

        // Read all file contents
        let mut file_contents: Vec<String> = Vec::new();
        for file in &files {
            if file == "-" {
                file_contents.push(ctx.stdin.clone());
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => file_contents.push(c),
                    Err(_) => {
                        return CommandResult::error(format!(
                            "paste: {}: No such file or directory\n",
                            file
                        ));
                    }
                }
            }
        }

        let mut output = String::new();

        if serial {
            // Serial mode: join all lines of each file into one line
            for content in &file_contents {
                let lines: Vec<&str> = content.lines().collect();
                for (j, line) in lines.iter().enumerate() {
                    output.push_str(line);
                    if j < lines.len() - 1 {
                        let delim_idx = j % delimiters.len();
                        output.push(delimiters[delim_idx]);
                    }
                }
                output.push('\n');
            }
        } else {
            // Parallel mode: merge corresponding lines
            let file_lines: Vec<Vec<&str>> = file_contents
                .iter()
                .map(|c| c.lines().collect::<Vec<&str>>())
                .collect();
            let max_lines = file_lines.iter().map(|l| l.len()).max().unwrap_or(0);

            for line_idx in 0..max_lines {
                for (file_idx, lines) in file_lines.iter().enumerate() {
                    if file_idx > 0 {
                        let delim_idx = (file_idx - 1) % delimiters.len();
                        output.push(delimiters[delim_idx]);
                    }
                    if line_idx < lines.len() {
                        output.push_str(lines[line_idx]);
                    }
                }
                output.push('\n');
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
    async fn test_paste_two_files() {
        let ctx = make_ctx(
            vec!["/a.txt", "/b.txt"],
            "",
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\nb\nc\n")],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\ta\n2\tb\n3\tc\n");
    }

    #[tokio::test]
    async fn test_paste_three_files() {
        let ctx = make_ctx(
            vec!["/a.txt", "/b.txt", "/c.txt"],
            "",
            vec![
                ("/a.txt", "1\n2\n"),
                ("/b.txt", "a\nb\n"),
                ("/c.txt", "x\ny\n"),
            ],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\ta\tx\n2\tb\ty\n");
    }

    #[tokio::test]
    async fn test_paste_uneven_lines() {
        let ctx = make_ctx(
            vec!["/a.txt", "/b.txt"],
            "",
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\n")],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\ta\n2\t\n3\t\n");
    }

    #[tokio::test]
    async fn test_paste_custom_delimiter() {
        let ctx = make_ctx(
            vec!["-d:", "/a.txt", "/b.txt"],
            "",
            vec![("/a.txt", "1\n2\n"), ("/b.txt", "a\nb\n")],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1:a\n2:b\n");
    }

    #[tokio::test]
    async fn test_paste_serial() {
        let ctx = make_ctx(
            vec!["-s", "/a.txt", "/b.txt"],
            "",
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\nb\nc\n")],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\t2\t3\na\tb\tc\n");
    }

    #[tokio::test]
    async fn test_paste_stdin() {
        let ctx = make_ctx(
            vec!["-", "/b.txt"],
            "1\n2\n3\n",
            vec![("/b.txt", "a\nb\nc\n")],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\ta\n2\tb\n3\tc\n");
    }

    #[tokio::test]
    async fn test_paste_no_files_error() {
        let ctx = make_ctx(vec![], "", vec![]).await;
        let result = PasteCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_paste_file_not_found() {
        let ctx = make_ctx(vec!["/nonexistent.txt"], "", vec![]).await;
        let result = PasteCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_paste_multiple_delimiters() {
        let ctx = make_ctx(
            vec!["-d:,", "/a.txt", "/b.txt", "/c.txt"],
            "",
            vec![
                ("/a.txt", "1\n2\n"),
                ("/b.txt", "a\nb\n"),
                ("/c.txt", "x\ny\n"),
            ],
        )
        .await;
        let result = PasteCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1:a,x\n2:b,y\n");
    }
}
