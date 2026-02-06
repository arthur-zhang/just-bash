// src/commands/sed/mod.rs
pub mod lexer;
pub mod parser;
pub mod types;
pub mod regex_utils;
pub mod executor;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::FileSystem;
use self::types::{SedCmd, RangeState, ExecuteContext};
use self::parser::parse_scripts;
use self::executor::{create_initial_state, execute_commands};

pub struct SedCommand;

struct ProcessResult {
    output: String,
    exit_code: Option<i32>,
    error_message: Option<String>,
}

#[async_trait]
impl Command for SedCommand {
    fn name(&self) -> &'static str {
        "sed"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: sed [OPTION]... {script} [input-file]...\n\n\
                 Stream editor for filtering and transforming text.\n\n\
                 Options:\n  \
                 -n, --quiet, --silent  suppress automatic printing of pattern space\n  \
                 -e script              add the script to commands to be executed\n  \
                 -f script-file         read script from file\n  \
                 -i, --in-place         edit files in place\n  \
                 -E, -r, --regexp-extended  use extended regular expressions\n      \
                 --help             display this help and exit\n"
                    .to_string(),
            );
        }

        let mut scripts: Vec<String> = Vec::new();
        let mut script_files: Vec<String> = Vec::new();
        let mut silent = false;
        let mut in_place = false;
        let mut extended_regex = false;
        let mut files: Vec<String> = Vec::new();

        let args = &ctx.args;
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-n" || arg == "--quiet" || arg == "--silent" {
                silent = true;
            } else if arg == "-i" || arg == "--in-place" {
                in_place = true;
            } else if arg.starts_with("-i") && arg.len() > 2 {
                in_place = true;
            } else if arg == "-E" || arg == "-r" || arg == "--regexp-extended" {
                extended_regex = true;
            } else if arg == "-e" {
                if i + 1 < args.len() {
                    i += 1;
                    scripts.push(args[i].clone());
                }
            } else if arg == "-f" {
                if i + 1 < args.len() {
                    i += 1;
                    script_files.push(args[i].clone());
                }
            } else if arg.starts_with("--") {
                return CommandResult::error(format!("sed: unknown option: {}\n", arg));
            } else if arg == "-" {
                files.push(arg.clone());
            } else if arg.starts_with('-') && arg.len() > 1 {
                let chars: Vec<char> = arg[1..].chars().collect();
                let mut needs_next_arg = false;
                let mut next_is_for = ' ';
                for &c in &chars {
                    match c {
                        'n' => silent = true,
                        'i' => in_place = true,
                        'E' | 'r' => extended_regex = true,
                        'e' => { needs_next_arg = true; next_is_for = 'e'; }
                        'f' => { needs_next_arg = true; next_is_for = 'f'; }
                        _ => {
                            return CommandResult::error(
                                format!("sed: unknown option: -{}\n", c),
                            );
                        }
                    }
                }
                if needs_next_arg && i + 1 < args.len() {
                    i += 1;
                    if next_is_for == 'e' {
                        scripts.push(args[i].clone());
                    } else {
                        script_files.push(args[i].clone());
                    }
                }
            } else if scripts.is_empty() && script_files.is_empty() {
                scripts.push(arg.clone());
            } else {
                files.push(arg.clone());
            }
            i += 1;
        }

        // Read scripts from -f files
        for script_file in &script_files {
            let script_path = ctx.fs.resolve_path(&ctx.cwd, script_file);
            match ctx.fs.read_file(&script_path).await {
                Ok(content) => {
                    for line in content.split('\n') {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            scripts.push(trimmed.to_string());
                        }
                    }
                }
                Err(_) => {
                    return CommandResult::error(format!(
                        "sed: couldn't open file {}: No such file or directory\n",
                        script_file
                    ));
                }
            }
        }

        if scripts.is_empty() {
            return CommandResult::error("sed: no script specified\n".to_string());
        }

        let script_refs: Vec<&str> = scripts.iter().map(|s| s.as_str()).collect();
        let parse_result = parse_scripts(&script_refs, extended_regex);
        if let Some(err) = parse_result.error {
            return CommandResult::error(format!("sed: {}\n", err));
        }

        let commands = parse_result.commands;
        let effective_silent = silent || parse_result.silent_mode;

        if in_place {
            if files.is_empty() {
                return CommandResult::error(
                    "sed: -i requires at least one file argument\n".to_string(),
                );
            }
            for file in &files {
                if file == "-" { continue; }
                let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&file_path).await {
                    Ok(file_content) => {
                        let result = process_content(
                            &file_content, &commands, effective_silent,
                            Some(file.as_str()), &ctx.fs, &ctx.cwd,
                        ).await;
                        if let Some(ref err_msg) = result.error_message {
                            return CommandResult::error(format!("{}\n", err_msg));
                        }
                        let _ = ctx.fs.write_file(&file_path, result.output.as_bytes()).await;
                    }
                    Err(_) => {
                        return CommandResult::error(format!(
                            "sed: {}: No such file or directory\n", file
                        ));
                    }
                }
            }
            return CommandResult::success(String::new());
        }

        if files.is_empty() {
            let result = process_content(
                &ctx.stdin, &commands, effective_silent, None, &ctx.fs, &ctx.cwd,
            ).await;
            return CommandResult::with_exit_code(
                result.output,
                result.error_message.map(|e| format!("{}\n", e)).unwrap_or_default(),
                result.exit_code.unwrap_or(0),
            );
        }

        let mut content = String::new();
        let mut stdin_consumed = false;
        for file in &files {
            let file_content: String;
            if file == "-" {
                if stdin_consumed {
                    file_content = String::new();
                } else {
                    file_content = ctx.stdin.clone();
                    stdin_consumed = true;
                }
            } else {
                let file_path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&file_path).await {
                    Ok(c) => file_content = c,
                    Err(_) => {
                        return CommandResult::error(format!(
                            "sed: {}: No such file or directory\n", file
                        ));
                    }
                }
            }
            if !content.is_empty() && !file_content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&file_content);
        }

        let result = process_content(
            &content, &commands, effective_silent,
            if files.len() == 1 { Some(files[0].as_str()) } else { None },
            &ctx.fs, &ctx.cwd,
        ).await;
        CommandResult::with_exit_code(
            result.output,
            result.error_message.map(|e| format!("{}\n", e)).unwrap_or_default(),
            result.exit_code.unwrap_or(0),
        )
    }
}

async fn process_content(
    content: &str,
    commands: &[SedCmd],
    silent: bool,
    filename: Option<&str>,
    fs: &Arc<dyn FileSystem>,
    cwd: &str,
) -> ProcessResult {
    let input_ends_with_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.split('\n').collect();
    if !lines.is_empty() && lines.last() == Some(&"") {
        lines.pop();
    }

    let total_lines = lines.len();
    let mut output = String::new();
    let mut exit_code: Option<i32> = None;
    let mut last_output_was_auto_print = false;

    let mut hold_space = String::new();
    let mut last_pattern: Option<String> = None;
    let mut range_states: HashMap<String, RangeState> = HashMap::new();
    let mut file_writes: HashMap<String, String> = HashMap::new();

    let lines_owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let mut line_index = 0;

    while line_index < lines_owned.len() {
        let mut state = create_initial_state(total_lines, filename, range_states.clone());
        state.pattern_space = lines_owned[line_index].clone();
        state.hold_space = hold_space.clone();
        state.last_pattern = last_pattern.clone();
        state.line_number = line_index + 1;
        state.substitution_made = false;

        let mut ctx = ExecuteContext {
            lines: lines_owned.clone(),
            current_line_index: line_index,
        };

        let mut cycle_iterations = 0;
        let max_cycle_iterations = 10000;
        state.lines_consumed_in_cycle = 0;

        loop {
            cycle_iterations += 1;
            if cycle_iterations > max_cycle_iterations {
                break;
            }

            state.restart_cycle = false;
            state.pending_file_reads.clear();
            state.pending_file_writes.clear();

            execute_commands(commands, &mut state, &mut ctx);

            // Process pending file reads
            for read in &state.pending_file_reads {
                let file_path = fs.resolve_path(cwd, &read.filename);
                if let Ok(file_content) = fs.read_file(&file_path).await {
                    if read.whole_file {
                        let trimmed = file_content.trim_end_matches('\n');
                        state.append_buffer.push(trimmed.to_string());
                    }
                }
            }

            // Accumulate file writes
            for write in &state.pending_file_writes {
                let file_path = fs.resolve_path(cwd, &write.filename);
                let existing = file_writes.entry(file_path).or_insert_with(String::new);
                existing.push_str(&write.content);
            }

            if !state.restart_cycle || state.deleted || state.quit || state.quit_silent {
                break;
            }
        }

        line_index += state.lines_consumed_in_cycle;
        hold_space = state.hold_space.clone();
        last_pattern = state.last_pattern.clone();
        range_states = state.range_states.clone();

        // Output from n command
        if !silent {
            for ln in &state.n_command_output {
                output.push_str(ln);
                output.push('\n');
            }
        }

        // Output line numbers from = command, l command, p command
        let had_line_number_output = !state.line_number_output.is_empty();
        for ln in &state.line_number_output {
            output.push_str(ln);
            output.push('\n');
        }

        // Handle insert commands (marked with __INSERT__ prefix)
        let mut inserts: Vec<String> = Vec::new();
        let mut appends: Vec<String> = Vec::new();
        for item in &state.append_buffer {
            if let Some(text) = item.strip_prefix("__INSERT__") {
                inserts.push(text.to_string());
            } else {
                appends.push(item.clone());
            }
        }

        for text in &inserts {
            output.push_str(text);
            output.push('\n');
        }

        let mut had_pattern_space_output = false;
        if !state.deleted && !state.quit_silent {
            if silent {
                if state.printed {
                    output.push_str(&state.pattern_space);
                    output.push('\n');
                    had_pattern_space_output = true;
                }
            } else {
                output.push_str(&state.pattern_space);
                output.push('\n');
                had_pattern_space_output = true;
            }
        } else if state.changed_text.is_some() {
            output.push_str(state.changed_text.as_ref().unwrap());
            output.push('\n');
            had_pattern_space_output = true;
        }

        for text in &appends {
            output.push_str(text);
            output.push('\n');
        }

        let had_output = had_line_number_output || had_pattern_space_output;
        last_output_was_auto_print = had_output && appends.is_empty();

        if state.quit || state.quit_silent {
            if state.exit_code.is_some() {
                exit_code = state.exit_code;
            }
            if state.error_message.is_some() {
                return ProcessResult {
                    output: String::new(),
                    exit_code: Some(exit_code.unwrap_or(1)),
                    error_message: state.error_message,
                };
            }
            break;
        }

        line_index += 1;
    }

    // Flush file writes
    for (file_path, file_content) in &file_writes {
        let _ = fs.write_file(file_path, file_content.as_bytes()).await;
    }

    // Strip trailing newline if input didn't have one and last output was auto-print
    if !input_ends_with_newline && last_output_was_auto_print && output.ends_with('\n') {
        output.pop();
    }

    ProcessResult {
        output,
        exit_code,
        error_message: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::collections::HashMap;
    use crate::fs::InMemoryFs;
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

    #[tokio::test]
    async fn test_basic_substitution() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/world/rust/"], "hello world\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello rust\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_global_substitution() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/o/0/g"], "foo boo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "f00 b00\n");
    }

    #[tokio::test]
    async fn test_silent_mode_with_print() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "2p"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\n");
    }

    #[tokio::test]
    async fn test_line_range_delete() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["1,2d"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "c\n");
    }

    #[tokio::test]
    async fn test_pattern_match_delete() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["/foo/d"], "foo\nbar\nfoo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "bar\n");
    }

    #[tokio::test]
    async fn test_multiple_expressions() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-e", "s/a/x/", "-e", "s/b/y/"], "ab\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "xy\n");
    }

    #[tokio::test]
    async fn test_in_place_editing() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", b"old content\n").await.unwrap();
        let cmd = SedCommand;
        let ctx = make_ctx_with_fs(vec!["-i", "s/old/new/", "/test.txt"], "", fs.clone());
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/test.txt").await.unwrap();
        assert_eq!(content, "new content\n");
    }

    #[tokio::test]
    async fn test_extended_regex() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-E", "s/[0-9]+/NUM/"], "abc123\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "abcNUM\n");
    }

    #[tokio::test]
    async fn test_hold_space_workflow() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "1h;2{H;g;p}"], "a\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_append_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2a\\ new"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\nnew\nc\n");
    }

    #[tokio::test]
    async fn test_insert_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2i\\ new"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nnew\nb\nc\n");
    }

    #[tokio::test]
    async fn test_change_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2c\\ new"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nnew\nc\n");
    }

    #[tokio::test]
    async fn test_quit_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2q"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_quit_silent_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2Q"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\n");
    }

    #[tokio::test]
    async fn test_step_address() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "0~2p"], "a\nb\nc\nd\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\nd\n");
    }

    #[tokio::test]
    async fn test_transliterate() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["y/abc/ABC/"], "abc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "ABC\n");
    }

    #[tokio::test]
    async fn test_delete_first_line_cycle() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["N;P;D"], "1\n2\n3\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn test_empty_script() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec![""], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello\n");
    }

    #[tokio::test]
    async fn test_no_script_error() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec![], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("no script"));
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/a/b/", "nonexistent"], "");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file"));
    }

    #[tokio::test]
    async fn test_case_insensitive() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/HELLO/hi/i"], "Hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hi\n");
    }

    #[tokio::test]
    async fn test_nth_occurrence() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/a/X/2"], "aaa\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "aXa\n");
    }

    #[tokio::test]
    async fn test_backreferences() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/\\(hel\\)\\(lo\\)/\\2\\1/"], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "lohel\n");
    }

    #[tokio::test]
    async fn test_ampersand_replacement() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/world/[&]/"], "hello world\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello [world]\n");
    }

    #[tokio::test]
    async fn test_negated_address() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["2!d"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\n");
    }

    #[tokio::test]
    async fn test_pattern_range() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["/start/,/end/d"], "a\nstart\nmid\nend\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_last_line_address() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["$d"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\nb\n");
    }

    #[tokio::test]
    async fn test_list_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "l"], "a\tb\n");
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("\\t"));
        assert!(result.stdout.contains("$"));
    }

    #[tokio::test]
    async fn test_line_number_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["="], "a\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "1\na\n2\nb\n");
    }

    #[tokio::test]
    async fn test_zap_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["z"], "hello\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "\n");
    }

    #[tokio::test]
    async fn test_print_first_line() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "N;P"], "a\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "a\n");
    }

    #[tokio::test]
    async fn test_grouped_commands() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "2{ p; = }"], "a\nb\nc\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "b\n2\n");
    }

    #[tokio::test]
    async fn test_substitution_tracking_t() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/a/x/;t end;d;:end"], "abc\nxyz\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "xbc\n");
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn test_substitution_tracking_T() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/a/x/;T end;p;:end"], "abc\nxyz\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "xbc\nxbc\nxyz\n");
    }

    #[tokio::test]
    async fn test_read_file_command() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/append.txt", b"appended").await.unwrap();
        let cmd = SedCommand;
        let ctx = make_ctx_with_fs(vec!["r /append.txt"], "line\n", fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "line\nappended\n");
    }

    #[tokio::test]
    async fn test_write_file_command() {
        let fs = Arc::new(InMemoryFs::new());
        let cmd = SedCommand;
        let ctx = make_ctx_with_fs(vec!["w /output.txt"], "hello\n", fs.clone());
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/output.txt").await.unwrap();
        assert_eq!(content, "hello\n");
    }

    #[tokio::test]
    async fn test_stdin_marker() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/a/b/", "-"], "aaa\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "baa\n");
    }

    #[tokio::test]
    async fn test_multiple_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file1.txt", b"a\n").await.unwrap();
        fs.write_file("/file2.txt", b"b\n").await.unwrap();
        let cmd = SedCommand;
        let ctx = make_ctx_with_fs(vec!["s/./X/", "/file1.txt", "/file2.txt"], "", fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "X\nX\n");
    }

    #[tokio::test]
    async fn test_bre_mode() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/\\(foo\\)/[\\1]/"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "[foo]\n");
    }

    #[tokio::test]
    async fn test_ere_mode() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-E", "s/(foo)/[\\1]/"], "foo\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "[foo]\n");
    }

    #[tokio::test]
    async fn test_posix_classes() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["s/[[:digit:]]/X/g"], "a1b2\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "aXbX\n");
    }

    #[tokio::test]
    async fn test_help_flag() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["--help"], "");
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("Usage:"));
        assert!(result.stdout.contains("sed"));
    }

    #[tokio::test]
    async fn test_script_file() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/script.sed", b"s/old/new/").await.unwrap();
        let cmd = SedCommand;
        let ctx = make_ctx_with_fs(vec!["-f", "/script.sed"], "old text\n", fs);
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "new text\n");
    }

    #[tokio::test]
    async fn test_branch_to_end() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["/foo/b;d"], "foo\nbar\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "foo\n");
    }

    #[tokio::test]
    async fn test_exchange_command() {
        let cmd = SedCommand;
        let ctx = make_ctx(vec!["-n", "x;p"], "a\nb\n");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "\na\n");
    }
}