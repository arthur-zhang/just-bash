use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct OdCommand;

const HELP: &str = "od - dump files in octal and other formats

Usage: od [OPTION]... [FILE]...

Options:
  -c           print as characters
  -A RADIX     output offset radix (d, o, x, n for none)
  -t TYPE      select output format (x1 for hex, c for char, o for octal)
  --help       display this help and exit";

#[derive(Clone, Copy, PartialEq)]
enum OutputFormat {
    Octal,
    Hex,
    Char,
}

fn format_char_byte(code: u8) -> String {
    match code {
        0 => "  \\0".to_string(),
        7 => "  \\a".to_string(),
        8 => "  \\b".to_string(),
        9 => "  \\t".to_string(),
        10 => "  \\n".to_string(),
        11 => "  \\v".to_string(),
        12 => "  \\f".to_string(),
        13 => "  \\r".to_string(),
        32..=126 => format!("   {}", code as char),
        _ => format!(" {:03o}", code),
    }
}

fn format_hex_byte(code: u8, has_char: bool) -> String {
    if has_char {
        format!("  {:02x}", code)
    } else {
        format!(" {:02x}", code)
    }
}

fn format_octal_byte(code: u8) -> String {
    format!(" {:03o}", code)
}

#[async_trait]
impl Command for OdCommand {
    fn name(&self) -> &'static str {
        "od"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut address_mode = true;
        let mut formats: Vec<OutputFormat> = Vec::new();
        let mut files = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-c" => {
                    formats.push(OutputFormat::Char);
                    i += 1;
                }
                "-An" => {
                    address_mode = false;
                    i += 1;
                }
                "-A" => {
                    i += 1;
                    if i < ctx.args.len() && ctx.args[i] == "n" {
                        address_mode = false;
                    }
                    i += 1;
                }
                "-t" => {
                    i += 1;
                    if i < ctx.args.len() {
                        let fmt = &ctx.args[i];
                        if fmt == "x1" {
                            formats.push(OutputFormat::Hex);
                        } else if fmt == "c" {
                            formats.push(OutputFormat::Char);
                        } else if fmt.starts_with('o') {
                            formats.push(OutputFormat::Octal);
                        }
                    }
                    i += 1;
                }
                s if !s.starts_with('-') || s == "-" => {
                    files.push(arg.clone());
                    i += 1;
                }
                _ => i += 1,
            }
        }

        if formats.is_empty() {
            formats.push(OutputFormat::Octal);
        }

        let input = if !files.is_empty() && files[0] != "-" {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[0]);
            match ctx.fs.read_file(&path).await {
                Ok(content) => content,
                Err(_) => {
                    return CommandResult::error(format!(
                        "od: {}: No such file or directory\n",
                        files[0]
                    ));
                }
            }
        } else {
            ctx.stdin.clone()
        };

        let bytes: Vec<u8> = input.bytes().collect();
        let has_char = formats.contains(&OutputFormat::Char);
        let bytes_per_line = 16;
        let mut lines = Vec::new();

        for offset in (0..bytes.len()).step_by(bytes_per_line) {
            let chunk: Vec<u8> = bytes[offset..].iter().take(bytes_per_line).copied().collect();

            for (fmt_idx, fmt) in formats.iter().enumerate() {
                let formatted: Vec<String> = match fmt {
                    OutputFormat::Char => chunk.iter().map(|&b| format_char_byte(b)).collect(),
                    OutputFormat::Hex => chunk.iter().map(|&b| format_hex_byte(b, has_char)).collect(),
                    OutputFormat::Octal => chunk.iter().map(|&b| format_octal_byte(b)).collect(),
                };

                let prefix = if fmt_idx == 0 && address_mode {
                    format!("{:07o} ", offset)
                } else if fmt_idx > 0 || !address_mode {
                    if address_mode { "        ".to_string() } else { String::new() }
                } else {
                    String::new()
                };

                lines.push(format!("{}{}", prefix, formatted.join("")));
            }
        }

        if address_mode && !bytes.is_empty() {
            lines.push(format!("{:07o}", bytes.len()));
        }

        if lines.is_empty() {
            CommandResult::success(String::new())
        } else {
            CommandResult::success(format!("{}\n", lines.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    fn create_ctx(args: Vec<&str>) -> CommandContext {
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

    #[tokio::test]
    async fn test_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = OdCommand.execute(ctx).await;
        assert!(result.stdout.contains("od"));
        assert!(result.stdout.contains("-c"));
    }

    #[tokio::test]
    async fn test_octal_output() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "AB".to_string();
        let result = OdCommand.execute(ctx).await;
        assert!(result.stdout.contains("101"));
        assert!(result.stdout.contains("102"));
    }

    #[tokio::test]
    async fn test_char_output() {
        let mut ctx = create_ctx(vec!["-c"]);
        ctx.stdin = "AB".to_string();
        let result = OdCommand.execute(ctx).await;
        assert!(result.stdout.contains("A"));
        assert!(result.stdout.contains("B"));
    }

    #[tokio::test]
    async fn test_hex_output() {
        let mut ctx = create_ctx(vec!["-t", "x1"]);
        ctx.stdin = "AB".to_string();
        let result = OdCommand.execute(ctx).await;
        assert!(result.stdout.contains("41"));
        assert!(result.stdout.contains("42"));
    }

    #[test]
    fn test_format_char_byte() {
        assert!(format_char_byte(b'A').contains("A"));
        assert!(format_char_byte(b'\n').contains("\\n"));
        assert!(format_char_byte(0).contains("\\0"));
    }

    #[test]
    fn test_format_octal_byte() {
        assert_eq!(format_octal_byte(65).trim(), "101");
    }
}
