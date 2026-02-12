use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct ColumnCommand;

const HELP: &str = "column - columnate lists

Usage: column [OPTION]... [FILE]...

Options:
  -t           Create a table (determine columns from input)
  -s SEP       Input field delimiter (default: whitespace)
  -o SEP       Output field delimiter (default: two spaces)
  -c WIDTH     Output width for fill mode (default: 80)
  -n           Don't merge multiple adjacent delimiters
  --help       Display this help and exit";

fn split_fields(line: &str, separator: Option<&str>, no_merge: bool) -> Vec<String> {
    if let Some(sep) = separator {
        if no_merge {
            line.split(sep).map(|s| s.to_string()).collect()
        } else {
            line.split(sep).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
        }
    } else if no_merge {
        line.split(|c| c == ' ' || c == '\t').map(|s| s.to_string()).collect()
    } else {
        line.split_whitespace().map(|s| s.to_string()).collect()
    }
}

fn calculate_column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = Vec::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i >= widths.len() {
                widths.push(cell.len());
            } else if cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }
    widths
}

fn format_table(rows: &[Vec<String>], output_sep: &str) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let widths = calculate_column_widths(rows);
    let mut lines = Vec::new();
    for row in rows {
        let mut cells = Vec::new();
        for (i, cell) in row.iter().enumerate() {
            if i == row.len() - 1 {
                cells.push(cell.clone());
            } else {
                cells.push(format!("{:width$}", cell, width = widths[i]));
            }
        }
        lines.push(cells.join(output_sep));
    }
    lines.join("\n")
}

fn format_fill(items: &[String], width: usize, output_sep: &str) -> String {
    if items.is_empty() {
        return String::new();
    }
    let max_item_width = items.iter().map(|s| s.len()).max().unwrap_or(0);
    let sep_width = output_sep.len();
    let column_width = max_item_width + sep_width;
    let num_columns = ((width + sep_width) / column_width).max(1);
    let num_rows = (items.len() + num_columns - 1) / num_columns;
    let mut lines = Vec::new();
    for row in 0..num_rows {
        let mut cells = Vec::new();
        for col in 0..num_columns {
            let index = col * num_rows + row;
            if index < items.len() {
                let is_last = col == num_columns - 1 || (col + 1) * num_rows + row >= items.len();
                if is_last {
                    cells.push(items[index].clone());
                } else {
                    cells.push(format!("{:width$}", items[index], width = max_item_width));
                }
            }
        }
        lines.push(cells.join(output_sep));
    }
    lines.join("\n")
}

#[async_trait]
impl Command for ColumnCommand {
    fn name(&self) -> &'static str {
        "column"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut table_mode = false;
        let mut separator: Option<String> = None;
        let mut output_sep = "  ".to_string();
        let mut width = 80usize;
        let mut no_merge = false;
        let mut files = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-t" | "--table" => {
                    table_mode = true;
                    i += 1;
                }
                "-s" => {
                    i += 1;
                    if i < ctx.args.len() {
                        separator = Some(ctx.args[i].clone());
                    }
                    i += 1;
                }
                "-o" => {
                    i += 1;
                    if i < ctx.args.len() {
                        output_sep = ctx.args[i].clone();
                    }
                    i += 1;
                }
                "-c" => {
                    i += 1;
                    if i < ctx.args.len() {
                        width = ctx.args[i].parse().unwrap_or(80);
                    }
                    i += 1;
                }
                "-n" => {
                    no_merge = true;
                    i += 1;
                }
                "--" => {
                    files.extend(ctx.args[i + 1..].iter().cloned());
                    break;
                }
                _ => {
                    files.push(arg.clone());
                    i += 1;
                }
            }
        }

        let content = if files.is_empty() {
            ctx.stdin.clone()
        } else {
            let mut parts = Vec::new();
            for file in &files {
                if file == "-" {
                    parts.push(ctx.stdin.clone());
                } else {
                    let path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&path).await {
                        Ok(c) => parts.push(c),
                        Err(_) => {
                            return CommandResult::error(format!(
                                "column: {}: No such file or directory\n",
                                file
                            ));
                        }
                    }
                }
            }
            parts.join("")
        };

        if content.is_empty() || content.trim().is_empty() {
            return CommandResult::success(String::new());
        }

        let mut lines: Vec<&str> = content.split('\n').collect();
        let has_trailing = content.ends_with('\n') && lines.last() == Some(&"");
        if has_trailing {
            lines.pop();
        }
        let non_empty: Vec<&str> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();

        let output = if table_mode {
            let rows: Vec<Vec<String>> = non_empty
                .iter()
                .map(|line| split_fields(line, separator.as_deref(), no_merge))
                .collect();
            format_table(&rows, &output_sep)
        } else {
            let mut items = Vec::new();
            for line in &non_empty {
                items.extend(split_fields(line, separator.as_deref(), no_merge));
            }
            format_fill(&items, width, &output_sep)
        };

        if output.is_empty() {
            CommandResult::success(String::new())
        } else {
            CommandResult::success(format!("{}\n", output))
        }
    }
}
