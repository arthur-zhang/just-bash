// src/commands/cut/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct CutCommand;

/// Represents a single range element in a LIST specification.
#[derive(Debug, Clone)]
enum RangeSpec {
    Single(usize),
    Range(usize, usize),
    FromStart(usize),   // -M  (1 to M)
    ToEnd(usize),       // N-  (N to end)
}

/// Parse a LIST string like "1,3-5,7-" into a vector of RangeSpec.
fn parse_list(list: &str) -> Result<Vec<RangeSpec>, String> {
    let mut specs = Vec::new();
    for part in list.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = part.find('-') {
            let left = &part[..idx];
            let right = &part[idx + 1..];
            if left.is_empty() && right.is_empty() {
                return Err("cut: invalid range with no endpoint: -".to_string());
            } else if left.is_empty() {
                // -M
                let m: usize = right.parse().map_err(|_| {
                    format!("cut: invalid range: {}", part)
                })?;
                if m == 0 {
                    return Err("cut: fields and positions are numbered from 1".to_string());
                }
                specs.push(RangeSpec::FromStart(m));
            } else if right.is_empty() {
                // N-
                let n: usize = left.parse().map_err(|_| {
                    format!("cut: invalid range: {}", part)
                })?;
                if n == 0 {
                    return Err("cut: fields and positions are numbered from 1".to_string());
                }
                specs.push(RangeSpec::ToEnd(n));
            } else {
                // N-M
                let n: usize = left.parse().map_err(|_| {
                    format!("cut: invalid range: {}", part)
                })?;
                let m: usize = right.parse().map_err(|_| {
                    format!("cut: invalid range: {}", part)
                })?;
                if n == 0 || m == 0 {
                    return Err("cut: fields and positions are numbered from 1".to_string());
                }
                specs.push(RangeSpec::Range(n, m));
            }
        } else {
            let n: usize = part.parse().map_err(|_| {
                format!("cut: invalid field value: {}", part)
            })?;
            if n == 0 {
                return Err("cut: fields and positions are numbered from 1".to_string());
            }
            specs.push(RangeSpec::Single(n));
        }
    }
    if specs.is_empty() {
        return Err("cut: invalid list argument".to_string());
    }
    Ok(specs)
}

/// Expand range specs into a sorted, deduplicated list of 1-based indices.
/// `max` is the total number of items available (chars or fields).
fn expand_indices(specs: &[RangeSpec], max: usize) -> Vec<usize> {
    let mut indices = Vec::new();
    for spec in specs {
        match spec {
            RangeSpec::Single(n) => {
                if *n <= max {
                    indices.push(*n);
                }
            }
            RangeSpec::Range(n, m) => {
                let start = *n;
                let end = (*m).min(max);
                for i in start..=end {
                    indices.push(i);
                }
            }
            RangeSpec::FromStart(m) => {
                let end = (*m).min(max);
                for i in 1..=end {
                    indices.push(i);
                }
            }
            RangeSpec::ToEnd(n) => {
                for i in *n..=max {
                    indices.push(i);
                }
            }
        }
    }
    indices.sort();
    indices.dedup();
    indices
}

#[async_trait]
impl Command for CutCommand {
    fn name(&self) -> &'static str {
        "cut"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: cut OPTION... [FILE]...\n\n\
                 Print selected parts of lines from each FILE to standard output.\n\n\
                 Options:\n\
                   -c LIST    select only these characters\n\
                   -f LIST    select only these fields\n\
                   -d DELIM   use DELIM instead of TAB for field delimiter\n\
                   -s, --only-delimited  do not print lines not containing delimiters\n\
                       --help display this help and exit\n"
                    .to_string(),
            );
        }

        let mut char_list: Option<String> = None;
        let mut field_list: Option<String> = None;
        let mut delimiter: Option<String> = None;
        let mut only_delimited = false;
        let mut files: Vec<String> = Vec::new();

        let args = &ctx.args;
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-s" || arg == "--only-delimited" {
                only_delimited = true;
            } else if arg == "-c" {
                i += 1;
                if i >= args.len() {
                    return CommandResult::error(
                        "cut: option requires an argument -- 'c'\n".to_string(),
                    );
                }
                char_list = Some(args[i].clone());
            } else if arg.starts_with("-c") && arg.len() > 2 {
                char_list = Some(arg[2..].to_string());
            } else if arg == "-f" {
                i += 1;
                if i >= args.len() {
                    return CommandResult::error(
                        "cut: option requires an argument -- 'f'\n".to_string(),
                    );
                }
                field_list = Some(args[i].clone());
            } else if arg.starts_with("-f") && arg.len() > 2 {
                field_list = Some(arg[2..].to_string());
            } else if arg == "-d" {
                i += 1;
                if i >= args.len() {
                    return CommandResult::error(
                        "cut: option requires an argument -- 'd'\n".to_string(),
                    );
                }
                delimiter = Some(args[i].clone());
            } else if arg.starts_with("-d") && arg.len() > 2 {
                delimiter = Some(arg[2..].to_string());
            } else if !arg.starts_with('-') || arg == "-" {
                files.push(arg.clone());
            }
            i += 1;
        }

        // Must specify -c or -f
        if char_list.is_none() && field_list.is_none() {
            return CommandResult::error(
                "cut: you must specify a list of bytes, characters, or fields\n".to_string(),
            );
        }

        let delim = delimiter.unwrap_or_else(|| "\t".to_string());
        // Take only the first character of the delimiter string
        let delim_char = delim.chars().next().unwrap_or('\t');
        let delim_str = delim_char.to_string();

        // Read input
        let input = if files.is_empty() || (files.len() == 1 && files[0] == "-") {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[0]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => {
                    return CommandResult::error(format!(
                        "cut: {}: No such file or directory\n",
                        files[0]
                    ));
                }
            }
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let lines: Vec<&str> = input.lines().collect();
        let mut output = String::new();

        if let Some(ref clist) = char_list {
            let specs = match parse_list(clist) {
                Ok(s) => s,
                Err(e) => return CommandResult::error(format!("{}\n", e)),
            };
            for line in &lines {
                let chars: Vec<char> = line.chars().collect();
                let indices = expand_indices(&specs, chars.len());
                let selected: String =
                    indices.iter().filter_map(|&i| chars.get(i - 1)).collect();
                output.push_str(&selected);
                output.push('\n');
            }
        } else if let Some(ref flist) = field_list {
            let specs = match parse_list(flist) {
                Ok(s) => s,
                Err(e) => return CommandResult::error(format!("{}\n", e)),
            };
            for line in &lines {
                if !line.contains(delim_char) {
                    if !only_delimited {
                        output.push_str(line);
                        output.push('\n');
                    }
                    continue;
                }
                let fields: Vec<&str> = line.split(delim_char).collect();
                let indices = expand_indices(&specs, fields.len());
                let selected: Vec<&str> = indices
                    .iter()
                    .filter_map(|&i| fields.get(i - 1).copied())
                    .collect();
                output.push_str(&selected.join(&delim_str));
                output.push('\n');
            }
        }

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
    async fn test_cut_first_field_colon() {
        let ctx = make_ctx(
            vec!["-d:", "-f1"],
            "root:x:0:0\nuser:x:1000:1000\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "root\nuser\n");
    }

    #[tokio::test]
    async fn test_cut_multiple_fields() {
        let ctx = make_ctx(
            vec!["-d:", "-f1,3"],
            "a:b:c:d\n1:2:3:4\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a:c\n1:3\n");
    }

    #[tokio::test]
    async fn test_cut_field_range() {
        let ctx = make_ctx(
            vec!["-d:", "-f2-4"],
            "a:b:c:d:e\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "b:c:d\n");
    }

    #[tokio::test]
    async fn test_cut_csv_comma() {
        let ctx = make_ctx(
            vec!["-d,", "-f2"],
            "name,age,city\njohn,30,nyc\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "age\n30\n");
    }

    #[tokio::test]
    async fn test_cut_tab_default() {
        let ctx = make_ctx(
            vec!["-f1"],
            "a\tb\tc\n1\t2\t3\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\n1\n");
    }

    #[tokio::test]
    async fn test_cut_characters() {
        let ctx = make_ctx(
            vec!["-c1-5"],
            "hello world\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello\n");
    }

    #[tokio::test]
    async fn test_cut_specific_chars() {
        let ctx = make_ctx(
            vec!["-c1,3,5"],
            "abcdefg\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "ace\n");
    }

    #[tokio::test]
    async fn test_cut_stdin() {
        let ctx = make_ctx(
            vec!["-d:", "-f1"],
            "a:b:c\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\n");
    }

    #[tokio::test]
    async fn test_cut_open_range() {
        let ctx = make_ctx(
            vec!["-d:", "-f3-"],
            "a:b:c:d:e\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "c:d:e\n");
    }

    #[tokio::test]
    async fn test_cut_file_not_found() {
        let ctx = make_ctx(
            vec!["-d:", "-f1", "/nonexistent.txt"],
            "",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cut_no_field_or_char() {
        let ctx = make_ctx(
            vec![],
            "hello\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cut_only_delimited_s() {
        let ctx = make_ctx(
            vec!["-d:", "-f1", "-s"],
            "a:b\nno-delim\nc:d\n",
            vec![],
        )
        .await;
        let result = CutCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nc\n");
    }
}
