// src/commands/seq/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct SeqCommand;

#[async_trait]
impl Command for SeqCommand {
    fn name(&self) -> &'static str { "seq" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        let mut separator = "\n".to_string();
        let mut equalize_width = false;
        let mut nums: Vec<String> = Vec::new();
        let mut i = 0;

        while i < args.len() {
            let arg = &args[i];
            if arg == "-s" && i + 1 < args.len() {
                separator = args[i + 1].clone();
                i += 2;
                continue;
            }
            if arg == "-w" {
                equalize_width = true;
                i += 1;
                continue;
            }
            if arg == "--" {
                i += 1;
                break;
            }
            if arg.starts_with('-') && arg != "-" {
                // Check for -sSTRING
                if arg.starts_with("-s") && arg.len() > 2 {
                    separator = arg[2..].to_string();
                    i += 1;
                    continue;
                }
                if arg == "-ws" || arg == "-sw" {
                    equalize_width = true;
                    if i + 1 < args.len() {
                        separator = args[i + 1].clone();
                        i += 2;
                        continue;
                    }
                }
                // Could be negative number
            }
            nums.push(arg.clone());
            i += 1;
        }
        // Collect remaining args
        while i < args.len() {
            nums.push(args[i].clone());
            i += 1;
        }

        if nums.is_empty() {
            return CommandResult::with_exit_code("".into(), "seq: missing operand\n".into(), 1);
        }

        let (first, increment, last) = if nums.len() == 1 {
            (1.0, 1.0, parse_num(&nums[0]))
        } else if nums.len() == 2 {
            (parse_num(&nums[0]), 1.0, parse_num(&nums[1]))
        } else {
            (parse_num(&nums[0]), parse_num(&nums[1]), parse_num(&nums[2]))
        };

        // Validate numbers
        if first.is_nan() || increment.is_nan() || last.is_nan() {
            let invalid = nums.iter().find(|n| parse_num(n).is_nan()).unwrap();
            return CommandResult::with_exit_code("".into(), format!("seq: invalid floating point argument: '{}'\n", invalid), 1);
        }

        if increment == 0.0 {
            return CommandResult::with_exit_code("".into(), "seq: invalid Zero increment value: '0'\n".into(), 1);
        }

        // Determine precision
        let precision = [first, increment, last].iter().map(|n| get_precision(*n)).max().unwrap_or(0);

        let mut results: Vec<String> = Vec::new();
        let max_iterations = 100000;
        let mut iterations = 0;

        if increment > 0.0 {
            let mut n = first;
            while n <= last + 1e-10 {
                if iterations > max_iterations { break; }
                iterations += 1;
                if precision > 0 { results.push(format!("{:.prec$}", n, prec = precision)); }
                else { results.push(format!("{}", n.round() as i64)); }
                n += increment;
            }
        } else {
            let mut n = first;
            while n >= last - 1e-10 {
                if iterations > max_iterations { break; }
                iterations += 1;
                if precision > 0 { results.push(format!("{:.prec$}", n, prec = precision)); }
                else { results.push(format!("{}", n.round() as i64)); }
                n += increment;
            }
        }

        // Equalize width
        if equalize_width && !results.is_empty() {
            let max_len = results.iter().map(|r| r.replace('-', "").len()).max().unwrap_or(0);
            for r in results.iter_mut() {
                let is_negative = r.starts_with('-');
                let num = if is_negative { &r[1..] } else { &r[..] };
                let padded = format!("{:0>width$}", num, width = max_len);
                *r = if is_negative { format!("-{}", padded) } else { padded };
            }
        }

        let output = results.join(&separator);
        if output.is_empty() {
            CommandResult::success("".into())
        } else {
            CommandResult::success(format!("{}\n", output))
        }
    }
}

fn parse_num(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::NAN)
}

fn get_precision(n: f64) -> usize {
    let s = format!("{}", n);
    match s.find('.') {
        Some(i) => s.len() - i - 1,
        None => 0,
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

    #[tokio::test]
    async fn test_seq_1_to_5() { let r = SeqCommand.execute(make_ctx(vec!["5"])).await; assert_eq!(r.stdout, "1\n2\n3\n4\n5\n"); }
    #[tokio::test]
    async fn test_seq_3_to_7() { let r = SeqCommand.execute(make_ctx(vec!["3", "7"])).await; assert_eq!(r.stdout, "3\n4\n5\n6\n7\n"); }
    #[tokio::test]
    async fn test_seq_increment() { let r = SeqCommand.execute(make_ctx(vec!["1", "2", "10"])).await; assert_eq!(r.stdout, "1\n3\n5\n7\n9\n"); }
    #[tokio::test]
    async fn test_seq_single() { let r = SeqCommand.execute(make_ctx(vec!["1"])).await; assert_eq!(r.stdout, "1\n"); }
    #[tokio::test]
    async fn test_seq_start_gt_end() { let r = SeqCommand.execute(make_ctx(vec!["5", "1"])).await; assert_eq!(r.stdout, ""); assert_eq!(r.exit_code, 0); }
    #[tokio::test]
    async fn test_seq_decrement() { let r = SeqCommand.execute(make_ctx(vec!["5", "-1", "1"])).await; assert_eq!(r.stdout, "5\n4\n3\n2\n1\n"); }
    #[tokio::test]
    async fn test_seq_negative_start() { let r = SeqCommand.execute(make_ctx(vec!["-3", "3"])).await; assert_eq!(r.stdout, "-3\n-2\n-1\n0\n1\n2\n3\n"); }
    #[tokio::test]
    async fn test_seq_negative_end() { let r = SeqCommand.execute(make_ctx(vec!["2", "-1", "-2"])).await; assert_eq!(r.stdout, "2\n1\n0\n-1\n-2\n"); }
    #[tokio::test]
    async fn test_seq_all_negative() { let r = SeqCommand.execute(make_ctx(vec!["-5", "-1", "-10"])).await; assert_eq!(r.stdout, "-5\n-6\n-7\n-8\n-9\n-10\n"); }
    #[tokio::test]
    async fn test_seq_float_incr() { let r = SeqCommand.execute(make_ctx(vec!["1", "0.5", "3"])).await; assert_eq!(r.stdout, "1.0\n1.5\n2.0\n2.5\n3.0\n"); }
    #[tokio::test]
    async fn test_seq_float_range() { let r = SeqCommand.execute(make_ctx(vec!["1.5", "3.5"])).await; assert_eq!(r.stdout, "1.5\n2.5\n3.5\n"); }
    #[tokio::test]
    async fn test_seq_separator_space() { let r = SeqCommand.execute(make_ctx(vec!["-s", " ", "5"])).await; assert_eq!(r.stdout, "1 2 3 4 5\n"); }
    #[tokio::test]
    async fn test_seq_separator_comma() { let r = SeqCommand.execute(make_ctx(vec!["-s", ",", "3"])).await; assert_eq!(r.stdout, "1,2,3\n"); }
    #[tokio::test]
    async fn test_seq_separator_empty() { let r = SeqCommand.execute(make_ctx(vec!["-s", "", "3"])).await; assert_eq!(r.stdout, "123\n"); }
    #[tokio::test]
    async fn test_seq_width() { let r = SeqCommand.execute(make_ctx(vec!["-w", "8", "12"])).await; assert_eq!(r.stdout, "08\n09\n10\n11\n12\n"); }
    #[tokio::test]
    async fn test_seq_width_large() {
        let r = SeqCommand.execute(make_ctx(vec!["-w", "1", "100"])).await;
        let lines: Vec<&str> = r.stdout.trim().split('\n').collect();
        assert_eq!(lines[0], "001"); assert_eq!(lines[9], "010"); assert_eq!(lines[99], "100");
    }
    #[tokio::test]
    async fn test_seq_missing() { let r = SeqCommand.execute(make_ctx(vec![])).await; assert!(r.stderr.contains("missing operand")); assert_eq!(r.exit_code, 1); }
    #[tokio::test]
    async fn test_seq_invalid() { let r = SeqCommand.execute(make_ctx(vec!["abc"])).await; assert!(r.stderr.contains("invalid")); assert_eq!(r.exit_code, 1); }
    #[tokio::test]
    async fn test_seq_zero_incr() { let r = SeqCommand.execute(make_ctx(vec!["1", "0", "5"])).await; assert!(r.stderr.contains("Zero increment")); assert_eq!(r.exit_code, 1); }
}
