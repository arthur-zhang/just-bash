// src/commands/echo/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct EchoCommand;

#[async_trait]
impl Command for EchoCommand {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        let mut no_newline = false;
        // When xpg_echo is enabled, interpret escapes by default (like echo -e)
        let mut interpret_escapes = false;
        let mut start_index = 0;

        // Parse flags
        while start_index < args.len() {
            let arg = &args[start_index];
            match arg.as_str() {
                "-n" => {
                    no_newline = true;
                    start_index += 1;
                }
                "-e" => {
                    interpret_escapes = true;
                    start_index += 1;
                }
                "-E" => {
                    interpret_escapes = false;
                    start_index += 1;
                }
                "-ne" | "-en" => {
                    no_newline = true;
                    interpret_escapes = true;
                    start_index += 1;
                }
                _ => break,
            }
        }

        let mut output: String = args[start_index..].join(" ");

        if interpret_escapes {
            let result = process_escapes(&output);
            output = result.output;
            if result.stop {
                // \c encountered - suppress newline and stop
                return CommandResult::success(output);
            }
        }

        if !no_newline {
            output.push('\n');
        }

        CommandResult::success(output)
    }
}

/// Result of processing escape sequences
struct EscapeResult {
    output: String,
    stop: bool,
}

/// Process echo -e escape sequences
fn process_escapes(input: &str) -> EscapeResult {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' {
            if i + 1 >= chars.len() {
                result.push('\\');
                break;
            }

            let next = chars[i + 1];

            match next {
                '\\' => {
                    result.push('\\');
                    i += 2;
                }
                'n' => {
                    result.push('\n');
                    i += 2;
                }
                't' => {
                    result.push('\t');
                    i += 2;
                }
                'r' => {
                    result.push('\r');
                    i += 2;
                }
                'a' => {
                    result.push('\x07');
                    i += 2;
                }
                'b' => {
                    result.push('\x08');
                    i += 2;
                }
                'f' => {
                    result.push('\x0c');
                    i += 2;
                }
                'v' => {
                    result.push('\x0b');
                    i += 2;
                }
                'e' | 'E' => {
                    result.push('\x1b');
                    i += 2;
                }
                'c' => {
                    // \c stops output and suppresses trailing newline
                    return EscapeResult {
                        output: result,
                        stop: true,
                    };
                }
                '0' => {
                    // \0NNN - octal (up to 3 digits after the 0)
                    let mut octal = String::new();
                    let mut j = i + 2;
                    while j < chars.len() && j < i + 5 && chars[j] >= '0' && chars[j] <= '7' {
                        octal.push(chars[j]);
                        j += 1;
                    }
                    if octal.is_empty() {
                        // \0 alone is NUL
                        result.push('\0');
                    } else {
                        let code = u32::from_str_radix(&octal, 8).unwrap_or(0) % 256;
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                        }
                    }
                    i = j;
                }
                'x' => {
                    // \xHH - hex (1-2 hex digits)
                    let mut hex = String::new();
                    let mut j = i + 2;
                    while j < chars.len() && j < i + 4 && chars[j].is_ascii_hexdigit() {
                        hex.push(chars[j]);
                        j += 1;
                    }
                    if hex.is_empty() {
                        // \x with no valid hex digits - output literally
                        result.push('\\');
                        result.push('x');
                        i += 2;
                    } else {
                        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                        }
                        i = j;
                    }
                }
                'u' => {
                    // \uHHHH - 4-digit unicode
                    let mut hex = String::new();
                    let mut j = i + 2;
                    while j < chars.len() && j < i + 6 && chars[j].is_ascii_hexdigit() {
                        hex.push(chars[j]);
                        j += 1;
                    }
                    if hex.is_empty() {
                        result.push('\\');
                        result.push('u');
                        i += 2;
                    } else {
                        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                        }
                        i = j;
                    }
                }
                'U' => {
                    // \UHHHHHHHH - 8-digit unicode
                    let mut hex = String::new();
                    let mut j = i + 2;
                    while j < chars.len() && j < i + 10 && chars[j].is_ascii_hexdigit() {
                        hex.push(chars[j]);
                        j += 1;
                    }
                    if hex.is_empty() {
                        result.push('\\');
                        result.push('U');
                        i += 2;
                    } else {
                        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        match char::from_u32(code) {
                            Some(c) => result.push(c),
                            None => {
                                // Invalid code point, output as-is
                                result.push('\\');
                                result.push('U');
                                result.push_str(&hex);
                            }
                        }
                        i = j;
                    }
                }
                _ => {
                    // Unknown escape - keep the backslash and character
                    result.push('\\');
                    result.push(next);
                    i += 2;
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    EscapeResult {
        output: result,
        stop: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
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
    async fn test_echo_simple_text() {
        let ctx = make_ctx(vec!["hello", "world"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello world\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_echo_empty() {
        let ctx = make_ctx(vec![]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "\n");
    }

    #[tokio::test]
    async fn test_echo_multiple_args() {
        let ctx = make_ctx(vec!["one", "two", "three"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "one two three\n");
    }

    #[tokio::test]
    async fn test_echo_n_flag() {
        let ctx = make_ctx(vec!["-n", "hello"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello");
    }

    #[tokio::test]
    async fn test_echo_e_flag_newline() {
        let ctx = make_ctx(vec!["-e", "hello\\nworld"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\nworld\n");
    }

    #[tokio::test]
    async fn test_echo_e_flag_tab() {
        let ctx = make_ctx(vec!["-e", "col1\\tcol2"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "col1\tcol2\n");
    }

    #[tokio::test]
    async fn test_echo_e_flag_carriage_return() {
        let ctx = make_ctx(vec!["-e", "hello\\rworld"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\rworld\n");
    }

    #[tokio::test]
    async fn test_echo_combined_en_flags() {
        let ctx = make_ctx(vec!["-en", "hello\\nworld"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\nworld");
    }

    #[tokio::test]
    async fn test_echo_combined_ne_flags() {
        let ctx = make_ctx(vec!["-ne", "a\\tb"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\tb");
    }

    #[tokio::test]
    async fn test_echo_e_disable() {
        let ctx = make_ctx(vec!["-E", "hello\\nworld"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\\nworld\n");
    }

    #[tokio::test]
    async fn test_echo_multiple_escapes() {
        let ctx = make_ctx(vec!["-e", "a\\nb\\nc"]);
        let cmd = EchoCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\nc\n");
    }
}
