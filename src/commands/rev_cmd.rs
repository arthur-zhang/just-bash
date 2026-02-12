use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct RevCommand;

fn reverse_string(s: &str) -> String {
    s.chars().rev().collect()
}

fn process_content(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let has_trailing_newline = content.ends_with('\n') && lines.last() == Some(&"");

    let lines_to_process: Vec<&str> = if has_trailing_newline {
        lines[..lines.len() - 1].to_vec()
    } else {
        lines
    };

    let reversed: Vec<String> = lines_to_process.iter().map(|l| reverse_string(l)).collect();

    if has_trailing_newline {
        format!("{}\n", reversed.join("\n"))
    } else {
        reversed.join("\n")
    }
}

#[async_trait]
impl Command for RevCommand {
    fn name(&self) -> &'static str {
        "rev"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "rev - reverse lines characterwise\n\nUsage: rev [file ...]\n\nCopies the specified files to standard output, reversing the order of characters in every line.\n".to_string()
            );
        }

        let mut files = Vec::new();
        let mut after_dashdash = false;

        for arg in &ctx.args {
            if after_dashdash {
                files.push(arg.clone());
            } else if arg == "--" {
                after_dashdash = true;
            } else if arg.starts_with('-') && arg != "-" {
                return CommandResult::error(format!("rev: invalid option -- '{}'\n", &arg[1..]));
            } else {
                files.push(arg.clone());
            }
        }

        let mut output = String::new();

        if files.is_empty() {
            output = process_content(&ctx.stdin);
        } else {
            for file in &files {
                if file == "-" {
                    output.push_str(&process_content(&ctx.stdin));
                } else {
                    let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&file_path).await {
                        Ok(content) => {
                            output.push_str(&process_content(&content));
                        }
                        Err(_) => {
                            return CommandResult::with_exit_code(
                                output,
                                format!("rev: {}: No such file or directory\n", file),
                                1,
                            );
                        }
                    }
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
    async fn test_rev_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: "hello\nworld\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = RevCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "olleh\ndlrow\n");
    }

    #[tokio::test]
    async fn test_rev_no_trailing_newline() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: "hello".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = RevCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "olleh");
    }

    #[test]
    fn test_reverse_string() {
        assert_eq!(reverse_string("hello"), "olleh");
        assert_eq!(reverse_string(""), "");
        assert_eq!(reverse_string("a"), "a");
    }
}
