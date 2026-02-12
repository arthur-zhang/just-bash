// src/commands/date/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use chrono::{DateTime, Utc, Local, TimeZone, NaiveDateTime, Datelike, Timelike, Duration, NaiveDate};

pub struct DateCommand;

const HELP: &str = "Usage: date [OPTION]... [+FORMAT]\n\n\
Display the current time in the given FORMAT, or set the system date.\n\n\
Options:\n  -d STRING  display time described by STRING\n  -u         print UTC\n  -I         output in ISO 8601 format\n  -R         output in RFC 5322 format\n      --help display this help\n\n\
FORMAT controls the output. Common sequences:\n  %Y year  %m month  %d day  %H hour  %M minute  %S second\n  %F full date  %T full time  %a weekday  %b month name  %s timestamp\n";

#[async_trait]
impl Command for DateCommand {
    fn name(&self) -> &'static str { "date" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }

        let mut utc = false;
        let mut iso = false;
        let mut rfc = false;
        let mut date_str: Option<String> = None;
        let mut format_str: Option<String> = None;
        let mut i = 0;

        while i < args.len() {
            let a = &args[i];
            if a == "-u" || a == "--utc" || a == "--universal" {
                utc = true;
            } else if a == "-I" || a == "--iso-8601" {
                iso = true;
            } else if a == "-R" || a == "--rfc-2822" || a == "--rfc-email" {
                rfc = true;
            } else if a == "-d" {
                if i + 1 < args.len() { i += 1; date_str = Some(args[i].clone()); }
                else { return CommandResult::error("date: option '-d' requires an argument\n".into()); }
            } else if a.starts_with("--date=") {
                date_str = Some(a[7..].to_string());
            } else if a.starts_with("--date") {
                if i + 1 < args.len() { i += 1; date_str = Some(args[i].clone()); }
            } else if a.starts_with('+') {
                format_str = Some(a[1..].to_string());
            } else if a.starts_with("--") {
                return CommandResult::with_exit_code("".into(), format!("date: unrecognized option '{}'\n", a), 1);
            } else if a.starts_with('-') && a.len() > 1 {
                let ch = a.chars().nth(1).unwrap_or('?');
                if !"duIR".contains(ch) {
                    return CommandResult::with_exit_code("".into(), format!("date: invalid option -- '{}'\n", ch), 1);
                }
            }
            i += 1;
        }

        // Determine the date/time to use
        let now_utc = Utc::now();
        let dt_utc: DateTime<Utc> = if let Some(ref ds) = date_str {
            match parse_date_string(ds, now_utc) {
                Some(dt) => dt,
                None => return CommandResult::with_exit_code("".into(), format!("date: invalid date '{}'\n", ds), 1),
            }
        } else {
            now_utc
        };

        // Format the output
        let output = if iso {
            if utc { format!("{}\n", dt_utc.format("%Y-%m-%dT%H:%M:%S+00:00")) }
            else { let local: DateTime<Local> = dt_utc.into(); format!("{}\n", local.format("%Y-%m-%dT%H:%M:%S%:z")) }
        } else if rfc {
            if utc { format!("{}\n", dt_utc.format("%a, %d %b %Y %H:%M:%S +0000")) }
            else { let local: DateTime<Local> = dt_utc.into(); format!("{}\n", local.format("%a, %d %b %Y %H:%M:%S %z")) }
        } else if let Some(ref fmt) = format_str {
            let formatted = if utc { format_date(&dt_utc, fmt) } else { let local: DateTime<Local> = dt_utc.into(); format_date(&local, fmt) };
            format!("{}\n", formatted)
        } else {
            // Default format: "Tue Jan 15 12:00:00 UTC 2024"
            if utc { format!("{}\n", dt_utc.format("%a %b %d %H:%M:%S UTC %Y")) }
            else { let local: DateTime<Local> = dt_utc.into(); format!("{}\n", local.format("%a %b %d %H:%M:%S %Z %Y")) }
        };

        CommandResult::success(output)
    }
}

fn parse_date_string(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let s = s.trim();
    // Relative dates
    match s.to_lowercase().as_str() {
        "now" => return Some(now),
        "today" => { let local: DateTime<Local> = now.into(); let d = local.date_naive().and_hms_opt(0, 0, 0)?; return Local.from_local_datetime(&d).single().map(|dt| dt.with_timezone(&Utc)); }
        "yesterday" => { let local: DateTime<Local> = now.into(); let d = (local.date_naive() - Duration::days(1)).and_hms_opt(0, 0, 0)?; return Local.from_local_datetime(&d).single().map(|dt| dt.with_timezone(&Utc)); }
        "tomorrow" => { let local: DateTime<Local> = now.into(); let d = (local.date_naive() + Duration::days(1)).and_hms_opt(0, 0, 0)?; return Local.from_local_datetime(&d).single().map(|dt| dt.with_timezone(&Utc)); }
        _ => {}
    }
    // Try ISO 8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) { return Some(dt.with_timezone(&Utc)); }
    // Try common format with T separator
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Local.from_local_datetime(&ndt).single().map(|dt| dt.with_timezone(&Utc));
    }
    // Try date only
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let ndt = nd.and_hms_opt(0, 0, 0)?;
        return Local.from_local_datetime(&ndt).single().map(|dt| dt.with_timezone(&Utc));
    }
    // Try unix timestamp
    if let Ok(ts) = s.parse::<i64>() {
        return DateTime::from_timestamp(ts, 0).map(|dt| dt);
    }
    None
}

fn format_date<Tz: TimeZone>(dt: &DateTime<Tz>, fmt: &str) -> String where Tz::Offset: std::fmt::Display {
    let chars: Vec<char> = fmt.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                'Y' => result.push_str(&format!("{:04}", dt.year())),
                'm' => result.push_str(&format!("{:02}", dt.month())),
                'd' => result.push_str(&format!("{:02}", dt.day())),
                'H' => result.push_str(&format!("{:02}", dt.hour())),
                'M' => result.push_str(&format!("{:02}", dt.minute())),
                'S' => result.push_str(&format!("{:02}", dt.second())),
                'F' => result.push_str(&format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day())),
                'T' => result.push_str(&format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())),
                'I' => { let h = dt.hour() % 12; result.push_str(&format!("{:02}", if h == 0 { 12 } else { h })); }
                'p' => result.push_str(if dt.hour() < 12 { "AM" } else { "PM" }),
                'P' => result.push_str(if dt.hour() < 12 { "am" } else { "pm" }),
                'a' => { let days = ["Sun","Mon","Tue","Wed","Thu","Fri","Sat"]; result.push_str(days[dt.weekday().num_days_from_sunday() as usize]); }
                'A' => { let days = ["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]; result.push_str(days[dt.weekday().num_days_from_sunday() as usize]); }
                'b' | 'h' => { let ms = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]; result.push_str(ms[dt.month0() as usize]); }
                'B' => { let ms = ["January","February","March","April","May","June","July","August","September","October","November","December"]; result.push_str(ms[dt.month0() as usize]); }
                'e' => result.push_str(&format!("{:2}", dt.day())),
                'j' => result.push_str(&format!("{:03}", dt.ordinal())),
                's' => result.push_str(&format!("{}", dt.timestamp())),
                'u' => result.push_str(&format!("{}", dt.weekday().number_from_monday())),
                'w' => result.push_str(&format!("{}", dt.weekday().num_days_from_sunday())),
                'Z' => result.push_str(&format!("{}", dt.offset())),
                'z' => result.push_str(&format!("{}", dt.format("%z"))),
                'n' => result.push('\n'),
                't' => result.push('\t'),
                '%' => result.push('%'),
                'C' => result.push_str(&format!("{:02}", dt.year() / 100)),
                'y' => result.push_str(&format!("{:02}", dt.year() % 100)),
                'N' => result.push_str("000000000"),
                'R' => result.push_str(&format!("{:02}:{:02}", dt.hour(), dt.minute())),
                'r' => { let h = dt.hour() % 12; result.push_str(&format!("{:02}:{:02}:{:02} {}", if h == 0 { 12 } else { h }, dt.minute(), dt.second(), if dt.hour() < 12 { "AM" } else { "PM" })); }
                c => { result.push('%'); result.push(c); }
            }
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
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
    async fn test_date_year() { let r = DateCommand.execute(make_ctx(vec!["+%Y"])).await; assert!(r.stdout.trim().len() == 4); assert_eq!(r.exit_code, 0); }
    #[tokio::test]
    async fn test_date_iso() { let r = DateCommand.execute(make_ctx(vec!["-d", "2024-01-15T12:00:00", "+%Y-%m-%d"])).await; assert_eq!(r.stdout, "2024-01-15\n"); }
    #[tokio::test]
    async fn test_date_utc_z() { let r = DateCommand.execute(make_ctx(vec!["-u", "+%z"])).await; assert_eq!(r.stdout, "+0000\n"); }
    #[tokio::test]
    async fn test_date_percent() { let r = DateCommand.execute(make_ctx(vec!["+%%"])).await; assert_eq!(r.stdout, "%\n"); }
    #[tokio::test]
    async fn test_date_invalid() { let r = DateCommand.execute(make_ctx(vec!["-d", "invalid date string xyz"])).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("invalid date")); }
    #[tokio::test]
    async fn test_date_unknown_opt() { let r = DateCommand.execute(make_ctx(vec!["--unknown"])).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("unrecognized")); }
    #[tokio::test]
    async fn test_date_help() { let r = DateCommand.execute(make_ctx(vec!["--help"])).await; assert!(r.stdout.contains("date")); assert!(r.stdout.contains("FORMAT")); assert_eq!(r.exit_code, 0); }
    #[tokio::test]
    async fn test_date_default() { let r = DateCommand.execute(make_ctx(vec![])).await; assert_eq!(r.exit_code, 0); assert!(!r.stdout.is_empty()); }
    #[tokio::test]
    async fn test_date_tab_newline() { let r = DateCommand.execute(make_ctx(vec!["+%Y%n%m"])).await; assert!(r.stdout.contains("\n")); }
    #[tokio::test]
    async fn test_date_month() { let r = DateCommand.execute(make_ctx(vec!["+%m"])).await; let m = r.stdout.trim(); assert!(m.len() == 2 && m.parse::<u32>().unwrap() >= 1 && m.parse::<u32>().unwrap() <= 12); }
    #[tokio::test]
    async fn test_date_day() { let r = DateCommand.execute(make_ctx(vec!["+%d"])).await; let d = r.stdout.trim(); assert!(d.len() == 2 && d.parse::<u32>().unwrap() >= 1 && d.parse::<u32>().unwrap() <= 31); }
    #[tokio::test]
    async fn test_date_full_date() { let r = DateCommand.execute(make_ctx(vec!["+%F"])).await; assert!(r.stdout.trim().len() == 10); assert!(r.stdout.contains("-")); }
    #[tokio::test]
    async fn test_date_full_time() { let r = DateCommand.execute(make_ctx(vec!["+%T"])).await; assert!(r.stdout.trim().len() == 8); assert!(r.stdout.contains(":")); }
    #[tokio::test]
    async fn test_date_hour() { let r = DateCommand.execute(make_ctx(vec!["+%H"])).await; let h = r.stdout.trim(); assert!(h.len() == 2 && h.parse::<u32>().unwrap() <= 23); }
    #[tokio::test]
    async fn test_date_12hour() { let r = DateCommand.execute(make_ctx(vec!["+%I"])).await; let h = r.stdout.trim(); assert!(h.len() == 2 && h.parse::<u32>().unwrap() >= 1 && h.parse::<u32>().unwrap() <= 12); }
    #[tokio::test]
    async fn test_date_minute() { let r = DateCommand.execute(make_ctx(vec!["+%M"])).await; let m = r.stdout.trim(); assert!(m.len() == 2 && m.parse::<u32>().unwrap() <= 59); }
    #[tokio::test]
    async fn test_date_second() { let r = DateCommand.execute(make_ctx(vec!["+%S"])).await; let s = r.stdout.trim(); assert!(s.len() == 2 && s.parse::<u32>().unwrap() <= 59); }
    #[tokio::test]
    async fn test_date_weekday() { let r = DateCommand.execute(make_ctx(vec!["+%a"])).await; let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]; assert!(days.contains(&r.stdout.trim())); }
    #[tokio::test]
    async fn test_date_month_name() { let r = DateCommand.execute(make_ctx(vec!["+%b"])).await; let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]; assert!(months.contains(&r.stdout.trim())); }
    #[tokio::test]
    async fn test_date_timestamp() { let r = DateCommand.execute(make_ctx(vec!["+%s"])).await; let ts = r.stdout.trim().parse::<i64>().unwrap(); assert!(ts > 1700000000); }
    #[tokio::test]
    async fn test_date_ampm() { let r = DateCommand.execute(make_ctx(vec!["+%p"])).await; assert!(r.stdout.trim() == "AM" || r.stdout.trim() == "PM"); }
    #[tokio::test]
    async fn test_date_combined_format() { let r = DateCommand.execute(make_ctx(vec!["+%Y-%m-%d %H:%M:%S"])).await; assert!(r.stdout.contains("-") && r.stdout.contains(":")); }
    #[tokio::test]
    async fn test_date_with_date_option() { let r = DateCommand.execute(make_ctx(vec!["--date=2024-06-20T12:00:00", "+%F"])).await; assert_eq!(r.stdout, "2024-06-20\n"); }
    #[tokio::test]
    async fn test_date_iso_format() { let r = DateCommand.execute(make_ctx(vec!["-I"])).await; assert!(r.stdout.contains("T") && r.stdout.contains(":")); }
    #[tokio::test]
    async fn test_date_rfc_format() { let r = DateCommand.execute(make_ctx(vec!["-R"])).await; let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]; assert!(days.iter().any(|d| r.stdout.contains(d))); }
    #[tokio::test]
    async fn test_date_utc_timezone() { let r = DateCommand.execute(make_ctx(vec!["-u", "+%Z"])).await; assert_eq!(r.stdout, "UTC\n"); }
    #[tokio::test]
    async fn test_date_parse_now() { let r = DateCommand.execute(make_ctx(vec!["-d", "now", "+%s"])).await; let ts = r.stdout.trim().parse::<i64>().unwrap(); assert!(ts > 1700000000); }
    #[tokio::test]
    async fn test_date_parse_today() { let r = DateCommand.execute(make_ctx(vec!["-d", "today", "+%F"])).await; assert!(r.stdout.trim().len() == 10); }
    #[tokio::test]
    async fn test_date_invalid_option() { let r = DateCommand.execute(make_ctx(vec!["-z"])).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("invalid option")); }
}
