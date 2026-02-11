// src/commands/sleep_cmd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use regex_lite::Regex;

pub struct SleepCommand;

const HELP: &str = "Usage: sleep NUMBER[SUFFIX]\n\ndelay for a specified amount of time\n\nSUFFIX may be:\n  s - seconds (default)\n  m - minutes\n  h - hours\n  d - days\n\nNUMBER may be a decimal number.\n";

fn parse_duration(arg: &str) -> Option<f64> {
    let re = Regex::new(r"^(\d+\.?\d*)(s|m|h|d)?$").unwrap();
    let caps = re.captures(arg)?;
    let value: f64 = caps.get(1)?.as_str().parse().ok()?;
    let suffix = caps.get(2).map(|m| m.as_str()).unwrap_or("s");
    match suffix {
        "s" => Some(value * 1000.0),
        "m" => Some(value * 60.0 * 1000.0),
        "h" => Some(value * 3600.0 * 1000.0),
        "d" => Some(value * 86400.0 * 1000.0),
        _ => None,
    }
}

#[async_trait]
impl Command for SleepCommand {
    fn name(&self) -> &'static str { "sleep" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }
        if args.is_empty() {
            return CommandResult::with_exit_code("".into(), "sleep: missing operand\n".into(), 1);
        }
        let mut total_ms: f64 = 0.0;
        for arg in args {
            match parse_duration(arg) {
                Some(ms) => total_ms += ms,
                None => return CommandResult::with_exit_code("".into(), format!("sleep: invalid time interval '{}'\n", arg), 1),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(total_ms as u64));
        CommandResult::success("".into())
    }
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

    #[test]
    fn test_parse_seconds() { assert_eq!(parse_duration("2"), Some(2000.0)); }
    #[test]
    fn test_parse_decimal() { assert_eq!(parse_duration("0.5"), Some(500.0)); }
    #[test]
    fn test_parse_s() { assert_eq!(parse_duration("3s"), Some(3000.0)); }
    #[test]
    fn test_parse_m() { assert_eq!(parse_duration("2m"), Some(120000.0)); }
    #[test]
    fn test_parse_h() { assert_eq!(parse_duration("1h"), Some(3600000.0)); }
    #[test]
    fn test_parse_d() { assert_eq!(parse_duration("1d"), Some(86400000.0)); }
    #[test]
    fn test_parse_dec_m() { assert_eq!(parse_duration("0.5m"), Some(30000.0)); }
    #[test]
    fn test_parse_short() { assert_eq!(parse_duration("0.01"), Some(10.0)); }
    #[test]
    fn test_parse_invalid() { assert_eq!(parse_duration("abc"), None); }
    #[test]
    fn test_parse_bad_suffix() { assert_eq!(parse_duration("1x"), None); }

    #[tokio::test]
    async fn test_missing() { let r = SleepCommand.execute(make_ctx(vec![])).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("missing operand")); }
    #[tokio::test]
    async fn test_invalid() { let r = SleepCommand.execute(make_ctx(vec!["abc"])).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("invalid time interval")); }
    #[tokio::test]
    async fn test_bad_suffix() { let r = SleepCommand.execute(make_ctx(vec!["1x"])).await; assert_eq!(r.exit_code, 1); }
    #[tokio::test]
    async fn test_help() { let r = SleepCommand.execute(make_ctx(vec!["--help"])).await; assert!(r.stdout.contains("sleep")); assert!(r.stdout.contains("delay")); }
    #[tokio::test]
    async fn test_short_sleep() { let r = SleepCommand.execute(make_ctx(vec!["0.001"])).await; assert_eq!(r.exit_code, 0); }
}
