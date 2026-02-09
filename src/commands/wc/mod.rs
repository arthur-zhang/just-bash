// src/commands/wc/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct WcCommand;

#[derive(Default)]
struct Stats {
    lines: usize,
    words: usize,
    chars: usize,
}

fn count_stats(content: &str) -> Stats {
    let mut stats = Stats::default();
    let mut in_word = false;

    for c in content.chars() {
        stats.chars += 1;
        if c == '\n' {
            stats.lines += 1;
            if in_word {
                stats.words += 1;
                in_word = false;
            }
        } else if c == ' ' || c == '\t' || c == '\r' {
            if in_word {
                stats.words += 1;
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }

    if in_word {
        stats.words += 1;
    }

    stats
}

#[async_trait]
impl Command for WcCommand {
    fn name(&self) -> &'static str {
        "wc"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: wc [OPTION]... [FILE]...\n\n\
                 Print newline, word, and byte counts for each FILE.\n\n\
                 Options:\n\
                   -c, --bytes    print the byte counts\n\
                   -m, --chars    print the character counts\n\
                   -l, --lines    print the newline counts\n\
                   -w, --words    print the word counts\n\
                       --help     display this help and exit\n".to_string()
            );
        }

        let mut show_lines = false;
        let mut show_words = false;
        let mut show_chars = false;
        let mut files: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-l" | "--lines" => show_lines = true,
                "-w" | "--words" => show_words = true,
                "-c" | "--bytes" | "-m" | "--chars" => show_chars = true,
                _ if !arg.starts_with('-') => files.push(arg.clone()),
                _ => {}
            }
        }

        // 如果没有指定任何标志，显示全部
        if !show_lines && !show_words && !show_chars {
            show_lines = true;
            show_words = true;
            show_chars = true;
        }

        if files.is_empty() {
            files.push("-".to_string());
        }

        let mut all_stats: Vec<(Stats, Option<String>)> = Vec::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("wc: {}: No such file or directory\n", file));
                        exit_code = 1;
                        continue;
                    }
                }
            };

            let stats = count_stats(&content);
            let filename = if file == "-" { None } else { Some(file.clone()) };
            all_stats.push((stats, filename));
        }

        // 计算最大宽度用于对齐
        let mut max_lines = 0;
        let mut max_words = 0;
        let mut max_chars = 0;
        for (stats, _) in &all_stats {
            max_lines = max_lines.max(stats.lines);
            max_words = max_words.max(stats.words);
            max_chars = max_chars.max(stats.chars);
        }

        let width = if all_stats.len() > 1 { 7 } else { 0 };
        let width = width
            .max(max_lines.to_string().len())
            .max(max_words.to_string().len())
            .max(max_chars.to_string().len());

        let mut stdout = String::new();
        let mut total = Stats::default();

        for (stats, filename) in &all_stats {
            let mut parts: Vec<String> = Vec::new();
            if show_lines {
                parts.push(format!("{:>width$}", stats.lines, width = width));
            }
            if show_words {
                parts.push(format!("{:>width$}", stats.words, width = width));
            }
            if show_chars {
                parts.push(format!("{:>width$}", stats.chars, width = width));
            }

            let line = if let Some(name) = filename {
                format!("{} {}\n", parts.join(" "), name)
            } else {
                format!("{}\n", parts.join(" "))
            };
            stdout.push_str(&line);

            total.lines += stats.lines;
            total.words += stats.words;
            total.chars += stats.chars;
        }

        // 如果有多个文件，显示总计
        if all_stats.len() > 1 {
            let mut parts: Vec<String> = Vec::new();
            if show_lines {
                parts.push(format!("{:>width$}", total.lines, width = width));
            }
            if show_words {
                parts.push(format!("{:>width$}", total.words, width = width));
            }
            if show_chars {
                parts.push(format!("{:>width$}", total.chars, width = width));
            }
            stdout.push_str(&format!("{} total\n", parts.join(" ")));
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
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
    async fn test_wc_all() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "hello world\nfoo bar\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        // 2 lines, 4 words, 20 chars
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("4"));
        assert!(result.stdout.contains("20"));
    }

    #[tokio::test]
    async fn test_wc_lines_only() {
        let ctx = make_ctx_with_files(
            vec!["-l", "/test.txt"],
            vec![("/test.txt", "line1\nline2\nline3\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.trim().starts_with("3"));
    }

    #[tokio::test]
    async fn test_wc_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\nccc\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("total"));
    }
}
