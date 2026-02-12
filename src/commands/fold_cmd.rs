use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct FoldCommand;

const HELP: &str = "fold - wrap each input line to fit in specified width

Usage: fold [OPTION]... [FILE]...

Options:
  -w WIDTH    Use WIDTH columns instead of 80
  -s          Break at spaces
  -b          Count bytes rather than columns
  --help      Display this help and exit";

fn get_char_width(c: char, current_column: usize, count_bytes: bool) -> i32 {
    if count_bytes {
        return c.len_utf8() as i32;
    }

    match c {
        '\t' => (8 - (current_column % 8)) as i32,
        '\x08' => -1, // backspace
        _ => 1,
    }
}

fn fold_line(line: &str, width: usize, break_at_spaces: bool, count_bytes: bool) -> String {
    if line.is_empty() {
        return String::new();
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_column: usize = 0;
    let mut last_space_index: Option<usize> = None;
    let mut last_space_column: usize = 0;

    for c in line.chars() {
        let char_width = get_char_width(c, current_column, count_bytes);

        if current_column as i32 + char_width > width as i32 && !current_line.is_empty() {
            if break_at_spaces && last_space_index.is_some() {
                let idx = last_space_index.unwrap();
                result.push(current_line[..=idx].to_string());
                current_line = format!("{}{}", &current_line[idx + 1..], c);
                current_column = current_column - last_space_column - 1 + char_width.max(0) as usize;
                last_space_index = None;
                last_space_column = 0;
            } else {
                result.push(current_line.clone());
                current_line = c.to_string();
                current_column = char_width.max(0) as usize;
                last_space_index = None;
                last_space_column = 0;
            }
        } else {
            current_line.push(c);
            current_column = (current_column as i32 + char_width).max(0) as usize;

            if c == ' ' || c == '\t' {
                last_space_index = Some(current_line.len() - 1);
                last_space_column = current_column - char_width.max(0) as usize;
            }
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    result.join("\n")
}

fn process_content(content: &str, width: usize, break_at_spaces: bool, count_bytes: bool) -> String {
    if content.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = content.split('\n').collect();
    let has_trailing_newline = content.ends_with('\n') && lines.last() == Some(&"");

    let lines_to_process: Vec<&str> = if has_trailing_newline {
        lines[..lines.len() - 1].to_vec()
    } else {
        lines
    };

    let folded: Vec<String> = lines_to_process
        .iter()
        .map(|line| fold_line(line, width, break_at_spaces, count_bytes))
        .collect();

    if has_trailing_newline {
        format!("{}\n", folded.join("\n"))
    } else {
        folded.join("\n")
    }
}

#[async_trait]
impl Command for FoldCommand {
    fn name(&self) -> &'static str {
        "fold"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut width = 80usize;
        let mut break_at_spaces = false;
        let mut count_bytes = false;
        let mut files = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];

            if arg == "--help" {
                return CommandResult::success(format!("{}\n", HELP));
            } else if arg == "-w" && i + 1 < ctx.args.len() {
                match ctx.args[i + 1].parse::<usize>() {
                    Ok(w) if w >= 1 => width = w,
                    _ => {
                        return CommandResult::error(format!(
                            "fold: invalid number of columns: '{}'\n",
                            ctx.args[i + 1]
                        ));
                    }
                }
                i += 2;
            } else if arg.starts_with("-w") && arg.len() > 2 {
                match arg[2..].parse::<usize>() {
                    Ok(w) if w >= 1 => width = w,
                    _ => {
                        return CommandResult::error(format!(
                            "fold: invalid number of columns: '{}'\n",
                            &arg[2..]
                        ));
                    }
                }
                i += 1;
            } else if arg == "-s" {
                break_at_spaces = true;
                i += 1;
            } else if arg == "-b" {
                count_bytes = true;
                i += 1;
            } else if arg == "-bs" || arg == "-sb" {
                break_at_spaces = true;
                count_bytes = true;
                i += 1;
            } else if arg == "--" {
                files.extend(ctx.args[i + 1..].iter().cloned());
                break;
            } else if arg.starts_with('-') && arg != "-" {
                let flags = &arg[1..];
                let mut chars = flags.chars().peekable();
                while let Some(c) = chars.next() {
                    match c {
                        's' => break_at_spaces = true,
                        'b' => count_bytes = true,
                        'w' => {
                            let rest: String = chars.collect();
                            if rest.is_empty() {
                                if i + 1 < ctx.args.len() {
                                    match ctx.args[i + 1].parse::<usize>() {
                                        Ok(w) if w >= 1 => {
                                            width = w;
                                            i += 1;
                                        }
                                        _ => {
                                            return CommandResult::error(format!(
                                                "fold: invalid number of columns: '{}'\n",
                                                ctx.args[i + 1]
                                            ));
                                        }
                                    }
                                }
                            } else {
                                match rest.parse::<usize>() {
                                    Ok(w) if w >= 1 => width = w,
                                    _ => {
                                        return CommandResult::error(format!(
                                            "fold: invalid number of columns: '{}'\n",
                                            rest
                                        ));
                                    }
                                }
                            }
                            break;
                        }
                        _ => {
                            return CommandResult::error(format!("fold: invalid option -- '{}'\n", c));
                        }
                    }
                }
                i += 1;
            } else {
                files.push(arg.clone());
                i += 1;
            }
        }

        let mut output = String::new();

        if files.is_empty() {
            output = process_content(&ctx.stdin, width, break_at_spaces, count_bytes);
        } else {
            for file in &files {
                let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&file_path).await {
                    Ok(content) => {
                        output.push_str(&process_content(&content, width, break_at_spaces, count_bytes));
                    }
                    Err(_) => {
                        return CommandResult::with_exit_code(
                            output,
                            format!("fold: {}: No such file or directory\n", file),
                            1,
                        );
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
    async fn test_fold_default_width() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-w10".to_string()],
            stdin: "hello world foo bar\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FoldCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains('\n'));
    }

    #[tokio::test]
    async fn test_fold_word_wrap() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-sw10".to_string()],
            stdin: "hello world\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FoldCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello \nworld\n");
    }
}
