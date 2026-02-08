// src/commands/grep/mod.rs
use async_trait::async_trait;
use regex_lite::Regex;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct GrepCommand;

struct GrepOptions {
    pattern: String,
    ignore_case: bool,
    invert_match: bool,
    count_only: bool,
    files_with_matches: bool,
    files_without_matches: bool,
    line_number: bool,
    only_matching: bool,
    quiet: bool,
    fixed_strings: bool,
    max_count: Option<usize>,
    files: Vec<String>,
}

impl Default for GrepOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            ignore_case: false,
            invert_match: false,
            count_only: false,
            files_with_matches: false,
            files_without_matches: false,
            line_number: false,
            only_matching: false,
            quiet: false,
            fixed_strings: false,
            max_count: None,
            files: Vec::new(),
        }
    }
}

fn parse_grep_args(args: &[String]) -> Result<GrepOptions, String> {
    let mut opts = GrepOptions::default();
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-e" && i + 1 < args.len() {
            i += 1;
            opts.pattern = args[i].clone();
        } else if arg == "-i" || arg == "--ignore-case" {
            opts.ignore_case = true;
        } else if arg == "-v" || arg == "--invert-match" {
            opts.invert_match = true;
        } else if arg == "-c" || arg == "--count" {
            opts.count_only = true;
        } else if arg == "-l" || arg == "--files-with-matches" {
            opts.files_with_matches = true;
        } else if arg == "-L" || arg == "--files-without-match" {
            opts.files_without_matches = true;
        } else if arg == "-n" || arg == "--line-number" {
            opts.line_number = true;
        } else if arg == "-o" || arg == "--only-matching" {
            opts.only_matching = true;
        } else if arg == "-q" || arg == "--quiet" || arg == "--silent" {
            opts.quiet = true;
        } else if arg == "-F" || arg == "--fixed-strings" {
            opts.fixed_strings = true;
        } else if arg == "-E" || arg == "--extended-regexp" {
            // 默认就是扩展正则
        } else if arg == "-m" && i + 1 < args.len() {
            i += 1;
            opts.max_count = args[i].parse().ok();
        } else if let Some(n) = arg.strip_prefix("-m") {
            opts.max_count = n.parse().ok();
        } else if !arg.starts_with('-') {
            positional.push(arg.clone());
        }
        i += 1;
    }

    // 第一个位置参数是 pattern（如果没有用 -e 指定）
    if opts.pattern.is_empty() {
        if positional.is_empty() {
            return Err("grep: no pattern specified".to_string());
        }
        opts.pattern = positional.remove(0);
    }

    opts.files = positional;
    Ok(opts)
}

fn build_regex(opts: &GrepOptions) -> Result<Regex, String> {
    let mut pattern = opts.pattern.clone();

    // 固定字符串模式：转义所有正则特殊字符
    if opts.fixed_strings {
        pattern = regex_lite::escape(&pattern);
    }

    // 忽略大小写
    if opts.ignore_case {
        pattern = format!("(?i){}", pattern);
    }

    Regex::new(&pattern).map_err(|e| format!("grep: invalid pattern: {}", e))
}

#[async_trait]
impl Command for GrepCommand {
    fn name(&self) -> &'static str {
        "grep"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: grep [OPTION]... PATTERN [FILE]...\n\n\
                 Search for PATTERN in each FILE.\n\n\
                 Options:\n\
                   -E, --extended-regexp  PATTERN is an extended regular expression\n\
                   -F, --fixed-strings    PATTERN is a set of newline-separated strings\n\
                   -i, --ignore-case      ignore case distinctions\n\
                   -v, --invert-match     select non-matching lines\n\
                   -c, --count            print only a count of matching lines\n\
                   -l, --files-with-matches  print only names of FILEs with matches\n\
                   -L, --files-without-match  print only names of FILEs without matches\n\
                   -n, --line-number      print line number with output lines\n\
                   -o, --only-matching    show only the part of a line matching PATTERN\n\
                   -q, --quiet            suppress all normal output\n\
                   -m NUM, --max-count=NUM  stop after NUM matches\n\
                       --help             display this help and exit\n".to_string()
            );
        }

        let opts = match parse_grep_args(&ctx.args) {
            Ok(o) => o,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        let regex = match build_regex(&opts) {
            Ok(r) => r,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 1; // 默认没有匹配

        // 如果没有文件，从 stdin 读取
        let files = if opts.files.is_empty() {
            vec!["-".to_string()]
        } else {
            opts.files.clone()
        };

        let show_filename = files.len() > 1;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("grep: {}: No such file or directory\n", file));
                        continue;
                    }
                }
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = 0;
            let mut matched_lines: Vec<(usize, &str)> = Vec::new();

            for (line_num, line) in lines.iter().enumerate() {
                let matches = regex.is_match(line);
                let should_output = if opts.invert_match { !matches } else { matches };

                if should_output {
                    file_matches += 1;
                    matched_lines.push((line_num + 1, line));

                    if let Some(max) = opts.max_count {
                        if file_matches >= max {
                            break;
                        }
                    }
                }
            }

            if file_matches > 0 {
                exit_code = 0;
            }

            if opts.quiet {
                if file_matches > 0 {
                    return CommandResult::with_exit_code(String::new(), stderr, 0);
                }
                continue;
            }

            if opts.files_with_matches {
                if file_matches > 0 {
                    stdout.push_str(&format!("{}\n", file));
                }
                continue;
            }

            if opts.files_without_matches {
                if file_matches == 0 {
                    stdout.push_str(&format!("{}\n", file));
                }
                continue;
            }

            if opts.count_only {
                if show_filename {
                    stdout.push_str(&format!("{}:{}\n", file, file_matches));
                } else {
                    stdout.push_str(&format!("{}\n", file_matches));
                }
                continue;
            }

            // 输出匹配的行
            for (line_num, line) in matched_lines {
                let prefix = if show_filename {
                    if opts.line_number {
                        format!("{}:{}:", file, line_num)
                    } else {
                        format!("{}:", file)
                    }
                } else if opts.line_number {
                    format!("{}:", line_num)
                } else {
                    String::new()
                };

                if opts.only_matching {
                    for mat in regex.find_iter(line) {
                        stdout.push_str(&format!("{}{}\n", prefix, mat.as_str()));
                    }
                } else {
                    stdout.push_str(&format!("{}{}\n", prefix, line));
                }
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
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
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_grep_basic() {
        let ctx = make_ctx_with_files(
            vec!["hello", "/test.txt"],
            vec![("/test.txt", "hello world\nfoo bar\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("hello world"));
        assert!(result.stdout.contains("hello again"));
        assert!(!result.stdout.contains("foo bar"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_ignore_case() {
        let ctx = make_ctx_with_files(
            vec!["-i", "HELLO", "/test.txt"],
            vec![("/test.txt", "Hello World\nhello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("Hello World"));
        assert!(result.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn test_grep_invert() {
        let ctx = make_ctx_with_files(
            vec!["-v", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "world");
    }

    #[tokio::test]
    async fn test_grep_count() {
        let ctx = make_ctx_with_files(
            vec!["-c", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_grep_line_number() {
        let ctx = make_ctx_with_files(
            vec!["-n", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("1:hello"));
        assert!(result.stdout.contains("3:hello again"));
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let ctx = make_ctx_with_files(
            vec!["notfound", "/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_grep_fixed_strings() {
        let ctx = make_ctx_with_files(
            vec!["-F", "a.b", "/test.txt"],
            vec![("/test.txt", "a.b\naXb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "a.b");
    }
}
