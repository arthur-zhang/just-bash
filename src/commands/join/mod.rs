// src/commands/join/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct JoinCommand;

#[derive(Debug, Default)]
struct JoinOptions {
    field1: usize,
    field2: usize,
    separator: Option<char>,
    print_unpairable1: bool,
    print_unpairable2: bool,
    only_unpairable1: bool,
    only_unpairable2: bool,
    empty_string: Option<String>,
    output_format: Option<Vec<(usize, usize)>>,
    ignore_case: bool,
}

fn split_fields(line: &str, sep: Option<char>) -> Vec<String> {
    match sep {
        Some(c) => line.split(c).map(|s| s.to_string()).collect(),
        None => line.split_whitespace().map(|s| s.to_string()).collect(),
    }
}

fn format_line(
    join_key: &str,
    fields1: &[String],
    fields2: &[String],
    join_field1: usize,
    join_field2: usize,
    output_format: &Option<Vec<(usize, usize)>>,
    empty_string: &Option<String>,
    sep: Option<char>,
) -> String {
    let sep_str = match sep {
        Some(c) => c.to_string(),
        None => " ".to_string(),
    };
    let empty = empty_string.clone().unwrap_or_default();

    if let Some(fmt) = output_format {
        let parts: Vec<String> = fmt
            .iter()
            .map(|&(filenum, field_idx)| {
                if field_idx == 0 {
                    return join_key.to_string();
                }
                let real_idx = field_idx - 1;
                let fields = if filenum == 1 { fields1 } else { fields2 };
                fields
                    .get(real_idx)
                    .cloned()
                    .unwrap_or_else(|| empty.clone())
            })
            .collect();
        parts.join(&sep_str)
    } else {
        let mut parts: Vec<String> = vec![join_key.to_string()];
        for (i, f) in fields1.iter().enumerate() {
            if i != join_field1 {
                parts.push(f.clone());
            }
        }
        for (i, f) in fields2.iter().enumerate() {
            if i != join_field2 {
                parts.push(f.clone());
            }
        }
        parts.join(&sep_str)
    }
}

fn format_unpairable(
    fields: &[String],
    filenum: usize,
    join_field: usize,
    output_format: &Option<Vec<(usize, usize)>>,
    empty_string: &Option<String>,
    sep: Option<char>,
) -> String {
    let sep_str = match sep {
        Some(c) => c.to_string(),
        None => " ".to_string(),
    };
    let empty = empty_string.clone().unwrap_or_default();
    let join_key = fields.get(join_field).map(|s| s.as_str()).unwrap_or("");

    if let Some(fmt) = output_format {
        let parts: Vec<String> = fmt
            .iter()
            .map(|&(fnum, field_idx)| {
                if field_idx == 0 {
                    return join_key.to_string();
                }
                let real_idx = field_idx - 1;
                if fnum == filenum {
                    fields
                        .get(real_idx)
                        .cloned()
                        .unwrap_or_else(|| empty.clone())
                } else {
                    empty.clone()
                }
            })
            .collect();
        parts.join(&sep_str)
    } else {
        fields.join(&sep_str)
    }
}

fn keys_match(a: &str, b: &str, ignore_case: bool) -> bool {
    if ignore_case {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

fn parse_output_format(s: &str) -> Result<Vec<(usize, usize)>, String> {
    let mut result = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((fnum_str, field_str)) = part.split_once('.') {
            let fnum: usize = fnum_str
                .parse()
                .map_err(|_| format!("invalid format: {}", part))?;
            let field: usize = field_str
                .parse()
                .map_err(|_| format!("invalid format: {}", part))?;
            if fnum != 1 && fnum != 2 {
                return Err(format!("invalid file number: {}", fnum));
            }
            result.push((fnum, field));
        } else {
            return Err(format!("invalid format: {}", part));
        }
    }
    Ok(result)
}

fn parse_args(args: &[String]) -> Result<(JoinOptions, Vec<String>), String> {
    let mut opts = JoinOptions::default();
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if arg == "-1" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- '1'".to_string());
            }
            let v: usize = args[i]
                .parse()
                .map_err(|_| format!("join: invalid field number: '{}'", args[i]))?;
            opts.field1 = v.saturating_sub(1);
        } else if arg == "-2" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- '2'".to_string());
            }
            let v: usize = args[i]
                .parse()
                .map_err(|_| format!("join: invalid field number: '{}'", args[i]))?;
            opts.field2 = v.saturating_sub(1);
        } else if arg == "-t" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- 't'".to_string());
            }
            let ch = args[i]
                .chars()
                .next()
                .ok_or("join: empty separator")?;
            opts.separator = Some(ch);
        } else if arg.starts_with("-t") && arg.len() > 2 {
            let ch = arg[2..].chars().next().unwrap();
            opts.separator = Some(ch);
        } else if arg == "-a" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- 'a'".to_string());
            }
            match args[i].as_str() {
                "1" => opts.print_unpairable1 = true,
                "2" => opts.print_unpairable2 = true,
                _ => return Err(format!("join: invalid file number: '{}'", args[i])),
            }
        } else if arg == "-a1" {
            opts.print_unpairable1 = true;
        } else if arg == "-a2" {
            opts.print_unpairable2 = true;
        } else if arg == "-v" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- 'v'".to_string());
            }
            match args[i].as_str() {
                "1" => opts.only_unpairable1 = true,
                "2" => opts.only_unpairable2 = true,
                _ => return Err(format!("join: invalid file number: '{}'", args[i])),
            }
        } else if arg == "-v1" {
            opts.only_unpairable1 = true;
        } else if arg == "-v2" {
            opts.only_unpairable2 = true;
        } else if arg == "-e" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- 'e'".to_string());
            }
            opts.empty_string = Some(args[i].clone());
        } else if arg == "-o" {
            i += 1;
            if i >= args.len() {
                return Err("join: option requires an argument -- 'o'".to_string());
            }
            opts.output_format = Some(parse_output_format(&args[i])?);
        } else if arg == "-i" || arg == "--ignore-case" {
            opts.ignore_case = true;
        } else if arg == "--help" {
            // handled before parse_args
        } else if arg.starts_with('-') && arg != "-" {
            return Err(format!("join: invalid option -- '{}'", &arg[1..]));
        } else {
            files.push(arg.clone());
        }
        i += 1;
    }

    Ok((opts, files))
}

#[async_trait]
impl Command for JoinCommand {
    fn name(&self) -> &'static str {
        "join"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: join [OPTION]... FILE1 FILE2\n\n\
                 For each pair of input lines with identical join fields, write a line to\n\
                 standard output. The default join field is the first, delimited by blanks.\n\n\
                 Options:\n\
                   -1 FIELD     join on this FIELD of file 1\n\
                   -2 FIELD     join on this FIELD of file 2\n\
                   -t CHAR      use CHAR as input and output field separator\n\
                   -a FILENUM   also print unpairable lines from file FILENUM\n\
                   -v FILENUM   like -a FILENUM, but suppress joined output lines\n\
                   -e STRING    replace missing (empty) input fields with STRING\n\
                   -o FORMAT    obey FORMAT while constructing output line\n\
                   -i, --ignore-case  ignore differences in case when comparing fields\n\
                       --help   display this help and exit\n"
                    .to_string(),
            );
        }

        let (opts, files) = match parse_args(&ctx.args) {
            Ok(v) => v,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        if files.len() != 2 {
            return CommandResult::error(
                "join: missing operand\nTry 'join --help' for more information.\n".to_string(),
            );
        }

        let read_file = |path: &str, stdin: &str, fs: &std::sync::Arc<dyn crate::fs::FileSystem>, cwd: &str| {
            let path = path.to_string();
            let stdin = stdin.to_string();
            let fs = fs.clone();
            let cwd = cwd.to_string();
            async move {
                if path == "-" {
                    Ok(stdin)
                } else {
                    let resolved = fs.resolve_path(&cwd, &path);
                    fs.read_file(&resolved).await.map_err(|_| {
                        format!("join: {}: No such file or directory", path)
                    })
                }
            }
        };

        let content1 = match read_file(&files[0], &ctx.stdin, &ctx.fs, &ctx.cwd).await {
            Ok(c) => c,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };
        let content2 = match read_file(&files[1], &ctx.stdin, &ctx.fs, &ctx.cwd).await {
            Ok(c) => c,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        let lines1: Vec<&str> = content1.lines().collect();
        let lines2: Vec<&str> = content2.lines().collect();

        let parsed1: Vec<Vec<String>> = lines1.iter().map(|l| split_fields(l, opts.separator)).collect();
        let parsed2: Vec<Vec<String>> = lines2.iter().map(|l| split_fields(l, opts.separator)).collect();

        let suppress_paired = opts.only_unpairable1 || opts.only_unpairable2;

        let mut output = String::new();
        let mut matched1 = vec![false; lines1.len()];
        let mut matched2 = vec![false; lines2.len()];

        // Find all matches (inner join)
        for (i, f1) in parsed1.iter().enumerate() {
            let key1 = f1.get(opts.field1).map(|s| s.as_str()).unwrap_or("");
            for (j, f2) in parsed2.iter().enumerate() {
                let key2 = f2.get(opts.field2).map(|s| s.as_str()).unwrap_or("");
                if keys_match(key1, key2, opts.ignore_case) {
                    matched1[i] = true;
                    matched2[j] = true;
                    if !suppress_paired {
                        let line = format_line(
                            key1,
                            f1,
                            f2,
                            opts.field1,
                            opts.field2,
                            &opts.output_format,
                            &opts.empty_string,
                            opts.separator,
                        );
                        output.push_str(&line);
                        output.push('\n');
                    }
                }
            }
        }

        // Print unpairable lines from file 1
        if opts.print_unpairable1 || opts.only_unpairable1 {
            for (i, f1) in parsed1.iter().enumerate() {
                if !matched1[i] {
                    let line = format_unpairable(
                        f1,
                        1,
                        opts.field1,
                        &opts.output_format,
                        &opts.empty_string,
                        opts.separator,
                    );
                    output.push_str(&line);
                    output.push('\n');
                }
            }
        }

        // Print unpairable lines from file 2
        if opts.print_unpairable2 || opts.only_unpairable2 {
            for (j, f2) in parsed2.iter().enumerate() {
                if !matched2[j] {
                    let line = format_unpairable(
                        f2,
                        2,
                        opts.field2,
                        &opts.output_format,
                        &opts.empty_string,
                        opts.separator,
                    );
                    output.push_str(&line);
                    output.push('\n');
                }
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
    async fn test_join_basic() {
        let ctx = make_ctx(
            vec!["/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n3 cherry\n"),
                ("/file2.txt", "1 red\n2 yellow\n3 red\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(result.stdout.contains("2 banana yellow"));
        assert!(result.stdout.contains("3 cherry red"));
    }

    #[tokio::test]
    async fn test_join_only_matching() {
        let ctx = make_ctx(
            vec!["/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n"),
                ("/file2.txt", "1 red\n3 green\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(!result.stdout.contains("2"));
        assert!(!result.stdout.contains("3"));
    }

    #[tokio::test]
    async fn test_join_custom_field() {
        let ctx = make_ctx(
            vec!["-1", "2", "-2", "1", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "apple 1\nbanana 2\n"),
                ("/file2.txt", "1 red\n2 yellow\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(result.stdout.contains("2 banana yellow"));
    }

    #[tokio::test]
    async fn test_join_custom_separator() {
        let ctx = make_ctx(
            vec!["-t:", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1:apple\n2:banana\n"),
                ("/file2.txt", "1:red\n2:yellow\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1:apple:red"));
    }

    #[tokio::test]
    async fn test_join_left_outer_a1() {
        let ctx = make_ctx(
            vec!["-a1", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n3 cherry\n"),
                ("/file2.txt", "1 red\n3 green\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(result.stdout.contains("3 cherry green"));
        assert!(result.stdout.contains("2 banana"));
    }

    #[tokio::test]
    async fn test_join_right_outer_a2() {
        let ctx = make_ctx(
            vec!["-a2", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n3 cherry\n"),
                ("/file2.txt", "1 red\n2 yellow\n3 green\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(result.stdout.contains("3 cherry green"));
        assert!(result.stdout.contains("2 yellow"));
    }

    #[tokio::test]
    async fn test_join_anti_v1() {
        let ctx = make_ctx(
            vec!["-v1", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n3 cherry\n"),
                ("/file2.txt", "1 red\n3 green\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("2 banana"));
        assert!(!result.stdout.contains("1 apple red"));
        assert!(!result.stdout.contains("3 cherry green"));
    }

    #[tokio::test]
    async fn test_join_empty_replacement() {
        let ctx = make_ctx(
            vec!["-a1", "-e", "EMPTY", "-o", "1.1,1.2,2.2", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n"),
                ("/file2.txt", "1 red\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1 apple red"));
        assert!(result.stdout.contains("2 banana EMPTY"));
    }

    #[tokio::test]
    async fn test_join_ignore_case() {
        let ctx = make_ctx(
            vec!["-i", "/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "A hello\nB world\n"),
                ("/file2.txt", "a foo\nb bar\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello foo"));
        assert!(result.stdout.contains("world bar"));
    }

    #[tokio::test]
    async fn test_join_missing_file() {
        let ctx = make_ctx(
            vec!["/file1.txt"],
            "",
            vec![("/file1.txt", "1 apple\n")],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_join_no_matches() {
        let ctx = make_ctx(
            vec!["/file1.txt", "/file2.txt"],
            "",
            vec![
                ("/file1.txt", "1 apple\n2 banana\n"),
                ("/file2.txt", "3 red\n4 yellow\n"),
            ],
        )
        .await;
        let result = JoinCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }
}
