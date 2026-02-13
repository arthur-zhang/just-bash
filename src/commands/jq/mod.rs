// src/commands/jq/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::query_engine::{Value, parse, evaluate};
use crate::commands::query_engine::context::{EvalContext, JqError};

pub struct JqCommand;

// ---------------------------------------------------------------------------
// JSON stream parser - handles concatenated JSON values
// ---------------------------------------------------------------------------

fn parse_json_stream(input: &str) -> Result<Vec<Value>, String> {
    let mut results = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        // Skip whitespace
        while pos < len && (bytes[pos] as char).is_whitespace() {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        let start_pos = pos;
        let ch = bytes[pos] as char;

        if ch == '{' || ch == '[' {
            let open = ch;
            let close = if ch == '{' { '}' } else { ']' };
            let mut depth = 1i32;
            let mut in_string = false;
            let mut is_escaped = false;
            pos += 1;

            while pos < len && depth > 0 {
                let c = bytes[pos] as char;
                if is_escaped {
                    is_escaped = false;
                } else if c == '\\' {
                    is_escaped = true;
                } else if c == '"' {
                    in_string = !in_string;
                } else if !in_string {
                    if c == open {
                        depth += 1;
                    } else if c == close {
                        depth -= 1;
                    }
                }
                pos += 1;
            }

            if depth != 0 {
                return Err(format!(
                    "Unexpected end of JSON input at position {} (unclosed {})",
                    pos, open
                ));
            }

            let slice = &input[start_pos..pos];
            let serde_val: serde_json::Value = serde_json::from_str(slice)
                .map_err(|e| format!("Invalid JSON: {}", e))?;
            results.push(Value::from_serde_json(serde_val));
        } else if ch == '"' {
            // Parse string
            let mut is_escaped = false;
            pos += 1;
            while pos < len {
                let c = bytes[pos] as char;
                if is_escaped {
                    is_escaped = false;
                } else if c == '\\' {
                    is_escaped = true;
                } else if c == '"' {
                    pos += 1;
                    break;
                }
                pos += 1;
            }
            let slice = &input[start_pos..pos];
            let serde_val: serde_json::Value = serde_json::from_str(slice)
                .map_err(|e| format!("Invalid JSON: {}", e))?;
            results.push(Value::from_serde_json(serde_val));
        } else if ch == '-' || ch.is_ascii_digit() {
            // Parse number
            while pos < len && matches!(bytes[pos] as char, '0'..='9' | '.' | 'e' | 'E' | '+' | '-') {
                pos += 1;
            }
            let slice = &input[start_pos..pos];
            let serde_val: serde_json::Value = serde_json::from_str(slice)
                .map_err(|e| format!("Invalid JSON: {}", e))?;
            results.push(Value::from_serde_json(serde_val));
        } else if input[pos..].starts_with("true") {
            results.push(Value::Bool(true));
            pos += 4;
        } else if input[pos..].starts_with("false") {
            results.push(Value::Bool(false));
            pos += 5;
        } else if input[pos..].starts_with("null") {
            results.push(Value::Null);
            pos += 4;
        } else {
            let context_end = std::cmp::min(pos + 10, len);
            let context = &input[pos..context_end];
            let word = context.split_whitespace().next().unwrap_or(context);
            return Err(format!(
                "Invalid JSON at position {}: unexpected '{}'",
                start_pos, word
            ));
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// JSON formatting
// ---------------------------------------------------------------------------

fn format_json_string(s: &str) -> String {
    let mut result = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return "null".to_string();
    }
    if n == (n as i64) as f64 && n.abs() < 1e18 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn format_value(
    v: &Value,
    compact: bool,
    raw: bool,
    sort_keys: bool,
    use_tab: bool,
    indent: usize,
) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("{}", b),
        Value::Number(n) => format_number(*n),
        Value::String(s) => {
            if raw {
                s.clone()
            } else {
                format_json_string(s)
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            if compact {
                let items: Vec<String> = arr
                    .iter()
                    .map(|x| format_value(x, true, false, sort_keys, use_tab, 0))
                    .collect();
                return format!("[{}]", items.join(","));
            }
            let indent_str = if use_tab { "\t" } else { "  " };
            let items: Vec<String> = arr
                .iter()
                .map(|x| {
                    format!(
                        "{}{}",
                        indent_str.repeat(indent + 1),
                        format_value(x, false, false, sort_keys, use_tab, indent + 1)
                    )
                })
                .collect();
            format!(
                "[\n{}\n{}]",
                items.join(",\n"),
                indent_str.repeat(indent)
            )
        }
        Value::Object(obj) => {
            let keys: Vec<&String> = if sort_keys {
                let mut ks: Vec<&String> = obj.keys().collect();
                ks.sort();
                ks
            } else {
                obj.keys().collect()
            };
            if keys.is_empty() {
                return "{}".to_string();
            }
            if compact {
                let items: Vec<String> = keys
                    .iter()
                    .map(|k| {
                        format!(
                            "{}:{}",
                            format_json_string(k),
                            format_value(
                                obj.get(*k).unwrap(),
                                true,
                                false,
                                sort_keys,
                                use_tab,
                                0,
                            )
                        )
                    })
                    .collect();
                return format!("{{{}}}", items.join(","));
            }
            let indent_str = if use_tab { "\t" } else { "  " };
            let items: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}{}: {}",
                        indent_str.repeat(indent + 1),
                        format_json_string(k),
                        format_value(
                            obj.get(*k).unwrap(),
                            false,
                            false,
                            sort_keys,
                            use_tab,
                            indent + 1,
                        )
                    )
                })
                .collect();
            format!(
                "{{\n{}\n{}}}",
                items.join(",\n"),
                indent_str.repeat(indent)
            )
        }
    }
}

// ---------------------------------------------------------------------------
// CLI options
// ---------------------------------------------------------------------------

struct JqOptions {
    raw: bool,
    compact: bool,
    exit_status: bool,
    slurp: bool,
    null_input: bool,
    join_output: bool,
    sort_keys: bool,
    use_tab: bool,
    filter: String,
    files: Vec<String>,
}

fn parse_jq_args(args: &[String]) -> Result<JqOptions, CommandResult> {
    let mut raw = false;
    let mut compact = false;
    let mut exit_status = false;
    let mut slurp = false;
    let mut null_input = false;
    let mut join_output = false;
    let mut sort_keys = false;
    let mut use_tab = false;
    let mut filter = ".".to_string();
    let mut filter_set = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-r" || a == "--raw-output" {
            raw = true;
        } else if a == "-c" || a == "--compact-output" {
            compact = true;
        } else if a == "-e" || a == "--exit-status" {
            exit_status = true;
        } else if a == "-s" || a == "--slurp" {
            slurp = true;
        } else if a == "-n" || a == "--null-input" {
            null_input = true;
        } else if a == "-j" || a == "--join-output" {
            join_output = true;
        } else if a == "-a" || a == "--ascii" {
            // ignored
        } else if a == "-S" || a == "--sort-keys" {
            sort_keys = true;
        } else if a == "-C" || a == "--color" {
            // ignored
        } else if a == "-M" || a == "--monochrome" {
            // ignored
        } else if a == "--tab" {
            use_tab = true;
        } else if a == "-" {
            files.push("-".to_string());
        } else if a.starts_with("--") {
            return Err(CommandResult::with_exit_code(
                String::new(),
                format!("jq: Unknown option: {}\n", a),
                2,
            ));
        } else if a.starts_with('-') {
            // Combined short flags like -rc
            for c in a[1..].chars() {
                match c {
                    'r' => raw = true,
                    'c' => compact = true,
                    'e' => exit_status = true,
                    's' => slurp = true,
                    'n' => null_input = true,
                    'j' => join_output = true,
                    'a' => { /* ignored */ }
                    'S' => sort_keys = true,
                    'C' => { /* ignored */ }
                    'M' => { /* ignored */ }
                    _ => {
                        return Err(CommandResult::with_exit_code(
                            String::new(),
                            format!("jq: Unknown option: -{}\n", c),
                            2,
                        ));
                    }
                }
            }
        } else if !filter_set {
            filter = a.clone();
            filter_set = true;
        } else {
            files.push(a.clone());
        }
        i += 1;
    }

    Ok(JqOptions {
        raw,
        compact,
        exit_status,
        slurp,
        null_input,
        join_output,
        sort_keys,
        use_tab,
        filter,
        files,
    })
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

const JQ_HELP: &str = "\
Usage: jq [OPTIONS] FILTER [FILE]

command-line JSON processor

Options:
  -r, --raw-output  output strings without quotes
  -c, --compact     compact output (no pretty printing)
  -e, --exit-status set exit status based on output
  -s, --slurp       read entire input into array
  -n, --null-input  don't read any input
  -j, --join-output don't print newlines after each output
  -a, --ascii       force ASCII output
  -S, --sort-keys   sort object keys
  -C, --color       colorize output (ignored)
  -M, --monochrome  monochrome output (ignored)
      --tab         use tabs for indentation
      --help        display this help and exit
";

// ---------------------------------------------------------------------------
// Command implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Command for JqCommand {
    fn name(&self) -> &'static str {
        "jq"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        // Check for --help
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(JQ_HELP.to_string());
        }

        let opts = match parse_jq_args(&ctx.args) {
            Ok(o) => o,
            Err(r) => return r,
        };

        // Build list of inputs
        let mut inputs: Vec<(String, String)> = Vec::new(); // (source, content)
        if opts.null_input {
            // No input
        } else if opts.files.is_empty()
            || (opts.files.len() == 1 && opts.files[0] == "-")
        {
            inputs.push(("stdin".to_string(), ctx.stdin.clone()));
        } else {
            for file in &opts.files {
                if file == "-" {
                    inputs.push(("stdin".to_string(), ctx.stdin.clone()));
                } else {
                    let path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.read_file(&path).await {
                        Ok(content) => {
                            inputs.push((file.clone(), content));
                        }
                        Err(_) => {
                            return CommandResult::with_exit_code(
                                String::new(),
                                format!(
                                    "jq: {}: No such file or directory\n",
                                    file
                                ),
                                2,
                            );
                        }
                    }
                }
            }
        }

        // Parse the filter
        let ast = match parse(&opts.filter) {
            Ok(a) => a,
            Err(e) => {
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("jq: parse error: {}\n", e),
                    5,
                );
            }
        };

        let mut eval_ctx = EvalContext::with_env(ctx.env.clone());

        // Evaluate
        let values: Vec<Value> = if opts.null_input {
            match evaluate(&Value::Null, &ast, &mut eval_ctx) {
                Ok(v) => v,
                Err(e) => return jq_error_result(e),
            }
        } else if opts.slurp {
            let mut items: Vec<Value> = Vec::new();
            for (_source, content) in &inputs {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    match parse_json_stream(trimmed) {
                        Ok(parsed) => items.extend(parsed),
                        Err(e) => {
                            return CommandResult::with_exit_code(
                                String::new(),
                                format!("jq: parse error: {}\n", e),
                                5,
                            );
                        }
                    }
                }
            }
            let arr = Value::Array(items);
            match evaluate(&arr, &ast, &mut eval_ctx) {
                Ok(v) => v,
                Err(e) => return jq_error_result(e),
            }
        } else {
            let mut all_values: Vec<Value> = Vec::new();
            for (_source, content) in &inputs {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let json_values = match parse_json_stream(trimmed) {
                    Ok(v) => v,
                    Err(e) => {
                        return CommandResult::with_exit_code(
                            String::new(),
                            format!("jq: parse error: {}\n", e),
                            5,
                        );
                    }
                };
                for json_val in &json_values {
                    match evaluate(json_val, &ast, &mut eval_ctx) {
                        Ok(v) => all_values.extend(v),
                        Err(e) => return jq_error_result(e),
                    }
                }
            }
            all_values
        };

        // Format output
        let formatted: Vec<String> = values
            .iter()
            .map(|v| {
                format_value(
                    v,
                    opts.compact,
                    opts.raw,
                    opts.sort_keys,
                    opts.use_tab,
                    0,
                )
            })
            .collect();

        let separator = if opts.join_output { "" } else { "\n" };
        let output = formatted.join(separator);

        let exit_code = if opts.exit_status
            && (values.is_empty()
                || values.iter().all(|v| {
                    matches!(v, Value::Null | Value::Bool(false))
                }))
        {
            1
        } else {
            0
        };

        let stdout = if output.is_empty() {
            String::new()
        } else if opts.join_output {
            output
        } else {
            format!("{}\n", output)
        };

        CommandResult::with_exit_code(stdout, String::new(), exit_code)
    }
}

fn jq_error_result(e: JqError) -> CommandResult {
    match e {
        JqError::ExecutionLimit(msg) => CommandResult::with_exit_code(
            String::new(),
            format!("jq: {}\n", msg),
            5,
        ),
        JqError::Runtime(msg) if msg.contains("Unknown function") => {
            CommandResult::with_exit_code(
                String::new(),
                format!("jq: error: {}\n", msg),
                3,
            )
        }
        _ => CommandResult::with_exit_code(
            String::new(),
            format!("jq: parse error: {}\n", e),
            5,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: &[&str], stdin: &str) -> CommandContext {
        CommandContext {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    async fn make_ctx_with_files(
        args: &[&str],
        stdin: &str,
        files: &[(&str, &str)],
    ) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content.as_bytes()).await.unwrap();
        }
        CommandContext {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_jq_basic_identity() {
        let ctx = make_ctx(&["."], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"a\": 1"));
    }

    #[tokio::test]
    async fn test_jq_field_access() {
        let ctx = make_ctx(&[".a"], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1");
    }

    #[tokio::test]
    async fn test_jq_raw_output() {
        let ctx = make_ctx(&["-r", ".name"], r#"{"name":"hello"}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_jq_compact_output() {
        let ctx = make_ctx(&["-c", "."], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn test_jq_sort_keys() {
        let ctx = make_ctx(&["-Sc", "."], r#"{"b":2,"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"a":1,"b":2}"#);
    }

    #[tokio::test]
    async fn test_jq_null_input() {
        let ctx = make_ctx(&["-n", "1 + 2"], "");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "3");
    }

    #[tokio::test]
    async fn test_jq_slurp() {
        let ctx = make_ctx(&["-s", "."], "1\n2\n3");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Should produce an array [1,2,3]
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("3"));
    }

    #[tokio::test]
    async fn test_jq_exit_status_null() {
        let ctx = make_ctx(&["-e", ".x"], r#"{"x":null}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_jq_exit_status_false() {
        let ctx = make_ctx(&["-e", ".x"], r#"{"x":false}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_jq_join_output() {
        let ctx = make_ctx(&["-j", ".[]"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "123");
    }

    #[tokio::test]
    async fn test_jq_tab_indent() {
        let ctx = make_ctx(&["--tab", "."], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\t\"a\""));
    }

    #[tokio::test]
    async fn test_jq_json_stream() {
        let ctx = make_ctx(&["."], r#"{"a":1}{"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"a\": 1"));
        assert!(result.stdout.contains("\"b\": 2"));
    }

    #[tokio::test]
    async fn test_jq_parse_error() {
        let ctx = make_ctx(&["."], "not json");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 5);
        assert!(result.stderr.contains("parse error"));
    }

    #[tokio::test]
    async fn test_jq_unknown_function() {
        let ctx = make_ctx(&["foo"], "{}");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 3);
        assert!(result.stderr.contains("Unknown function"));
    }

    #[tokio::test]
    async fn test_jq_help() {
        let ctx = make_ctx(&["--help"], "");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Usage: jq"));
        assert!(result.stdout.contains("--raw-output"));
    }

    #[tokio::test]
    async fn test_jq_combined_flags() {
        let ctx = make_ctx(&["-rc", ".name"], r#"{"name":"hello"}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_jq_array_construction() {
        let ctx = make_ctx(&["[.[] | . * 2]"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("4"));
        assert!(result.stdout.contains("6"));
    }

    #[tokio::test]
    async fn test_jq_object_construction() {
        let ctx = make_ctx(
            &["-c", r#"{name: .n, age: .a}"#],
            r#"{"n":"Alice","a":30}"#,
        );
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let out = result.stdout.trim();
        assert!(out.contains("\"name\":\"Alice\""));
        assert!(out.contains("\"age\":30"));
    }

    #[tokio::test]
    async fn test_jq_select() {
        let ctx = make_ctx(&["[.[] | select(. > 2)]"], "[1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let out = result.stdout.trim();
        assert!(out.contains("3"));
        assert!(out.contains("4"));
        assert!(!out.contains("1"));
    }

    #[tokio::test]
    async fn test_jq_map() {
        let ctx = make_ctx(&["-c", "map(. + 1)"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[2,3,4]");
    }

    #[tokio::test]
    async fn test_jq_sort() {
        let ctx = make_ctx(&["-c", "sort"], "[3,1,2]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[1,2,3]");
    }

    #[tokio::test]
    async fn test_jq_keys() {
        let ctx = make_ctx(&["-c", "keys"], r#"{"b":2,"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"["a","b"]"#);
    }

    #[tokio::test]
    async fn test_jq_length() {
        let ctx = make_ctx(&["length"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "3");
    }

    #[tokio::test]
    async fn test_jq_pipe_chain() {
        let ctx = make_ctx(&[".[] | . * 2 | select(. > 4)"], "[1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.trim().lines().collect();
        assert_eq!(lines, vec!["6", "8"]);
    }

    #[tokio::test]
    async fn test_jq_reduce() {
        let ctx = make_ctx(
            &["reduce .[] as $x (0; . + $x)"],
            "[1,2,3]",
        );
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "6");
    }

    #[tokio::test]
    async fn test_jq_string_interpolation() {
        let ctx = make_ctx(
            &[r#""Name: \(.name)""#],
            r#"{"name":"Alice"}"#,
        );
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"Name: Alice\"");
    }

    #[tokio::test]
    async fn test_jq_nested_access() {
        let ctx = make_ctx(&[".a.b.c"], r#"{"a":{"b":{"c":42}}}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_jq_array_index() {
        let ctx = make_ctx(&[".[1]"], "[10,20,30]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "20");
    }

    #[tokio::test]
    async fn test_jq_negative_index() {
        let ctx = make_ctx(&[".[-1]"], "[10,20,30]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "30");
    }

    #[tokio::test]
    async fn test_jq_if_then_else() {
        let ctx = make_ctx(
            &[r#"if . > 0 then "pos" else "neg" end"#],
            "1",
        );
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"pos\"");
    }

    #[tokio::test]
    async fn test_jq_try_catch() {
        let ctx = make_ctx(
            &[r#"try .a.b catch "err""#],
            "1",
        );
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"err\"");
    }

    #[tokio::test]
    async fn test_jq_file_input() {
        let ctx = make_ctx_with_files(
            &[".", "/data.json"],
            "",
            &[("/data.json", r#"{"key":"value"}"#)],
        )
        .await;
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"key\""));
        assert!(result.stdout.contains("\"value\""));
    }

    #[tokio::test]
    async fn test_jq_file_not_found() {
        let ctx = make_ctx(&[".", "/nonexistent.json"], "");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("No such file"));
    }

    #[tokio::test]
    async fn test_jq_slurp_multiple() {
        let ctx = make_ctx(&["-sc", "."], "1\n2\n3");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[1,2,3]");
    }

    #[tokio::test]
    async fn test_jq_empty_input() {
        let ctx = make_ctx(&["."], "");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    // ===== Basic Operations =====

    #[tokio::test]
    async fn test_jq_pretty_print_arrays() {
        let ctx = make_ctx(&["."], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("3"));
    }

    #[tokio::test]
    async fn test_jq_nested_key_access() {
        let ctx = make_ctx(&[".a.b"], r#"{"a":{"b":"nested"}}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"nested\"");
    }

    #[tokio::test]
    async fn test_jq_missing_key_returns_null() {
        let ctx = make_ctx(&[".missing"], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "null");
    }

    #[tokio::test]
    async fn test_jq_access_numeric_values() {
        let ctx = make_ctx(&[".count"], r#"{"count":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_jq_access_boolean_values() {
        let ctx = make_ctx(&[".active"], r#"{"active":true}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_array_element_access() {
        let ctx = make_ctx(&[".[0]"], r#"["a","b","c"]"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"a\"");
    }

    #[tokio::test]
    async fn test_jq_array_negative_index() {
        let ctx = make_ctx(&[".[-1]"], r#"["a","b","c"]"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"c\"");
    }

    #[tokio::test]
    async fn test_jq_array_out_of_bounds() {
        let ctx = make_ctx(&[".[99]"], "[1,2]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "null");
    }

    #[tokio::test]
    async fn test_jq_array_iteration() {
        let ctx = make_ctx(&[".[]"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1\n2\n3");
    }

    #[tokio::test]
    async fn test_jq_object_values_iteration() {
        let ctx = make_ctx(&[".[]"], r#"{"a":1,"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let output = result.stdout.trim();
        assert!(output.contains("1"));
        assert!(output.contains("2"));
    }

    #[tokio::test]
    async fn test_jq_nested_array_iteration() {
        let ctx = make_ctx(&[".items[]"], r#"{"items":[1,2,3]}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1\n2\n3");
    }

    #[tokio::test]
    async fn test_jq_pipe_filters() {
        let ctx = make_ctx(&[".data | .value"], r#"{"data":{"value":42}}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_jq_chain_multiple_pipes() {
        let ctx = make_ctx(&[".a | .b | .c"], r#"{"a":{"b":{"c":"deep"}}}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"deep\"");
    }

    #[tokio::test]
    async fn test_jq_array_slice_start_end() {
        let ctx = make_ctx(&[".[2:4]"], "[0,1,2,3,4,5]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("3"));
    }

    #[tokio::test]
    async fn test_jq_array_slice_from_start() {
        let ctx = make_ctx(&[".[:3]"], "[0,1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("0"));
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
    }

    #[tokio::test]
    async fn test_jq_array_slice_to_end() {
        let ctx = make_ctx(&[".[3:]"], "[0,1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("3"));
        assert!(result.stdout.contains("4"));
    }

    #[tokio::test]
    async fn test_jq_string_slice() {
        let ctx = make_ctx(&[".[1:4]"], r#""hello""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"ell\"");
    }

    #[tokio::test]
    async fn test_jq_slice_negative_index() {
        let ctx = make_ctx(&[".[-2:]"], "[0,1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("3"));
        assert!(result.stdout.contains("4"));
    }

    #[tokio::test]
    async fn test_jq_comma_operator_two_values() {
        let ctx = make_ctx(&[".a, .b"], r#"{"a":1,"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1\n2");
    }

    #[tokio::test]
    async fn test_jq_comma_operator_three_values() {
        let ctx = make_ctx(&[".x, .y, .z"], r#"{"x":1,"y":2,"z":3}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1\n2\n3");
    }

    // ===== Filters =====

    #[tokio::test]
    async fn test_jq_select_filter() {
        let ctx = make_ctx(&["[.[] | select(. > 3)]"], "[1,2,3,4,5]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("4"));
        assert!(result.stdout.contains("5"));
    }

    #[tokio::test]
    async fn test_jq_map_transform() {
        let ctx = make_ctx(&["map(. * 2)"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("4"));
        assert!(result.stdout.contains("6"));
    }

    #[tokio::test]
    async fn test_jq_select_objects_by_field() {
        let ctx = make_ctx(&["-c", "[.[] | select(.n > 2)]"], r#"[{"n":1},{"n":5},{"n":2}]"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains(r#"{"n":5}"#));
    }

    #[tokio::test]
    async fn test_jq_has_object_key() {
        let ctx = make_ctx(&[r#"has("foo")"#], r#"{"foo":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_has_missing_key() {
        let ctx = make_ctx(&[r#"has("bar")"#], r#"{"foo":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "false");
    }

    #[tokio::test]
    async fn test_jq_has_array_index() {
        let ctx = make_ctx(&["has(1)"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_contains_array() {
        let ctx = make_ctx(&["contains([2])"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_contains_object() {
        let ctx = make_ctx(&[r#"contains({"a":1})"#], r#"{"a":1,"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_any_with_expression() {
        let ctx = make_ctx(&["any(. > 3)"], "[1,2,3,4,5]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_all_with_expression() {
        let ctx = make_ctx(&["all(. > 0)"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_if_then_else_2() {
        let ctx = make_ctx(&[r#"if . > 3 then "big" else "small" end"#], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"big\"");
    }

    #[tokio::test]
    async fn test_jq_if_else_branch() {
        let ctx = make_ctx(&[r#"if . > 3 then "big" else "small" end"#], "2");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"small\"");
    }

    #[tokio::test]
    async fn test_jq_if_elif() {
        let ctx = make_ctx(&[r#"if . > 10 then "big" elif . > 3 then "medium" else "small" end"#], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"medium\"");
    }

    #[tokio::test]
    async fn test_jq_optional_operator_null() {
        let ctx = make_ctx(&[".foo?"], "null");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "null");
    }

    #[tokio::test]
    async fn test_jq_optional_operator_present() {
        let ctx = make_ctx(&[".foo?"], r#"{"foo":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_jq_try_catch_2() {
        let ctx = make_ctx(&[r#"try error("oops") catch "caught""#], "1");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"caught\"");
    }

    #[tokio::test]
    async fn test_jq_variable_binding() {
        let ctx = make_ctx(&[". as $x | $x * $x"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "25");
    }

    #[tokio::test]
    async fn test_jq_variable_in_object() {
        let ctx = make_ctx(&["-c", ". as $n | {value: $n, doubled: ($n * 2)}"], "3");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"value":3,"doubled":6}"#);
    }

    // ===== Builtin Functions =====

    #[tokio::test]
    async fn test_jq_keys_sorted() {
        let ctx = make_ctx(&["keys"], r#"{"b":1,"a":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"a\""));
        assert!(result.stdout.contains("\"b\""));
    }

    #[tokio::test]
    async fn test_jq_length_array() {
        let ctx = make_ctx(&["length"], "[1,2,3,4,5]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "5");
    }

    #[tokio::test]
    async fn test_jq_length_string() {
        let ctx = make_ctx(&["length"], r#""hello""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "5");
    }

    #[tokio::test]
    async fn test_jq_length_object() {
        let ctx = make_ctx(&["length"], r#"{"a":1,"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_jq_type_object() {
        let ctx = make_ctx(&["type"], r#"{"a":1}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"object\"");
    }

    #[tokio::test]
    async fn test_jq_type_array() {
        let ctx = make_ctx(&["type"], "[1,2]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"array\"");
    }

    #[tokio::test]
    async fn test_jq_first_element() {
        let ctx = make_ctx(&["first"], "[5,10,15]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "5");
    }

    #[tokio::test]
    async fn test_jq_last_element() {
        let ctx = make_ctx(&["last"], "[5,10,15]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "15");
    }

    #[tokio::test]
    async fn test_jq_reverse_array() {
        let ctx = make_ctx(&["-c", "reverse"], "[1,2,3]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[3,2,1]");
    }

    #[tokio::test]
    async fn test_jq_sort_array() {
        let ctx = make_ctx(&["-c", "sort"], "[3,1,2]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[1,2,3]");
    }

    #[tokio::test]
    async fn test_jq_unique_array() {
        let ctx = make_ctx(&["-c", "unique"], "[1,2,1,3,2]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "[1,2,3]");
    }

    #[tokio::test]
    async fn test_jq_add_numbers() {
        let ctx = make_ctx(&["add"], "[1,2,3,4]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "10");
    }

    #[tokio::test]
    async fn test_jq_min_array() {
        let ctx = make_ctx(&["min"], "[5,2,8,1,9]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "1");
    }

    #[tokio::test]
    async fn test_jq_max_array() {
        let ctx = make_ctx(&["max"], "[5,2,8,1,9]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "9");
    }

    // ===== Operators =====

    #[tokio::test]
    async fn test_jq_add_numbers_op() {
        let ctx = make_ctx(&[". + 3"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "8");
    }

    #[tokio::test]
    async fn test_jq_subtract_numbers() {
        let ctx = make_ctx(&[". - 4"], "10");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "6");
    }

    #[tokio::test]
    async fn test_jq_multiply_numbers() {
        let ctx = make_ctx(&[". * 7"], "6");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_jq_divide_numbers() {
        let ctx = make_ctx(&[". / 4"], "20");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "5");
    }

    #[tokio::test]
    async fn test_jq_modulo_numbers() {
        let ctx = make_ctx(&[". % 5"], "17");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_jq_concatenate_strings() {
        let ctx = make_ctx(&[".a + .b"], r#"{"a":"foo","b":"bar"}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"foobar\"");
    }

    #[tokio::test]
    async fn test_jq_concatenate_arrays() {
        let ctx = make_ctx(&[".[0] + .[1]"], "[[1,2],[3,4]]");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("3"));
        assert!(result.stdout.contains("4"));
    }

    #[tokio::test]
    async fn test_jq_merge_objects() {
        let ctx = make_ctx(&["-c", ".[0] + .[1]"], r#"[{"a":1},{"b":2}]"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"a":1,"b":2}"#);
    }

    #[tokio::test]
    async fn test_jq_compare_equal() {
        let ctx = make_ctx(&[". == 5"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_compare_not_equal() {
        let ctx = make_ctx(&[". != 3"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_compare_less_than() {
        let ctx = make_ctx(&[". < 5"], "3");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_compare_greater_than() {
        let ctx = make_ctx(&[". > 5"], "10");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_compare_less_equal() {
        let ctx = make_ctx(&[". <= 5"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_compare_greater_equal() {
        let ctx = make_ctx(&[". >= 5"], "5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_logical_and() {
        let ctx = make_ctx(&[". and true"], "true");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_logical_or() {
        let ctx = make_ctx(&[". or true"], "false");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_logical_not() {
        let ctx = make_ctx(&["not"], "true");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "false");
    }

    #[tokio::test]
    async fn test_jq_alternative_operator_null() {
        let ctx = make_ctx(&[r#".a // "default""#], r#"{"a":null}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"default\"");
    }

    #[tokio::test]
    async fn test_jq_alternative_operator_value() {
        let ctx = make_ctx(&[r#".a // "default""#], r#"{"a":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    // ===== Math Functions =====

    #[tokio::test]
    async fn test_jq_floor() {
        let ctx = make_ctx(&["floor"], "3.7");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "3");
    }

    #[tokio::test]
    async fn test_jq_ceil() {
        let ctx = make_ctx(&["ceil"], "3.2");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "4");
    }

    #[tokio::test]
    async fn test_jq_round() {
        let ctx = make_ctx(&["round"], "3.5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "4");
    }

    #[tokio::test]
    async fn test_jq_sqrt() {
        let ctx = make_ctx(&["sqrt"], "16");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "4");
    }

    #[tokio::test]
    async fn test_jq_abs() {
        let ctx = make_ctx(&["abs"], "-5");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "5");
    }

    // ===== Type Conversion =====

    #[tokio::test]
    async fn test_jq_tostring() {
        let ctx = make_ctx(&["tostring"], "42");
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"42\"");
    }

    #[tokio::test]
    async fn test_jq_tonumber() {
        let ctx = make_ctx(&["tonumber"], r#""42""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    // ===== String Functions =====

    #[tokio::test]
    async fn test_jq_split_strings() {
        let ctx = make_ctx(&[r#"split(",")"#], r#""a,b,c""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"a\""));
        assert!(result.stdout.contains("\"b\""));
        assert!(result.stdout.contains("\"c\""));
    }

    #[tokio::test]
    async fn test_jq_join_arrays() {
        let ctx = make_ctx(&[r#"join("-")"#], r#"["a","b","c"]"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"a-b-c\"");
    }

    #[tokio::test]
    async fn test_jq_test_regex() {
        let ctx = make_ctx(&[r#"test("bar")"#], r#""foobar""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_startswith() {
        let ctx = make_ctx(&[r#"startswith("hello")"#], r#""hello world""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_endswith() {
        let ctx = make_ctx(&[r#"endswith("world")"#], r#""hello world""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "true");
    }

    #[tokio::test]
    async fn test_jq_ltrimstr() {
        let ctx = make_ctx(&[r#"ltrimstr("hello ")"#], r#""hello world""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"world\"");
    }

    #[tokio::test]
    async fn test_jq_rtrimstr() {
        let ctx = make_ctx(&[r#"rtrimstr(" world")"#], r#""hello world""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"hello\"");
    }

    #[tokio::test]
    async fn test_jq_ascii_downcase() {
        let ctx = make_ctx(&["ascii_downcase"], r#""HELLO""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"hello\"");
    }

    #[tokio::test]
    async fn test_jq_ascii_upcase() {
        let ctx = make_ctx(&["ascii_upcase"], r#""hello""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "\"HELLO\"");
    }

    #[tokio::test]
    async fn test_jq_index_in_string() {
        let ctx = make_ctx(&[r#"index("bar")"#], r#""foobar""#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "3");
    }

    // ===== Object and Array Construction =====

    #[tokio::test]
    async fn test_jq_object_construction_static_keys() {
        let ctx = make_ctx(&["-c", "{n: .name, v: .value}"], r#"{"name":"test","value":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"n":"test","v":42}"#);
    }

    #[tokio::test]
    async fn test_jq_object_construction_shorthand() {
        let ctx = make_ctx(&["-c", "{name, value}"], r#"{"name":"test","value":42}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"name":"test","value":42}"#);
    }

    #[tokio::test]
    async fn test_jq_array_construction_from_values() {
        let ctx = make_ctx(&["[.a, .b]"], r#"{"a":1,"b":2}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
    }

    #[tokio::test]
    async fn test_jq_array_from_object_values() {
        let ctx = make_ctx(&["[.[]]"], r#"{"a":1,"b":2,"c":3}"#);
        let cmd = JqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("1"));
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("3"));
    }
}
