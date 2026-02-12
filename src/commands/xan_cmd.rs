use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct XanCommand;

#[async_trait]
impl Command for XanCommand {
    fn name(&self) -> &'static str { "xan" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.is_empty() || ctx.args.iter().any(|a| a == "--help" || a == "-h") {
            return CommandResult::success(HELP.to_string());
        }

        let subcmd = &ctx.args[0];
        let sub_args: Vec<String> = ctx.args[1..].to_vec();

        match subcmd.as_str() {
            "headers" => cmd_headers(&ctx, &sub_args).await,
            "count" => cmd_count(&ctx, &sub_args).await,
            "head" => cmd_head(&ctx, &sub_args).await,
            "tail" => cmd_tail(&ctx, &sub_args).await,
            "select" => cmd_select(&ctx, &sub_args).await,
            "slice" => cmd_slice(&ctx, &sub_args).await,
            "reverse" => cmd_reverse(&ctx, &sub_args).await,
            "sort" => cmd_sort(&ctx, &sub_args).await,
            "filter" => cmd_filter(&ctx, &sub_args).await,
            "search" => cmd_search(&ctx, &sub_args).await,
            "view" => cmd_view(&ctx, &sub_args).await,
            "to" => cmd_to(&ctx, &sub_args).await,
            "from" => cmd_from(&ctx, &sub_args).await,
            _ => CommandResult::error(format!("xan: unknown command '{}'\n", subcmd)),
        }
    }
}

const HELP: &str = r#"xan - CSV toolkit for data manipulation

Usage: xan <COMMAND> [OPTIONS] [FILE]

Commands:
  headers    Show column names
  count      Count rows
  head       Show first N rows
  tail       Show last N rows
  select     Select columns
  slice      Extract row range
  reverse    Reverse row order
  sort       Sort rows
  filter     Filter rows by expression
  search     Filter rows by regex
  view       Pretty print as table
  to         Convert to JSON
  from       Convert from JSON
"#;

fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(input.as_bytes());

    for result in reader.records() {
        if let Ok(record) = result {
            rows.push(record.iter().map(|s| s.to_string()).collect());
        }
    }
    rows
}

fn rows_to_csv(rows: &[Vec<String>]) -> String {
    let mut wtr = csv::Writer::from_writer(vec![]);
    for row in rows {
        let _ = wtr.write_record(row);
    }
    String::from_utf8(wtr.into_inner().unwrap_or_default()).unwrap_or_default()
}

async fn read_input(ctx: &CommandContext, args: &[String]) -> Result<String, CommandResult> {
    let file_arg = args.iter().find(|a| !a.starts_with('-'));

    if let Some(file) = file_arg {
        if file == "-" {
            return Ok(ctx.stdin.clone());
        }
        let path = ctx.fs.resolve_path(&ctx.cwd, file);
        ctx.fs.read_file(&path).await.map_err(|_| {
            CommandResult::error(format!("xan: {}: No such file or directory\n", file))
        })
    } else {
        Ok(ctx.stdin.clone())
    }
}

async fn cmd_headers(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.is_empty() {
        return CommandResult::success(String::new());
    }
    let mut out = String::new();
    for (i, h) in rows[0].iter().enumerate() {
        out.push_str(&format!("{}\t{}\n", i, h));
    }
    CommandResult::success(out)
}

async fn cmd_count(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    let count = if rows.is_empty() { 0 } else { rows.len() - 1 };
    CommandResult::success(format!("{}\n", count))
}

async fn cmd_head(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let mut n = 10usize;
    for i in 0..args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(10);
        }
    }

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.is_empty() {
        return CommandResult::success(String::new());
    }
    let end = (n + 1).min(rows.len());
    CommandResult::success(rows_to_csv(&rows[..end]))
}

async fn cmd_tail(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let mut n = 10usize;
    for i in 0..args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(10);
        }
    }

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 {
        return CommandResult::success(rows_to_csv(&rows));
    }
    let start = if rows.len() > n + 1 { rows.len() - n } else { 1 };
    let mut result = vec![rows[0].clone()];
    result.extend_from_slice(&rows[start..]);
    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_select(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let cols_arg = args.iter().find(|a| !a.starts_with('-'));
    let cols: Vec<&str> = cols_arg.map(|s| s.split(',').collect()).unwrap_or_default();

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.is_empty() || cols.is_empty() {
        return CommandResult::success(rows_to_csv(&rows));
    }

    let header = &rows[0];
    let indices: Vec<usize> = cols.iter().filter_map(|c| {
        if let Ok(i) = c.parse::<usize>() {
            Some(i)
        } else {
            header.iter().position(|h| h == *c)
        }
    }).collect();

    let result: Vec<Vec<String>> = rows.iter().map(|row| {
        indices.iter().filter_map(|&i| row.get(i).cloned()).collect()
    }).collect();

    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_slice(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let mut start = 0usize;
    let mut end = usize::MAX;

    for i in 0..args.len() {
        if args[i] == "-s" && i + 1 < args.len() {
            start = args[i + 1].parse().unwrap_or(0);
        }
        if args[i] == "-e" && i + 1 < args.len() {
            end = args[i + 1].parse().unwrap_or(usize::MAX);
        }
    }

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 {
        return CommandResult::success(rows_to_csv(&rows));
    }

    let data_start = (start + 1).min(rows.len());
    let data_end = (end + 1).min(rows.len());
    let mut result = vec![rows[0].clone()];
    if data_start < data_end {
        result.extend_from_slice(&rows[data_start..data_end]);
    }
    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_reverse(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 {
        return CommandResult::success(rows_to_csv(&rows));
    }
    let mut result = vec![rows[0].clone()];
    result.extend(rows[1..].iter().rev().cloned());
    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_sort(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let mut col_name: Option<&str> = None;
    let mut numeric = false;
    let mut reverse = false;

    for i in 0..args.len() {
        if args[i] == "-N" { numeric = true; }
        if args[i] == "-r" { reverse = true; }
        if !args[i].starts_with('-') && col_name.is_none() {
            col_name = Some(&args[i]);
        }
    }

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 {
        return CommandResult::success(rows_to_csv(&rows));
    }

    let col_idx = col_name.and_then(|name| {
        if let Ok(i) = name.parse::<usize>() { Some(i) }
        else { rows[0].iter().position(|h| h == name) }
    }).unwrap_or(0);

    let mut data: Vec<Vec<String>> = rows[1..].to_vec();
    data.sort_by(|a, b| {
        let va = a.get(col_idx).map(|s| s.as_str()).unwrap_or("");
        let vb = b.get(col_idx).map(|s| s.as_str()).unwrap_or("");
        if numeric {
            let na: f64 = va.parse().unwrap_or(0.0);
            let nb: f64 = vb.parse().unwrap_or(0.0);
            na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            va.cmp(vb)
        }
    });

    if reverse { data.reverse(); }

    let mut result = vec![rows[0].clone()];
    result.extend(data);
    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_filter(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let expr = args.iter().find(|a| !a.starts_with('-') && a.contains(|c| c == '>' || c == '<' || c == '='));

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 || expr.is_none() {
        return CommandResult::success(rows_to_csv(&rows));
    }

    let expr = expr.unwrap();
    let (col, op, val) = parse_filter_expr(expr);
    let col_idx = rows[0].iter().position(|h| h == &col);

    if col_idx.is_none() {
        return CommandResult::success(rows_to_csv(&rows));
    }
    let col_idx = col_idx.unwrap();

    let mut result = vec![rows[0].clone()];
    for row in &rows[1..] {
        if let Some(cell) = row.get(col_idx) {
            if matches_filter(cell, &op, &val) {
                result.push(row.clone());
            }
        }
    }
    CommandResult::success(rows_to_csv(&result))
}

fn parse_filter_expr(expr: &str) -> (String, String, String) {
    for op in &[">=", "<=", "!=", "==", ">", "<", "="] {
        if let Some(pos) = expr.find(op) {
            let col = expr[..pos].trim().to_string();
            let val = expr[pos + op.len()..].trim().to_string();
            return (col, op.to_string(), val);
        }
    }
    (String::new(), String::new(), String::new())
}

fn matches_filter(cell: &str, op: &str, val: &str) -> bool {
    let cell_num: Option<f64> = cell.parse().ok();
    let val_num: Option<f64> = val.parse().ok();

    match op {
        ">" => cell_num.zip(val_num).map(|(c, v)| c > v).unwrap_or(cell > val),
        "<" => cell_num.zip(val_num).map(|(c, v)| c < v).unwrap_or(cell < val),
        ">=" => cell_num.zip(val_num).map(|(c, v)| c >= v).unwrap_or(cell >= val),
        "<=" => cell_num.zip(val_num).map(|(c, v)| c <= v).unwrap_or(cell <= val),
        "==" | "=" => cell == val,
        "!=" => cell != val,
        _ => false,
    }
}

async fn cmd_search(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let pattern = args.iter().find(|a| !a.starts_with('-'));

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.len() <= 1 || pattern.is_none() {
        return CommandResult::success(rows_to_csv(&rows));
    }

    let re = match regex_lite::Regex::new(pattern.unwrap()) {
        Ok(r) => r,
        Err(_) => return CommandResult::error("xan: invalid regex\n".to_string()),
    };

    let mut result = vec![rows[0].clone()];
    for row in &rows[1..] {
        if row.iter().any(|cell| re.is_match(cell)) {
            result.push(row.clone());
        }
    }
    CommandResult::success(rows_to_csv(&result))
}

async fn cmd_view(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };
    let rows = parse_csv(&input);
    if rows.is_empty() {
        return CommandResult::success(String::new());
    }

    let col_widths: Vec<usize> = (0..rows[0].len()).map(|i| {
        rows.iter().map(|r| r.get(i).map(|s| s.len()).unwrap_or(0)).max().unwrap_or(0)
    }).collect();

    let mut out = String::new();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            let width = col_widths.get(i).copied().unwrap_or(0);
            out.push_str(&format!("{:width$}", cell, width = width));
            if i < row.len() - 1 { out.push_str("  "); }
        }
        out.push('\n');
    }
    CommandResult::success(out)
}

async fn cmd_to(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let format = args.iter().find(|a| !a.starts_with('-')).map(|s| s.as_str()).unwrap_or("json");

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };

    if format != "json" {
        return CommandResult::error(format!("xan: unsupported format '{}'\n", format));
    }

    let rows = parse_csv(&input);
    if rows.is_empty() {
        return CommandResult::success("[]\n".to_string());
    }

    let headers = &rows[0];
    let mut json_rows: Vec<serde_json::Value> = Vec::new();

    for row in &rows[1..] {
        let mut obj = serde_json::Map::new();
        for (i, h) in headers.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or_default();
            obj.insert(h.clone(), serde_json::Value::String(val));
        }
        json_rows.push(serde_json::Value::Object(obj));
    }

    let json = serde_json::to_string_pretty(&json_rows).unwrap_or_else(|_| "[]".to_string());
    CommandResult::success(format!("{}\n", json))
}

async fn cmd_from(ctx: &CommandContext, args: &[String]) -> CommandResult {
    let format = args.iter().find(|a| !a.starts_with('-')).map(|s| s.as_str()).unwrap_or("json");

    let input = match read_input(ctx, args).await {
        Ok(i) => i,
        Err(e) => return e,
    };

    if format != "json" {
        return CommandResult::error(format!("xan: unsupported format '{}'\n", format));
    }

    let arr: Vec<serde_json::Value> = match serde_json::from_str(&input) {
        Ok(a) => a,
        Err(_) => return CommandResult::error("xan: invalid JSON\n".to_string()),
    };

    if arr.is_empty() {
        return CommandResult::success(String::new());
    }

    let headers: Vec<String> = if let Some(serde_json::Value::Object(obj)) = arr.first() {
        obj.keys().cloned().collect()
    } else {
        return CommandResult::error("xan: expected array of objects\n".to_string());
    };

    let mut rows: Vec<Vec<String>> = vec![headers.clone()];
    for item in &arr {
        if let serde_json::Value::Object(obj) = item {
            let row: Vec<String> = headers.iter().map(|h| {
                obj.get(h).map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                }).unwrap_or_default()
            }).collect();
            rows.push(row);
        }
    }

    CommandResult::success(rows_to_csv(&rows))
}
