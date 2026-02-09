use std::time::SystemTime;

use super::types::*;

/// Evaluate a find expression against an EvalContext, returning an EvalResult.
pub fn evaluate(expr: &Expression, ctx: &EvalContext) -> EvalResult {
    match expr {
        Expression::Name { pattern, case_insensitive } => {
            let matches = if *case_insensitive {
                glob_match(&pattern.to_lowercase(), &ctx.name.to_lowercase())
            } else {
                glob_match(pattern, &ctx.name)
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Path { pattern, case_insensitive } => {
            let matches = if *case_insensitive {
                glob_match(&pattern.to_lowercase(), &ctx.relative_path.to_lowercase())
            } else {
                glob_match(pattern, &ctx.relative_path)
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Regex { pattern, case_insensitive } => {
            let matches = if *case_insensitive {
                regex_lite::Regex::new(&format!("(?i){}", pattern))
                    .map(|re| re.is_match(&ctx.relative_path))
                    .unwrap_or(false)
            } else {
                regex_lite::Regex::new(pattern)
                    .map(|re| re.is_match(&ctx.relative_path))
                    .unwrap_or(false)
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Type(file_type) => {
            let matches = match file_type {
                FileType::File => ctx.is_file,
                FileType::Directory => ctx.is_directory,
                FileType::Symlink => ctx.is_symlink,
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Empty => {
            EvalResult { matches: ctx.is_empty, pruned: false, printed: false, output: String::new() }
        }
        Expression::Mtime { days, comparison } => {
            let now = SystemTime::now();
            let duration = now.duration_since(ctx.mtime).unwrap_or_default();
            let days_ago = (duration.as_secs() / 86400) as i64;
            let matches = match comparison {
                Comparison::GreaterThan => days_ago > *days,
                Comparison::LessThan => days_ago < *days,
                Comparison::Exact => days_ago == *days,
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Newer { reference_path: _ } => {
            let matches = match ctx.newer_ref_mtime {
                Some(ref_mtime) => ctx.mtime > ref_mtime,
                None => false,
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Size { value, unit, comparison } => {
            let multiplier: i64 = match unit {
                SizeUnit::Bytes => 1,
                SizeUnit::Kilobytes => 1024,
                SizeUnit::Megabytes => 1048576,
                SizeUnit::Gigabytes => 1073741824,
                SizeUnit::Blocks => 512,
            };
            let target_bytes = *value * multiplier;
            let matches = match comparison {
                Comparison::GreaterThan => (ctx.size as i64) > target_bytes,
                Comparison::LessThan => (ctx.size as i64) < target_bytes,
                Comparison::Exact => {
                    if matches!(unit, SizeUnit::Blocks) {
                        let file_blocks = ((ctx.size as i64) + 511) / 512;
                        file_blocks == *value
                    } else {
                        (ctx.size as i64) == target_bytes
                    }
                }
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Perm { mode, match_type } => {
            let file_mode = ctx.mode & 0o777;
            let target_mode = *mode & 0o777;
            let matches = match match_type {
                PermMatch::Exact => file_mode == target_mode,
                PermMatch::AllBits => (file_mode & target_mode) == target_mode,
                PermMatch::AnyBits => (file_mode & target_mode) != 0,
            };
            EvalResult { matches, pruned: false, printed: false, output: String::new() }
        }
        Expression::Prune => {
            EvalResult { matches: true, pruned: true, printed: false, output: String::new() }
        }
        Expression::Print => {
            let mut output = ctx.relative_path.clone();
            output.push('\n');
            EvalResult { matches: true, pruned: false, printed: true, output }
        }
        Expression::Print0 => {
            let mut output = ctx.relative_path.clone();
            output.push('\0');
            EvalResult { matches: true, pruned: false, printed: true, output }
        }
        Expression::Printf { format } => {
            let output = format_printf(format, ctx);
            EvalResult { matches: true, pruned: false, printed: true, output }
        }
        Expression::Delete => {
            EvalResult { matches: true, pruned: false, printed: true, output: String::new() }
        }
        Expression::Exec { .. } => {
            EvalResult { matches: true, pruned: false, printed: true, output: String::new() }
        }
        Expression::Not(inner) => {
            let inner_result = evaluate(inner, ctx);
            EvalResult {
                matches: !inner_result.matches,
                pruned: inner_result.pruned,
                printed: inner_result.printed,
                output: inner_result.output,
            }
        }
        Expression::And(left, right) => {
            let left_result = evaluate(left, ctx);
            if !left_result.matches {
                return EvalResult {
                    matches: false,
                    pruned: left_result.pruned,
                    printed: left_result.printed,
                    output: left_result.output,
                };
            }
            let right_result = evaluate(right, ctx);
            EvalResult {
                matches: right_result.matches,
                pruned: left_result.pruned || right_result.pruned,
                printed: left_result.printed || right_result.printed,
                output: format!("{}{}", left_result.output, right_result.output),
            }
        }
        Expression::Or(left, right) => {
            let left_result = evaluate(left, ctx);
            if left_result.matches {
                return left_result;
            }
            let right_result = evaluate(right, ctx);
            EvalResult {
                matches: right_result.matches,
                pruned: left_result.pruned || right_result.pruned,
                printed: right_result.printed,
                output: format!("{}{}", left_result.output, right_result.output),
            }
        }
    }
}
/// Glob-style pattern matching supporting *, ?, [...], [!...].
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_inner(&pat, &txt, 0, 0)
}

fn glob_match_inner(pat: &[char], txt: &[char], mut pi: usize, mut ti: usize) -> bool {
    let mut star_pi: Option<usize> = None;
    let mut star_ti: usize = 0;

    while ti < txt.len() {
        if pi < pat.len() && pat[pi] == '*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if pi < pat.len() && pat[pi] == '?' {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == '[' {
            if let Some((matched, end)) = match_char_class(pat, pi, txt[ti]) {
                if matched {
                    pi = end;
                    ti += 1;
                } else if let Some(sp) = star_pi {
                    pi = sp + 1;
                    star_ti += 1;
                    ti = star_ti;
                } else {
                    return false;
                }
            } else if let Some(sp) = star_pi {
                pi = sp + 1;
                star_ti += 1;
                ti = star_ti;
            } else {
                return false;
            }
        } else if pi < pat.len() && pat[pi] == txt[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }

    pi == pat.len()
}
/// Match a character class like [abc], [a-z], [!abc].
/// Returns Some((matched, end_index)) where end_index is past the ']'.
fn match_char_class(pat: &[char], start: usize, ch: char) -> Option<(bool, usize)> {
    let mut i = start + 1; // skip '['
    if i >= pat.len() {
        return None;
    }

    let negated = pat[i] == '!' || pat[i] == '^';
    if negated {
        i += 1;
    }

    let mut matched = false;
    let mut first = true;

    while i < pat.len() && (pat[i] != ']' || first) {
        first = false;
        if i + 2 < pat.len() && pat[i + 1] == '-' && pat[i + 2] != ']' {
            // Range: a-z
            let lo = pat[i];
            let hi = pat[i + 2];
            if ch >= lo && ch <= hi {
                matched = true;
            }
            i += 3;
        } else {
            if pat[i] == ch {
                matched = true;
            }
            i += 1;
        }
    }

    if i < pat.len() && pat[i] == ']' {
        let result = if negated { !matched } else { matched };
        Some((result, i + 1))
    } else {
        None // unclosed bracket
    }
}
/// Format a printf-style format string using find context.
fn format_printf(format: &str, ctx: &EvalContext) -> String {
    // First process escape sequences
    let processed = process_escapes(format);
    let chars: Vec<char> = processed.chars().collect();
    let mut output = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            i += 1;
            if chars[i] == '%' {
                output.push('%');
                i += 1;
                continue;
            }

            let directive = chars[i];
            match directive {
                'f' => {
                    output.push_str(&ctx.name);
                    i += 1;
                }
                'h' => {
                    let last_slash = ctx.relative_path.rfind('/');
                    let dir = match last_slash {
                        Some(0) => "/",
                        Some(pos) => &ctx.relative_path[..pos],
                        None => ".",
                    };
                    output.push_str(dir);
                    i += 1;
                }
                'p' => {
                    output.push_str(&ctx.relative_path);
                    i += 1;
                }
                'P' => {
                    let sp = &ctx.starting_point;
                    let value = if ctx.relative_path == *sp {
                        String::new()
                    } else if ctx.relative_path.starts_with(&format!("{}/", sp)) {
                        ctx.relative_path[sp.len() + 1..].to_string()
                    } else if sp == "." && ctx.relative_path.starts_with("./") {
                        ctx.relative_path[2..].to_string()
                    } else {
                        ctx.relative_path.clone()
                    };
                    output.push_str(&value);
                    i += 1;
                }
                's' => {
                    output.push_str(&ctx.size.to_string());
                    i += 1;
                }
                'd' => {
                    output.push_str(&ctx.depth.to_string());
                    i += 1;
                }
                'm' => {
                    output.push_str(&format!("{:o}", ctx.mode & 0o777));
                    i += 1;
                }
                'M' => {
                    output.push_str(&format_symbolic_mode(ctx.mode, ctx.is_directory));
                    i += 1;
                }
                't' => {
                    output.push_str(&format_ctime_date(ctx.mtime));
                    i += 1;
                }
                'T' => {
                    if i + 1 < chars.len() {
                        let time_fmt = chars[i + 1];
                        output.push_str(&format_time_directive(ctx.mtime, time_fmt));
                        i += 2;
                    } else {
                        output.push_str("%T");
                        i += 1;
                    }
                }
                _ => {
                    output.push('%');
                    output.push(directive);
                    i += 1;
                }
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    output
}
/// Process escape sequences in a format string.
fn process_escapes(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => { result.push('\n'); i += 2; }
                't' => { result.push('\t'); i += 2; }
                '0' => { result.push('\0'); i += 2; }
                '\\' => { result.push('\\'); i += 2; }
                _ => { result.push(chars[i]); i += 1; }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Format permissions in symbolic form like -rwxr-xr-x.
fn format_symbolic_mode(mode: u32, is_directory: bool) -> String {
    let perms = mode & 0o777;
    let mut result = String::with_capacity(10);
    result.push(if is_directory { 'd' } else { '-' });
    result.push(if perms & 0o400 != 0 { 'r' } else { '-' });
    result.push(if perms & 0o200 != 0 { 'w' } else { '-' });
    result.push(if perms & 0o100 != 0 { 'x' } else { '-' });
    result.push(if perms & 0o040 != 0 { 'r' } else { '-' });
    result.push(if perms & 0o020 != 0 { 'w' } else { '-' });
    result.push(if perms & 0o010 != 0 { 'x' } else { '-' });
    result.push(if perms & 0o004 != 0 { 'r' } else { '-' });
    result.push(if perms & 0o002 != 0 { 'w' } else { '-' });
    result.push(if perms & 0o001 != 0 { 'x' } else { '-' });
    result
}

/// Format date in ctime format.
fn format_ctime_date(mtime: SystemTime) -> String {
    let duration = mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    format_unix_timestamp_ctime(secs)
}

fn format_unix_timestamp_ctime(timestamp: i64) -> String {
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                   "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    // Simple date calculation from unix timestamp
    let (year, month, day, hour, min, sec, wday) = unix_to_date(timestamp);

    format!("{} {} {:>2} {:02}:{:02}:{:02} {}",
        days[wday as usize % 7],
        months[month as usize],
        day, hour, min, sec, year)
}
/// Format time with %T directive format character.
fn format_time_directive(mtime: SystemTime, fmt: char) -> String {
    let duration = mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let (year, month, day, hour, min, sec, _wday) = unix_to_date(secs);

    match fmt {
        '@' => format!("{}", secs),
        'Y' => format!("{}", year),
        'm' => format!("{:02}", month + 1),
        'd' => format!("{:02}", day),
        'H' => format!("{:02}", hour),
        'M' => format!("{:02}", min),
        'S' => format!("{:02}", sec),
        'T' => format!("{:02}:{:02}:{:02}", hour, min, sec),
        'F' => format!("{}-{:02}-{:02}", year, month + 1, day),
        _ => format!("%T{}", fmt),
    }
}

/// Convert a unix timestamp to (year, month_0based, day, hour, min, sec, weekday).
fn unix_to_date(timestamp: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let secs_per_day: i64 = 86400;
    let mut days = timestamp / secs_per_day;
    let mut remaining = timestamp % secs_per_day;
    if remaining < 0 {
        days -= 1;
        remaining += secs_per_day;
    }
    let hour = remaining / 3600;
    remaining %= 3600;
    let min = remaining / 60;
    let sec = remaining % 60;

    // Day of week: Jan 1 1970 was Thursday (4)
    let wday = ((days % 7) + 4 + 7) % 7;

    // Calculate year/month/day from days since epoch
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0i64;
    for md in &month_days {
        if days < *md {
            break;
        }
        days -= *md;
        month += 1;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec, wday)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_ctx(name: &str, relative_path: &str, is_file: bool, is_directory: bool) -> EvalContext {
        EvalContext {
            name: name.to_string(),
            path: format!("/test/{}", name),
            relative_path: relative_path.to_string(),
            is_file,
            is_directory,
            is_symlink: false,
            size: 100,
            mode: 0o644,
            mtime: SystemTime::now(),
            depth: 0,
            is_empty: false,
            newer_ref_mtime: None,
            starting_point: ".".to_string(),
        }
    }

    // --- Glob matching tests ---

    #[test]
    fn test_glob_star_matches_txt() {
        assert!(glob_match("*.txt", "file.txt"));
        assert!(!glob_match("*.txt", "file.rs"));
    }

    #[test]
    fn test_glob_question_mark() {
        assert!(glob_match("file?.rs", "file1.rs"));
        assert!(!glob_match("file?.rs", "file12.rs"));
    }

    #[test]
    fn test_glob_char_class() {
        assert!(glob_match("[abc].txt", "a.txt"));
        assert!(glob_match("[abc].txt", "b.txt"));
        assert!(!glob_match("[abc].txt", "d.txt"));
    }

    #[test]
    fn test_glob_char_range() {
        assert!(glob_match("[a-z].txt", "m.txt"));
        assert!(!glob_match("[a-z].txt", "M.txt"));
    }

    #[test]
    fn test_glob_negated_class() {
        assert!(!glob_match("[!abc].txt", "a.txt"));
        assert!(glob_match("[!abc].txt", "d.txt"));
    }

    // --- Name matching tests ---

    #[test]
    fn test_name_glob_matching() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let expr = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
    }

    #[test]
    fn test_name_case_insensitive() {
        let ctx = make_ctx("FILE.TXT", "./FILE.TXT", true, false);
        let expr = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: true };
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
    }
    // --- Type matching tests ---

    #[test]
    fn test_type_file() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let expr = Expression::Type(FileType::File);
        assert!(evaluate(&expr, &ctx).matches);
    }

    #[test]
    fn test_type_directory() {
        let ctx = make_ctx("dir", "./dir", false, true);
        let expr = Expression::Type(FileType::Directory);
        assert!(evaluate(&expr, &ctx).matches);
    }

    // --- Size comparison tests ---

    #[test]
    fn test_size_greater_than() {
        let mut ctx = make_ctx("big.bin", "./big.bin", true, false);
        ctx.size = 2048;
        let expr = Expression::Size { value: 1, unit: SizeUnit::Kilobytes, comparison: Comparison::GreaterThan };
        assert!(evaluate(&expr, &ctx).matches);
    }

    #[test]
    fn test_size_less_than() {
        let mut ctx = make_ctx("small.bin", "./small.bin", true, false);
        ctx.size = 500;
        let expr = Expression::Size { value: 1, unit: SizeUnit::Megabytes, comparison: Comparison::LessThan };
        assert!(evaluate(&expr, &ctx).matches);
    }

    #[test]
    fn test_size_exact() {
        let mut ctx = make_ctx("exact.bin", "./exact.bin", true, false);
        ctx.size = 1024;
        let expr = Expression::Size { value: 1024, unit: SizeUnit::Bytes, comparison: Comparison::Exact };
        assert!(evaluate(&expr, &ctx).matches);
    }

    // --- Mtime comparison tests ---

    #[test]
    fn test_mtime_greater_than() {
        let mut ctx = make_ctx("old.txt", "./old.txt", true, false);
        // Set mtime to 10 days ago
        ctx.mtime = SystemTime::now() - Duration::from_secs(10 * 86400 + 100);
        let expr = Expression::Mtime { days: 5, comparison: Comparison::GreaterThan };
        assert!(evaluate(&expr, &ctx).matches);
    }
    // --- Permission matching tests ---

    #[test]
    fn test_perm_exact() {
        let mut ctx = make_ctx("script.sh", "./script.sh", true, false);
        ctx.mode = 0o755;
        let expr = Expression::Perm { mode: 0o755, match_type: PermMatch::Exact };
        assert!(evaluate(&expr, &ctx).matches);
    }

    #[test]
    fn test_perm_all_bits() {
        let mut ctx = make_ctx("script.sh", "./script.sh", true, false);
        ctx.mode = 0o755;
        let expr = Expression::Perm { mode: 0o111, match_type: PermMatch::AllBits };
        assert!(evaluate(&expr, &ctx).matches);
    }

    #[test]
    fn test_perm_any_bits() {
        let mut ctx = make_ctx("script.sh", "./script.sh", true, false);
        ctx.mode = 0o644;
        let expr = Expression::Perm { mode: 0o100, match_type: PermMatch::AnyBits };
        assert!(!evaluate(&expr, &ctx).matches);
        let expr2 = Expression::Perm { mode: 0o004, match_type: PermMatch::AnyBits };
        assert!(evaluate(&expr2, &ctx).matches);
    }

    // --- NOT expression ---

    #[test]
    fn test_not_expression() {
        let ctx = make_ctx("file.rs", "./file.rs", true, false);
        let inner = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let expr = Expression::Not(Box::new(inner));
        assert!(evaluate(&expr, &ctx).matches);
    }

    // --- AND expression (short-circuit) ---

    #[test]
    fn test_and_short_circuit() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let left = Expression::Type(FileType::Directory); // false
        let right = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let expr = Expression::And(Box::new(left), Box::new(right));
        let result = evaluate(&expr, &ctx);
        assert!(!result.matches);
    }

    #[test]
    fn test_and_both_true() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let left = Expression::Type(FileType::File);
        let right = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let expr = Expression::And(Box::new(left), Box::new(right));
        assert!(evaluate(&expr, &ctx).matches);
    }
    // --- OR expression (short-circuit) ---

    #[test]
    fn test_or_short_circuit() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let left = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let right = Expression::Type(FileType::Directory);
        let expr = Expression::Or(Box::new(left), Box::new(right));
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
    }

    #[test]
    fn test_or_fallthrough() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let left = Expression::Type(FileType::Directory); // false
        let right = Expression::Name { pattern: "*.txt".to_string(), case_insensitive: false };
        let expr = Expression::Or(Box::new(left), Box::new(right));
        assert!(evaluate(&expr, &ctx).matches);
    }

    // --- Prune flag propagation ---

    #[test]
    fn test_prune_flag() {
        let ctx = make_ctx("node_modules", "./node_modules", false, true);
        let expr = Expression::Prune;
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
        assert!(result.pruned);
    }

    // --- Print output format ---

    #[test]
    fn test_print_output() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let expr = Expression::Print;
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
        assert!(result.printed);
        assert_eq!(result.output, "./file.txt\n");
    }

    // --- Print0 output format ---

    #[test]
    fn test_print0_output() {
        let ctx = make_ctx("file.txt", "./file.txt", true, false);
        let expr = Expression::Print0;
        let result = evaluate(&expr, &ctx);
        assert!(result.matches);
        assert!(result.printed);
        assert_eq!(result.output, "./file.txt\0");
    }

    // --- Printf format directives ---

    #[test]
    fn test_printf_directives() {
        let mut ctx = make_ctx("file.txt", "./dir/file.txt", true, false);
        ctx.size = 42;
        ctx.depth = 2;
        ctx.mode = 0o755;
        ctx.starting_point = ".".to_string();

        let expr = Expression::Printf { format: "%f\\n".to_string() };
        let result = evaluate(&expr, &ctx);
        assert_eq!(result.output, "file.txt\n");

        let expr2 = Expression::Printf { format: "%p %s\\n".to_string() };
        let result2 = evaluate(&expr2, &ctx);
        assert_eq!(result2.output, "./dir/file.txt 42\n");

        let expr3 = Expression::Printf { format: "%m %d\\n".to_string() };
        let result3 = evaluate(&expr3, &ctx);
        assert_eq!(result3.output, "755 2\n");

        let expr4 = Expression::Printf { format: "%h\\n".to_string() };
        let result4 = evaluate(&expr4, &ctx);
        assert_eq!(result4.output, "./dir\n");

        let expr5 = Expression::Printf { format: "%%\\n".to_string() };
        let result5 = evaluate(&expr5, &ctx);
        assert_eq!(result5.output, "%\n");
    }
}
