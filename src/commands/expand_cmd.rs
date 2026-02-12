use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct ExpandCommand;

const HELP: &str = "expand - convert tabs to spaces

Usage: expand [OPTION]... [FILE]...

Options:
  -t N        Use N spaces per tab (default: 8)
  -t LIST     Use comma-separated list of tab stops
  -i          Only convert leading tabs on each line
  --help      Display this help and exit";

fn parse_tab_stops(spec: &str) -> Option<Vec<usize>> {
    let parts: Vec<&str> = spec.split(',').map(|s| s.trim()).collect();
    let mut stops = Vec::new();

    for part in parts {
        let num: usize = part.parse().ok()?;
        if num < 1 {
            return None;
        }
        stops.push(num);
    }

    for i in 1..stops.len() {
        if stops[i] <= stops[i - 1] {
            return None;
        }
    }

    Some(stops)
}

fn get_tab_width(column: usize, tab_stops: &[usize]) -> usize {
    if tab_stops.len() == 1 {
        let tab_width = tab_stops[0];
        return tab_width - (column % tab_width);
    }

    for &stop in tab_stops {
        if stop > column {
            return stop - column;
        }
    }

    if tab_stops.len() >= 2 {
        let last_interval = tab_stops[tab_stops.len() - 1] - tab_stops[tab_stops.len() - 2];
        let last_stop = tab_stops[tab_stops.len() - 1];
        let stops_after_last = (column - last_stop) / last_interval + 1;
        let next_stop = last_stop + stops_after_last * last_interval;
        return next_stop - column;
    }

    1
}

fn expand_line(line: &str, tab_stops: &[usize], leading_only: bool) -> String {
    let mut result = String::new();
    let mut column = 0;
    let mut in_leading_whitespace = true;

    for c in line.chars() {
        if c == '\t' {
            if leading_only && !in_leading_whitespace {
                result.push(c);
                column += 1;
            } else {
                let spaces = get_tab_width(column, tab_stops);
                result.push_str(&" ".repeat(spaces));
                column += spaces;
            }
        } else {
            if c != ' ' && c != '\t' {
                in_leading_whitespace = false;
            }
            result.push(c);
            column += 1;
        }
    }

    result
}

fn process_content(content: &str, tab_stops: &[usize], leading_only: bool) -> String {
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

    let expanded: Vec<String> = lines_to_process
        .iter()
        .map(|line| expand_line(line, tab_stops, leading_only))
        .collect();

    if has_trailing_newline {
        format!("{}\n", expanded.join("\n"))
    } else {
        expanded.join("\n")
    }
}

#[async_trait]
impl Command for ExpandCommand {
    fn name(&self) -> &'static str {
        "expand"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut tab_stops = vec![8usize];
        let mut leading_only = false;
        let mut files = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];

            if arg == "--help" {
                return CommandResult::success(format!("{}\n", HELP));
            } else if arg == "-t" && i + 1 < ctx.args.len() {
                match parse_tab_stops(&ctx.args[i + 1]) {
                    Some(stops) => tab_stops = stops,
                    None => {
                        return CommandResult::error(format!(
                            "expand: invalid tab size: '{}'\n",
                            ctx.args[i + 1]
                        ));
                    }
                }
                i += 2;
            } else if arg.starts_with("-t") && arg.len() > 2 {
                match parse_tab_stops(&arg[2..]) {
                    Some(stops) => tab_stops = stops,
                    None => {
                        return CommandResult::error(format!(
                            "expand: invalid tab size: '{}'\n",
                            &arg[2..]
                        ));
                    }
                }
                i += 1;
            } else if arg == "-i" || arg == "--initial" {
                leading_only = true;
                i += 1;
            } else if arg == "--" {
                files.extend(ctx.args[i + 1..].iter().cloned());
                break;
            } else if arg.starts_with('-') && arg != "-" {
                return CommandResult::error(format!("expand: invalid option -- '{}'\n", &arg[1..]));
            } else {
                files.push(arg.clone());
                i += 1;
            }
        }

        let mut output = String::new();

        if files.is_empty() {
            output = process_content(&ctx.stdin, &tab_stops, leading_only);
        } else {
            for file in &files {
                let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&file_path).await {
                    Ok(content) => {
                        output.push_str(&process_content(&content, &tab_stops, leading_only));
                    }
                    Err(_) => {
                        return CommandResult::with_exit_code(
                            output,
                            format!("expand: {}: No such file or directory\n", file),
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
    async fn test_expand_default_tabs() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: "a\tb\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = ExpandCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a       b\n");
    }

    #[tokio::test]
    async fn test_expand_custom_tab_size() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-t4".to_string()],
            stdin: "a\tb\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = ExpandCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a   b\n");
    }

    #[test]
    fn test_parse_tab_stops() {
        assert_eq!(parse_tab_stops("4"), Some(vec![4]));
        assert_eq!(parse_tab_stops("4,8,12"), Some(vec![4, 8, 12]));
        assert_eq!(parse_tab_stops("0"), None);
        assert_eq!(parse_tab_stops("4,2"), None);
    }
}
