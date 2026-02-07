// src/commands/awk/mod.rs
pub mod builtins;
pub mod coercion;
pub mod context;
pub mod expressions;
pub mod fields;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod statements;
pub mod types;
pub mod variables;

use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use self::context::AwkContext;
use self::interpreter::AwkInterpreter;
use self::parser::parse;
use self::types::AwkPattern;

pub struct AwkCommand;

/// Process escape sequences in a string (for -F and -v options).
fn process_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => result.push('\t'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('b') => result.push('\x08'),
                Some('f') => result.push('\x0c'),
                Some('a') => result.push('\x07'),
                Some('v') => result.push('\x0b'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[async_trait]
impl Command for AwkCommand {
    fn name(&self) -> &'static str {
        "awk"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        // Handle --help
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: awk [OPTIONS] 'PROGRAM' [FILE...]\n\n\
                 Pattern scanning and text processing language.\n\n\
                 Options:\n  \
                 -F FS      use FS as field separator\n  \
                 -v VAR=VAL assign VAL to variable VAR\n      \
                 --help     display this help and exit\n"
                    .to_string(),
            );
        }

        let mut field_sep = " ".to_string();
        let mut preset_vars: Vec<(String, String)> = Vec::new();
        let mut program_idx: Option<usize> = None;

        // Parse options
        let args = &ctx.args;
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-F" && i + 1 < args.len() {
                i += 1;
                field_sep = process_escapes(&args[i]);
                i += 1;
            } else if arg.starts_with("-F") && arg.len() > 2 {
                field_sep = process_escapes(&arg[2..]);
                i += 1;
            } else if arg == "-v" && i + 1 < args.len() {
                i += 1;
                let assignment = &args[i];
                if let Some(eq_idx) = assignment.find('=') {
                    let var_name = assignment[..eq_idx].to_string();
                    let var_value = process_escapes(&assignment[eq_idx + 1..]);
                    preset_vars.push((var_name, var_value));
                }
                i += 1;
            } else if arg.starts_with("--") {
                return CommandResult::error(format!("awk: unknown option: {}\n", arg));
            } else if arg.starts_with('-') && arg.len() > 1 {
                // Check for unknown single-char options
                let opt_char = arg.chars().nth(1).unwrap();
                if opt_char != 'F' && opt_char != 'v' {
                    return CommandResult::error(format!("awk: unknown option: -{}\n", opt_char));
                }
                i += 1;
            } else {
                // First non-option argument is the program
                program_idx = Some(i);
                break;
            }
        }

        // Check for missing program
        let program_idx = match program_idx {
            Some(idx) => idx,
            None => {
                return CommandResult::error("awk: missing program\n".to_string());
            }
        };

        let program_text = &args[program_idx];
        let files: Vec<String> = args[program_idx + 1..].to_vec();

        // Parse the AWK program
        let ast = match parse(program_text) {
            Ok(ast) => ast,
            Err(e) => {
                return CommandResult::error(format!("awk: {}\n", e));
            }
        };

        // Create context with field separator
        let mut awk_ctx = AwkContext::with_fs(&field_sep);

        // Set preset variables
        for (name, value) in preset_vars {
            awk_ctx.vars.insert(name, value);
        }

        // Set up ARGC/ARGV
        awk_ctx.argc = files.len() + 1;
        awk_ctx.argv.insert("0".to_string(), "awk".to_string());
        for (i, file) in files.iter().enumerate() {
            awk_ctx.argv.insert((i + 1).to_string(), file.clone());
        }

        // Set up ENVIRON from ctx.env
        for (key, value) in &ctx.env {
            awk_ctx.environ.insert(key.clone(), value.clone());
        }

        // Create interpreter
        let mut interp = AwkInterpreter::new(awk_ctx, ast.clone());

        // Execute BEGIN blocks
        interp.execute_begin();

        // Check if we should exit after BEGIN
        if interp.ctx.should_exit {
            // Still run END blocks (AWK semantics)
            interp.execute_end();
            return CommandResult::with_exit_code(
                interp.get_output().to_string(),
                String::new(),
                interp.get_exit_code(),
            );
        }

        // Check if there are main rules or END blocks
        let has_main_rules = ast.rules.iter().any(|rule| {
            !matches!(
                rule.pattern,
                Some(AwkPattern::Begin) | Some(AwkPattern::End)
            )
        });
        let has_end_blocks = ast.rules.iter().any(|rule| {
            matches!(rule.pattern, Some(AwkPattern::End))
        });

        // If no main rules and no END blocks, skip file reading
        if !has_main_rules && !has_end_blocks {
            return CommandResult::with_exit_code(
                interp.get_output().to_string(),
                String::new(),
                interp.get_exit_code(),
            );
        }

        // Collect file contents
        struct FileData {
            filename: String,
            lines: Vec<String>,
        }
        let mut file_data_list: Vec<FileData> = Vec::new();

        if !files.is_empty() {
            for file in &files {
                if file == "-" {
                    // Read from stdin
                    let lines = split_lines(&ctx.stdin);
                    file_data_list.push(FileData {
                        filename: String::new(),
                        lines,
                    });
                } else {
                    let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&file_path).await {
                        Ok(content) => {
                            let lines = split_lines(&content);
                            file_data_list.push(FileData {
                                filename: file.clone(),
                                lines,
                            });
                        }
                        Err(_) => {
                            return CommandResult::error(format!(
                                "awk: {}: No such file or directory\n",
                                file
                            ));
                        }
                    }
                }
            }
        } else {
            // Read from stdin
            let lines = split_lines(&ctx.stdin);
            file_data_list.push(FileData {
                filename: String::new(),
                lines,
            });
        }

        // Process each file
        for file_data in file_data_list {
            interp.ctx.filename = file_data.filename;
            interp.ctx.fnr = 0;
            interp.ctx.should_next_file = false;

            // Store lines for getline support
            interp.ctx.lines = Some(file_data.lines.clone());
            interp.ctx.line_index = Some(0);

            let mut line_idx = 0;
            while line_idx < file_data.lines.len() {
                interp.ctx.line_index = Some(line_idx);
                interp.execute_line(&file_data.lines[line_idx]);

                if interp.ctx.should_exit || interp.ctx.should_next_file {
                    break;
                }

                // Check if getline advanced the line index
                if let Some(new_idx) = interp.ctx.line_index {
                    line_idx = new_idx;
                }
                line_idx += 1;
            }

            if interp.ctx.should_exit {
                break;
            }
        }

        // Execute END blocks (always run, even after exit)
        interp.execute_end();

        CommandResult::with_exit_code(
            interp.get_output().to_string(),
            String::new(),
            interp.get_exit_code(),
        )
    }
}

/// Split content into lines, removing trailing empty line if present.
fn split_lines(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
    if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::collections::HashMap;
    use crate::fs::{InMemoryFs, FileSystem};
    use crate::commands::CommandContext;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
        }
    }

    fn make_ctx_with_fs(args: Vec<&str>, stdin: &str, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    fn make_ctx_with_env(args: Vec<&str>, stdin: &str, env: HashMap<String, String>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env,
            fs: Arc::new(InMemoryFs::new()),
        }
    }

    // ─── Basic Functionality Tests ────────────────────────────────

    #[tokio::test]
    async fn test_print_all_lines() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print }"], "line1\nline2\nline3\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "line1\nline2\nline3\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_print_first_field() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $1 }"], "hello world\nfoo bar\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\nfoo\n");
    }

    #[tokio::test]
    async fn test_print_multiple_fields_with_ofs() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $1, $2 }"], "a b c\nx y z\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a b\nx y\n");
    }

    #[tokio::test]
    async fn test_custom_field_separator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-F:", "{ print $1 }"], "root:x:0:0\nuser:x:1000:1000\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "root\nuser\n");
    }

    #[tokio::test]
    async fn test_custom_field_separator_combined() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-F:", "{ print $2 }"], "a:b:c\nx:y:z\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\ny\n");
    }

    #[tokio::test]
    async fn test_preset_variable() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-v", "x=10", "{ print x }"], "line\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "10\n");
    }

    #[tokio::test]
    async fn test_begin_main_end() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { print \"start\" } { print } END { print \"end\" }"],
            "middle\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "start\nmiddle\nend\n");
    }

    // ─── Pattern Tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_regex_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["/foo/ { print }"], "foo\nbar\nfoobar\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "foo\nfoobar\n");
    }

    #[tokio::test]
    async fn test_expression_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["NR > 2 { print }"], "a\nb\nc\nd\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "c\nd\n");
    }

    #[tokio::test]
    async fn test_range_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["/start/,/end/ { print }"], "before\nstart\nmiddle\nend\nafter\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "start\nmiddle\nend\n");
    }

    #[tokio::test]
    async fn test_pattern_without_action() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["NR == 2"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\n");
    }

    // ─── Operator Tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_arithmetic_operators() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $1 + $2 }"], "3 4\n10 5\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "7\n15\n");
    }

    #[tokio::test]
    async fn test_string_concatenation() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $1 $2 }"], "hello world\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "helloworld\n");
    }

    #[tokio::test]
    async fn test_comparison_operators() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["$1 > 10 { print }"], "5\n15\n8\n20\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "15\n20\n");
    }

    #[tokio::test]
    async fn test_regex_match_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["$1 ~ /^a/ { print }"], "apple\nbanana\napricot\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "apple\napricot\n");
    }

    #[tokio::test]
    async fn test_ternary_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print ($1 > 0 ? \"pos\" : \"neg\") }"], "5\n-3\n0\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "pos\nneg\nneg\n");
    }

    #[tokio::test]
    async fn test_assignment_accumulator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ sum += $1 } END { print sum }"], "1\n2\n3\n4\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "10\n");
    }

    // ─── Control Flow Tests ───────────────────────────────────────

    #[tokio::test]
    async fn test_if_else() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["{ if ($1 > 5) print \"big\"; else print \"small\" }"],
            "3\n8\n5\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "small\nbig\nsmall\n");
    }

    #[tokio::test]
    async fn test_while_loop() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { i=1; while (i <= 3) { print i; i++ } }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_for_loop() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { for (i=1; i<=3; i++) print i }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_for_in_loop() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[1]=10; a[2]=20; for (k in a) sum += a[k]; print sum }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "30\n");
    }

    #[tokio::test]
    async fn test_break_statement() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { for (i=1; i<=10; i++) { if (i > 3) break; print i } }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_continue_statement() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { for (i=1; i<=5; i++) { if (i == 3) continue; print i } }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n4\n5\n");
    }

    #[tokio::test]
    async fn test_next_statement() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["/skip/ { next } { print }"], "a\nskip\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_exit_statement() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print; if (NR == 2) exit }"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_exit_with_code() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { exit 42 }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 42);
    }

    // ─── Function Tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_user_defined_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["function double(x) { return x * 2 } BEGIN { print double(5) }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "10\n");
    }

    #[tokio::test]
    async fn test_length_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print length($0) }"], "hello\nhi\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "5\n2\n");
    }

    #[tokio::test]
    async fn test_substr_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print substr($0, 2, 3) }"], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ell\n");
    }

    #[tokio::test]
    async fn test_index_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print index($0, \"ll\") }"], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "3\n");
    }

    #[tokio::test]
    async fn test_split_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["{ n = split($0, arr, \":\"); print n, arr[1], arr[2] }"],
            "a:b:c\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "3 a b\n");
    }

    #[tokio::test]
    async fn test_sub_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ sub(/o/, \"0\"); print }"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "f0o\n");
    }

    #[tokio::test]
    async fn test_gsub_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ gsub(/o/, \"0\"); print }"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "f00\n");
    }

    #[tokio::test]
    async fn test_match_function() {
        let cmd = AwkCommand;
        // Test match function with string pattern (regex literals need special handling)
        let ctx = make_ctx(
            vec!["{ print match($0, \"abc\") }"],
            "xyzabcdef\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "4\n");
    }

    #[tokio::test]
    async fn test_tolower_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print tolower($0) }"], "HELLO\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\n");
    }

    #[tokio::test]
    async fn test_toupper_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print toupper($0) }"], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "HELLO\n");
    }

    #[tokio::test]
    async fn test_int_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { print int(3.7) }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "3\n");
    }

    #[tokio::test]
    async fn test_sqrt_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { print sqrt(16) }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "4\n");
    }

    #[tokio::test]
    async fn test_sprintf_function() {
        let cmd = AwkCommand;
        // Test basic sprintf functionality
        let ctx = make_ctx(vec!["BEGIN { print sprintf(\"%d\", 42) }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "42\n");
    }

    #[tokio::test]
    async fn test_printf_statement() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { printf \"%s=%d\\n\", \"x\", 10 }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "x=10\n");
    }


    // ─── Array Tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_associative_array() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[\"x\"] = 1; a[\"y\"] = 2; print a[\"x\"], a[\"y\"] }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1 2\n");
    }

    #[tokio::test]
    async fn test_delete_array_element() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[1]=10; a[2]=20; delete a[1]; for (k in a) print k, a[k] }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "2 20\n");
    }

    #[tokio::test]
    async fn test_delete_entire_array() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[1]=10; a[2]=20; delete a; for (k in a) print k }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_in_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[1]=10; if (1 in a) print \"yes\"; if (2 in a) print \"no\" }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "yes\n");
    }

    #[tokio::test]
    async fn test_multi_dimensional_array() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { a[1,2] = 10; print a[1,2] }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "10\n");
    }

    // ─── Field Tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_field_zero_is_entire_line() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $0 }"], "hello world\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello world\n");
    }

    #[tokio::test]
    async fn test_modify_field_rebuilds_line() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ $2 = \"X\"; print $0 }"], "a b c\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a X c\n");
    }

    #[tokio::test]
    async fn test_modify_line_resplits_fields() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ $0 = \"x y z\"; print $2 }"], "a b c\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "y\n");
    }

    #[tokio::test]
    async fn test_nf_is_last_field() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $NF }"], "a b c\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "c\n");
    }

    // ─── Built-in Variable Tests ──────────────────────────────────

    #[tokio::test]
    async fn test_nr_variable() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print NR }"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_nf_variable() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print NF }"], "a b c\nx y\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "3\n2\n");
    }

    #[tokio::test]
    async fn test_fs_ofs_ors() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { FS=\":\"; OFS=\"-\"; ORS=\"|\" } { print $1, $2 }"],
            "a:b\nx:y\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a-b|x-y|");
    }

    #[tokio::test]
    async fn test_argc_argv() {
        // Test ARGC/ARGV with no files - ARGC=1, ARGV[0]="awk"
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { print ARGC, ARGV[0], ARGV[1] }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1 awk \n");
    }

    #[tokio::test]
    async fn test_environ_variable() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "hello".to_string());
        let cmd = AwkCommand;
        let ctx = make_ctx_with_env(
            vec!["BEGIN { print ENVIRON[\"MY_VAR\"] }"],
            "",
            env,
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\n");
    }

    // ─── Edge Case Tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_input() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_multiple_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file1.txt", b"a\nb\n").await.unwrap();
        fs.write_file("/file2.txt", b"c\nd\n").await.unwrap();
        let cmd = AwkCommand;
        let ctx = CommandContext {
            args: vec!["{ print }".to_string(), "/file1.txt".to_string(), "/file2.txt".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\nc\nd\n");
    }

    #[tokio::test]
    async fn test_missing_program_error() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec![], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing program"));
    }

    #[tokio::test]
    async fn test_parse_error() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print "], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(!result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print }", "nonexistent.txt"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file"));
    }

    #[tokio::test]
    async fn test_help_flag() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["--help"], "");
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("Usage:"));
        assert!(result.stdout.contains("awk"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_unknown_option() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-x", "{ print }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("unknown option"));
    }

    // ─── Additional Tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_begin_only_no_input() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { print \"hello\" }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\n");
    }

    #[tokio::test]
    async fn test_end_only() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["END { print NR }"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "3\n");
    }

    #[tokio::test]
    async fn test_multiple_rules() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["/a/ { print \"has a\" } /b/ { print \"has b\" }"],
            "ab\na\nb\n",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "has a\nhas b\nhas a\nhas b\n");
    }

    #[tokio::test]
    async fn test_logical_and_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["NR > 1 && NR < 4 { print }"], "a\nb\nc\nd\ne\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\nc\n");
    }

    #[tokio::test]
    async fn test_logical_or_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["NR == 1 || NR == 3 { print }"], "a\nb\nc\nd\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nc\n");
    }

    #[tokio::test]
    async fn test_negation_pattern() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["!/skip/ { print }"], "a\nskip\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_modulo_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ print $1 % 3 }"], "10\n7\n9\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n1\n0\n");
    }

    #[tokio::test]
    async fn test_exponentiation() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { print 2^10 }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1024\n");
    }

    #[tokio::test]
    async fn test_pre_increment() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { x=5; print ++x, x }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "6 6\n");
    }

    #[tokio::test]
    async fn test_post_increment() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { x=5; print x++, x }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "5 6\n");
    }

    #[tokio::test]
    async fn test_unary_minus() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { x=5; print -x }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "-5\n");
    }

    #[tokio::test]
    async fn test_not_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["BEGIN { print !0, !1, !\"\" }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1 0 1\n");
    }

    #[tokio::test]
    async fn test_string_comparison() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["$1 == \"foo\" { print \"match\" }"], "foo\nbar\nfoo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "match\nmatch\n");
    }

    #[tokio::test]
    async fn test_not_match_operator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["$1 !~ /^a/ { print }"], "apple\nbanana\napricot\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "banana\n");
    }

    #[tokio::test]
    async fn test_do_while_loop() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { i=1; do { print i; i++ } while (i <= 3) }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_nested_loops() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["BEGIN { for (i=1; i<=2; i++) for (j=1; j<=2; j++) print i, j }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1 1\n1 2\n2 1\n2 2\n");
    }

    #[tokio::test]
    async fn test_recursive_function() {
        let cmd = AwkCommand;
        let ctx = make_ctx(
            vec!["function fact(n) { return n <= 1 ? 1 : n * fact(n-1) } BEGIN { print fact(5) }"],
            "",
        );
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "120\n");
    }

    #[tokio::test]
    async fn test_gsub_return_value() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ n = gsub(/o/, \"0\"); print n, $0 }"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "2 f00\n");
    }

    #[tokio::test]
    async fn test_sub_with_ampersand() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["{ sub(/o/, \"[&]\"); print }"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "f[o]o\n");
    }

    #[tokio::test]
    async fn test_escape_in_field_separator() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-F", "\\t", "{ print $1, $2 }"], "a\tb\tc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a b\n");
    }

    #[tokio::test]
    async fn test_escape_in_variable() {
        let cmd = AwkCommand;
        let ctx = make_ctx(vec!["-v", "x=a\\tb", "BEGIN { print x }"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\tb\n");
    }
}
