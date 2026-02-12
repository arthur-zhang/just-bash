use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct StringsCommand;

const HELP: &str = "strings - print the sequences of printable characters in files

Usage: strings [OPTION]... [FILE]...

Options:
  -n MIN       Print sequences of at least MIN characters (default: 4)
  -t FORMAT    Print offset before each string (o=octal, x=hex, d=decimal)
  -a           Scan the entire file (default behavior)
  --help       Display this help and exit";

#[derive(Clone, Copy)]
enum OffsetFormat {
    Octal,
    Hex,
    Decimal,
}

fn is_printable(byte: u8) -> bool {
    (32..=126).contains(&byte) || byte == 9
}

fn format_offset(offset: usize, format: Option<OffsetFormat>) -> String {
    match format {
        None => String::new(),
        Some(OffsetFormat::Octal) => format!("{:>7o} ", offset),
        Some(OffsetFormat::Hex) => format!("{:>7x} ", offset),
        Some(OffsetFormat::Decimal) => format!("{:>7} ", offset),
    }
}

fn extract_strings(data: &[u8], min_length: usize, offset_format: Option<OffsetFormat>) -> Vec<String> {
    let mut results = Vec::new();
    let mut current_string = String::new();
    let mut string_start = 0;

    for (i, &byte) in data.iter().enumerate() {
        if is_printable(byte) {
            if current_string.is_empty() {
                string_start = i;
            }
            current_string.push(byte as char);
        } else {
            if current_string.len() >= min_length {
                let prefix = format_offset(string_start, offset_format);
                results.push(format!("{}{}", prefix, current_string));
            }
            current_string.clear();
        }
    }

    if current_string.len() >= min_length {
        let prefix = format_offset(string_start, offset_format);
        results.push(format!("{}{}", prefix, current_string));
    }

    results
}

#[async_trait]
impl Command for StringsCommand {
    fn name(&self) -> &'static str {
        "strings"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut min_length = 4usize;
        let mut offset_format: Option<OffsetFormat> = None;
        let mut files = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];

            if arg == "--help" {
                return CommandResult::success(format!("{}\n", HELP));
            } else if arg == "-n" && i + 1 < ctx.args.len() {
                match ctx.args[i + 1].parse::<usize>() {
                    Ok(n) if n >= 1 => min_length = n,
                    _ => {
                        return CommandResult::error(format!(
                            "strings: invalid minimum string length: '{}'\n",
                            ctx.args[i + 1]
                        ));
                    }
                }
                i += 2;
            } else if arg.starts_with("-n") && arg.len() > 2 {
                match arg[2..].parse::<usize>() {
                    Ok(n) if n >= 1 => min_length = n,
                    _ => {
                        return CommandResult::error(format!(
                            "strings: invalid minimum string length: '{}'\n",
                            &arg[2..]
                        ));
                    }
                }
                i += 1;
            } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit()) {
                match arg[1..].parse::<usize>() {
                    Ok(n) if n >= 1 => min_length = n,
                    _ => {
                        return CommandResult::error(format!(
                            "strings: invalid minimum string length: '{}'\n",
                            &arg[1..]
                        ));
                    }
                }
                i += 1;
            } else if arg == "-t" && i + 1 < ctx.args.len() {
                offset_format = match ctx.args[i + 1].as_str() {
                    "o" => Some(OffsetFormat::Octal),
                    "x" => Some(OffsetFormat::Hex),
                    "d" => Some(OffsetFormat::Decimal),
                    f => {
                        return CommandResult::error(format!("strings: invalid radix: '{}'\n", f));
                    }
                };
                i += 2;
            } else if arg.starts_with("-t") && arg.len() == 3 {
                offset_format = match &arg[2..3] {
                    "o" => Some(OffsetFormat::Octal),
                    "x" => Some(OffsetFormat::Hex),
                    "d" => Some(OffsetFormat::Decimal),
                    f => {
                        return CommandResult::error(format!("strings: invalid radix: '{}'\n", f));
                    }
                };
                i += 1;
            } else if arg == "-a" || arg == "--all" {
                i += 1;
            } else if arg == "-e" && i + 1 < ctx.args.len() {
                let enc = &ctx.args[i + 1];
                if enc != "s" && enc != "S" {
                    return CommandResult::error(format!("strings: invalid encoding: '{}'\n", enc));
                }
                i += 2;
            } else if arg == "--" {
                files.extend(ctx.args[i + 1..].iter().cloned());
                break;
            } else if arg.starts_with('-') && arg != "-" {
                return CommandResult::error(format!("strings: invalid option -- '{}'\n", &arg[1..]));
            } else {
                files.push(arg.clone());
                i += 1;
            }
        }

        let mut output = String::new();

        if files.is_empty() {
            let strings = extract_strings(ctx.stdin.as_bytes(), min_length, offset_format);
            if !strings.is_empty() {
                output = format!("{}\n", strings.join("\n"));
            }
        } else {
            for file in &files {
                let content = if file == "-" {
                    ctx.stdin.clone()
                } else {
                    let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&file_path).await {
                        Ok(c) => c,
                        Err(_) => {
                            return CommandResult::with_exit_code(
                                output,
                                format!("strings: {}: No such file or directory\n", file),
                                1,
                            );
                        }
                    }
                };
                let strings = extract_strings(content.as_bytes(), min_length, offset_format);
                if !strings.is_empty() {
                    output.push_str(&format!("{}\n", strings.join("\n")));
                }
            }
        }

        CommandResult::success(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_strings_basic() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: "hello\x00world\x00test".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = StringsCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
        assert!(result.stdout.contains("world"));
    }

    #[tokio::test]
    async fn test_strings_min_length() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-n8".to_string()],
            stdin: "hello\x00worldtest".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = StringsCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.contains("hello"));
        assert!(result.stdout.contains("worldtest"));
    }
}
