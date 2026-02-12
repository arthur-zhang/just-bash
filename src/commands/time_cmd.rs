use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use std::time::Instant;

pub struct TimeCommand;

#[async_trait]
impl Command for TimeCommand {
    fn name(&self) -> &'static str {
        "time"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut format = "%e %M".to_string();
        let mut output_file: Option<String> = None;
        let mut append_mode = false;
        let mut posix_format = false;
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "-f" | "--format" => {
                    i += 1;
                    if i >= ctx.args.len() {
                        return CommandResult::error("time: missing argument to '-f'\n".to_string());
                    }
                    format = ctx.args[i].clone();
                    i += 1;
                }
                "-o" | "--output" => {
                    i += 1;
                    if i >= ctx.args.len() {
                        return CommandResult::error("time: missing argument to '-o'\n".to_string());
                    }
                    output_file = Some(ctx.args[i].clone());
                    i += 1;
                }
                "-a" | "--append" => {
                    append_mode = true;
                    i += 1;
                }
                "-v" | "--verbose" => {
                    format = "Command being timed: %C\nElapsed (wall clock) time: %e seconds\nMaximum resident set size (kbytes): %M".to_string();
                    i += 1;
                }
                "-p" | "--portability" => {
                    posix_format = true;
                    i += 1;
                }
                "--" => {
                    i += 1;
                    break;
                }
                s if s.starts_with('-') => {
                    i += 1;
                }
                _ => break,
            }
        }

        let command_args: Vec<String> = ctx.args[i..].to_vec();

        if command_args.is_empty() {
            return CommandResult::success(String::new());
        }

        let start_time = Instant::now();

        let command_string = command_args.join(" ");
        let result = if let Some(ref exec_fn) = ctx.exec_fn {
            exec_fn(
                command_string.clone(),
                ctx.stdin.clone(),
                ctx.cwd.clone(),
                ctx.env.clone(),
                ctx.fs.clone(),
            ).await
        } else {
            return CommandResult::error("time: exec not available\n".to_string());
        };

        let elapsed_seconds = start_time.elapsed().as_secs_f64();

        let timing_output = if posix_format {
            format!("real {:.2}\nuser 0.00\nsys 0.00\n", elapsed_seconds)
        } else {
            let mut output = format
                .replace("%e", &format!("{:.2}", elapsed_seconds))
                .replace("%E", &format_elapsed_time(elapsed_seconds))
                .replace("%M", "0")
                .replace("%S", "0.00")
                .replace("%U", "0.00")
                .replace("%P", "0%")
                .replace("%C", &command_string);
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output
        };

        if let Some(file) = output_file {
            let file_path = ctx.fs.resolve_path(&ctx.cwd, &file);
            let content = if append_mode && ctx.fs.exists(&file_path).await {
                match ctx.fs.read_file(&file_path).await {
                    Ok(existing) => existing + &timing_output,
                    Err(_) => timing_output.clone(),
                }
            } else {
                timing_output
            };
            if let Err(e) = ctx.fs.write_file(&file_path, content.as_bytes()).await {
                return CommandResult::with_exit_code(
                    result.stdout,
                    format!("{}time: cannot write to '{}': {}\n", result.stderr, file, e),
                    result.exit_code,
                );
            }
            result
        } else {
            CommandResult::with_exit_code(
                result.stdout,
                format!("{}{}", result.stderr, timing_output),
                result.exit_code,
            )
        }
    }
}

fn format_elapsed_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u64;
    let minutes = ((seconds % 3600.0) / 60.0) as u64;
    let secs = seconds % 60.0;

    if hours > 0 {
        format!("{}:{:02}:{:05.2}", hours, minutes, secs)
    } else {
        format!("{}:{:05.2}", minutes, secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_time() {
        assert_eq!(format_elapsed_time(65.5), "1:05.50");
        assert_eq!(format_elapsed_time(3665.5), "1:01:05.50");
    }
}
