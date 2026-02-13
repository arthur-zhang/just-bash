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

    // ============================================================
    // Case sensitivity tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_case_sensitive_by_default() {
        let ctx = make_ctx_with_files(
            vec!["hello", "/test.txt"],
            vec![("/test.txt", "Hello\nhello\nHELLO\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_ignore_case_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--ignore-case", "hello", "/test.txt"],
            vec![("/test.txt", "Hello\nhello\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "Hello\nhello\n");
    }

    // ============================================================
    // Line number tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_line_number_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--line-number", "match", "/test.txt"],
            vec![("/test.txt", "match\nno\nmatch\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1:match\n3:match\n");
    }

    #[tokio::test]
    async fn test_grep_line_number_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["-n", "test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test\nfoo\ntest\n"),
                ("/b.txt", "bar\ntest\n"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("/a.txt:1:test"));
        assert!(result.stdout.contains("/a.txt:3:test"));
        assert!(result.stdout.contains("/b.txt:2:test"));
    }

    // ============================================================
    // Invert match tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_invert_match_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--invert-match", "remove", "/test.txt"],
            vec![("/test.txt", "keep\nremove\nkeep\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "keep\nkeep\n");
    }

    #[tokio::test]
    async fn test_grep_invert_with_line_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-v", "-n", "skip", "/test.txt"],
            vec![("/test.txt", "keep1\nskip\nkeep2\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1:keep1\n3:keep2\n");
    }

    // ============================================================
    // Count tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_count_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--count", "test", "/test.txt"],
            vec![("/test.txt", "test\nfoo\ntest\nbar\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_grep_count_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["-c", "test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test\ntest\n"),
                ("/b.txt", "test\nfoo\n"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/a.txt:2\n/b.txt:1\n");
    }

    #[tokio::test]
    async fn test_grep_count_no_matches() {
        let ctx = make_ctx_with_files(
            vec!["-c", "missing", "/test.txt"],
            vec![("/test.txt", "hello\nworld\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "0");
        assert_eq!(result.exit_code, 1);
    }

    // ============================================================
    // Files with/without matches tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_files_with_matches() {
        let ctx = make_ctx_with_files(
            vec!["-l", "hello", "/a.txt", "/b.txt", "/c.txt"],
            vec![
                ("/a.txt", "hello world"),
                ("/b.txt", "goodbye"),
                ("/c.txt", "hello again"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/a.txt\n/c.txt\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--files-with-matches", "test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test"),
                ("/b.txt", "no match"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/a.txt\n");
    }

    #[tokio::test]
    async fn test_grep_files_without_match() {
        let ctx = make_ctx_with_files(
            vec!["-L", "hello", "/a.txt", "/b.txt", "/c.txt"],
            vec![
                ("/a.txt", "hello world"),
                ("/b.txt", "goodbye world"),
                ("/c.txt", "nothing here"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/b.txt\n/c.txt\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_files_without_match_all_have_matches() {
        let ctx = make_ctx_with_files(
            vec!["-L", "hello", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "hello"),
                ("/b.txt", "hello"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_grep_files_without_match_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--files-without-match", "hello", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "hello"),
                ("/b.txt", "goodbye"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/b.txt\n");
    }

    // ============================================================
    // Only matching tests (-o)
    // ============================================================

    #[tokio::test]
    async fn test_grep_only_matching() {
        let ctx = make_ctx_with_files(
            vec!["-o", "hello", "/test.txt"],
            vec![("/test.txt", "hello world hello\nfoo bar\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\nhello\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_only_matching_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--only-matching", "cat", "/test.txt"],
            vec![("/test.txt", "cat dog cat\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "cat\ncat\n");
    }

    #[tokio::test]
    async fn test_grep_only_matching_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["-o", "test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test one test\n"),
                ("/b.txt", "test two\n"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/a.txt:test\n/a.txt:test\n/b.txt:test\n");
    }

    #[tokio::test]
    async fn test_grep_only_matching_with_regex() {
        let ctx = make_ctx_with_files(
            vec!["-o", "[0-9]+", "/test.txt"],
            vec![("/test.txt", "abc123def456\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "123\n456\n");
    }

    #[tokio::test]
    async fn test_grep_only_matching_with_line_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-o", "-n", "test", "/test.txt"],
            vec![("/test.txt", "test one test\nfoo\ntest two\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1:test\n1:test\n3:test\n");
    }

    // ============================================================
    // Quiet mode tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_quiet_with_match() {
        let ctx = make_ctx_with_files(
            vec!["-q", "hello", "/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_quiet_no_match() {
        let ctx = make_ctx_with_files(
            vec!["-q", "missing", "/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_grep_quiet_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--quiet", "test", "/test.txt"],
            vec![("/test.txt", "test\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_silent_alias() {
        let ctx = make_ctx_with_files(
            vec!["--silent", "test", "/test.txt"],
            vec![("/test.txt", "test\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 0);
    }

    // ============================================================
    // Max count tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_max_count() {
        let ctx = make_ctx_with_files(
            vec!["-m", "2", "line", "/test.txt"],
            vec![("/test.txt", "line1\nline2\nline3\nline4\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "line1\nline2\n");
    }

    #[tokio::test]
    async fn test_grep_max_count_combined() {
        let ctx = make_ctx_with_files(
            vec!["-m2", "test", "/test.txt"],
            vec![("/test.txt", "test1\ntest2\ntest3\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test1\ntest2\n");
    }

    #[tokio::test]
    async fn test_grep_max_count_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["-m", "1", "test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test1\ntest2\n"),
                ("/b.txt", "test3\ntest4\n"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "/a.txt:test1\n/b.txt:test3\n");
    }

    // ============================================================
    // Fixed strings tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_fixed_strings_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--fixed-strings", "a.b", "/test.txt"],
            vec![("/test.txt", "a.b\naXb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "a.b");
    }

    #[tokio::test]
    async fn test_grep_fixed_strings_special_chars() {
        let ctx = make_ctx_with_files(
            vec!["-F", "a*b+c?", "/test.txt"],
            vec![("/test.txt", "a*b+c?\naaabbbccc\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "a*b+c?");
    }

    #[tokio::test]
    async fn test_grep_fixed_strings_brackets() {
        let ctx = make_ctx_with_files(
            vec!["-F", "[test]", "/test.txt"],
            vec![("/test.txt", "[test]\ntest\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "[test]");
    }

    // ============================================================
    // Extended regexp tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_extended_regexp() {
        let ctx = make_ctx_with_files(
            vec!["-E", "test|hello", "/test.txt"],
            vec![("/test.txt", "test\nworld\nhello\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test\nhello\n");
    }

    #[tokio::test]
    async fn test_grep_extended_regexp_long_form() {
        let ctx = make_ctx_with_files(
            vec!["--extended-regexp", "a+b+", "/test.txt"],
            vec![("/test.txt", "ab\naab\naaabbb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ab\naab\naaabbb\n");
    }

    // ============================================================
    // Pattern with -e flag tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_pattern_with_e_flag() {
        let ctx = make_ctx_with_files(
            vec!["-e", "hello", "/test.txt"],
            vec![("/test.txt", "hello world\nfoo bar\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "hello world");
    }

    // ============================================================
    // Multiple files tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_multiple_files_with_filename() {
        let ctx = make_ctx_with_files(
            vec!["test", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "test line\n"),
                ("/b.txt", "another test\n"),
            ],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("/a.txt:test line"));
        assert!(result.stdout.contains("/b.txt:another test"));
    }

    #[tokio::test]
    async fn test_grep_single_file_no_filename() {
        let ctx = make_ctx_with_files(
            vec!["test", "/test.txt"],
            vec![("/test.txt", "test line\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test line\n");
        assert!(!result.stdout.contains("/test.txt:"));
    }

    // ============================================================
    // Regex pattern tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_regex_dot() {
        let ctx = make_ctx_with_files(
            vec!["a.b", "/test.txt"],
            vec![("/test.txt", "aXb\na.b\nab\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "aXb\na.b\n");
    }

    #[tokio::test]
    async fn test_grep_regex_star() {
        let ctx = make_ctx_with_files(
            vec!["ab*", "/test.txt"],
            vec![("/test.txt", "a\nab\nabb\nabbb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nab\nabb\nabbb\n");
    }

    #[tokio::test]
    async fn test_grep_regex_plus() {
        let ctx = make_ctx_with_files(
            vec!["ab+", "/test.txt"],
            vec![("/test.txt", "a\nab\nabb\nabbb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ab\nabb\nabbb\n");
    }

    #[tokio::test]
    async fn test_grep_regex_question() {
        let ctx = make_ctx_with_files(
            vec!["ab?c", "/test.txt"],
            vec![("/test.txt", "ac\nabc\nabbc\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ac\nabc\n");
    }

    #[tokio::test]
    async fn test_grep_regex_bracket() {
        let ctx = make_ctx_with_files(
            vec!["[abc]", "/test.txt"],
            vec![("/test.txt", "a\nb\nc\nd\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_grep_regex_negated_bracket() {
        let ctx = make_ctx_with_files(
            vec!["[^abc]", "/test.txt"],
            vec![("/test.txt", "a\nb\nc\nd\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "d");
    }

    #[tokio::test]
    async fn test_grep_regex_anchor_start() {
        let ctx = make_ctx_with_files(
            vec!["^test", "/test.txt"],
            vec![("/test.txt", "test line\n  test line\ntest\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test line\ntest\n");
    }

    #[tokio::test]
    async fn test_grep_regex_anchor_end() {
        let ctx = make_ctx_with_files(
            vec!["test$", "/test.txt"],
            vec![("/test.txt", "test\ntest line\nmy test\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test\nmy test\n");
    }

    #[tokio::test]
    async fn test_grep_regex_word_boundary() {
        let ctx = make_ctx_with_files(
            vec!["\\btest\\b", "/test.txt"],
            vec![("/test.txt", "test\ntesting\nmy test here\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "test\nmy test here\n");
    }

    // ============================================================
    // Combined options tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_count_with_invert() {
        let ctx = make_ctx_with_files(
            vec!["-c", "-v", "skip", "/test.txt"],
            vec![("/test.txt", "keep\nskip\nkeep\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_grep_ignore_case_with_only_matching() {
        let ctx = make_ctx_with_files(
            vec!["-i", "-o", "test", "/test.txt"],
            vec![("/test.txt", "Test TEST test\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "Test\nTEST\ntest\n");
    }

    #[tokio::test]
    async fn test_grep_max_count_with_line_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-m", "2", "-n", "test", "/test.txt"],
            vec![("/test.txt", "test1\nfoo\ntest2\nbar\ntest3\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1:test1\n3:test2\n");
    }

    // ============================================================
    // Empty and edge case tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_empty_file() {
        let ctx = make_ctx_with_files(
            vec!["test", "/empty.txt"],
            vec![("/empty.txt", "")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_grep_empty_pattern() {
        let ctx = make_ctx_with_files(
            vec!["", "/test.txt"],
            vec![("/test.txt", "hello\nworld\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        // Empty pattern matches every line
        assert_eq!(result.stdout, "hello\nworld\n");
    }

    #[tokio::test]
    async fn test_grep_no_newline_at_end() {
        let ctx = make_ctx_with_files(
            vec!["test", "/test.txt"],
            vec![("/test.txt", "test line")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("test line"));
    }

    // ============================================================
    // Real-world use case tests
    // ============================================================

    #[tokio::test]
    async fn test_grep_find_imports() {
        let ctx = make_ctx_with_files(
            vec!["^import", "/index.ts"],
            vec![("/index.ts", "import { foo } from './foo';\nconst x = 1;\nimport { bar } from './bar';\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "import { foo } from './foo';\nimport { bar } from './bar';\n");
    }

    #[tokio::test]
    async fn test_grep_find_ip_addresses() {
        let ctx = make_ctx_with_files(
            vec!["-E", "[0-9]+\\.[0-9]+\\.[0-9]+\\.[0-9]+", "/hosts.txt"],
            vec![("/hosts.txt", "localhost 127.0.0.1\nserver 192.168.1.100\ngateway 10.0.0.1\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "localhost 127.0.0.1\nserver 192.168.1.100\ngateway 10.0.0.1\n");
    }

    #[tokio::test]
    async fn test_grep_find_class_definitions() {
        let ctx = make_ctx_with_files(
            vec!["^class", "/code.ts"],
            vec![("/code.ts", "class User {\n  name: string;\n}\nclass Admin extends User {\n}\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "class User {\nclass Admin extends User {\n");
    }

    #[tokio::test]
    async fn test_grep_extract_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-o", "[0-9]+", "/data.txt"],
            vec![("/data.txt", "Price: 100, Quantity: 50, Total: 5000\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "100\n50\n5000\n");
    }

    #[tokio::test]
    async fn test_grep_filter_log_errors() {
        let ctx = make_ctx_with_files(
            vec!["-i", "error", "/app.log"],
            vec![("/app.log", "INFO: Starting\nERROR: Connection failed\nWARN: Retry\nerror: timeout\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ERROR: Connection failed\nerror: timeout\n");
    }
}
