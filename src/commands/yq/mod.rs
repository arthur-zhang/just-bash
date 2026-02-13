// src/commands/yq/mod.rs
pub mod formats;

use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::query_engine::{Value, parse, evaluate};
use crate::commands::query_engine::context::{EvalContext, JqError};
use formats::*;

pub struct YqCommand;

// ---------------------------------------------------------------------------
// CLI options
// ---------------------------------------------------------------------------

struct YqOptions {
    input_format: Option<Format>,
    output_format: Option<Format>,
    raw: bool,
    compact: bool,
    exit_status: bool,
    slurp: bool,
    null_input: bool,
    join_output: bool,
    pretty_print: bool,
    indent: usize,
    front_matter: bool,
    xml_attribute_prefix: String,
    xml_content_name: String,
    csv_delimiter: String,
    csv_header: bool,
    inplace: bool,
    filter: String,
    files: Vec<String>,
}

fn parse_format(s: &str) -> Result<Format, String> {
    match s {
        "yaml" | "yml" | "y" => Ok(Format::Yaml),
        "json" | "j" => Ok(Format::Json),
        "xml" | "x" => Ok(Format::Xml),
        "ini" | "i" => Ok(Format::Ini),
        "csv" | "c" => Ok(Format::Csv),
        "toml" | "t" => Ok(Format::Toml),
        _ => Err(format!("yq: Unknown format: {}\n", s)),
    }
}

fn parse_yq_args(args: &[String]) -> Result<YqOptions, CommandResult> {
    let mut opts = YqOptions {
        input_format: None,
        output_format: None,
        raw: false,
        compact: false,
        exit_status: false,
        slurp: false,
        null_input: false,
        join_output: false,
        pretty_print: false,
        indent: 2,
        front_matter: false,
        xml_attribute_prefix: "+@".to_string(),
        xml_content_name: "+content".to_string(),
        csv_delimiter: String::new(),
        csv_header: true,
        inplace: false,
        filter: ".".to_string(),
        files: Vec::new(),
    };

    let mut filter_set = false;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-p" || a == "--input-format" {
            i += 1;
            if i >= args.len() {
                return Err(CommandResult::with_exit_code(
                    String::new(),
                    "yq: -p requires an argument\n".to_string(),
                    2,
                ));
            }
            opts.input_format = Some(parse_format(&args[i]).map_err(|e| {
                CommandResult::with_exit_code(String::new(), e, 2)
            })?);
        } else if a.starts_with("-p") && a.len() > 2 {
            let fmt_str = &a[2..];
            opts.input_format = Some(parse_format(fmt_str).map_err(|e| {
                CommandResult::with_exit_code(String::new(), e, 2)
            })?);
        } else if a == "-o" || a == "--output-format" {
            i += 1;
            if i >= args.len() {
                return Err(CommandResult::with_exit_code(
                    String::new(),
                    "yq: -o requires an argument\n".to_string(),
                    2,
                ));
            }
            opts.output_format = Some(parse_format(&args[i]).map_err(|e| {
                CommandResult::with_exit_code(String::new(), e, 2)
            })?);
        } else if a.starts_with("-o") && a.len() > 2 {
            let fmt_str = &a[2..];
            opts.output_format = Some(parse_format(fmt_str).map_err(|e| {
                CommandResult::with_exit_code(String::new(), e, 2)
            })?);
        } else if a == "-I" || a == "--indent" {
            i += 1;
            if i >= args.len() {
                return Err(CommandResult::with_exit_code(
                    String::new(),
                    "yq: -I requires an argument\n".to_string(),
                    2,
                ));
            }
            opts.indent = args[i].parse().unwrap_or(2);
        } else if a == "--xml-attribute-prefix" {
            i += 1;
            if i < args.len() {
                opts.xml_attribute_prefix = args[i].clone();
            }
        } else if a == "--xml-content-name" {
            i += 1;
            if i < args.len() {
                opts.xml_content_name = args[i].clone();
            }
        } else if a == "--csv-delimiter" {
            i += 1;
            if i < args.len() {
                opts.csv_delimiter = args[i].clone();
            }
        } else if a == "--csv-header" {
            opts.csv_header = true;
        } else if a == "--no-csv-header" {
            opts.csv_header = false;
        } else if a == "-i" || a == "--inplace" {
            opts.inplace = true;
        } else if a == "-r" || a == "--raw-output" {
            opts.raw = true;
        } else if a == "-c" || a == "--compact-output" || a == "--compact" {
            opts.compact = true;
        } else if a == "-e" || a == "--exit-status" {
            opts.exit_status = true;
        } else if a == "-s" || a == "--slurp" {
            opts.slurp = true;
        } else if a == "-n" || a == "--null-input" {
            opts.null_input = true;
        } else if a == "-j" || a == "--join-output" {
            opts.join_output = true;
        } else if a == "-f" || a == "--front-matter" {
            opts.front_matter = true;
        } else if a == "-P" || a == "--prettyPrint" {
            opts.pretty_print = true;
        } else if a == "-" {
            opts.files.push("-".to_string());
        } else if a.starts_with("--") {
            return Err(CommandResult::with_exit_code(
                String::new(),
                format!("yq: Unknown option: {}\n", a),
                2,
            ));
        } else if a.starts_with('-') && a.len() > 1 {
            // Combined short flags like -rc
            for c in a[1..].chars() {
                match c {
                    'r' => opts.raw = true,
                    'c' => opts.compact = true,
                    'e' => opts.exit_status = true,
                    's' => opts.slurp = true,
                    'n' => opts.null_input = true,
                    'j' => opts.join_output = true,
                    'f' => opts.front_matter = true,
                    'P' => opts.pretty_print = true,
                    'i' => opts.inplace = true,
                    _ => {
                        return Err(CommandResult::with_exit_code(
                            String::new(),
                            format!("yq: Unknown option: -{}\n", c),
                            2,
                        ));
                    }
                }
            }
        } else if !filter_set {
            opts.filter = a.clone();
            filter_set = true;
        } else {
            opts.files.push(a.clone());
        }
        i += 1;
    }

    Ok(opts)
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

const YQ_HELP: &str = "\
Usage: yq [OPTIONS] FILTER [FILE]

command-line YAML/JSON/XML/INI/CSV/TOML processor

Options:
  -p, --input-format FMT   input format (yaml/json/xml/ini/csv/toml)
  -o, --output-format FMT  output format (yaml/json/xml/ini/csv/toml)
  -i, --inplace            edit files in place
  -r, --raw-output         output strings without quotes
  -c, --compact            compact output
  -e, --exit-status        set exit status based on output
  -s, --slurp              read entire input into array
  -n, --null-input          don't read any input
  -j, --join-output        don't print newlines after each output
  -f, --front-matter       process front-matter
  -P, --prettyPrint        pretty print output
  -I, --indent N           indentation level (default: 2)
      --xml-attribute-prefix  XML attribute prefix (default: +@)
      --xml-content-name      XML text content key (default: +content)
      --csv-delimiter         CSV delimiter
      --csv-header            CSV has header row (default)
      --no-csv-header         CSV has no header row
      --help                  display this help and exit
";

// ---------------------------------------------------------------------------
// JSON formatting (reused from jq for raw/compact output)
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

fn format_value_json(v: &Value, compact: bool, raw: bool) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("{}", b),
        Value::Number(n) => {
            if !n.is_finite() {
                return "null".to_string();
            }
            if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => {
            if raw { s.clone() } else { format_json_string(s) }
        }
        Value::Array(_) | Value::Object(_) => {
            if compact {
                v.to_json_string_compact()
            } else {
                v.to_json_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Command for YqCommand {
    fn name(&self) -> &'static str {
        "yq"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(YQ_HELP.to_string());
        }

        let opts = match parse_yq_args(&ctx.args) {
            Ok(o) => o,
            Err(r) => return r,
        };

        // Determine input format
        let input_fmt = opts.input_format.unwrap_or_else(|| {
            if let Some(f) = opts.files.first() {
                if f != "-" {
                    if let Some(fmt) = detect_format_from_extension(f) {
                        return fmt;
                    }
                }
            }
            Format::Yaml
        });

        // Determine output format
        let output_fmt = opts.output_format.unwrap_or(input_fmt);

        let format_opts = FormatOptions {
            input_format: input_fmt,
            output_format: output_fmt,
            raw: opts.raw,
            compact: opts.compact,
            pretty_print: opts.pretty_print,
            indent: opts.indent,
            xml_attribute_prefix: opts.xml_attribute_prefix.clone(),
            xml_content_name: opts.xml_content_name.clone(),
            csv_delimiter: opts.csv_delimiter.clone(),
            csv_header: opts.csv_header,
        };

        // Build list of inputs
        let mut inputs: Vec<(String, String)> = Vec::new();
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
                                    "yq: {}: No such file or directory\n",
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
                    format!("yq: parse error: {}\n", e),
                    5,
                );
            }
        };

        let mut eval_ctx = EvalContext::with_env(ctx.env.clone());

        // Evaluate
        let values: Vec<Value> = if opts.null_input {
            match evaluate(&Value::Null, &ast, &mut eval_ctx) {
                Ok(v) => v,
                Err(e) => return yq_error_result(e),
            }
        } else if opts.front_matter {
            // Front-matter mode: extract and process front-matter
            let mut all_values = Vec::new();
            for (_source, content) in &inputs {
                if let Some(fm) = extract_front_matter(content) {
                    match evaluate(&fm.front_matter, &ast, &mut eval_ctx) {
                        Ok(v) => all_values.extend(v),
                        Err(e) => return yq_error_result(e),
                    }
                } else {
                    // No front-matter, parse as normal
                    match parse_input(content.trim(), &format_opts) {
                        Ok(parsed) => {
                            match evaluate(&parsed, &ast, &mut eval_ctx) {
                                Ok(v) => all_values.extend(v),
                                Err(e) => return yq_error_result(e),
                            }
                        }
                        Err(e) => {
                            return CommandResult::with_exit_code(
                                String::new(),
                                format!("yq: {}\n", e),
                                5,
                            );
                        }
                    }
                }
            }
            all_values
        } else if opts.slurp {
            let mut items: Vec<Value> = Vec::new();
            for (_source, content) in &inputs {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if input_fmt == Format::Yaml {
                    items.extend(parse_all_yaml_documents(trimmed));
                } else {
                    match parse_input(trimmed, &format_opts) {
                        Ok(parsed) => items.push(parsed),
                        Err(e) => {
                            return CommandResult::with_exit_code(
                                String::new(),
                                format!("yq: {}\n", e),
                                5,
                            );
                        }
                    }
                }
            }
            let arr = Value::Array(items);
            match evaluate(&arr, &ast, &mut eval_ctx) {
                Ok(v) => v,
                Err(e) => return yq_error_result(e),
            }
        } else {
            let mut all_values: Vec<Value> = Vec::new();
            for (_source, content) in &inputs {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // For YAML, handle multi-document
                let parsed_values = if input_fmt == Format::Yaml {
                    parse_all_yaml_documents(trimmed)
                } else {
                    match parse_input(trimmed, &format_opts) {
                        Ok(v) => vec![v],
                        Err(e) => {
                            return CommandResult::with_exit_code(
                                String::new(),
                                format!("yq: {}\n", e),
                                5,
                            );
                        }
                    }
                };
                for parsed in &parsed_values {
                    match evaluate(parsed, &ast, &mut eval_ctx) {
                        Ok(v) => all_values.extend(v),
                        Err(e) => return yq_error_result(e),
                    }
                }
            }
            all_values
        };

        // Format output
        // yq defaults to raw string output (unlike jq which quotes strings)
        let effective_raw = opts.raw || output_fmt != Format::Json;

        let formatted: Vec<String> = values
            .iter()
            .map(|v| {
                // For scalar values, always use JSON-style formatting
                // (format_output is for structured data like objects/arrays)
                match v {
                    Value::Null | Value::Bool(_) | Value::Number(_) => {
                        format_value_json(v, opts.compact, effective_raw)
                    }
                    Value::String(_) => {
                        format_value_json(v, opts.compact, effective_raw)
                    }
                    _ => {
                        format_output(v, &format_opts)
                    }
                }
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

        // Handle inplace
        if opts.inplace && !opts.files.is_empty() {
            for file in &opts.files {
                if file != "-" {
                    let path = ctx.fs.resolve_path(&ctx.cwd, file);
                    let write_output = if output.is_empty() {
                        String::new()
                    } else {
                        format!("{}\n", output)
                    };
                    if let Err(_) = ctx.fs.write_file(
                        &path,
                        write_output.as_bytes(),
                    ).await {
                        return CommandResult::with_exit_code(
                            String::new(),
                            format!("yq: {}: write error\n", file),
                            2,
                        );
                    }
                }
            }
            return CommandResult::with_exit_code(
                String::new(), String::new(), exit_code,
            );
        }

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

fn yq_error_result(e: JqError) -> CommandResult {
    match e {
        JqError::ExecutionLimit(msg) => CommandResult::with_exit_code(
            String::new(),
            format!("yq: {}\n", msg),
            5,
        ),
        JqError::Runtime(msg) if msg.contains("Unknown function") => {
            CommandResult::with_exit_code(
                String::new(),
                format!("yq: error: {}\n", msg),
                3,
            )
        }
        _ => CommandResult::with_exit_code(
            String::new(),
            format!("yq: parse error: {}\n", e),
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
    use serde_json;

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
    async fn test_yq_basic_yaml() {
        let ctx = make_ctx(&[".name"], "name: hello\nage: 30");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_yq_output_json() {
        let ctx = make_ctx(
            &["-o", "json", "."],
            "name: hello\nage: 30",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("\"name\""));
        assert!(result.stdout.contains("\"hello\""));
        assert!(result.stdout.contains("30"));
    }

    #[tokio::test]
    async fn test_yq_input_json() {
        let ctx = make_ctx(
            &["-p", "json", ".a"],
            r#"{"a":42}"#,
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    #[tokio::test]
    async fn test_yq_input_toml() {
        let ctx = make_ctx(
            &["-p", "toml", ".package.name"],
            "[package]\nname = \"myapp\"",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "myapp");
    }

    #[tokio::test]
    async fn test_yq_input_csv() {
        let ctx = make_ctx(
            &["-p", "csv", ".[0].name"],
            "name,age\nAlice,30",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "Alice");
    }

    #[tokio::test]
    async fn test_yq_format_conversion_yaml_to_json() {
        let ctx = make_ctx(
            &["-o", "json", "-c", "."],
            "name: hello",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.trim().contains("\"name\":\"hello\""));
    }

    #[tokio::test]
    async fn test_yq_raw_output() {
        let ctx = make_ctx(
            &["-o", "json", "-r", ".name"],
            "name: hello",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_yq_compact_json() {
        let ctx = make_ctx(
            &["-o", "json", "-c", "."],
            "a: 1\nb: 2",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let out = result.stdout.trim();
        assert!(out.contains("\"a\":1"));
        assert!(out.contains("\"b\":2"));
    }

    #[tokio::test]
    async fn test_yq_null_input() {
        let ctx = make_ctx(&["-n", r#"{"a":1}"#], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("a"));
    }

    #[tokio::test]
    async fn test_yq_slurp_multi_doc() {
        let ctx = make_ctx(
            &["-s", "length"],
            "name: first\n---\nname: second",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_yq_front_matter() {
        let ctx = make_ctx(
            &["-f", ".title"],
            "---\ntitle: Hello\n---\nBody content",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "Hello");
    }

    #[tokio::test]
    async fn test_yq_exit_status() {
        let ctx = make_ctx(&["-e", ".x"], "x: null");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_yq_help() {
        let ctx = make_ctx(&["--help"], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Usage: yq"));
        assert!(result.stdout.contains("--input-format"));
    }

    #[tokio::test]
    async fn test_yq_combined_flags() {
        let ctx = make_ctx(
            &["-rc", ".name"],
            "name: hello",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_yq_error_handling() {
        let ctx = make_ctx(
            &["-p", "json", "."],
            "not json",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(!result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_yq_file_input() {
        let ctx = make_ctx_with_files(
            &[".", "/data.yaml"],
            "",
            &[("/data.yaml", "name: test\nvalue: 42")],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name"));
        assert!(result.stdout.contains("test"));
    }

    #[tokio::test]
    async fn test_yq_file_not_found() {
        let ctx = make_ctx(&[".", "/nonexistent.yaml"], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("No such file"));
    }

    #[tokio::test]
    async fn test_yq_unknown_option() {
        let ctx = make_ctx(&["--unknown-flag", "."], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Unknown option"));
    }

    #[tokio::test]
    async fn test_yq_yaml_to_toml() {
        let ctx = make_ctx(
            &["-o", "toml", "."],
            "name: test\nversion: 1",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name = \"test\""));
    }

    #[tokio::test]
    async fn test_yq_join_output() {
        let ctx = make_ctx(
            &["-o", "json", "-j", ".[]"],
            "- 1\n- 2\n- 3",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "123");
    }

    #[tokio::test]
    async fn test_yq_empty_input() {
        let ctx = make_ctx(&["."], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_yq_input_xml() {
        let ctx = make_ctx(
            &["-p", "xml", ".root.name"],
            "<root><name>hello</name></root>",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    // Additional YAML processing tests
    #[tokio::test]
    async fn test_yq_filter_nested_yaml() {
        let ctx = make_ctx(
            &[".config.database.host"],
            "config:\n  database:\n    host: localhost\n    port: 5432\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "localhost");
    }

    #[tokio::test]
    async fn test_yq_handle_arrays_in_yaml() {
        let ctx = make_ctx(
            &[".items[0].name"],
            "items:\n  - name: foo\n    value: 1\n  - name: bar\n    value: 2\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "foo");
    }

    #[tokio::test]
    async fn test_yq_iterate_over_arrays() {
        let ctx = make_ctx(
            &[".fruits[]"],
            "fruits:\n  - apple\n  - banana\n  - cherry\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "apple\nbanana\ncherry");
    }

    #[tokio::test]
    async fn test_yq_use_select_filter() {
        let ctx = make_ctx(
            &[".users[] | select(.active) | .name"],
            "users:\n  - name: alice\n    active: true\n  - name: bob\n    active: false\n  - name: charlie\n    active: true\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "alice\ncharlie");
    }

    // Output format tests
    #[tokio::test]
    async fn test_yq_output_compact_json() {
        let ctx = make_ctx(
            &["-c", "-o", "json", "."],
            "name: test\nvalue: 42\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#"{"name":"test","value":42}"#);
    }

    #[tokio::test]
    async fn test_yq_output_raw_strings() {
        let ctx = make_ctx(
            &["-r", "-o", "json", ".message"],
            "message: hello world\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello world");
    }

    // JSON input tests
    #[tokio::test]
    async fn test_yq_convert_json_to_yaml() {
        let ctx = make_ctx(
            &["-p", "json", "."],
            r#"{"name": "test", "value": 42}"#,
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name: test"));
        assert!(result.stdout.contains("value: 42"));
    }

    // XML input/output tests
    #[tokio::test]
    async fn test_yq_read_xml_with_attributes() {
        let ctx = make_ctx(
            &["-p", "xml", ".item[\"+@id\"]", "-o", "json"],
            r#"<item id="123" name="test"/>"#,
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), r#""123""#);
    }

    #[tokio::test]
    async fn test_yq_output_as_xml() {
        let ctx = make_ctx(
            &["-o", "xml", "."],
            "root:\n  name: test\n  value: 42\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("<root>"));
        assert!(result.stdout.contains("<name>test</name>"));
        assert!(result.stdout.contains("<value>42</value>"));
        assert!(result.stdout.contains("</root>"));
    }

    // stdin support tests
    #[tokio::test]
    async fn test_yq_read_from_stdin() {
        let ctx = make_ctx(&[".name"], "name: test\n");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "test");
    }

    #[tokio::test]
    async fn test_yq_accept_dash_for_stdin() {
        let ctx = make_ctx(&[".value", "-"], "value: 42\n");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "42");
    }

    // null input tests
    #[tokio::test]
    async fn test_yq_null_input_create_object() {
        let ctx = make_ctx(&["-n", r#"{"name": "created"}"#, "-o", "json"], "");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains(r#""name""#));
        assert!(result.stdout.contains(r#""created""#));
    }

    // slurp mode tests
    #[tokio::test]
    async fn test_yq_slurp_multiple_yaml_documents() {
        let ctx = make_ctx(
            &["-s", ".[0].name"],
            "---\nname: first\n---\nname: second\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "first");
    }

    // jq-style filter tests
    #[tokio::test]
    async fn test_yq_support_map_filter() {
        let ctx = make_ctx(
            &[".numbers | map(. * 2)"],
            "numbers:\n  - 1\n  - 2\n  - 3\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = result.stdout.trim();
        assert!(lines.contains("- 2"));
        assert!(lines.contains("- 4"));
        assert!(lines.contains("- 6"));
    }

    #[tokio::test]
    async fn test_yq_support_keys_filter() {
        let ctx = make_ctx(
            &[".config | keys"],
            "config:\n  host: localhost\n  port: 8080\n  debug: true\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("debug"));
        assert!(result.stdout.contains("host"));
        assert!(result.stdout.contains("port"));
    }

    #[tokio::test]
    async fn test_yq_support_length_filter() {
        let ctx = make_ctx(
            &[".items | length"],
            "items:\n  - a\n  - b\n  - c\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "3");
    }

    // Error handling tests
    #[tokio::test]
    async fn test_yq_handle_invalid_yaml() {
        let ctx = make_ctx(&["."], "invalid: yaml: syntax: error:");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 5);
        assert!(result.stderr.contains("parse error"));
    }

    // Format validation tests
    #[tokio::test]
    async fn test_yq_reject_invalid_input_format() {
        let ctx = make_ctx(&["-p", "badformat"], "{}");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Unknown format"));
    }

    #[tokio::test]
    async fn test_yq_reject_invalid_output_format() {
        let ctx = make_ctx(&["-o", "badformat"], "{}");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Unknown format"));
    }

    // INI format tests
    #[tokio::test]
    async fn test_yq_read_ini_and_extract_values() {
        let ctx = make_ctx(
            &["-p", "ini", ".database.host"],
            "[database]\nhost=localhost\nport=5432\n\n[server]\ndebug=true\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "localhost");
    }

    #[tokio::test]
    async fn test_yq_output_as_ini() {
        let ctx = make_ctx(
            &["-o", "ini", "."],
            "database:\n  host: localhost\n  port: 5432\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("[database]"));
        assert!(result.stdout.contains("host=localhost"));
        assert!(result.stdout.contains("port=5432"));
    }

    #[tokio::test]
    async fn test_yq_convert_yaml_to_ini() {
        let ctx = make_ctx(
            &["-o", "ini", "."],
            "name: test\nversion: 1\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name=test"));
        assert!(result.stdout.contains("version=1"));
    }

    // CSV format tests
    #[tokio::test]
    async fn test_yq_read_csv_with_headers() {
        let ctx = make_ctx(
            &["-p", "csv", ".[0].name"],
            "name,age,city\nalice,30,NYC\nbob,25,LA\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "alice");
    }

    #[tokio::test]
    async fn test_yq_read_csv_get_all_names() {
        let ctx = make_ctx(
            &["-p", "csv", ".[].name"],
            "name,age\nalice,30\nbob,25\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "alice\nbob");
    }

    #[tokio::test]
    async fn test_yq_filter_csv_rows() {
        let ctx = make_ctx(
            &["-p", "csv", r#"[.[] | select(.city == "NYC") | .name]"#, "-o", "json"],
            "name,age,city\nalice,30,NYC\nbob,25,LA\ncharlie,35,NYC\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let parsed: Vec<String> = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(parsed, vec!["alice", "charlie"]);
    }

    #[tokio::test]
    async fn test_yq_output_as_csv() {
        let ctx = make_ctx(
            &["-o", "csv", "."],
            "- name: alice\n  age: 30\n- name: bob\n  age: 25\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name,age"));
        assert!(result.stdout.contains("alice,30"));
        assert!(result.stdout.contains("bob,25"));
    }

    #[tokio::test]
    async fn test_yq_convert_json_to_csv() {
        let ctx = make_ctx(
            &["-p", "json", "-o", "csv", "."],
            r#"[{"name":"alice","score":95},{"name":"bob","score":87}]"#,
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("name,score"));
        assert!(result.stdout.contains("alice,95"));
        assert!(result.stdout.contains("bob,87"));
    }

    // join-output mode tests
    #[tokio::test]
    async fn test_yq_join_output_no_newlines() {
        let ctx = make_ctx(
            &["-j", ".items[]"],
            "items:\n  - a\n  - b\n  - c\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abc");
    }

    // exit-status mode tests
    #[tokio::test]
    async fn test_yq_exit_status_truthy() {
        let ctx = make_ctx(&["-e", ".value"], "value: true\n");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_yq_exit_status_null() {
        let ctx = make_ctx(&["-e", ".missing"], "value: 42\n");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_yq_exit_status_false() {
        let ctx = make_ctx(&["-e", ".value"], "value: false\n");
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    // indent option tests
    #[tokio::test]
    async fn test_yq_custom_indent() {
        let ctx = make_ctx(
            &["-o", "json", "-I", "4", "."],
            "items:\n  - a\n  - b\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("    \"a\""));
    }

    // combined short options tests
    #[tokio::test]
    async fn test_yq_combined_rc_flags() {
        let ctx = make_ctx(
            &["-rc", "-o", "json", ".msg"],
            "msg: hello\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_yq_combined_cej_flags() {
        let ctx = make_ctx(
            &["-cej", "-o", "json", ".items[]"],
            "items:\n  - 1\n  - 2\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "12");
    }

    // jq builtin functions tests
    #[tokio::test]
    async fn test_yq_support_first() {
        let ctx = make_ctx(
            &[".items | first"],
            "items:\n  - a\n  - b\n  - c\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "a");
    }

    #[tokio::test]
    async fn test_yq_support_last() {
        let ctx = make_ctx(
            &[".items | last"],
            "items:\n  - a\n  - b\n  - c\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "c");
    }

    #[tokio::test]
    async fn test_yq_support_add_for_numbers() {
        let ctx = make_ctx(
            &[".nums | add"],
            "nums:\n  - 1\n  - 2\n  - 3\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "6");
    }

    #[tokio::test]
    async fn test_yq_support_min() {
        let ctx = make_ctx(
            &[".nums | min"],
            "nums:\n  - 5\n  - 2\n  - 8\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_yq_support_max() {
        let ctx = make_ctx(
            &[".nums | max"],
            "nums:\n  - 5\n  - 2\n  - 8\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "8");
    }

    #[tokio::test]
    async fn test_yq_support_unique() {
        let ctx = make_ctx(
            &[".items | unique", "-o", "json"],
            "items:\n  - a\n  - b\n  - a\n  - c\n  - b\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let parsed: Vec<String> = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(parsed, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn test_yq_support_sort_by() {
        let ctx = make_ctx(
            &[".items | sort_by(.name) | .[0].name"],
            "items:\n  - name: b\n    val: 2\n  - name: a\n    val: 1\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "a");
    }

    #[tokio::test]
    async fn test_yq_support_reverse() {
        let ctx = make_ctx(
            &[".items | reverse", "-o", "json"],
            "items:\n  - 1\n  - 2\n  - 3\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let parsed: Vec<i32> = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(parsed, vec![3, 2, 1]);
    }

    #[tokio::test]
    async fn test_yq_support_group_by() {
        let ctx = make_ctx(
            &[".items | group_by(.type) | length"],
            "items:\n  - type: a\n    v: 1\n  - type: b\n    v: 2\n  - type: a\n    v: 3\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "2");
    }

    // CSV options tests
    #[tokio::test]
    async fn test_yq_csv_no_header() {
        let ctx = make_ctx(
            &["-p", "csv", "--no-csv-header", ".[0][0]"],
            "alice,30\nbob,25\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "alice");
    }

    // TOML format tests
    #[tokio::test]
    async fn test_yq_read_toml_extract_values() {
        let ctx = make_ctx(
            &[".package.name"],
            "[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n\n[dependencies]\nserde = \"1.0\"\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "my-app");
    }

    #[tokio::test]
    async fn test_yq_output_as_toml() {
        let ctx = make_ctx(
            &["-o", "toml", "."],
            "server:\n  host: localhost\n  port: 8080\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("[server]"));
        assert!(result.stdout.contains("host = \"localhost\""));
        assert!(result.stdout.contains("port = 8080"));
    }

    #[tokio::test]
    async fn test_yq_convert_json_to_toml() {
        let ctx = make_ctx(
            &["-p", "json", "-o", "toml", "."],
            r#"{"app": {"name": "test", "version": "2.0"}}"#,
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("[app]"));
        assert!(result.stdout.contains("name = \"test\""));
    }

    // inplace mode tests
    #[tokio::test]
    async fn test_yq_modify_file_inplace() {
        let ctx = make_ctx_with_files(
            &["-i", r#".version = "2.0""#, "/data.yaml"],
            "",
            &[("/data.yaml", "version: 1.0\nname: test\n")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");

        let content = fs.read_file("/data.yaml").await.unwrap();
        assert!(content.contains("version: \"2.0\""));
    }

    // front-matter tests
    #[tokio::test]
    async fn test_yq_extract_yaml_front_matter() {
        let ctx = make_ctx(
            &["--front-matter", ".title"],
            "---\ntitle: My Post\ndate: 2024-01-01\ntags:\n  - tech\n  - web\n---\n\n# Content here\n\nThis is the post body.\n",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "My Post");
    }

    #[tokio::test]
    async fn test_yq_extract_front_matter_tags_array() {
        let ctx = make_ctx(
            &["--front-matter", ".tags[]"],
            "---\ntitle: Test\ntags:\n  - a\n  - b\n---\nContent",
        );
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "a\nb");
    }

    // format auto-detection tests
    #[tokio::test]
    async fn test_yq_auto_detect_json_extension() {
        let ctx = make_ctx_with_files(
            &[".name", "/data.json"],
            "",
            &[("/data.json", r#"{"name": "test", "value": 42}"#)],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "test");
    }

    #[tokio::test]
    async fn test_yq_auto_detect_xml_extension() {
        let ctx = make_ctx_with_files(
            &[".root.name", "/data.xml"],
            "",
            &[("/data.xml", "<root><name>test</name></root>")],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "test");
    }

    #[tokio::test]
    async fn test_yq_auto_detect_csv_extension() {
        let ctx = make_ctx_with_files(
            &[".[0].name", "/data.csv"],
            "",
            &[("/data.csv", "name,age\nalice,30\nbob,25\n")],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "alice");
    }

    #[tokio::test]
    async fn test_yq_auto_detect_ini_extension() {
        let ctx = make_ctx_with_files(
            &[".database.host", "/config.ini"],
            "",
            &[("/config.ini", "[database]\nhost=localhost\n")],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "localhost");
    }

    #[tokio::test]
    async fn test_yq_explicit_format_overrides_auto_detection() {
        let ctx = make_ctx_with_files(
            &["-p", "yaml", ".name", "/data.json"],
            "",
            &[("/data.json", "name: yaml-content\n")],
        ).await;
        let cmd = YqCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "yaml-content");
    }
}