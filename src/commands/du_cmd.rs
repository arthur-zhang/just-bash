use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct DuCommand;

const HELP: &str = "du - estimate file space usage

Usage: du [OPTION]... [FILE]...

Options:
  -a          write counts for all files, not just directories
  -h          print sizes in human readable format
  -s          display only a total for each argument
  -c          produce a grand total
  --max-depth=N  print total for directory only if N or fewer levels deep
  --help      display this help and exit";

struct DuOptions {
    all_files: bool,
    human_readable: bool,
    summarize: bool,
    grand_total: bool,
    max_depth: Option<usize>,
}

fn format_size(bytes: u64, human_readable: bool) -> String {
    if !human_readable {
        return ((bytes + 1023) / 1024).max(1).to_string();
    }
    if bytes < 1024 {
        bytes.to_string()
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[async_trait]
impl Command for DuCommand {
    fn name(&self) -> &'static str {
        "du"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut options = DuOptions {
            all_files: false,
            human_readable: false,
            summarize: false,
            grand_total: false,
            max_depth: None,
        };
        let mut targets = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-a" => { options.all_files = true; i += 1; }
                "-h" => { options.human_readable = true; i += 1; }
                "-s" => { options.summarize = true; i += 1; }
                "-c" => { options.grand_total = true; i += 1; }
                s if s.starts_with("--max-depth=") => {
                    options.max_depth = s[12..].parse().ok();
                    i += 1;
                }
                "--" => {
                    targets.extend(ctx.args[i + 1..].iter().cloned());
                    break;
                }
                s if !s.starts_with('-') => {
                    targets.push(arg.clone());
                    i += 1;
                }
                _ => i += 1,
            }
        }

        if targets.is_empty() {
            targets.push(".".to_string());
        }

        let mut output = String::new();
        let mut stderr = String::new();
        let mut grand_total_size = 0u64;

        for target in &targets {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, target);
            match calculate_size(&ctx, &full_path, target, &options, 0).await {
                Ok((out, size)) => {
                    output.push_str(&out);
                    grand_total_size += size;
                }
                Err(e) => {
                    stderr.push_str(&e);
                }
            }
        }

        if options.grand_total && !targets.is_empty() {
            output.push_str(&format!(
                "{}\ttotal\n",
                format_size(grand_total_size, options.human_readable)
            ));
        }

        if stderr.is_empty() {
            CommandResult::success(output)
        } else {
            CommandResult::with_exit_code(output, stderr, 1)
        }
    }
}

async fn calculate_size(
    ctx: &CommandContext,
    full_path: &str,
    display_path: &str,
    options: &DuOptions,
    depth: usize,
) -> Result<(String, u64), String> {
    let stat = ctx.fs.stat(full_path).await
        .map_err(|_| format!("du: cannot access '{}': No such file or directory\n", display_path))?;

    if !stat.is_directory {
        let out = if options.all_files || depth == 0 {
            format!("{}\t{}\n", format_size(stat.size, options.human_readable), display_path)
        } else {
            String::new()
        };
        return Ok((out, stat.size));
    }

    let entries = ctx.fs.readdir(full_path).await.unwrap_or_default();
    let mut dir_size = 0u64;
    let mut output = String::new();

    let mut sorted_entries = entries;
    sorted_entries.sort();

    for entry in &sorted_entries {
        let entry_path = if full_path == "/" {
            format!("/{}", entry)
        } else {
            format!("{}/{}", full_path, entry)
        };
        let entry_display = if display_path == "." {
            entry.clone()
        } else {
            format!("{}/{}", display_path, entry)
        };

        if let Ok(entry_stat) = ctx.fs.stat(&entry_path).await {
            if entry_stat.is_directory {
                let (sub_out, sub_size) = Box::pin(calculate_size(
                    ctx, &entry_path, &entry_display, options, depth + 1
                )).await?;
                dir_size += sub_size;
                if !options.summarize {
                    if options.max_depth.is_none() || depth + 1 <= options.max_depth.unwrap() {
                        output.push_str(&sub_out);
                    }
                }
            } else {
                dir_size += entry_stat.size;
                if options.all_files && !options.summarize {
                    output.push_str(&format!(
                        "{}\t{}\n",
                        format_size(entry_stat.size, options.human_readable),
                        entry_display
                    ));
                }
            }
        }
    }

    if options.summarize || options.max_depth.is_none() || depth <= options.max_depth.unwrap() {
        output.push_str(&format!(
            "{}\t{}\n",
            format_size(dir_size, options.human_readable),
            display_path
        ));
    }

    Ok((output, dir_size))
}
