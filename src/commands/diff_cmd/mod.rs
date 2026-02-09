// src/commands/diff_cmd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use similar::{ChangeTag, TextDiff};

pub struct DiffCommand;

#[async_trait]
impl Command for DiffCommand {
    fn name(&self) -> &'static str {
        "diff"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: diff [OPTION]... FILE1 FILE2\n\n\
                 Compare files line by line.\n\n\
                 Options:\n\
                   -u, --unified                output unified diff format (default)\n\
                   -q, --brief                  report only whether files differ\n\
                   -s, --report-identical-files  report when files are the same\n\
                   -i, --ignore-case            ignore case differences\n\
                       --help                   display this help and exit\n"
                    .to_string(),
            );
        }

        // Parse arguments
        let mut brief = false;
        let mut report_same = false;
        let mut ignore_case = false;
        let mut files: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-u" | "--unified" => { /* accepted but always default */ }
                "-q" | "--brief" => brief = true,
                "-s" | "--report-identical-files" => report_same = true,
                "-i" | "--ignore-case" => ignore_case = true,
                _ if !arg.starts_with('-') || arg == "-" => files.push(arg.clone()),
                _ => {}
            }
        }

        if files.len() < 2 {
            return CommandResult::with_exit_code(
                String::new(),
                "diff: missing operand\n".to_string(),
                2,
            );
        }

        let f1 = &files[0];
        let f2 = &files[1];

        // Read file 1
        let c1 = if f1 == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, f1);
            match ctx.fs.read_file(&path).await {
                Ok(content) => content,
                Err(_) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("diff: {}: No such file or directory\n", f1),
                        2,
                    );
                }
            }
        };

        // Read file 2
        let c2 = if f2 == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, f2);
            match ctx.fs.read_file(&path).await {
                Ok(content) => content,
                Err(_) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("diff: {}: No such file or directory\n", f2),
                        2,
                    );
                }
            }
        };

        // For comparison, optionally lowercase
        let t1 = if ignore_case { c1.to_lowercase() } else { c1.clone() };
        let t2 = if ignore_case { c2.to_lowercase() } else { c2.clone() };

        // Check if identical
        if t1 == t2 {
            if report_same {
                return CommandResult::with_exit_code(
                    format!("Files {} and {} are identical\n", f1, f2),
                    String::new(),
                    0,
                );
            }
            return CommandResult::with_exit_code(String::new(), String::new(), 0);
        }

        // Files differ
        if brief {
            return CommandResult::with_exit_code(
                format!("Files {} and {} differ\n", f1, f2),
                String::new(),
                1,
            );
        }

        // Generate unified diff using original content (not lowercased)
        let output = format_unified_diff(f1, f2, &c1, &c2);
        CommandResult::with_exit_code(output, String::new(), 1)
    }
}

/// Format a unified diff with 3 context lines and proper headers.
fn format_unified_diff(file1: &str, file2: &str, content1: &str, content2: &str) -> String {
    let diff = TextDiff::from_lines(content1, content2);
    let mut output = String::new();

    // Headers
    output.push_str(&format!("--- {}\n", file1));
    output.push_str(&format!("+++ {}\n", file2));

    // Generate hunks with 3 lines of context
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        // Hunk header
        output.push_str(&format!("{}\n", hunk.header()));

        // Hunk content
        for change in hunk.iter_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            output.push_str(sign);
            output.push_str(change.value());
            // Ensure each change line ends with newline
            if !change.value().ends_with('\n') {
                output.push('\n');
                output.push_str("\\ No newline at end of file\n");
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{InMemoryFs, FileSystem};
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn make_ctx_with_files(
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
    async fn test_identical_files_exit_0_no_output() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "b.txt"],
            "",
            vec![("/a.txt", "hello\nworld\n"), ("/b.txt", "hello\nworld\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
    }

    #[tokio::test]
    async fn test_different_files_exit_1_unified_diff() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "b.txt"],
            "",
            vec![("/a.txt", "hello\nworld\n"), ("/b.txt", "hello\nrust\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.contains("--- a.txt"));
        assert!(result.stdout.contains("+++ b.txt"));
        assert!(result.stdout.contains("-world"));
        assert!(result.stdout.contains("+rust"));
    }

    #[tokio::test]
    async fn test_brief_mode() {
        let ctx = make_ctx_with_files(
            vec!["-q", "a.txt", "b.txt"],
            "",
            vec![("/a.txt", "hello\n"), ("/b.txt", "world\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stdout, "Files a.txt and b.txt differ\n");
    }

    #[tokio::test]
    async fn test_report_identical() {
        let ctx = make_ctx_with_files(
            vec!["-s", "a.txt", "b.txt"],
            "",
            vec![("/a.txt", "same\n"), ("/b.txt", "same\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Files a.txt and b.txt are identical\n");
    }

    #[tokio::test]
    async fn test_ignore_case() {
        let ctx = make_ctx_with_files(
            vec!["-i", "a.txt", "b.txt"],
            "",
            vec![("/a.txt", "Hello\nWorld\n"), ("/b.txt", "hello\nworld\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_stdin_as_file1() {
        let ctx = make_ctx_with_files(
            vec!["-", "b.txt"],
            "hello\nworld\n",
            vec![("/b.txt", "hello\nrust\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.contains("--- -"));
        assert!(result.stdout.contains("+++ b.txt"));
    }

    #[tokio::test]
    async fn test_missing_file_exit_2() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "nonexistent.txt"],
            "",
            vec![("/a.txt", "hello\n")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("nonexistent.txt"));
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_empty_files_identical() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "b.txt"],
            "",
            vec![("/a.txt", ""), ("/b.txt", "")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_file_vs_empty_file() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "b.txt"],
            "",
            vec![("/a.txt", "hello\nworld\n"), ("/b.txt", "")],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.contains("-hello"));
        assert!(result.stdout.contains("-world"));
    }

    #[tokio::test]
    async fn test_added_removed_lines_format() {
        let ctx = make_ctx_with_files(
            vec!["a.txt", "b.txt"],
            "",
            vec![
                ("/a.txt", "line1\nline2\nline3\n"),
                ("/b.txt", "line1\nmodified\nline3\nnewline\n"),
            ],
        ).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        // Should contain context line
        assert!(result.stdout.contains(" line1\n"));
        // Should contain removed line
        assert!(result.stdout.contains("-line2\n"));
        // Should contain added line
        assert!(result.stdout.contains("+modified\n"));
        // Should contain hunk header
        assert!(result.stdout.contains("@@"));
    }

    #[tokio::test]
    async fn test_missing_operand() {
        let ctx = make_ctx_with_files(vec!["a.txt"], "", vec![("/a.txt", "hello\n")]).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("missing operand"));
    }

    #[tokio::test]
    async fn test_help_flag() {
        let ctx = make_ctx_with_files(vec!["--help"], "", vec![]).await;
        let result = DiffCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Usage: diff"));
    }
}
