// src/commands/split_cmd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use regex_lite::Regex;

pub struct SplitCommand;

const HELP: &str = "Usage: split [OPTION]... [FILE [PREFIX]]\n\nOutput pieces of FILE to PREFIXaa, PREFIXab, ...; default size is 1000 lines, and default PREFIX is 'x'.\n\nOptions:\n  -l N         Put N lines per output file\n  -b SIZE      Put SIZE bytes per output file (K, M, G suffixes)\n  -n CHUNKS    Split into CHUNKS equal-sized files\n  -d           Use numeric suffixes\n  -a LENGTH    Use suffixes of length LENGTH (default: 2)\n  --additional-suffix=SUFFIX  Append SUFFIX to file names\n";

fn resolve_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') { path.to_string() }
    else { format!("{}/{}", cwd.trim_end_matches('/'), path) }
}

fn parse_size(s: &str) -> Option<usize> {
    let re = Regex::new(r"^(\d+)([KMGTPEZY]?)([B]?)$").unwrap();
    let caps = re.captures(s)?;
    let num: usize = caps.get(1)?.as_str().parse().ok()?;
    if num < 1 { return None; }
    let suffix = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let mult: usize = match suffix {
        "" => 1,
        "K" | "k" => 1024,
        "M" | "m" => 1024 * 1024,
        "G" | "g" => 1024 * 1024 * 1024,
        "T" | "t" => 1024usize.pow(4),
        "P" | "p" => 1024usize.pow(5),
        _ => return None,
    };
    Some(num * mult)
}

fn generate_suffix(index: usize, use_numeric: bool, length: usize) -> String {
    if use_numeric {
        return format!("{:0>width$}", index, width = length);
    }
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut suffix = String::new();
    let mut remaining = index;
    for _ in 0..length {
        suffix.insert(0, chars[remaining % 26]);
        remaining /= 26;
    }
    suffix
}

#[derive(PartialEq)]
enum SplitMode { Lines, Bytes, Chunks }

#[async_trait]
impl Command for SplitCommand {
    fn name(&self) -> &'static str { "split" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }

        let mut mode = SplitMode::Lines;
        let mut lines = 1000usize;
        let mut bytes = 0usize;
        let mut chunks = 0usize;
        let mut use_numeric = false;
        let mut suffix_length = 2usize;
        let mut additional_suffix = String::new();
        let mut positional: Vec<String> = Vec::new();
        let mut i = 0;

        while i < args.len() {
            let arg = &args[i];
            if arg == "-l" && i + 1 < args.len() {
                match args[i+1].parse::<usize>() {
                    Ok(n) if n >= 1 => { mode = SplitMode::Lines; lines = n; i += 2; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid number of lines: '{}'\n", args[i+1]), 1),
                }
            } else if arg.starts_with("-l") && arg.len() > 2 {
                let val = &arg[2..];
                match val.parse::<usize>() {
                    Ok(n) if n >= 1 => { mode = SplitMode::Lines; lines = n; i += 1; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid number of lines: '{}'\n", val), 1),
                }
            } else if arg == "-b" && i + 1 < args.len() {
                match parse_size(&args[i+1]) {
                    Some(n) => { mode = SplitMode::Bytes; bytes = n; i += 2; }
                    None => return CommandResult::with_exit_code("".into(), format!("split: invalid number of bytes: '{}'\n", args[i+1]), 1),
                }
            } else if arg.starts_with("-b") && arg.len() > 2 {
                match parse_size(&arg[2..]) {
                    Some(n) => { mode = SplitMode::Bytes; bytes = n; i += 1; }
                    None => return CommandResult::with_exit_code("".into(), format!("split: invalid number of bytes: '{}'\n", &arg[2..]), 1),
                }
            } else if arg == "-n" && i + 1 < args.len() {
                match args[i+1].parse::<usize>() {
                    Ok(n) if n >= 1 => { mode = SplitMode::Chunks; chunks = n; i += 2; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid number of chunks: '{}'\n", args[i+1]), 1),
                }
            } else if arg.starts_with("-n") && arg.len() > 2 && arg[2..].chars().all(|c| c.is_ascii_digit()) {
                match arg[2..].parse::<usize>() {
                    Ok(n) if n >= 1 => { mode = SplitMode::Chunks; chunks = n; i += 1; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid number of chunks: '{}'\n", &arg[2..]), 1),
                }
            } else if arg == "-a" && i + 1 < args.len() {
                match args[i+1].parse::<usize>() {
                    Ok(n) if n >= 1 => { suffix_length = n; i += 2; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid suffix length: '{}'\n", args[i+1]), 1),
                }
            } else if arg.starts_with("-a") && arg.len() > 2 && arg[2..].chars().all(|c| c.is_ascii_digit()) {
                match arg[2..].parse::<usize>() {
                    Ok(n) if n >= 1 => { suffix_length = n; i += 1; }
                    _ => return CommandResult::with_exit_code("".into(), format!("split: invalid suffix length: '{}'\n", &arg[2..]), 1),
                }
            } else if arg == "-d" || arg == "--numeric-suffixes" {
                use_numeric = true; i += 1;
            } else if arg.starts_with("--additional-suffix=") {
                additional_suffix = arg["--additional-suffix=".len()..].to_string(); i += 1;
            } else if arg == "--additional-suffix" && i + 1 < args.len() {
                additional_suffix = args[i+1].clone(); i += 2;
            } else if arg == "--" {
                positional.extend(args[i+1..].iter().cloned()); break;
            } else if arg.starts_with("--") && arg.len() > 2 {
                return CommandResult::with_exit_code("".into(), format!("split: unrecognized option '{}'\n", arg), 1);
            } else if arg.starts_with('-') && arg != "-" && arg.len() == 2 {
                let ch = arg.chars().nth(1).unwrap();
                if !"lbnad".contains(ch) {
                    return CommandResult::with_exit_code("".into(), format!("split: invalid option -- '{}'\n", ch), 1);
                }
                i += 1;
            } else {
                positional.push(arg.clone()); i += 1;
            }
        }

        let input_file = positional.first().map(|s| s.as_str()).unwrap_or("-");
        let prefix = positional.get(1).map(|s| s.as_str()).unwrap_or("x");

        // Read input
        let content = if input_file == "-" {
            ctx.stdin.clone()
        } else {
            let path = resolve_path(&ctx.cwd, input_file);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::with_exit_code("".into(), format!("split: {}: No such file or directory\n", input_file), 1),
            }
        };

        if content.is_empty() {
            return CommandResult::success("".into());
        }

        let file_chunks: Vec<String> = match mode {
            SplitMode::Lines => split_by_lines(&content, lines),
            SplitMode::Bytes => split_by_bytes(&content, bytes),
            SplitMode::Chunks => split_into_chunks(&content, chunks),
        };

        for (idx, chunk) in file_chunks.iter().enumerate() {
            if chunk.is_empty() { continue; }
            let suffix = generate_suffix(idx, use_numeric, suffix_length);
            let filename = format!("{}{}{}", prefix, suffix, additional_suffix);
            let path = resolve_path(&ctx.cwd, &filename);
            let _ = ctx.fs.write_file(&path, chunk.as_bytes()).await;
        }

        CommandResult::success("".into())
    }
}

fn split_by_lines(content: &str, lines_per_file: usize) -> Vec<String> {
    let lines: Vec<&str> = content.split('\n').collect();
    let has_trailing = content.ends_with('\n') && lines.last() == Some(&"");
    let lines: Vec<&str> = if has_trailing { lines[..lines.len()-1].to_vec() } else { lines };
    let mut chunks = Vec::new();
    for chunk_lines in lines.chunks(lines_per_file) {
        let is_last = chunks.len() == (lines.len() + lines_per_file - 1) / lines_per_file - 1;
        let joined = chunk_lines.join("\n");
        let chunk = if is_last && !has_trailing { joined } else { format!("{}\n", joined) };
        chunks.push(chunk);
    }
    chunks
}

fn split_by_bytes(content: &str, bytes_per_file: usize) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut chunks = Vec::new();
    for chunk in bytes.chunks(bytes_per_file) {
        chunks.push(String::from_utf8_lossy(chunk).to_string());
    }
    chunks
}

fn split_into_chunks(content: &str, num_chunks: usize) -> Vec<String> {
    let bytes = content.as_bytes();
    let bytes_per_chunk = (bytes.len() + num_chunks - 1) / num_chunks;
    let mut chunks = Vec::new();
    for i in 0..num_chunks {
        let start = i * bytes_per_chunk;
        let end = std::cmp::min(start + bytes_per_chunk, bytes.len());
        if start < bytes.len() {
            chunks.push(String::from_utf8_lossy(&bytes[start..end]).to_string());
        } else {
            chunks.push(String::new());
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: stdin.into(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    #[tokio::test]
    async fn test_split_default_prefix() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "1\n2\n3\n".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-l", "1", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert_eq!(fs.read_file("/xaa").await.unwrap(), "1\n");
        assert_eq!(fs.read_file("/xab").await.unwrap(), "2\n");
        assert_eq!(fs.read_file("/xac").await.unwrap(), "3\n");
    }

    #[tokio::test]
    async fn test_split_custom_prefix() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "1\n2\n3\n".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-l", "1", "/test.txt", "part_"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert!(fs.read_file("/part_aa").await.is_ok());
    }

    #[tokio::test]
    async fn test_split_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["-l", "1"], "a\nb\nc\n", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert_eq!(fs.read_file("/xaa").await.unwrap(), "a\n");
    }

    #[tokio::test]
    async fn test_split_empty() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec![], "", fs)).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_split_by_bytes() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "abcdefghij".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-b", "4", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert_eq!(fs.read_file("/xaa").await.unwrap(), "abcd");
        assert_eq!(fs.read_file("/xab").await.unwrap(), "efgh");
        assert_eq!(fs.read_file("/xac").await.unwrap(), "ij");
    }

    #[tokio::test]
    async fn test_split_by_bytes_attached() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "abcdefghij".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-b5", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert_eq!(fs.read_file("/xaa").await.unwrap(), "abcde");
    }

    #[tokio::test]
    async fn test_split_chunks() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "abcdefghij".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-n", "2", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        let a = fs.read_file("/xaa").await.unwrap();
        let b = fs.read_file("/xab").await.unwrap();
        assert_eq!(format!("{}{}", a, b), "abcdefghij");
    }

    #[tokio::test]
    async fn test_split_numeric() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "1\n2\n3\n".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-d", "-l", "1", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert!(fs.read_file("/x00").await.is_ok());
        assert!(fs.read_file("/x01").await.is_ok());
    }

    #[tokio::test]
    async fn test_split_suffix_length() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "1\n2\n".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-a", "3", "-l", "1", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert!(fs.read_file("/xaaa").await.is_ok());
    }

    #[tokio::test]
    async fn test_split_additional_suffix() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "1\n2\n".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["--additional-suffix=.txt", "-l", "1", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert!(fs.read_file("/xaa.txt").await.is_ok());
    }

    #[tokio::test]
    async fn test_split_no_trailing_newline() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "line1\nline2".as_bytes()).await.unwrap();
        let r = SplitCommand.execute(make_ctx(vec!["-l", "1", "/test.txt"], "", fs.clone())).await;
        assert_eq!(r.exit_code, 0);
        assert_eq!(fs.read_file("/xab").await.unwrap(), "line2");
    }

    #[tokio::test]
    async fn test_split_invalid_lines() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["-l", "abc"], "", fs)).await;
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("invalid number of lines"));
    }

    #[tokio::test]
    async fn test_split_invalid_bytes() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["-b", "xyz"], "", fs)).await;
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("invalid number of bytes"));
    }

    #[tokio::test]
    async fn test_split_missing_file() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["/nonexistent"], "", fs)).await;
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.to_lowercase().contains("no such file"));
    }

    #[tokio::test]
    async fn test_split_help() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["--help"], "", fs)).await;
        assert_eq!(r.exit_code, 0);
        assert!(r.stdout.contains("split"));
        assert!(r.stdout.contains("Usage"));
    }

    #[tokio::test]
    async fn test_split_unknown_flag() {
        let fs = Arc::new(InMemoryFs::new());
        let r = SplitCommand.execute(make_ctx(vec!["-z"], "", fs)).await;
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("invalid option"));
    }
}
