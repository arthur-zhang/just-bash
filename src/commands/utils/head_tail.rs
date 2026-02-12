// src/commands/utils/head_tail.rs
use crate::commands::{CommandContext, CommandResult};

#[derive(Debug, Clone)]
pub struct HeadTailOptions {
    pub lines: usize,
    pub bytes: Option<usize>,
    pub quiet: bool,
    pub verbose: bool,
    pub files: Vec<String>,
    pub from_line: bool, // tail +N 语法
}

impl Default for HeadTailOptions {
    fn default() -> Self {
        Self {
            lines: 10,
            bytes: None,
            quiet: false,
            verbose: false,
            files: Vec::new(),
            from_line: false,
        }
    }
}

pub enum HeadTailParseResult {
    Ok(HeadTailOptions),
    Err(CommandResult),
}

pub fn parse_head_tail_args(args: &[String], cmd_name: &str) -> HeadTailParseResult {
    let mut opts = HeadTailOptions::default();
    let is_tail = cmd_name == "tail";

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-n" && i + 1 < args.len() {
            i += 1;
            let next_arg = &args[i];
            if is_tail && next_arg.starts_with('+') {
                opts.from_line = true;
                opts.lines = next_arg[1..].parse().unwrap_or(10);
            } else {
                opts.lines = next_arg.parse().unwrap_or(10);
            }
        } else if is_tail && arg.starts_with("-n+") {
            opts.from_line = true;
            opts.lines = arg[3..].parse().unwrap_or(10);
        } else if arg.starts_with("-n") && arg.len() > 2 {
            opts.lines = arg[2..].parse().unwrap_or(10);
        } else if arg == "-c" && i + 1 < args.len() {
            i += 1;
            opts.bytes = args[i].parse().ok();
        } else if arg.starts_with("-c") && arg.len() > 2 {
            opts.bytes = arg[2..].parse().ok();
        } else if arg.starts_with("--bytes=") {
            opts.bytes = arg[8..].parse().ok();
        } else if arg.starts_with("--lines=") {
            opts.lines = arg[8..].parse().unwrap_or(10);
        } else if arg == "-q" || arg == "--quiet" || arg == "--silent" {
            opts.quiet = true;
        } else if arg == "-v" || arg == "--verbose" {
            opts.verbose = true;
        } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit()) {
            opts.lines = arg[1..].parse().unwrap_or(10);
        } else if arg.starts_with("--") {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: unrecognized option '{}'\n", cmd_name, arg)
            ));
        } else if arg.starts_with('-') && arg != "-" {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: invalid option -- '{}'\n", cmd_name, &arg[1..])
            ));
        } else {
            opts.files.push(arg.clone());
        }
        i += 1;
    }

    // 验证 bytes
    if let Some(bytes) = opts.bytes {
        if bytes == 0 {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: invalid number of bytes\n", cmd_name)
            ));
        }
    }

    HeadTailParseResult::Ok(opts)
}

pub async fn process_head_tail_files<F>(
    ctx: &CommandContext,
    opts: &HeadTailOptions,
    cmd_name: &str,
    processor: F,
) -> CommandResult
where
    F: Fn(&str) -> String,
{
    // 如果没有文件，从 stdin 读取
    if opts.files.is_empty() {
        return CommandResult::success(processor(&ctx.stdin));
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    let show_headers = opts.verbose || (!opts.quiet && opts.files.len() > 1);
    let mut files_processed = 0;

    for file in &opts.files {
        let path = ctx.fs.resolve_path(&ctx.cwd, file);
        match ctx.fs.read_file(&path).await {
            Ok(content) => {
                if show_headers {
                    if files_processed > 0 {
                        stdout.push('\n');
                    }
                    stdout.push_str(&format!("==> {} <==\n", file));
                }
                stdout.push_str(&processor(&content));
                files_processed += 1;
            }
            Err(_) => {
                stderr.push_str(&format!("{}: {}: No such file or directory\n", cmd_name, file));
                exit_code = 1;
            }
        }
    }

    CommandResult::with_exit_code(stdout, stderr, exit_code)
}

pub fn get_head(content: &str, lines: usize, bytes: Option<usize>) -> String {
    if let Some(b) = bytes {
        return content.chars().take(b).collect();
    }

    if lines == 0 {
        return String::new();
    }

    let mut pos = 0;
    let mut line_count = 0;
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();

    while pos < len && line_count < lines {
        if chars[pos] == '\n' {
            line_count += 1;
        }
        pos += 1;
    }

    if pos > 0 {
        chars[..pos].iter().collect()
    } else {
        String::new()
    }
}

pub fn get_tail(content: &str, lines: usize, bytes: Option<usize>, from_line: bool) -> String {
    if let Some(b) = bytes {
        let chars: Vec<char> = content.chars().collect();
        let start = chars.len().saturating_sub(b);
        return chars[start..].iter().collect();
    }

    let len = content.len();
    if len == 0 {
        return String::new();
    }

    // +N 语法：从第 N 行开始
    if from_line {
        let mut pos = 0;
        let mut line_count = 1;
        let chars: Vec<char> = content.chars().collect();
        while pos < chars.len() && line_count < lines {
            if chars[pos] == '\n' {
                line_count += 1;
            }
            pos += 1;
        }
        let result: String = chars[pos..].iter().collect();
        if result.ends_with('\n') {
            result
        } else {
            format!("{}\n", result)
        }
    } else {
        if lines == 0 {
            return String::new();
        }

        // 从后向前扫描
        let chars: Vec<char> = content.chars().collect();
        let mut pos = chars.len();
        if pos > 0 && chars[pos - 1] == '\n' {
            pos -= 1;
        }

        let mut line_count = 0;
        while pos > 0 && line_count < lines {
            pos -= 1;
            if chars[pos] == '\n' {
                line_count += 1;
                if line_count == lines {
                    pos += 1;
                    break;
                }
            }
        }

        let result: String = chars[pos..].iter().collect();
        if content.ends_with('\n') {
            result
        } else {
            format!("{}\n", result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_head_lines() {
        let content = "line1\nline2\nline3\nline4\n";
        assert_eq!(get_head(content, 2, None), "line1\nline2\n");
    }

    #[test]
    fn test_get_head_bytes() {
        let content = "hello world";
        assert_eq!(get_head(content, 10, Some(5)), "hello");
    }

    #[test]
    fn test_get_head_zero_lines() {
        let content = "line1\nline2\n";
        assert_eq!(get_head(content, 0, None), "");
    }

    #[test]
    fn test_get_tail_lines() {
        let content = "line1\nline2\nline3\nline4\n";
        assert_eq!(get_tail(content, 2, None, false), "line3\nline4\n");
    }

    #[test]
    fn test_get_tail_bytes() {
        let content = "hello world";
        assert_eq!(get_tail(content, 10, Some(5), false), "world");
    }

    #[test]
    fn test_get_tail_from_line() {
        let content = "line1\nline2\nline3\nline4\n";
        let result = get_tail(content, 2, None, true);
        assert!(result.contains("line2"));
    }

    #[test]
    fn test_parse_head_tail_args_lines() {
        let args: Vec<String> = vec!["-n", "5"].into_iter().map(String::from).collect();
        if let HeadTailParseResult::Ok(opts) = parse_head_tail_args(&args, "head") {
            assert_eq!(opts.lines, 5);
        } else {
            panic!("Expected Ok");
        }
    }

    #[test]
    fn test_parse_head_tail_args_bytes() {
        let args: Vec<String> = vec!["-c", "100"].into_iter().map(String::from).collect();
        if let HeadTailParseResult::Ok(opts) = parse_head_tail_args(&args, "head") {
            assert_eq!(opts.bytes, Some(100));
        } else {
            panic!("Expected Ok");
        }
    }

    #[test]
    fn test_parse_head_tail_args_quiet() {
        let args: Vec<String> = vec!["-q", "file.txt"].into_iter().map(String::from).collect();
        if let HeadTailParseResult::Ok(opts) = parse_head_tail_args(&args, "head") {
            assert!(opts.quiet);
            assert_eq!(opts.files, vec!["file.txt"]);
        } else {
            panic!("Expected Ok");
        }
    }
}
