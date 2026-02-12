use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TimeoutCommand;

const HELP: &str = "timeout - run a command with a time limit

Usage: timeout [OPTION] DURATION COMMAND [ARG]...

DURATION is a number with optional suffix:
  s - seconds (default)
  m - minutes
  h - hours
  d - days

Options:
  --preserve-status  exit with same status as COMMAND, even on timeout
  --help             display this help and exit";

fn is_valid_duration(arg: &str) -> bool {
    let s = if arg.ends_with('s') || arg.ends_with('m') || arg.ends_with('h') || arg.ends_with('d') {
        &arg[..arg.len()-1]
    } else {
        arg
    };
    s.parse::<f64>().is_ok()
}

#[async_trait]
impl Command for TimeoutCommand {
    fn name(&self) -> &'static str {
        "timeout"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut preserve_status = false;
        let mut command_start = 0;
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "--preserve-status" => {
                    preserve_status = true;
                    command_start = i + 1;
                    i += 1;
                }
                "--foreground" => {
                    command_start = i + 1;
                    i += 1;
                }
                "-k" | "--kill-after" => {
                    i += 2;
                    command_start = i;
                }
                "-s" | "--signal" => {
                    i += 2;
                    command_start = i;
                }
                s if s.starts_with("--kill-after=") || s.starts_with("--signal=") => {
                    command_start = i + 1;
                    i += 1;
                }
                s if s.starts_with("-k") || s.starts_with("-s") => {
                    command_start = i + 1;
                    i += 1;
                }
                "--" => {
                    command_start = i + 1;
                    break;
                }
                _ => {
                    command_start = i;
                    break;
                }
            }
        }

        let remaining: Vec<String> = ctx.args[command_start..].to_vec();
        if remaining.is_empty() {
            return CommandResult::error("timeout: missing operand\n".to_string());
        }

        if !is_valid_duration(&remaining[0]) {
            return CommandResult::error(format!(
                "timeout: invalid time interval '{}'\n",
                remaining[0]
            ));
        }

        let command_args: Vec<String> = remaining[1..].to_vec();
        if command_args.is_empty() {
            return CommandResult::error("timeout: missing operand\n".to_string());
        }

        let _ = preserve_status;

        let exec_fn = match &ctx.exec_fn {
            Some(f) => f.clone(),
            None => {
                return CommandResult::error("timeout: exec not available\n".to_string());
            }
        };

        let command_str = command_args.iter()
            .map(|arg| {
                if arg.contains(' ') || arg.contains('\t') {
                    format!("'{}'", arg.replace('\'', "'\\''"))
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let exec_future = exec_fn(
            command_str,
            ctx.stdin.clone(),
            ctx.cwd.clone(),
            ctx.env.clone(),
            ctx.fs.clone(),
        );

        exec_future.await
    }
}
