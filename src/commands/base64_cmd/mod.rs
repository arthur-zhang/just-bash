// src/commands/base64_cmd/mod.rs
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct Base64Command;

/// Read binary data from files or stdin.
/// Returns concatenated bytes from all sources.
async fn read_input(ctx: &CommandContext, files: &[String]) -> Result<Vec<u8>, CommandResult> {
    // No files or single "-" means read from stdin
    if files.is_empty() || (files.len() == 1 && files[0] == "-") {
        return Ok(ctx.stdin.as_bytes().to_vec());
    }

    let mut result = Vec::new();
    for file in files {
        if file == "-" {
            result.extend_from_slice(ctx.stdin.as_bytes());
            continue;
        }
        let path = ctx.fs.resolve_path(&ctx.cwd, file);
        match ctx.fs.read_file_buffer(&path).await {
            Ok(data) => result.extend_from_slice(&data),
            Err(_) => {
                return Err(CommandResult::error(format!(
                    "base64: {}: No such file or directory\n",
                    file
                )));
            }
        }
    }
    Ok(result)
}

#[async_trait]
impl Command for Base64Command {
    fn name(&self) -> &'static str {
        "base64"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        // Check --help
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: base64 [OPTION]... [FILE]\n\
                 base64 encode/decode data and print to standard output.\n\n\
                 Options:\n\
                   -d, --decode    decode data\n\
                   -w, --wrap=COLS wrap encoded lines after COLS character (default 76, 0 to disable)\n\
                       --help      display this help and exit\n"
                    .to_string(),
            );
        }

        // Parse arguments
        let mut decode = false;
        let mut wrap_cols: usize = 76;
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "-d" | "--decode" => decode = true,
                "-w" => {
                    if i + 1 < args.len() {
                        i += 1;
                        match args[i].parse::<usize>() {
                            Ok(n) => wrap_cols = n,
                            Err(_) => {
                                return CommandResult::error(format!(
                                    "base64: invalid wrap size: '{}'\n",
                                    args[i]
                                ));
                            }
                        }
                    }
                }
                _ if arg.starts_with("--wrap=") => {
                    let val = &arg["--wrap=".len()..];
                    match val.parse::<usize>() {
                        Ok(n) => wrap_cols = n,
                        Err(_) => {
                            return CommandResult::error(format!(
                                "base64: invalid wrap size: '{}'\n",
                                val
                            ));
                        }
                    }
                }
                _ => {
                    files.push(arg.clone());
                }
            }
            i += 1;
        }

        let data = match read_input(&ctx, &files).await {
            Ok(d) => d,
            Err(e) => return e,
        };

        if decode {
            // Decode: strip whitespace, then decode base64
            let input = String::from_utf8_lossy(&data);
            let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
            match STANDARD.decode(&cleaned) {
                Ok(decoded) => {
                    // Output decoded bytes as a string (lossy for binary)
                    let output = String::from_utf8_lossy(&decoded).to_string();
                    CommandResult::success(output)
                }
                Err(_) => CommandResult::error("base64: invalid input\n".to_string()),
            }
        } else {
            // Encode
            let mut encoded = STANDARD.encode(&data);

            if wrap_cols > 0 && !encoded.is_empty() {
                let mut wrapped = String::new();
                let chars: Vec<char> = encoded.chars().collect();
                for chunk in chars.chunks(wrap_cols) {
                    let line: String = chunk.iter().collect();
                    wrapped.push_str(&line);
                    wrapped.push('\n');
                }
                encoded = wrapped;
            }

            CommandResult::success(encoded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>) -> CommandContext {
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

    fn make_ctx_with_stdin(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    fn make_ctx_with_fs(args: Vec<&str>, fs: Arc<InMemoryFs>) -> CommandContext {
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

    fn make_ctx_with_stdin_and_fs(args: Vec<&str>, stdin: &str, fs: Arc<InMemoryFs>) -> CommandContext {
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
    async fn test_encode_simple_string() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec![], "Hello, World!");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // "Hello, World!" -> "SGVsbG8sIFdvcmxkIQ=="
        assert_eq!(result.stdout, "SGVsbG8sIFdvcmxkIQ==\n");
    }

    #[tokio::test]
    async fn test_encode_with_default_wrap() {
        let cmd = Base64Command;
        // Create a string long enough to exceed 76 chars when encoded
        let long_input = "A".repeat(60); // 60 bytes -> 80 base64 chars
        let ctx = make_ctx_with_stdin(vec![], &long_input);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.trim_end_matches('\n').split('\n').collect();
        // First line should be exactly 76 chars
        assert_eq!(lines[0].len(), 76);
        // Should have 2 lines (80 chars wrapped at 76)
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn test_encode_with_no_wrap() {
        let cmd = Base64Command;
        let long_input = "A".repeat(60);
        let ctx = make_ctx_with_stdin(vec!["-w", "0"], &long_input);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // With -w 0, no wrapping, no trailing newline from wrapping
        assert!(!result.stdout.contains('\n'));
        let expected = STANDARD.encode("A".repeat(60).as_bytes());
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_encode_with_custom_wrap() {
        let cmd = Base64Command;
        let input = "Hello, World!"; // encodes to "SGVsbG8sIFdvcmxkIQ==" (20 chars)
        let ctx = make_ctx_with_stdin(vec!["-w", "10"], input);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines[0].len(), 10);
        assert_eq!(lines[1].len(), 10);
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn test_decode_valid_base64() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec!["-d"], "SGVsbG8sIFdvcmxkIQ==");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Hello, World!");
    }

    #[tokio::test]
    async fn test_decode_with_whitespace() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec!["--decode"], "SGVsbG8s\nIFdvcmxk\nIQ==\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Hello, World!");
    }

    #[tokio::test]
    async fn test_decode_invalid_base64() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec!["-d"], "!!!invalid!!!");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stderr, "base64: invalid input\n");
    }

    #[tokio::test]
    async fn test_read_from_stdin() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec!["-"], "test data");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let expected = STANDARD.encode(b"test data");
        // Default wrap at 76, "test data" encodes to 12 chars so single line + newline
        assert_eq!(result.stdout, format!("{}\n", expected));
    }

    #[tokio::test]
    async fn test_read_from_file() {
        let cmd = Base64Command;
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", b"file content").await.unwrap();
        let ctx = make_ctx_with_fs(vec!["/test.txt"], fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let expected = STANDARD.encode(b"file content");
        assert_eq!(result.stdout, format!("{}\n", expected));
    }

    #[tokio::test]
    async fn test_read_from_multiple_files() {
        let cmd = Base64Command;
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"hello").await.unwrap();
        fs.write_file("/b.txt", b"world").await.unwrap();
        let ctx = make_ctx_with_fs(vec!["/a.txt", "/b.txt"], fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let expected = STANDARD.encode(b"helloworld");
        assert_eq!(result.stdout, format!("{}\n", expected));
    }

    #[tokio::test]
    async fn test_binary_round_trip() {
        let cmd = Base64Command;
        // Use ASCII-safe binary data for round-trip (avoids UTF-8 lossy issues)
        let binary_data = b"Hello\x00World\x01\x02\x03test";
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/binary.dat", binary_data).await.unwrap();

        // Encode
        let ctx = make_ctx_with_fs(vec!["-w", "0", "/binary.dat"], fs.clone());
        let encode_result = cmd.execute(ctx).await;
        assert_eq!(encode_result.exit_code, 0);

        // Verify the encoded output is valid base64 that decodes back
        let decoded = STANDARD.decode(&encode_result.stdout).unwrap();
        assert_eq!(decoded, binary_data);
    }

    #[tokio::test]
    async fn test_empty_input() {
        let cmd = Base64Command;
        let ctx = make_ctx_with_stdin(vec![], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Empty input encodes to empty string
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let cmd = Base64Command;
        let ctx = make_ctx(vec!["/nonexistent.txt"]);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_wrap_with_long_flag() {
        let cmd = Base64Command;
        let input = "Hello, World!"; // 20 base64 chars
        let ctx = make_ctx_with_stdin(vec!["--wrap=10"], input);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines[0].len(), 10);
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn test_stdin_with_dash_and_file() {
        let cmd = Base64Command;
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"world").await.unwrap();
        let ctx = make_ctx_with_stdin_and_fs(vec!["-", "/a.txt"], "hello", fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let expected = STANDARD.encode(b"helloworld");
        assert_eq!(result.stdout, format!("{}\n", expected));
    }
}
