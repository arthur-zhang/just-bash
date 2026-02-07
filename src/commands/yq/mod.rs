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

    fn make_ctx(args: &[&str], stdin: &str) -> CommandContext {
        CommandContext {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
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
}