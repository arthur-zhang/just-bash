// src/commands/nl/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct NlCommand;

/// Body numbering style
enum BodyStyle {
    All,       // a: number all lines
    NonEmpty,  // t: number non-empty lines only (default)
    None,      // n: number no lines
}

/// Number format
enum NumberFormat {
    LeftJustified,   // ln
    RightSpaces,     // rn (default)
    RightZeros,      // rz
}

struct NlOptions {
    body_style: BodyStyle,
    number_format: NumberFormat,
    width: usize,
    separator: String,
    start: i64,
    increment: i64,
}

impl Default for NlOptions {
    fn default() -> Self {
        Self {
            body_style: BodyStyle::NonEmpty,
            number_format: NumberFormat::RightSpaces,
            width: 6,
            separator: "\t".to_string(),
            start: 1,
            increment: 1,
        }
    }
}

fn format_number(num: i64, fmt: &NumberFormat, width: usize) -> String {
    match fmt {
        NumberFormat::RightSpaces => format!("{:>width$}", num, width = width),
        NumberFormat::LeftJustified => format!("{:<width$}", num, width = width),
        NumberFormat::RightZeros => format!("{:0>width$}", num, width = width),
    }
}

fn parse_options(args: &[String]) -> Result<(NlOptions, Vec<String>), String> {
    let mut opts = NlOptions::default();
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        if arg == "--help" {
            return Err("help".to_string());
        }

        if arg.starts_with('-') && arg.len() > 1 && !arg.starts_with("--") {
            let flag = &arg[1..2];
            // Value can be attached (-bSTYLE) or separate (-b STYLE)
            let value = if arg.len() > 2 {
                arg[2..].to_string()
            } else if i + 1 < args.len() {
                i += 1;
                args[i].clone()
            } else {
                return Err(format!("nl: option requires an argument -- '{}'", flag));
            };

            match flag {
                "b" => {
                    opts.body_style = match value.as_str() {
                        "a" => BodyStyle::All,
                        "t" => BodyStyle::NonEmpty,
                        "n" => BodyStyle::None,
                        _ => return Err(format!("nl: invalid body numbering style: '{}'", value)),
                    };
                }
                "n" => {
                    opts.number_format = match value.as_str() {
                        "ln" => NumberFormat::LeftJustified,
                        "rn" => NumberFormat::RightSpaces,
                        "rz" => NumberFormat::RightZeros,
                        _ => return Err(format!("nl: invalid line numbering format: '{}'", value)),
                    };
                }
                "w" => {
                    opts.width = value.parse::<usize>().map_err(|_| {
                        format!("nl: invalid line number field width: '{}'", value)
                    })?;
                }
                "s" => {
                    opts.separator = value;
                }
                "v" => {
                    opts.start = value.parse::<i64>().map_err(|_| {
                        format!("nl: invalid starting line number: '{}'", value)
                    })?;
                }
                "i" => {
                    opts.increment = value.parse::<i64>().map_err(|_| {
                        format!("nl: invalid line number increment: '{}'", value)
                    })?;
                }
                _ => {
                    return Err(format!("nl: invalid option -- '{}'", flag));
                }
            }
        } else {
            files.push(arg.clone());
        }

        i += 1;
    }

    Ok((opts, files))
}

const HELP_TEXT: &str = "\
Usage: nl [OPTION]... [FILE]...

Write each FILE to standard output, with line numbers added.

Options:
  -b STYLE   use STYLE for numbering body lines (a=all, t=non-empty, n=none)
  -n FORMAT  use FORMAT for line numbers (ln=left, rn=right, rz=right-zeros)
  -w WIDTH   use WIDTH columns for line numbers (default 6)
  -s SEP     use SEP as separator after number (default TAB)
  -v START   first line number (default 1)
  -i INCR    line number increment (default 1)
      --help display this help and exit
";

#[async_trait]
impl Command for NlCommand {
    fn name(&self) -> &'static str {
        "nl"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let (opts, files) = match parse_options(&ctx.args) {
            Ok(v) => v,
            Err(e) if e == "help" => {
                return CommandResult::success(HELP_TEXT.to_string());
            }
            Err(e) => {
                return CommandResult::error(format!("{}\n", e));
            }
        };

        // Collect input from files or stdin
        let mut inputs: Vec<String> = Vec::new();

        if files.is_empty() || (files.len() == 1 && files[0] == "-") {
            inputs.push(ctx.stdin.clone());
        } else {
            for file in &files {
                if file == "-" {
                    inputs.push(ctx.stdin.clone());
                } else {
                    let path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&path).await {
                        Ok(content) => inputs.push(content),
                        Err(_) => {
                            return CommandResult::error(format!(
                                "nl: {}: No such file or directory\n",
                                file
                            ));
                        }
                    }
                }
            }
        }

        let combined: String = inputs.join("");
        if combined.is_empty() {
            return CommandResult::success(String::new());
        }

        let lines: Vec<&str> = combined.lines().collect();
        let mut output = String::new();
        let mut line_num = opts.start;

        for line in &lines {
            let should_number = match opts.body_style {
                BodyStyle::All => true,
                BodyStyle::NonEmpty => !line.is_empty(),
                BodyStyle::None => false,
            };

            if should_number {
                let num_str = format_number(line_num, &opts.number_format, opts.width);
                output.push_str(&num_str);
                output.push_str(&opts.separator);
                output.push_str(line);
                output.push('\n');
                line_num += opts.increment;
            } else {
                // Empty/unnumbered line: pad with spaces for width + separator
                let padding = " ".repeat(opts.width);
                output.push_str(&padding);
                output.push_str(&opts.separator);
                output.push_str(line);
                output.push('\n');
            }
        }

        // If original input ended with newline, we already have it from lines iteration
        // If not, we still added newlines per line above which is correct for nl behavior

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
    async fn test_nl_stdin() {
        let ctx = make_ctx(vec!["-ba"], "hello\nworld\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("hello"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("world"));
    }

    #[tokio::test]
    async fn test_nl_file() {
        let ctx = make_ctx(
            vec!["-ba", "/test.txt"],
            "",
            vec![("/test.txt", "alpha\nbeta\n")],
        )
        .await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("alpha"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("beta"));
    }

    #[tokio::test]
    async fn test_nl_skip_empty_default() {
        // Default -bt: skip empty lines
        let ctx = make_ctx(vec![], "a\n\nb\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert_eq!(lines.len(), 3);
        // First line should have number 1 and "a"
        assert!(lines[0].contains("1"));
        assert!(lines[0].contains("a"));
        // Second line (empty) should NOT have a number
        assert!(!lines[1].contains("2"));
        // Third line should have number 2 and "b"
        assert!(lines[2].contains("2"));
        assert!(lines[2].contains("b"));
    }

    #[tokio::test]
    async fn test_nl_all_lines_ba() {
        let ctx = make_ctx(vec!["-ba"], "a\n\nb\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("1"));
        assert!(lines[1].contains("2"));
        assert!(lines[2].contains("3"));
    }

    #[tokio::test]
    async fn test_nl_no_lines_bn() {
        let ctx = make_ctx(vec!["-bn"], "a\nb\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        // No line should contain a digit (no numbering)
        for line in &lines {
            assert!(!line.chars().any(|c| c.is_ascii_digit()));
        }
    }

    #[tokio::test]
    async fn test_nl_left_justify_ln() {
        let ctx = make_ctx(vec!["-ba", "-nln"], "a\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Left justified: "1     \ta"
        assert!(result.stdout.starts_with("1"));
    }

    #[tokio::test]
    async fn test_nl_right_zeros_rz() {
        let ctx = make_ctx(vec!["-ba", "-nrz"], "a\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("000001"));
    }

    #[tokio::test]
    async fn test_nl_width() {
        let ctx = make_ctx(vec!["-ba", "-w3"], "a\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Width 3, right justified: "  1\ta"
        assert!(result.stdout.starts_with("  1\t"));
    }

    #[tokio::test]
    async fn test_nl_separator() {
        let ctx = make_ctx(vec!["-ba", "-s:"], "a\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains(":a"));
    }

    #[tokio::test]
    async fn test_nl_start_number() {
        let ctx = make_ctx(vec!["-ba", "-v10"], "a\nb\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("10"));
        assert!(result.stdout.contains("11"));
    }

    #[tokio::test]
    async fn test_nl_increment() {
        let ctx = make_ctx(vec!["-ba", "-i5"], "a\nb\nc\n", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert!(lines[0].contains("1"));
        assert!(lines[1].contains("6"));
        assert!(lines[2].contains("11"));
    }

    #[tokio::test]
    async fn test_nl_empty_input() {
        let ctx = make_ctx(vec![], "", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_nl_file_not_found() {
        let ctx = make_ctx(vec!["/nonexistent.txt"], "", vec![]).await;
        let result = NlCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }
}
