// src/commands/printf/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct PrintfCommand;

const HELP: &str = "Usage: printf FORMAT [ARGUMENT]...\n\n\
Print ARGUMENT(s) according to FORMAT.\n\n\
FORMAT is a string with escape sequences and format specifiers:\n  %%  literal %\n  %s  string\n  %d  decimal integer\n  %f  floating point\n  %x  hexadecimal\n  %o  octal\n  %c  character\n  %b  string with escape interpretation\n\n\
Escape sequences:\n  \\\\  backslash  \\n  newline  \\t  tab  \\r  carriage return\n  \\xHH hex  \\0NNN octal  \\uHHHH unicode  \\UHHHHHHHH unicode\n";

#[async_trait]
impl Command for PrintfCommand {
    fn name(&self) -> &'static str { "printf" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.is_empty() {
            return CommandResult::with_exit_code("".into(), "printf: usage: printf format [arguments]\n".into(), 2);
        }
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }

        let format = &args[0];
        let arguments: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        let mut output = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let mut arg_idx = 0;

        // Process format string, repeating if we have unused arguments
        loop {
            let start_arg_idx = arg_idx;
            let chars: Vec<char> = format.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                if chars[i] == '\\' {
                    let (esc, advance) = process_escape(&chars, i);
                    output.push_str(&esc);
                    i += advance;
                } else if chars[i] == '%' {
                    if i + 1 >= chars.len() { output.push('%'); i += 1; continue; }
                    if chars[i + 1] == '%' { output.push('%'); i += 2; continue; }
                    // Parse format specifier
                    let (formatted, advance, consumed, parse_err) = process_format_spec(&chars, i, &arguments, arg_idx);
                    output.push_str(&formatted);
                    arg_idx += consumed;
                    i += advance;
                    if let Some(err_msg) = parse_err {
                        stderr.push_str(&err_msg);
                        exit_code = 1;
                    }
                } else {
                    output.push(chars[i]);
                    i += 1;
                }
            }

            // If no arguments were consumed this pass, or we've used all args, stop
            if arg_idx <= start_arg_idx || arg_idx >= arguments.len() { break; }
        }

        CommandResult::with_exit_code(output, stderr, exit_code)
    }
}

fn process_escape(chars: &[char], pos: usize) -> (String, usize) {
    if pos + 1 >= chars.len() { return ("\\".into(), 1); }
    let next = chars[pos + 1];
    match next {
        '\\' => ("\\".into(), 2),
        'n' => ("\n".into(), 2),
        't' => ("\t".into(), 2),
        'r' => ("\r".into(), 2),
        'a' => ("\x07".into(), 2),
        'b' => ("\x08".into(), 2),
        'f' => ("\x0c".into(), 2),
        'v' => ("\x0b".into(), 2),
        'e' | 'E' => ("\x1b".into(), 2),
        '0' => {
            let mut oct = String::new();
            let mut j = pos + 2;
            while j < chars.len() && j < pos + 5 && chars[j] >= '0' && chars[j] <= '7' { oct.push(chars[j]); j += 1; }
            let code = if oct.is_empty() { 0 } else { u32::from_str_radix(&oct, 8).unwrap_or(0) % 256 };
            (char::from_u32(code).map_or(String::new(), |c| c.to_string()), j - pos)
        }
        'x' => {
            let mut hex = String::new();
            let mut j = pos + 2;
            while j < chars.len() && j < pos + 4 && chars[j].is_ascii_hexdigit() { hex.push(chars[j]); j += 1; }
            if hex.is_empty() { ("\\x".into(), 2) }
            else { let code = u32::from_str_radix(&hex, 16).unwrap_or(0); (char::from_u32(code).map_or(String::new(), |c| c.to_string()), j - pos) }
        }
        'u' => {
            let mut hex = String::new();
            let mut j = pos + 2;
            while j < chars.len() && j < pos + 6 && chars[j].is_ascii_hexdigit() { hex.push(chars[j]); j += 1; }
            if hex.is_empty() { ("\\u".into(), 2) }
            else { let code = u32::from_str_radix(&hex, 16).unwrap_or(0); (char::from_u32(code).map_or(String::new(), |c| c.to_string()), j - pos) }
        }
        'U' => {
            let mut hex = String::new();
            let mut j = pos + 2;
            while j < chars.len() && j < pos + 10 && chars[j].is_ascii_hexdigit() { hex.push(chars[j]); j += 1; }
            if hex.is_empty() { ("\\U".into(), 2) }
            else { let code = u32::from_str_radix(&hex, 16).unwrap_or(0); (char::from_u32(code).map_or(String::new(), |c| c.to_string()), j - pos) }
        }
        '1'..='7' => {
            // Octal without leading 0 (e.g. \101 = 'A')
            let mut oct = String::new();
            oct.push(next);
            let mut j = pos + 2;
            while j < chars.len() && j < pos + 4 && chars[j] >= '0' && chars[j] <= '7' { oct.push(chars[j]); j += 1; }
            let code = u32::from_str_radix(&oct, 8).unwrap_or(0) % 256;
            (char::from_u32(code).map_or(String::new(), |c| c.to_string()), j - pos)
        }
        _ => { let mut s = String::from('\\'); s.push(next); (s, 2) }
    }
}

/// Returns (formatted_str, chars_advanced, args_consumed, optional_error_msg)
fn process_format_spec(chars: &[char], pos: usize, args: &[&str], arg_idx: usize) -> (String, usize, usize, Option<String>) {
    let mut i = pos + 1;
    // Collect flags
    while i < chars.len() && "-+ 0#'".contains(chars[i]) { i += 1; }
    // Collect width
    while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    // Collect precision
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    }
    if i >= chars.len() { return ("%".into(), 1, 0, None); }

    let specifier = chars[i];
    let spec_str: String = chars[pos..=i].iter().collect();
    let advance = i - pos + 1;
    let arg = if arg_idx < args.len() { args[arg_idx] } else { "" };

    match specifier {
        's' => {
            let formatted = apply_string_format(&spec_str, arg);
            (formatted, advance, 1, None)
        }
        'd' | 'i' => {
            let (val, err) = parse_int_arg(arg);
            let formatted = apply_int_format(&spec_str, val, specifier);
            (formatted, advance, 1, err)
        }
        'f' | 'g' | 'e' => {
            let (val, err) = parse_float_arg(arg);
            let formatted = apply_float_format(&spec_str, val, specifier);
            (formatted, advance, 1, err)
        }
        'x' | 'X' => {
            let (val, err) = parse_int_arg(arg);
            let formatted = apply_hex_format(&spec_str, val, specifier);
            (formatted, advance, 1, err)
        }
        'o' => {
            let (val, err) = parse_int_arg(arg);
            let formatted = apply_oct_format(&spec_str, val);
            (formatted, advance, 1, err)
        }
        'c' => {
            let ch = arg.chars().next().unwrap_or('\0');
            if ch == '\0' { (String::new(), advance, 1, None) }
            else { (ch.to_string(), advance, 1, None) }
        }
        'b' => {
            let processed = process_b_escape(arg);
            (processed, advance, 1, None)
        }
        'q' => {
            let quoted = shell_quote(arg);
            (quoted, advance, 1, None)
        }
        _ => { (spec_str, advance, 0, None) }
    }
}

fn parse_int_arg(s: &str) -> (i64, Option<String>) {
    if s.is_empty() { return (0, None); }
    // Handle quoted character
    if s.len() >= 2 && (s.starts_with('\'') || s.starts_with('"')) {
        let ch = s.chars().nth(1).unwrap_or('\0');
        return (ch as i64, None);
    }
    // Handle hex
    if s.starts_with("0x") || s.starts_with("0X") {
        return match i64::from_str_radix(&s[2..], 16) {
            Ok(v) => (v, None),
            Err(_) => (0, Some(format!("printf: '{}': invalid number\n", s))),
        };
    }
    // Handle octal
    if s.starts_with('0') && s.len() > 1 && s.chars().skip(1).all(|c| c >= '0' && c <= '7') {
        return match i64::from_str_radix(&s[1..], 8) {
            Ok(v) => (v, None),
            Err(_) => (0, Some(format!("printf: '{}': invalid number\n", s))),
        };
    }
    match s.parse::<i64>() {
        Ok(v) => (v, None),
        Err(_) => (0, Some(format!("printf: '{}': invalid number\n", s))),
    }
}

fn parse_float_arg(s: &str) -> (f64, Option<String>) {
    if s.is_empty() { return (0.0, None); }
    match s.parse::<f64>() {
        Ok(v) => (v, None),
        Err(_) => (0.0, Some(format!("printf: '{}': invalid number\n", s))),
    }
}

fn apply_string_format(spec: &str, val: &str) -> String {
    // Parse width and precision from spec
    let inner = &spec[1..spec.len()-1]; // remove % and s
    let left_justify = inner.contains('-');
    let inner = inner.replace('-', "");
    let (width, precision) = parse_width_prec(&inner);
    let mut s = val.to_string();
    if let Some(p) = precision {
        if s.len() > p { s = s[..p].to_string(); }
    }
    if let Some(w) = width {
        if s.len() < w {
            let pad = " ".repeat(w - s.len());
            s = if left_justify { format!("{}{}", s, pad) } else { format!("{}{}", pad, s) };
        }
    }
    s
}

fn apply_int_format(spec: &str, val: i64, _specifier: char) -> String {
    let inner = &spec[1..spec.len()-1];
    let left_justify = inner.contains('-');
    let zero_pad = inner.contains('0') && !left_justify;
    let plus = inner.contains('+');
    let space = inner.contains(' ');
    let clean: String = inner.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let (width, _) = parse_width_prec(&clean);
    let mut s = if val < 0 { format!("{}", val) }
    else if plus { format!("+{}", val) }
    else if space { format!(" {}", val) }
    else { format!("{}", val) };
    if let Some(w) = width {
        if s.len() < w {
            let pad_char = if zero_pad { '0' } else { ' ' };
            let padding: String = std::iter::repeat(pad_char).take(w - s.len()).collect();
            s = if left_justify { format!("{}{}", s, " ".repeat(w - s.len())) }
                else if zero_pad && (s.starts_with('-') || s.starts_with('+') || s.starts_with(' ')) {
                    let first = s.remove(0);
                    format!("{}{}{}", first, padding, s)
                } else { format!("{}{}", padding, s) };
        }
    }
    s
}

fn apply_float_format(spec: &str, val: f64, specifier: char) -> String {
    let inner = &spec[1..spec.len()-1];
    let left_justify = inner.contains('-');
    let zero_pad = inner.contains('0') && !left_justify;
    let clean: String = inner.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let (width, precision) = parse_width_prec(&clean);
    let prec = precision.unwrap_or(6);
    let mut s = match specifier {
        'e' | 'E' => format!("{:.*e}", prec, val),
        'g' | 'G' => format!("{:.*}", prec, val),
        _ => format!("{:.prec$}", val, prec = prec),
    };
    if let Some(w) = width {
        if s.len() < w {
            let pad = if zero_pad { "0" } else { " " };
            let padding: String = std::iter::repeat(pad).take(w - s.len()).collect::<Vec<_>>().join("");
            s = if left_justify { format!("{}{}", s, " ".repeat(w - s.len())) } else { format!("{}{}", padding, s) };
        }
    }
    s
}

fn apply_hex_format(_spec: &str, val: i64, specifier: char) -> String {
    if specifier == 'X' { format!("{:X}", val as u64) }
    else { format!("{:x}", val as u64) }
}

fn apply_oct_format(_spec: &str, val: i64) -> String {
    format!("{:o}", val as u64)
}

fn parse_width_prec(s: &str) -> (Option<usize>, Option<usize>) {
    if s.is_empty() { return (None, None); }
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    let width = if !parts[0].is_empty() { parts[0].parse().ok() } else { None };
    let precision = if parts.len() > 1 { parts[1].parse().ok() } else { None };
    (width, precision)
}

fn process_b_escape(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let (esc, adv) = process_escape(&chars, i);
            result.push_str(&esc);
            i += adv;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() { return "''".to_string(); }
    if s.chars().all(|c| c.is_alphanumeric() || "-_./,:@".contains(c)) { return s.to_string(); }
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: String::new(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    #[tokio::test]
    async fn test_printf_string() { let r = PrintfCommand.execute(make_ctx(vec!["Hello %s", "world"])).await; assert_eq!(r.stdout, "Hello world"); }
    #[tokio::test]
    async fn test_printf_int() { let r = PrintfCommand.execute(make_ctx(vec!["Number: %d", "42"])).await; assert_eq!(r.stdout, "Number: 42"); }
    #[tokio::test]
    async fn test_printf_float() { let r = PrintfCommand.execute(make_ctx(vec!["Value: %f", "3.14"])).await; assert_eq!(r.stdout, "Value: 3.140000"); }
    #[tokio::test]
    async fn test_printf_hex() { let r = PrintfCommand.execute(make_ctx(vec!["Hex: %x", "255"])).await; assert_eq!(r.stdout, "Hex: ff"); }
    #[tokio::test]
    async fn test_printf_octal() { let r = PrintfCommand.execute(make_ctx(vec!["Octal: %o", "8"])).await; assert_eq!(r.stdout, "Octal: 10"); }
    #[tokio::test]
    async fn test_printf_percent() { let r = PrintfCommand.execute(make_ctx(vec!["100%%"])).await; assert_eq!(r.stdout, "100%"); }
    #[tokio::test]
    async fn test_printf_multi() { let r = PrintfCommand.execute(make_ctx(vec!["%s is %d years old", "Alice", "30"])).await; assert_eq!(r.stdout, "Alice is 30 years old"); }
    #[tokio::test]
    async fn test_printf_newline() { let r = PrintfCommand.execute(make_ctx(vec!["line1\\nline2"])).await; assert_eq!(r.stdout, "line1\nline2"); }
    #[tokio::test]
    async fn test_printf_tab() { let r = PrintfCommand.execute(make_ctx(vec!["col1\\tcol2"])).await; assert_eq!(r.stdout, "col1\tcol2"); }
    #[tokio::test]
    async fn test_printf_width() { let r = PrintfCommand.execute(make_ctx(vec!["%10s", "hi"])).await; assert_eq!(r.stdout, "        hi"); }
    #[tokio::test]
    async fn test_printf_prec() { let r = PrintfCommand.execute(make_ctx(vec!["%.2f", "3.14159"])).await; assert_eq!(r.stdout, "3.14"); }
    #[tokio::test]
    async fn test_printf_zero_pad() { let r = PrintfCommand.execute(make_ctx(vec!["%05d", "42"])).await; assert_eq!(r.stdout, "00042"); }
    #[tokio::test]
    async fn test_printf_left() { let r = PrintfCommand.execute(make_ctx(vec!["%-10s|", "hi"])).await; assert_eq!(r.stdout, "hi        |"); }
    #[tokio::test]
    async fn test_printf_no_args() { let r = PrintfCommand.execute(make_ctx(vec![])).await; assert_eq!(r.exit_code, 2); assert!(r.stderr.contains("usage")); }
    #[tokio::test]
    async fn test_printf_missing_args() { let r = PrintfCommand.execute(make_ctx(vec!["%s %s", "only"])).await; assert_eq!(r.stdout, "only "); }
    #[tokio::test]
    async fn test_printf_invalid_num() { let r = PrintfCommand.execute(make_ctx(vec!["%d", "notanumber"])).await; assert_eq!(r.stdout, "0"); assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("invalid number")); }
    #[tokio::test]
    async fn test_printf_help() { let r = PrintfCommand.execute(make_ctx(vec!["--help"])).await; assert!(r.stdout.contains("printf")); assert!(r.stdout.contains("FORMAT")); }
    #[tokio::test]
    async fn test_printf_escape_e() { let r = PrintfCommand.execute(make_ctx(vec!["\\e[31mred\\e[0m"])).await; assert_eq!(r.stdout, "\x1b[31mred\x1b[0m"); }
    #[tokio::test]
    async fn test_printf_unicode() { let r = PrintfCommand.execute(make_ctx(vec!["\\u2764"])).await; assert_eq!(r.stdout, "❤"); }
    #[tokio::test]
    async fn test_printf_unicode_u() { let r = PrintfCommand.execute(make_ctx(vec!["\\U1F600"])).await; assert_eq!(r.stdout, "😀"); }
    #[tokio::test]
    async fn test_printf_hex_x() { let r = PrintfCommand.execute(make_ctx(vec!["\\x41\\x42\\x43"])).await; assert_eq!(r.stdout, "ABC"); }
    #[tokio::test]
    async fn test_printf_octal_esc() { let r = PrintfCommand.execute(make_ctx(vec!["\\101\\102\\103"])).await; assert_eq!(r.stdout, "ABC"); }
}
