/// AWK Built-in Functions
///
/// Implementation of AWK built-in functions including string functions,
/// math functions, and printf/sprintf formatting.

use regex_lite::Regex;
use std::collections::HashMap;

use crate::commands::awk::coercion::{to_number, to_string};
use crate::commands::awk::context::AwkContext;

// ─── Result Type ─────────────────────────────────────────────────

/// Result of calling a built-in function.
#[derive(Debug, Clone)]
pub enum BuiltinResult {
    /// Simple value result
    Value(String),
    /// Value with a side effect (e.g., sub/gsub modifying a variable)
    ValueWithSideEffect {
        value: String,
        target_name: String,
        new_value: String,
    },
    /// Error occurred
    Error(String),
}

// ─── Printf Formatting ───────────────────────────────────────────

/// Format a printf/sprintf string with the given values.
///
/// Supports format specifiers: %s, %d, %i, %f, %e, %E, %g, %G, %x, %X, %o, %c, %%
/// Supports flags: -, +, space, 0
/// Supports width and precision (including * for dynamic values)
pub fn format_printf(format: &str, values: &[String]) -> String {
    let mut value_idx = 0;
    let mut result = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            let mut j = i + 1;
            let mut flags = String::new();
            let mut width = String::new();
            let mut precision = String::new();

            // Parse flags: -, +, space, #, 0
            while j < chars.len() && "-+ #0".contains(chars[j]) {
                flags.push(chars[j]);
                j += 1;
            }

            // Parse width (may be * for dynamic)
            if j < chars.len() && chars[j] == '*' {
                let w = values.get(value_idx).map(|v| to_number(v) as i64).unwrap_or(0);
                if w < 0 {
                    flags.push('-');
                    width = (-w).to_string();
                } else {
                    width = w.to_string();
                }
                value_idx += 1;
                j += 1;
            } else {
                while j < chars.len() && chars[j].is_ascii_digit() {
                    width.push(chars[j]);
                    j += 1;
                }
            }

            // Parse precision
            if j < chars.len() && chars[j] == '.' {
                j += 1;
                if j < chars.len() && chars[j] == '*' {
                    let p = values.get(value_idx).map(|v| to_number(v) as i64).unwrap_or(0);
                    precision = p.max(0).to_string();
                    value_idx += 1;
                    j += 1;
                } else {
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        precision.push(chars[j]);
                        j += 1;
                    }
                }
            }

            // Skip length modifiers (l, ll, h, hh, z, j)
            while j < chars.len() && "lhzj".contains(chars[j]) {
                j += 1;
            }

            if j >= chars.len() {
                result.push_str(&format[i..]);
                break;
            }

            let spec = chars[j];
            let val = values.get(value_idx).cloned().unwrap_or_default();

            match spec {
                's' => {
                    let mut s = val;
                    if !precision.is_empty() {
                        let prec: usize = precision.parse().unwrap_or(0);
                        s = s.chars().take(prec).collect();
                    }
                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'd' | 'i' => {
                    let num = to_number(&val) as i64;
                    let is_negative = num < 0;
                    let mut digits = num.abs().to_string();

                    // Precision for integers means minimum digits
                    if !precision.is_empty() {
                        let prec: usize = precision.parse().unwrap_or(0);
                        while digits.len() < prec {
                            digits.insert(0, '0');
                        }
                    }

                    // Add sign
                    let sign = if is_negative {
                        "-".to_string()
                    } else if flags.contains('+') {
                        "+".to_string()
                    } else if flags.contains(' ') {
                        " ".to_string()
                    } else {
                        String::new()
                    };

                    let mut s = format!("{}{}", sign, digits);

                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else if flags.contains('0') && precision.is_empty() {
                            let pad_len = w.saturating_sub(sign.len());
                            s = format!("{}{:0>width$}", sign, digits, width = pad_len);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'f' => {
                    let num = to_number(&val);
                    let prec: usize = if precision.is_empty() {
                        6
                    } else {
                        precision.parse().unwrap_or(6)
                    };
                    let mut s = format!("{:.prec$}", num, prec = prec);
                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'e' | 'E' => {
                    let num = to_number(&val);
                    let prec: usize = if precision.is_empty() {
                        6
                    } else {
                        precision.parse().unwrap_or(6)
                    };
                    let mut s = format!("{:.prec$e}", num, prec = prec);
                    if spec == 'E' {
                        s = s.to_uppercase();
                    }
                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'g' | 'G' => {
                    let num = to_number(&val);
                    let prec: usize = if precision.is_empty() {
                        6
                    } else {
                        precision.parse().unwrap_or(6)
                    };
                    let mut s = format_g_specifier(num, prec);
                    if spec == 'G' {
                        s = s.to_uppercase();
                    }
                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'x' | 'X' => {
                    let num = to_number(&val) as i64;
                    let mut digits = format!("{:x}", num.abs());
                    if spec == 'X' {
                        digits = digits.to_uppercase();
                    }

                    if !precision.is_empty() {
                        let prec: usize = precision.parse().unwrap_or(0);
                        while digits.len() < prec {
                            digits.insert(0, '0');
                        }
                    }

                    let sign = if num < 0 { "-" } else { "" };
                    let mut s = format!("{}{}", sign, digits);

                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else if flags.contains('0') && precision.is_empty() {
                            let pad_len = w.saturating_sub(sign.len());
                            s = format!("{}{:0>width$}", sign, digits, width = pad_len);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'o' => {
                    let num = to_number(&val) as i64;
                    let mut digits = format!("{:o}", num.abs());

                    if !precision.is_empty() {
                        let prec: usize = precision.parse().unwrap_or(0);
                        while digits.len() < prec {
                            digits.insert(0, '0');
                        }
                    }

                    let sign = if num < 0 { "-" } else { "" };
                    let mut s = format!("{}{}", sign, digits);

                    if !width.is_empty() {
                        let w: usize = width.parse().unwrap_or(0);
                        if flags.contains('-') {
                            s = format!("{:<width$}", s, width = w);
                        } else if flags.contains('0') && precision.is_empty() {
                            let pad_len = w.saturating_sub(sign.len());
                            s = format!("{}{:0>width$}", sign, digits, width = pad_len);
                        } else {
                            s = format!("{:>width$}", s, width = w);
                        }
                    }
                    result.push_str(&s);
                    value_idx += 1;
                }
                'c' => {
                    let c = if let Ok(n) = val.parse::<f64>() {
                        char::from_u32(n as u32).unwrap_or('\0')
                    } else {
                        val.chars().next().unwrap_or('\0')
                    };
                    result.push(c);
                    value_idx += 1;
                }
                '%' => {
                    result.push('%');
                }
                _ => {
                    result.push_str(&format[i..=j]);
                }
            }
            i = j + 1;
        } else if chars[i] == '\\' && i + 1 < chars.len() {
            let esc = chars[i + 1];
            match esc {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                _ => result.push(esc),
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Format a number using %g-style formatting.
fn format_g_specifier(n: f64, precision: usize) -> String {
    if n == 0.0 {
        return "0".to_string();
    }

    let prec = if precision == 0 { 1 } else { precision };
    let exp = n.abs().log10().floor() as i32;

    if exp < -4 || exp >= prec as i32 {
        // Use scientific notation
        let s = format!("{:.prec$e}", n, prec = prec.saturating_sub(1));
        trim_trailing_zeros_scientific(&s)
    } else {
        // Use fixed notation
        let s = format!("{:.prec$}", n, prec = prec);
        trim_trailing_zeros(&s)
    }
}

/// Remove trailing zeros after decimal point.
fn trim_trailing_zeros(s: &str) -> String {
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0');
        if trimmed.ends_with('.') {
            trimmed[..trimmed.len() - 1].to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Remove trailing zeros in scientific notation mantissa.
fn trim_trailing_zeros_scientific(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let mantissa = &s[..e_pos];
        let exponent = &s[e_pos..];
        let trimmed = trim_trailing_zeros(mantissa);
        format!("{}{}", trimmed, exponent)
    } else {
        s.to_string()
    }
}

// ─── String Functions ────────────────────────────────────────────

/// length(s) - Return the length of string s (default $0 if no arg).
pub fn builtin_length(args: &[String], ctx: &AwkContext) -> String {
    let s = if args.is_empty() {
        &ctx.line
    } else {
        &args[0]
    };
    to_string(s.chars().count() as f64)
}

/// substr(s, start, len?) - Return substring of s starting at position start (1-indexed).
/// If len is provided, return at most len characters.
pub fn builtin_substr(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }

    let s = &args[0];
    let chars: Vec<char> = s.chars().collect();

    let start = if args.len() > 1 {
        (to_number(&args[1]) as i64 - 1).max(0) as usize
    } else {
        return String::new();
    };

    if start >= chars.len() {
        return String::new();
    }

    if args.len() > 2 {
        let len = (to_number(&args[2]) as i64).max(0) as usize;
        chars[start..].iter().take(len).collect()
    } else {
        chars[start..].iter().collect()
    }
}

/// index(s, target) - Return position of target in s (1-indexed), or 0 if not found.
pub fn builtin_index(args: &[String]) -> String {
    if args.len() < 2 {
        return "0".to_string();
    }

    let s = &args[0];
    let target = &args[1];

    match s.find(target.as_str()) {
        Some(pos) => {
            // Convert byte position to character position
            let char_pos = s[..pos].chars().count() + 1;
            to_string(char_pos as f64)
        }
        None => "0".to_string(),
    }
}

/// split(s, array, sep?) - Split string s into array using separator sep.
/// Returns the number of elements. The array argument is handled by the caller.
pub fn builtin_split(
    s: &str,
    sep: Option<&str>,
    fs: &str,
) -> (usize, HashMap<String, String>) {
    let separator = sep.unwrap_or(fs);
    
    let parts: Vec<&str> = if separator == " " {
        s.split_whitespace().collect()
    } else if separator.len() == 1 {
        s.split(separator).collect()
    } else {
        // Treat as regex
        match Regex::new(separator) {
            Ok(re) => re.split(s).collect(),
            Err(_) => s.split(separator).collect(),
        }
    };

    let mut array = HashMap::new();
    for (i, part) in parts.iter().enumerate() {
        array.insert((i + 1).to_string(), part.to_string());
    }

    (parts.len(), array)
}

/// tolower(s) - Convert string to lowercase.
pub fn builtin_tolower(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }
    args[0].to_lowercase()
}

/// toupper(s) - Convert string to uppercase.
pub fn builtin_toupper(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }
    args[0].to_uppercase()
}

/// sprintf(format, args...) - Format string using printf-style formatting.
pub fn builtin_sprintf(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let format = &args[0];
    let values = &args[1..];
    format_printf(format, values)
}

// ─── Sub/Gsub Replacement Handling ───────────────────────────────

/// Process replacement string for sub/gsub.
/// & = matched text, \& = literal &, \\ = literal \
fn process_sub_replacement(replacement: &str, matched: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = replacement.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '&' {
                result.push('&');
                i += 2;
            } else if next == '\\' {
                result.push('\\');
                i += 2;
            } else {
                result.push(next);
                i += 2;
            }
        } else if chars[i] == '&' {
            result.push_str(matched);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// sub(regex, replacement, target?) - Replace first match of regex in target.
/// Returns 1 if replacement was made, 0 otherwise.
/// The target defaults to $0.
pub fn builtin_sub(
    pattern: &str,
    replacement: &str,
    target: &str,
) -> (String, String) {
    let regex = match Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => return ("0".to_string(), target.to_string()),
    };

    if let Some(m) = regex.find(target) {
        let matched = m.as_str();
        let new_text = process_sub_replacement(replacement, matched);
        let new_target = format!(
            "{}{}{}",
            &target[..m.start()],
            new_text,
            &target[m.end()..]
        );
        ("1".to_string(), new_target)
    } else {
        ("0".to_string(), target.to_string())
    }
}

/// gsub(regex, replacement, target?) - Replace all matches of regex in target.
/// Returns the number of replacements made.
pub fn builtin_gsub(
    pattern: &str,
    replacement: &str,
    target: &str,
) -> (String, String) {
    let regex = match Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => return ("0".to_string(), target.to_string()),
    };

    let mut count = 0;
    let mut result = String::new();
    let mut last_end = 0;

    for m in regex.find_iter(target) {
        result.push_str(&target[last_end..m.start()]);
        let matched = m.as_str();
        result.push_str(&process_sub_replacement(replacement, matched));
        last_end = m.end();
        count += 1;
    }
    result.push_str(&target[last_end..]);

    (to_string(count as f64), result)
}

/// match(s, regex) - Find regex match in s, set RSTART/RLENGTH.
/// Returns position of match (1-indexed) or 0 if not found.
pub fn builtin_match(s: &str, pattern: &str, ctx: &mut AwkContext) -> String {
    let regex = match Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => {
            ctx.rstart = 0;
            ctx.rlength = -1;
            return "0".to_string();
        }
    };

    if let Some(m) = regex.find(s) {
        // Convert byte position to character position
        let char_pos = s[..m.start()].chars().count() + 1;
        let char_len = m.as_str().chars().count();
        ctx.rstart = char_pos;
        ctx.rlength = char_len as i64;
        to_string(char_pos as f64)
    } else {
        ctx.rstart = 0;
        ctx.rlength = -1;
        "0".to_string()
    }
}

// ─── Gensub (GNU Extension) ──────────────────────────────────────

/// Process replacement string for gensub.
/// & or \0 = entire match, \1-\9 = capture groups, \n = newline, \t = tab
fn process_gensub_replacement(replacement: &str, matched: &str, groups: &[&str]) -> String {
    let mut result = String::new();
    let chars: Vec<char> = replacement.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '&' {
                result.push('&');
                i += 2;
            } else if next == '0' {
                result.push_str(matched);
                i += 2;
            } else if next >= '1' && next <= '9' {
                let idx = (next as usize) - ('1' as usize);
                if idx < groups.len() {
                    result.push_str(groups[idx]);
                }
                i += 2;
            } else if next == 'n' {
                result.push('\n');
                i += 2;
            } else if next == 't' {
                result.push('\t');
                i += 2;
            } else {
                result.push(next);
                i += 2;
            }
        } else if chars[i] == '&' {
            result.push_str(matched);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// gensub(regex, replacement, how, target?) - GNU extension for substitution with backrefs.
/// how = "g" for global, or a number for specific occurrence.
/// Returns the modified string (does not modify target in place).
pub fn builtin_gensub(
    pattern: &str,
    replacement: &str,
    how: &str,
    target: &str,
) -> String {
    let regex = match Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => return target.to_string(),
    };

    let is_global = how.eq_ignore_ascii_case("g");
    let occurrence: usize = if is_global {
        0
    } else {
        how.parse().unwrap_or(1)
    };

    if is_global {
        // Replace all occurrences
        let mut result = String::new();
        let mut last_end = 0;

        for caps in regex.captures_iter(target) {
            let m = caps.get(0).unwrap();
            result.push_str(&target[last_end..m.start()]);

            let matched = m.as_str();
            let groups: Vec<&str> = (1..caps.len())
                .map(|i| caps.get(i).map(|m| m.as_str()).unwrap_or(""))
                .collect();

            result.push_str(&process_gensub_replacement(replacement, matched, &groups));
            last_end = m.end();
        }
        result.push_str(&target[last_end..]);
        result
    } else {
        // Replace specific occurrence
        let mut count = 0;
        let mut result = String::new();
        let mut last_end = 0;
        let mut replaced = false;

        for caps in regex.captures_iter(target) {
            count += 1;
            let m = caps.get(0).unwrap();

            if count == occurrence && !replaced {
                result.push_str(&target[last_end..m.start()]);

                let matched = m.as_str();
                let groups: Vec<&str> = (1..caps.len())
                    .map(|i| caps.get(i).map(|m| m.as_str()).unwrap_or(""))
                    .collect();

                result.push_str(&process_gensub_replacement(replacement, matched, &groups));
                last_end = m.end();
                replaced = true;
            }
        }

        if replaced {
            result.push_str(&target[last_end..]);
            result
        } else {
            target.to_string()
        }
    }
}

// ─── Math Functions ──────────────────────────────────────────────

/// int(x) - Truncate to integer (toward zero).
pub fn builtin_int(args: &[String]) -> String {
    if args.is_empty() {
        return "0".to_string();
    }
    let n = to_number(&args[0]);
    to_string(n.trunc())
}

/// sqrt(x) - Square root.
pub fn builtin_sqrt(args: &[String]) -> String {
    if args.is_empty() {
        return "0".to_string();
    }
    let n = to_number(&args[0]);
    to_string(n.sqrt())
}

/// sin(x) - Sine (x in radians).
pub fn builtin_sin(args: &[String]) -> String {
    if args.is_empty() {
        return "0".to_string();
    }
    let n = to_number(&args[0]);
    to_string(n.sin())
}

/// cos(x) - Cosine (x in radians).
pub fn builtin_cos(args: &[String]) -> String {
    if args.is_empty() {
        return "0".to_string();
    }
    let n = to_number(&args[0]);
    to_string(n.cos())
}

/// atan2(y, x) - Arc tangent of y/x.
pub fn builtin_atan2(args: &[String]) -> String {
    let y = if !args.is_empty() {
        to_number(&args[0])
    } else {
        0.0
    };
    let x = if args.len() > 1 {
        to_number(&args[1])
    } else {
        0.0
    };
    to_string(y.atan2(x))
}

/// log(x) - Natural logarithm.
pub fn builtin_log(args: &[String]) -> String {
    if args.is_empty() {
        return to_string(f64::NEG_INFINITY);
    }
    let n = to_number(&args[0]);
    to_string(n.ln())
}

/// exp(x) - Exponential (e^x).
pub fn builtin_exp(args: &[String]) -> String {
    if args.is_empty() {
        return "1".to_string();
    }
    let n = to_number(&args[0]);
    to_string(n.exp())
}

/// rand() - Random number in [0, 1).
pub fn builtin_rand() -> String {
    // Use a simple LCG for reproducibility when seeded
    // In practice, the interpreter should maintain the RNG state
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let r = ((seed as u64).wrapping_mul(1103515245).wrapping_add(12345) % (1 << 31)) as f64
        / (1u64 << 31) as f64;
    to_string(r)
}

/// srand(seed?) - Seed the random number generator.
/// Returns the previous seed.
pub fn builtin_srand(args: &[String]) -> String {
    let seed = if args.is_empty() {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0) as f64
    } else {
        to_number(&args[0])
    };
    to_string(seed)
}

// ─── Stub Functions ──────────────────────────────────────────────

/// system() - Execute shell command (disabled for security).
pub fn builtin_system(_args: &[String]) -> BuiltinResult {
    BuiltinResult::Error("system() is not supported - shell execution not allowed".to_string())
}

/// close() - Close file (no-op in our implementation).
pub fn builtin_close(_args: &[String]) -> String {
    "0".to_string()
}

/// fflush() - Flush output (no-op in our implementation).
pub fn builtin_fflush(_args: &[String]) -> String {
    "0".to_string()
}

// ─── Built-in Function Registry ──────────────────────────────────

/// Check if a function name is a built-in function.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "length"
            | "substr"
            | "index"
            | "split"
            | "sub"
            | "gsub"
            | "match"
            | "gensub"
            | "tolower"
            | "toupper"
            | "sprintf"
            | "int"
            | "sqrt"
            | "sin"
            | "cos"
            | "atan2"
            | "log"
            | "exp"
            | "rand"
            | "srand"
            | "system"
            | "close"
            | "fflush"
    )
}

/// Call a built-in function by name.
/// Returns None if the function is not a built-in.
/// For functions like sub/gsub that modify variables, returns ValueWithSideEffect.
pub fn call_builtin(
    name: &str,
    args: &[String],
    ctx: &mut AwkContext,
) -> Option<BuiltinResult> {
    match name {
        "length" => Some(BuiltinResult::Value(builtin_length(args, ctx))),
        "substr" => Some(BuiltinResult::Value(builtin_substr(args))),
        "index" => Some(BuiltinResult::Value(builtin_index(args))),
        "tolower" => Some(BuiltinResult::Value(builtin_tolower(args))),
        "toupper" => Some(BuiltinResult::Value(builtin_toupper(args))),
        "sprintf" => Some(BuiltinResult::Value(builtin_sprintf(args))),
        "int" => Some(BuiltinResult::Value(builtin_int(args))),
        "sqrt" => Some(BuiltinResult::Value(builtin_sqrt(args))),
        "sin" => Some(BuiltinResult::Value(builtin_sin(args))),
        "cos" => Some(BuiltinResult::Value(builtin_cos(args))),
        "atan2" => Some(BuiltinResult::Value(builtin_atan2(args))),
        "log" => Some(BuiltinResult::Value(builtin_log(args))),
        "exp" => Some(BuiltinResult::Value(builtin_exp(args))),
        "rand" => Some(BuiltinResult::Value(builtin_rand())),
        "srand" => Some(BuiltinResult::Value(builtin_srand(args))),
        "match" => {
            if args.len() >= 2 {
                Some(BuiltinResult::Value(builtin_match(&args[0], &args[1], ctx)))
            } else {
                Some(BuiltinResult::Value("0".to_string()))
            }
        }
        "system" => Some(builtin_system(args)),
        "close" => Some(BuiltinResult::Value(builtin_close(args))),
        "fflush" => Some(BuiltinResult::Value(builtin_fflush(args))),
        // split, sub, gsub, gensub need special handling by the interpreter
        // because they modify arrays or variables
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── String Function Tests ───────────────────────────────────

    #[test]
    fn test_length_hello() {
        let args = vec!["hello".to_string()];
        let ctx = AwkContext::new();
        assert_eq!(builtin_length(&args, &ctx), "5");
    }

    #[test]
    fn test_length_empty() {
        let args = vec!["".to_string()];
        let ctx = AwkContext::new();
        assert_eq!(builtin_length(&args, &ctx), "0");
    }

    #[test]
    fn test_length_no_args_uses_line() {
        let args: Vec<String> = vec![];
        let mut ctx = AwkContext::new();
        ctx.line = "test line".to_string();
        assert_eq!(builtin_length(&args, &ctx), "9");
    }

    #[test]
    fn test_substr_from_position() {
        let args = vec!["hello".to_string(), "2".to_string()];
        assert_eq!(builtin_substr(&args), "ello");
    }

    #[test]
    fn test_substr_with_length() {
        let args = vec!["hello".to_string(), "2".to_string(), "3".to_string()];
        assert_eq!(builtin_substr(&args), "ell");
    }

    #[test]
    fn test_substr_start_beyond_string() {
        let args = vec!["hello".to_string(), "10".to_string()];
        assert_eq!(builtin_substr(&args), "");
    }

    #[test]
    fn test_index_found() {
        let args = vec!["hello".to_string(), "ll".to_string()];
        assert_eq!(builtin_index(&args), "3");
    }

    #[test]
    fn test_index_not_found() {
        let args = vec!["hello".to_string(), "xyz".to_string()];
        assert_eq!(builtin_index(&args), "0");
    }

    #[test]
    fn test_split_colon() {
        let (count, array) = builtin_split("a:b:c", Some(":"), " ");
        assert_eq!(count, 3);
        assert_eq!(array.get("1"), Some(&"a".to_string()));
        assert_eq!(array.get("2"), Some(&"b".to_string()));
        assert_eq!(array.get("3"), Some(&"c".to_string()));
    }

    #[test]
    fn test_split_whitespace() {
        let (count, array) = builtin_split("a b c", None, " ");
        assert_eq!(count, 3);
        assert_eq!(array.get("1"), Some(&"a".to_string()));
    }

    #[test]
    fn test_tolower() {
        let args = vec!["HELLO".to_string()];
        assert_eq!(builtin_tolower(&args), "hello");
    }

    #[test]
    fn test_toupper() {
        let args = vec!["hello".to_string()];
        assert_eq!(builtin_toupper(&args), "HELLO");
    }

    // ─── Sub/Gsub Tests ──────────────────────────────────────────

    #[test]
    fn test_sub_basic() {
        let (count, result) = builtin_sub("o", "0", "hello");
        assert_eq!(count, "1");
        assert_eq!(result, "hell0");
    }

    #[test]
    fn test_sub_no_match() {
        let (count, result) = builtin_sub("x", "y", "hello");
        assert_eq!(count, "0");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_sub_ampersand_replacement() {
        let (_, result) = builtin_sub("l", "[&]", "hello");
        assert_eq!(result, "he[l]lo");
    }

    #[test]
    fn test_gsub_basic() {
        let (count, result) = builtin_gsub("l", "L", "hello");
        assert_eq!(count, "2");
        assert_eq!(result, "heLLo");
    }

    #[test]
    fn test_gsub_no_match() {
        let (count, result) = builtin_gsub("x", "y", "hello");
        assert_eq!(count, "0");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_match_found() {
        let mut ctx = AwkContext::new();
        let result = builtin_match("hello world", "wor", &mut ctx);
        assert_eq!(result, "7");
        assert_eq!(ctx.rstart, 7);
        assert_eq!(ctx.rlength, 3);
    }

    #[test]
    fn test_match_not_found() {
        let mut ctx = AwkContext::new();
        let result = builtin_match("hello", "xyz", &mut ctx);
        assert_eq!(result, "0");
        assert_eq!(ctx.rstart, 0);
        assert_eq!(ctx.rlength, -1);
    }

    #[test]
    fn test_gensub_global() {
        let result = builtin_gensub("l", "L", "g", "hello");
        assert_eq!(result, "heLLo");
    }

    #[test]
    fn test_gensub_specific_occurrence() {
        let result = builtin_gensub("l", "L", "2", "hello");
        assert_eq!(result, "helLo");
    }

    // ─── Math Function Tests ─────────────────────────────────────

    #[test]
    fn test_int_positive() {
        let args = vec!["3.9".to_string()];
        assert_eq!(builtin_int(&args), "3");
    }

    #[test]
    fn test_int_negative() {
        let args = vec!["-3.9".to_string()];
        assert_eq!(builtin_int(&args), "-3");
    }

    #[test]
    fn test_sqrt_4() {
        let args = vec!["4".to_string()];
        assert_eq!(builtin_sqrt(&args), "2");
    }

    #[test]
    fn test_sin_zero() {
        let args = vec!["0".to_string()];
        assert_eq!(builtin_sin(&args), "0");
    }

    #[test]
    fn test_cos_zero() {
        let args = vec!["0".to_string()];
        assert_eq!(builtin_cos(&args), "1");
    }

    #[test]
    fn test_atan2_zero_one() {
        let args = vec!["0".to_string(), "1".to_string()];
        assert_eq!(builtin_atan2(&args), "0");
    }

    #[test]
    fn test_log_one() {
        let args = vec!["1".to_string()];
        assert_eq!(builtin_log(&args), "0");
    }

    #[test]
    fn test_exp_zero() {
        let args = vec!["0".to_string()];
        assert_eq!(builtin_exp(&args), "1");
    }

    // ─── Printf Format Tests ─────────────────────────────────────

    #[test]
    fn test_sprintf_string() {
        assert_eq!(format_printf("%s", &["hello".to_string()]), "hello");
    }

    #[test]
    fn test_sprintf_integer() {
        assert_eq!(format_printf("%d", &["42".to_string()]), "42");
    }

    #[test]
    fn test_sprintf_zero_padded() {
        assert_eq!(format_printf("%05d", &["42".to_string()]), "00042");
    }

    #[test]
    fn test_sprintf_left_justify() {
        assert_eq!(format_printf("%-10s", &["hi".to_string()]), "hi        ");
    }

    #[test]
    fn test_sprintf_char_from_number() {
        assert_eq!(format_printf("%c", &["65".to_string()]), "A");
    }

    #[test]
    fn test_sprintf_percent() {
        assert_eq!(format_printf("%%", &[]), "%");
    }

    #[test]
    fn test_sprintf_float() {
        assert_eq!(format_printf("%f", &["3.14159".to_string()]), "3.141590");
    }

    #[test]
    fn test_sprintf_float_precision() {
        assert_eq!(format_printf("%.2f", &["3.14159".to_string()]), "3.14");
    }

    #[test]
    fn test_sprintf_scientific() {
        let result = format_printf("%e", &["1234.5".to_string()]);
        assert!(result.contains('e'));
    }

    #[test]
    fn test_sprintf_hex_lower() {
        assert_eq!(format_printf("%x", &["255".to_string()]), "ff");
    }

    #[test]
    fn test_sprintf_hex_upper() {
        assert_eq!(format_printf("%X", &["255".to_string()]), "FF");
    }

    #[test]
    fn test_sprintf_octal() {
        assert_eq!(format_printf("%o", &["8".to_string()]), "10");
    }

    // ─── Registry Tests ──────────────────────────────────────────

    #[test]
    fn test_is_builtin_true() {
        assert!(is_builtin("length"));
        assert!(is_builtin("substr"));
        assert!(is_builtin("sqrt"));
    }

    #[test]
    fn test_is_builtin_false() {
        assert!(!is_builtin("myfunction"));
        assert!(!is_builtin("unknown"));
    }

    #[test]
    fn test_call_builtin_length() {
        let mut ctx = AwkContext::new();
        let result = call_builtin("length", &["test".to_string()], &mut ctx);
        match result {
            Some(BuiltinResult::Value(v)) => assert_eq!(v, "4"),
            _ => panic!("Expected Value result"),
        }
    }

    #[test]
    fn test_call_builtin_unknown() {
        let mut ctx = AwkContext::new();
        let result = call_builtin("unknown_func", &[], &mut ctx);
        assert!(result.is_none());
    }
}
