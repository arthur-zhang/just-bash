// src/commands/ls/mod.rs
use async_trait::async_trait;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::commands::{Command, CommandContext, CommandResult};

pub struct LsCommand;

fn format_mode(mode: u32, is_dir: bool, is_link: bool) -> String {
    let file_type = if is_link { 'l' } else if is_dir { 'd' } else { '-' };

    let perms = [
        if mode & 0o400 != 0 { 'r' } else { '-' },
        if mode & 0o200 != 0 { 'w' } else { '-' },
        if mode & 0o100 != 0 { 'x' } else { '-' },
        if mode & 0o040 != 0 { 'r' } else { '-' },
        if mode & 0o020 != 0 { 'w' } else { '-' },
        if mode & 0o010 != 0 { 'x' } else { '-' },
        if mode & 0o004 != 0 { 'r' } else { '-' },
        if mode & 0o002 != 0 { 'w' } else { '-' },
        if mode & 0o001 != 0 { 'x' } else { '-' },
    ];

    format!("{}{}", file_type, perms.iter().collect::<String>())
}

fn format_size(size: u64, human_readable: bool) -> String {
    if !human_readable {
        return size.to_string();
    }

    if size < 1024 {
        return size.to_string();
    }
    if size < 1024 * 1024 {
        let k = size as f64 / 1024.0;
        return if k < 10.0 { format!("{:.1}K", k) } else { format!("{}K", k as u64) };
    }
    if size < 1024 * 1024 * 1024 {
        let m = size as f64 / (1024.0 * 1024.0);
        return if m < 10.0 { format!("{:.1}M", m) } else { format!("{}M", m as u64) };
    }
    let g = size as f64 / (1024.0 * 1024.0 * 1024.0);
    if g < 10.0 { format!("{:.1}G", g) } else { format!("{}G", g as u64) }
}

fn format_time(mtime: SystemTime) -> String {
    let duration = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    // 简化的日期计算
    let days_since_epoch = secs / 86400;
    let year = 1970 + (days_since_epoch / 365) as i32;
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30).min(11) as usize;
    let day = (day_of_year % 30) + 1;

    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let six_months_ago = now_secs.saturating_sub(180 * 86400);

    if secs > six_months_ago {
        format!("{} {:>2} {:02}:{:02}", months[month], day, hour, minute)
    } else {
        format!("{} {:>2}  {}", months[month], day, year)
    }
}

#[async_trait]
impl Command for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: ls [OPTION]... [FILE]...\n\n\
                 List directory contents.\n\n\
                 Options:\n\
                   -a, --all          do not ignore entries starting with .\n\
                   -A, --almost-all   do not list implied . and ..\n\
                   -l                 use a long listing format\n\
                   -h, --human-readable  with -l, print sizes in human readable format\n\
                   -r, --reverse      reverse order while sorting\n\
                   -S                 sort by file size, largest first\n\
                   -t                 sort by time, newest first\n\
                   -d, --directory    list directories themselves, not their contents\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let mut show_all = false;
        let mut show_almost_all = false;
        let mut long_format = false;
        let mut human_readable = false;
        let mut reverse = false;
        let mut sort_by_size = false;
        let mut sort_by_time = false;
        let mut list_dir_itself = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-a" | "--all" => show_all = true,
                "-A" | "--almost-all" => show_almost_all = true,
                "-l" => long_format = true,
                "-h" | "--human-readable" => human_readable = true,
                "-r" | "--reverse" => reverse = true,
                "-S" => sort_by_size = true,
                "-t" => sort_by_time = true,
                "-d" | "--directory" => list_dir_itself = true,
                "-la" | "-al" => { long_format = true; show_all = true; }
                "-lh" | "-hl" => { long_format = true; human_readable = true; }
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let show_path_header = paths.len() > 1;

        for (idx, path) in paths.iter().enumerate() {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, path);

            let stat = match ctx.fs.stat(&full_path).await {
                Ok(s) => s,
                Err(_) => {
                    stderr.push_str(&format!("ls: cannot access '{}': No such file or directory\n", path));
                    exit_code = 2;
                    continue;
                }
            };

            if !stat.is_directory || list_dir_itself {
                if long_format {
                    let mode_str = format_mode(stat.mode, stat.is_directory, stat.is_symlink);
                    let size_str = format_size(stat.size, human_readable);
                    let time_str = format_time(stat.mtime);
                    stdout.push_str(&format!("{} 1 user user {:>5} {} {}\n",
                        mode_str, size_str, time_str, path));
                } else {
                    stdout.push_str(&format!("{}\n", path));
                }
                continue;
            }

            if show_path_header {
                if idx > 0 { stdout.push('\n'); }
                stdout.push_str(&format!("{}:\n", path));
            }

            let entries = match ctx.fs.readdir_with_file_types(&full_path).await {
                Ok(e) => e,
                Err(_) => {
                    stderr.push_str(&format!("ls: cannot open directory '{}'\n", path));
                    exit_code = 2;
                    continue;
                }
            };

            let mut filtered: Vec<_> = entries
                .into_iter()
                .filter(|e| {
                    if show_all { return true; }
                    if show_almost_all { return !e.name.starts_with('.') || (e.name != "." && e.name != ".."); }
                    !e.name.starts_with('.')
                })
                .collect();

            // 按名称排序
            filtered.sort_by(|a, b| a.name.cmp(&b.name));
            if reverse {
                filtered.reverse();
            }

            for entry in filtered {
                if long_format {
                    let entry_path = ctx.fs.resolve_path(&full_path, &entry.name);
                    if let Ok(stat) = ctx.fs.stat(&entry_path).await {
                        let mode_str = format_mode(stat.mode, entry.is_directory, entry.is_symlink);
                        let size_str = format_size(stat.size, human_readable);
                        let time_str = format_time(stat.mtime);
                        stdout.push_str(&format!("{} 1 user user {:>5} {} {}\n",
                            mode_str, size_str, time_str, entry.name));
                    }
                } else {
                    stdout.push_str(&format!("{}\n", entry.name));
                }
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs, MkdirOptions};
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx_with_structure(args: Vec<&str>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/testdir/file1.txt", b"content1").await.unwrap();
        fs.write_file("/testdir/file2.txt", b"content2content2").await.unwrap();
        fs.write_file("/testdir/.hidden", b"hidden").await.unwrap();
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    #[tokio::test]
    async fn test_ls_basic() {
        let ctx = make_ctx_with_structure(vec!["/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("file1.txt"));
        assert!(result.stdout.contains("file2.txt"));
        assert!(!result.stdout.contains(".hidden"));
    }

    #[tokio::test]
    async fn test_ls_all() {
        let ctx = make_ctx_with_structure(vec!["-a", "/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains(".hidden"));
    }

    #[tokio::test]
    async fn test_ls_long() {
        let ctx = make_ctx_with_structure(vec!["-l", "/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("rw"));
    }

    #[tokio::test]
    async fn test_ls_nonexistent() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["/nonexistent".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 2);
    }
}
