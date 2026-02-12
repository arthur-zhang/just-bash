// src/commands/sort/mod.rs
pub mod comparator;

use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use comparator::{SortOptions, parse_key_spec, create_comparator};

pub struct SortCommand;

#[async_trait]
impl Command for SortCommand {
    fn name(&self) -> &'static str {
        "sort"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: sort [OPTION]... [FILE]...\n\n\
                 Write sorted concatenation of all FILE(s) to standard output.\n\n\
                 Options:\n\
                   -r, --reverse              reverse the result of comparisons\n\
                   -n, --numeric-sort         compare according to string numerical value\n\
                   -u, --unique               output only unique lines\n\
                   -f, --ignore-case          fold lower case to upper case characters\n\
                   -h, --human-numeric-sort   compare human readable numbers (e.g., 2K 1G)\n\
                   -V, --version-sort         natural sort of (version) numbers within text\n\
                   -d, --dictionary-order     consider only blanks and alphanumeric characters\n\
                   -M, --month-sort           compare (unknown) < 'JAN' < ... < 'DEC'\n\
                   -b, --ignore-leading-blanks ignore leading blanks\n\
                   -s, --stable               stabilize sort by disabling last-resort comparison\n\
                   -c, --check                check for sorted input; do not sort\n\
                   -o FILE, --output=FILE     write result to FILE instead of standard output\n\
                   -k KEYDEF, --key=KEYDEF    sort via a key; KEYDEF gives location and type\n\
                   -t SEP, --field-separator=SEP  use SEP instead of non-blank to blank transition\n\
                       --help                 display this help and exit\n"
                    .to_string(),
            );
        }

        // Parse arguments
        let mut opts = SortOptions::default();
        let mut files: Vec<String> = Vec::new();
        let mut i = 0;
        let args = &ctx.args;

        while i < args.len() {
            let arg = &args[i];

            if arg == "--" {
                i += 1;
                while i < args.len() {
                    files.push(args[i].clone());
                    i += 1;
                }
                break;
            }

            if arg.starts_with("--") {
                match arg.as_str() {
                    "--reverse" => opts.reverse = true,
                    "--numeric-sort" => opts.numeric = true,
                    "--unique" => opts.unique = true,
                    "--ignore-case" => opts.ignore_case = true,
                    "--human-numeric-sort" => opts.human_numeric = true,
                    "--version-sort" => opts.version_sort = true,
                    "--dictionary-order" => opts.dictionary_order = true,
                    "--month-sort" => opts.month_sort = true,
                    "--ignore-leading-blanks" => opts.ignore_leading_blanks = true,
                    "--stable" => opts.stable = true,
                    "--check" => opts.check = true,
                    _ if arg.starts_with("--output=") => {
                        opts.output_file = Some(arg[9..].to_string());
                    }
                    _ if arg.starts_with("--key=") => {
                        opts.keys.push(parse_key_spec(&arg[6..]));
                    }
                    _ if arg.starts_with("--field-separator=") => {
                        let sep = &arg[18..];
                        if let Some(c) = sep.chars().next() {
                            opts.field_separator = Some(c);
                        }
                    }
                    _ => {
                        // Unknown long option, ignore
                    }
                }
                i += 1;
                continue;
            }

            if arg.starts_with('-') && arg.len() > 1 {
                // Could be combined short flags or flags with values
                let chars: Vec<char> = arg[1..].chars().collect();
                let mut j = 0;
                let mut consumed_next = false;

                while j < chars.len() {
                    match chars[j] {
                        'r' => opts.reverse = true,
                        'n' => opts.numeric = true,
                        'u' => opts.unique = true,
                        'f' => opts.ignore_case = true,
                        'h' => opts.human_numeric = true,
                        'V' => opts.version_sort = true,
                        'd' => opts.dictionary_order = true,
                        'M' => opts.month_sort = true,
                        'b' => opts.ignore_leading_blanks = true,
                        's' => opts.stable = true,
                        'c' => opts.check = true,
                        'o' => {
                            // -o FILE or -oFILE
                            let rest: String = chars[j + 1..].iter().collect();
                            if !rest.is_empty() {
                                opts.output_file = Some(rest);
                            } else if i + 1 < args.len() {
                                i += 1;
                                opts.output_file = Some(args[i].clone());
                                consumed_next = true;
                            }
                            j = chars.len(); // done with this arg
                            continue;
                        }
                        'k' => {
                            // -k KEYDEF or -kKEYDEF
                            let rest: String = chars[j + 1..].iter().collect();
                            if !rest.is_empty() {
                                opts.keys.push(parse_key_spec(&rest));
                            } else if i + 1 < args.len() {
                                i += 1;
                                opts.keys.push(parse_key_spec(&args[i]));
                                consumed_next = true;
                            }
                            j = chars.len(); // done with this arg
                            continue;
                        }
                        't' => {
                            // -t SEP or -tSEP
                            let rest: String = chars[j + 1..].iter().collect();
                            if !rest.is_empty() {
                                opts.field_separator = rest.chars().next();
                            } else if i + 1 < args.len() {
                                i += 1;
                                opts.field_separator = args[i].chars().next();
                                consumed_next = true;
                            }
                            j = chars.len(); // done with this arg
                            continue;
                        }
                        _ => {} // unknown flag, ignore
                    }
                    j += 1;
                }

                if consumed_next {
                    // already incremented i
                }
                i += 1;
                continue;
            }

            // Not a flag: it's a file argument
            files.push(arg.clone());
            i += 1;
        }

        // Read input
        let input = if files.is_empty() || (files.len() == 1 && files[0] == "-") {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &files[0]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => {
                    return CommandResult::error(format!(
                        "sort: {}: No such file or directory\n",
                        files[0]
                    ));
                }
            }
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let mut lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return CommandResult::success(String::new());
        }

        // Use a block to ensure the comparator (non-Send) is dropped before any await
        let output = {
            let comparator = create_comparator(&opts);

            // Check mode
            if opts.check {
                for idx in 1..lines.len() {
                    if comparator(lines[idx - 1], lines[idx]) == std::cmp::Ordering::Greater {
                        return CommandResult::with_exit_code(
                            String::new(),
                            format!(
                                "sort: -:{}:disorder: {}\n",
                                idx + 1,
                                lines[idx]
                            ),
                            1,
                        );
                    }
                }
                return CommandResult::success(String::new());
            }

            // Sort
            lines.sort_by(|a, b| comparator(a, b));

            // Unique
            if opts.unique {
                lines.dedup_by(|a, b| comparator(a, b) == std::cmp::Ordering::Equal);
            }

            // Build output
            let mut out = String::new();
            for line in &lines {
                out.push_str(line);
                out.push('\n');
            }
            out
        };

        // Write to output file if specified
        if let Some(ref out_path) = opts.output_file {
            let resolved = ctx.fs.resolve_path(&ctx.cwd, out_path);
            match ctx.fs.write_file(&resolved, output.as_bytes()).await {
                Ok(_) => return CommandResult::success(String::new()),
                Err(e) => {
                    return CommandResult::error(format!(
                        "sort: {}: {}\n",
                        out_path, e
                    ));
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
    async fn test_sort_alphabetical() {
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", "banana\napple\ncherry\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn test_sort_reverse() {
        let ctx = make_ctx(
            vec!["-r", "/test.txt"],
            "",
            vec![("/test.txt", "a\nb\nc\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "c\nb\na\n");
    }

    #[tokio::test]
    async fn test_sort_numeric() {
        let ctx = make_ctx(
            vec!["-n", "/test.txt"],
            "",
            vec![("/test.txt", "10\n2\n1\n20\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\n2\n10\n20\n");
    }

    #[tokio::test]
    async fn test_sort_numeric_reverse() {
        let ctx = make_ctx(
            vec!["-rn", "/test.txt"],
            "",
            vec![("/test.txt", "10\n2\n1\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "10\n2\n1\n");
    }

    #[tokio::test]
    async fn test_sort_unique() {
        let ctx = make_ctx(
            vec!["-u", "/test.txt"],
            "",
            vec![("/test.txt", "b\na\nb\nc\na\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_key_field() {
        let ctx = make_ctx(
            vec!["-k2", "/test.txt"],
            "",
            vec![("/test.txt", "a 3\nb 1\nc 2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "b 1\nc 2\na 3\n");
    }

    #[tokio::test]
    async fn test_sort_stdin() {
        let ctx = make_ctx(vec![], "z\na\nm\n", vec![]).await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nm\nz\n");
    }

    #[tokio::test]
    async fn test_sort_case_insensitive() {
        let ctx = make_ctx(
            vec!["-f", "/test.txt"],
            "",
            vec![("/test.txt", "B\na\nC\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nB\nC\n");
    }

    #[tokio::test]
    async fn test_sort_file_not_found() {
        let ctx = make_ctx(vec!["/nonexistent.txt"], "", vec![]).await;
        let result = SortCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_empty() {
        let ctx = make_ctx(vec![], "", vec![]).await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_sort_combined_nr() {
        let ctx = make_ctx(
            vec!["-nr", "/test.txt"],
            "",
            vec![("/test.txt", "5\n10\n1\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "10\n5\n1\n");
    }

    #[tokio::test]
    async fn test_sort_key_range() {
        let ctx = make_ctx(
            vec!["-k1,2", "/test.txt"],
            "",
            vec![("/test.txt", "b 2\na 1\nc 3\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a 1\nb 2\nc 3\n");
    }

    #[tokio::test]
    async fn test_sort_key_numeric_modifier() {
        let ctx = make_ctx(
            vec!["-k2n", "/test.txt"],
            "",
            vec![("/test.txt", "a 10\nb 2\nc 1\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "c 1\nb 2\na 10\n");
    }

    #[tokio::test]
    async fn test_sort_custom_delimiter() {
        let ctx = make_ctx(
            vec!["-t:", "-k2", "/test.txt"],
            "",
            vec![("/test.txt", "a:3\nb:1\nc:2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "b:1\nc:2\na:3\n");
    }

    #[tokio::test]
    async fn test_sort_check_sorted() {
        let ctx = make_ctx(
            vec!["-c", "/test.txt"],
            "",
            vec![("/test.txt", "a\nb\nc\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_check_unsorted() {
        let ctx = make_ctx(
            vec!["-c", "/test.txt"],
            "",
            vec![("/test.txt", "c\na\nb\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_long_reverse() {
        let ctx = make_ctx(
            vec!["--reverse", "/test.txt"],
            "",
            vec![("/test.txt", "a\nb\nc\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "c\nb\na\n");
    }

    #[tokio::test]
    async fn test_sort_long_numeric() {
        let ctx = make_ctx(
            vec!["--numeric-sort", "/test.txt"],
            "",
            vec![("/test.txt", "10\n2\n1\n20\n5\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\n2\n5\n10\n20\n");
    }

    #[tokio::test]
    async fn test_sort_long_unique() {
        let ctx = make_ctx(
            vec!["--unique", "/test.txt"],
            "",
            vec![("/test.txt", "apple\nbanana\napple\ncherry\nbanana\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn test_sort_long_ignore_case() {
        let ctx = make_ctx(
            vec!["--ignore-case", "/test.txt"],
            "",
            vec![("/test.txt", "Banana\napple\nCherry\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "apple\nBanana\nCherry\n");
    }

    #[tokio::test]
    async fn test_sort_case_insensitive_reverse() {
        let ctx = make_ctx(
            vec!["-fr", "/test.txt"],
            "",
            vec![("/test.txt", "apple\nBanana\ncherry\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "cherry\nBanana\napple\n");
    }

    #[tokio::test]
    async fn test_sort_case_insensitive_unique() {
        let ctx = make_ctx(
            vec!["-fu", "/test.txt"],
            "",
            vec![("/test.txt", "Apple\napple\nBanana\nbanana\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn test_sort_key_with_case_insensitive() {
        let ctx = make_ctx(
            vec!["-f", "-k2", "/test.txt"],
            "",
            vec![("/test.txt", "1 Zebra\n2 apple\n3 BANANA\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "2 apple\n3 BANANA\n1 Zebra\n");
    }

    #[tokio::test]
    async fn test_sort_key_single_field() {
        let ctx = make_ctx(
            vec!["-k2,2", "/test.txt"],
            "",
            vec![("/test.txt", "1 banana\n2 apple\n3 cherry\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "2 apple\n1 banana\n3 cherry\n");
    }

    #[tokio::test]
    async fn test_sort_key_reverse_modifier() {
        let ctx = make_ctx(
            vec!["-k1r", "/test.txt"],
            "",
            vec![("/test.txt", "a 1\nb 2\nc 3\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "c 3\nb 2\na 1\n");
    }

    #[tokio::test]
    async fn test_sort_key_combined_modifiers() {
        let ctx = make_ctx(
            vec!["-k2,2nr", "/test.txt"],
            "",
            vec![("/test.txt", "x 5\ny 10\nz 2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "y 10\nx 5\nz 2\n");
    }

    #[tokio::test]
    async fn test_sort_multiple_keys() {
        let ctx = make_ctx(
            vec!["-k1,1", "-k2,2n", "/test.txt"],
            "",
            vec![("/test.txt", "a 2\nb 1\na 1\nb 2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a 1\na 2\nb 1\nb 2\n");
    }

    #[tokio::test]
    async fn test_sort_key_char_position() {
        let ctx = make_ctx(
            vec!["-k1.2", "/test.txt"],
            "",
            vec![("/test.txt", "abc\nabc\nbac\naac\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "aac\nbac\nabc\nabc\n");
    }

    #[tokio::test]
    async fn test_sort_key_case_insensitive_modifier() {
        let ctx = make_ctx(
            vec!["-k1f", "/test.txt"],
            "",
            vec![("/test.txt", "Zebra\napple\nBANANA\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "apple\nBANANA\nZebra\n");
    }

    #[tokio::test]
    async fn test_sort_long_key_syntax() {
        let ctx = make_ctx(
            vec!["--key=1n", "/test.txt"],
            "",
            vec![("/test.txt", "3 c\n1 a\n2 b\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1 a\n2 b\n3 c\n");
    }

    #[tokio::test]
    async fn test_sort_human_numeric() {
        let ctx = make_ctx(
            vec!["-h", "/test.txt"],
            "",
            vec![("/test.txt", "1K\n2M\n500\n1G\n100K\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "500\n1K\n100K\n2M\n1G\n");
    }

    #[tokio::test]
    async fn test_sort_human_numeric_mixed_case() {
        let ctx = make_ctx(
            vec!["-h", "/test.txt"],
            "",
            vec![("/test.txt", "1k\n2M\n3g\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1k\n2M\n3g\n");
    }

    #[tokio::test]
    async fn test_sort_human_numeric_decimal() {
        let ctx = make_ctx(
            vec!["-h", "/test.txt"],
            "",
            vec![("/test.txt", "1.5K\n2K\n1K\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1K\n1.5K\n2K\n");
    }

    #[tokio::test]
    async fn test_sort_human_numeric_reverse() {
        let ctx = make_ctx(
            vec!["-hr", "/test.txt"],
            "",
            vec![("/test.txt", "1K\n1M\n1G\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1G\n1M\n1K\n");
    }

    #[tokio::test]
    async fn test_sort_version() {
        let ctx = make_ctx(
            vec!["-V", "/test.txt"],
            "",
            vec![("/test.txt", "file1.10\nfile1.2\nfile1.1\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "file1.1\nfile1.2\nfile1.10\n");
    }

    #[tokio::test]
    async fn test_sort_version_with_prefix() {
        let ctx = make_ctx(
            vec!["-V", "/test.txt"],
            "",
            vec![("/test.txt", "v2.0\nv1.10\nv1.2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "v1.2\nv1.10\nv2.0\n");
    }

    #[tokio::test]
    async fn test_sort_version_multi_part() {
        let ctx = make_ctx(
            vec!["-V", "/test.txt"],
            "",
            vec![("/test.txt", "1.0.0\n1.0.10\n1.0.2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1.0.0\n1.0.2\n1.0.10\n");
    }

    #[tokio::test]
    async fn test_sort_month() {
        let ctx = make_ctx(
            vec!["-M", "/test.txt"],
            "",
            vec![("/test.txt", "Mar\nJan\nDec\nFeb\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Jan\nFeb\nMar\nDec\n");
    }

    #[tokio::test]
    async fn test_sort_month_lowercase() {
        let ctx = make_ctx(
            vec!["-M", "/test.txt"],
            "",
            vec![("/test.txt", "mar\njan\nfeb\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "jan\nfeb\nmar\n");
    }

    #[tokio::test]
    async fn test_sort_month_unknown_first() {
        let ctx = make_ctx(
            vec!["-M", "/test.txt"],
            "",
            vec![("/test.txt", "Mar\nfoo\nJan\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "foo\nJan\nMar\n");
    }

    #[tokio::test]
    async fn test_sort_dictionary_order() {
        let ctx = make_ctx(
            vec!["-d", "/test.txt"],
            "",
            vec![("/test.txt", "b-c\na_b\nc.d\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a_b\nb-c\nc.d\n");
    }

    #[tokio::test]
    async fn test_sort_ignore_leading_blanks_numeric() {
        let ctx = make_ctx(
            vec!["-bn", "/test.txt"],
            "",
            vec![("/test.txt", "  2\n1\n   3\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1\n  2\n   3\n");
    }

    #[tokio::test]
    async fn test_sort_check_numeric() {
        let ctx = make_ctx(
            vec!["-cn", "/test.txt"],
            "",
            vec![("/test.txt", "1\n2\n10\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_output_file() {
        let ctx = make_ctx(
            vec!["-o", "/out.txt", "/test.txt"],
            "",
            vec![("/test.txt", "c\na\nb\n")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
        let content = fs.read_file("/out.txt").await.unwrap();
        assert_eq!(content, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_output_file_inplace() {
        let ctx = make_ctx(
            vec!["-o", "/test.txt", "/test.txt"],
            "",
            vec![("/test.txt", "c\na\nb\n")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/test.txt").await.unwrap();
        assert_eq!(content, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_output_file_long_syntax() {
        let ctx = make_ctx(
            vec!["--output=/out.txt", "/test.txt"],
            "",
            vec![("/test.txt", "c\na\nb\n")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/out.txt").await.unwrap();
        assert_eq!(content, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_stable() {
        let ctx = make_ctx(
            vec!["-s", "-k1,1", "/test.txt"],
            "",
            vec![("/test.txt", "1 b\n1 a\n2 c\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1 b\n1 a\n2 c\n");
    }

    #[tokio::test]
    async fn test_sort_key_human_modifier() {
        let ctx = make_ctx(
            vec!["-k2h", "/test.txt"],
            "",
            vec![("/test.txt", "a 1M\nb 1K\nc 1G\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "b 1K\na 1M\nc 1G\n");
    }

    #[tokio::test]
    async fn test_sort_key_version_modifier() {
        let ctx = make_ctx(
            vec!["-k2V", "/test.txt"],
            "",
            vec![("/test.txt", "a v1.10\nb v1.2\nc v2.0\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "b v1.2\na v1.10\nc v2.0\n");
    }

    #[tokio::test]
    async fn test_sort_key_month_modifier() {
        let ctx = make_ctx(
            vec!["-k2M", "/test.txt"],
            "",
            vec![("/test.txt", "2023 Mar\n2023 Jan\n2023 Feb\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "2023 Jan\n2023 Feb\n2023 Mar\n");
    }

    #[tokio::test]
    async fn test_sort_binary_content() {
        let ctx = make_ctx(
            vec!["/data.txt"],
            "",
            vec![("/data.txt", "c\na\nb\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_help() {
        let ctx = make_ctx(vec!["--help"], "", vec![]).await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("--ignore-case"));
        assert!(result.stdout.contains("-h"));
        assert!(result.stdout.contains("-V"));
        assert!(result.stdout.contains("-M"));
        assert!(result.stdout.contains("-d"));
        assert!(result.stdout.contains("-b"));
        assert!(result.stdout.contains("-c"));
        assert!(result.stdout.contains("-o"));
        assert!(result.stdout.contains("-s"));
    }

    #[tokio::test]
    async fn test_sort_long_human_numeric() {
        let ctx = make_ctx(
            vec!["--human-numeric-sort", "/test.txt"],
            "",
            vec![("/test.txt", "1K\n2M\n500\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "500\n1K\n2M\n");
    }

    #[tokio::test]
    async fn test_sort_long_version() {
        let ctx = make_ctx(
            vec!["--version-sort", "/test.txt"],
            "",
            vec![("/test.txt", "file1.10\nfile1.2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "file1.2\nfile1.10\n");
    }

    #[tokio::test]
    async fn test_sort_long_month() {
        let ctx = make_ctx(
            vec!["--month-sort", "/test.txt"],
            "",
            vec![("/test.txt", "Mar\nJan\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Jan\nMar\n");
    }

    #[tokio::test]
    async fn test_sort_long_dictionary_order() {
        let ctx = make_ctx(
            vec!["--dictionary-order", "/test.txt"],
            "",
            vec![("/test.txt", "b-c\na_b\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a_b\nb-c\n");
    }

    #[tokio::test]
    async fn test_sort_long_stable() {
        let ctx = make_ctx(
            vec!["--stable", "-k1,1", "/test.txt"],
            "",
            vec![("/test.txt", "1 b\n1 a\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "1 b\n1 a\n");
    }

    #[tokio::test]
    async fn test_sort_long_check() {
        let ctx = make_ctx(
            vec!["--check", "/test.txt"],
            "",
            vec![("/test.txt", "a\nb\nc\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_field_separator_long() {
        let ctx = make_ctx(
            vec!["--field-separator=:", "-k2", "/test.txt"],
            "",
            vec![("/test.txt", "c:3\na:1\nb:2\n")],
        )
        .await;
        let result = SortCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "a:1\nb:2\nc:3\n");
    }
}