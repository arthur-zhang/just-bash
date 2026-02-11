// src/commands/stat_cmd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct StatCommand;

const HELP: &str = "Usage: stat [OPTION]... FILE...\n\n\
display file or file system status\n\n\
Options:\n  -c FORMAT   use the specified FORMAT instead of the default\n      --help  display this help and exit\n\n\
FORMAT sequences:\n  %n  file name  %N  quoted file name  %s  size\n  %F  file type  %a  access rights (octal)  %A  access rights (human)\n  %u  user ID  %U  user name  %g  group ID  %G  group name\n";

fn format_mode_string(mode: u32, is_directory: bool) -> String {
    let type_char = if is_directory { 'd' } else { '-' };
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
    let mut s = String::with_capacity(10);
    s.push(type_char);
    for p in &perms { s.push(*p); }
    s
}

fn resolve_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') { path.to_string() }
    else { format!("{}/{}", cwd.trim_end_matches('/'), path) }
}

#[async_trait]
impl Command for StatCommand {
    fn name(&self) -> &'static str { "stat" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }

        let mut format: Option<String> = None;
        let mut files: Vec<String> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if a == "-c" && i + 1 < args.len() {
                i += 1;
                format = Some(args[i].clone());
            } else if a.starts_with("-c") && a.len() > 2 {
                format = Some(a[2..].to_string());
            } else if !a.starts_with('-') || a == "-" {
                files.push(a.clone());
            }
            i += 1;
        }

        if files.is_empty() {
            return CommandResult::with_exit_code("".into(), "stat: missing operand\n".into(), 1);
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut has_error = false;

        for file in &files {
            let full_path = resolve_path(&ctx.cwd, file);
            match ctx.fs.stat(&full_path).await {
                Ok(stat) => {
                    if let Some(ref fmt) = format {
                        let mut output = fmt.clone();
                        let mode_octal = format!("{:o}", stat.mode);
                        let mode_str = format_mode_string(stat.mode, stat.is_directory);
                        output = output.replace("%n", file);
                        output = output.replace("%N", &format!("'{}'", file));
                        output = output.replace("%s", &stat.size.to_string());
                        output = output.replace("%F", if stat.is_directory { "directory" } else { "regular file" });
                        output = output.replace("%a", &mode_octal);
                        output = output.replace("%A", &mode_str);
                        output = output.replace("%u", "1000");
                        output = output.replace("%U", "user");
                        output = output.replace("%g", "1000");
                        output = output.replace("%G", "group");
                        stdout.push_str(&format!("{}\n", output));
                    } else {
                        let mode_octal = format!("{:04o}", stat.mode);
                        let mode_str = format_mode_string(stat.mode, stat.is_directory);
                        let blocks = (stat.size + 511) / 512;
                        stdout.push_str(&format!("  File: {}\n", file));
                        stdout.push_str(&format!("  Size: {}\t\tBlocks: {}\n", stat.size, blocks));
                        stdout.push_str(&format!("Access: ({}/{})\n", mode_octal, mode_str));
                        let mtime_secs = stat.mtime.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        stdout.push_str(&format!("Modify: {}\n", mtime_secs));
                    }
                }
                Err(_) => {
                    stderr.push_str(&format!("stat: cannot stat '{}': No such file or directory\n", file));
                    has_error = true;
                }
            }
        }

        CommandResult::with_exit_code(stdout, stderr, if has_error { 1 } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx_with_fs(args: Vec<&str>, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: String::new(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    fn make_ctx(args: Vec<&str>) -> CommandContext {
        make_ctx_with_fs(args, Arc::new(InMemoryFs::new()))
    }

    #[tokio::test]
    async fn test_stat_file() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "hello world".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["/test.txt"], fs)).await;
        assert!(r.stdout.contains("File: /test.txt"));
        assert!(r.stdout.contains("Size: 11"));
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_stat_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/mydir/file.txt", "content".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["/mydir"], fs)).await;
        assert!(r.stdout.contains("File: /mydir"));
        assert!(r.stdout.contains("drwx"));
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_stat_missing() {
        let r = StatCommand.execute(make_ctx(vec!["/nonexistent"])).await;
        assert!(r.stderr.contains("No such file or directory"));
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn test_stat_no_operand() {
        let r = StatCommand.execute(make_ctx(vec![])).await;
        assert!(r.stderr.contains("missing operand"));
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn test_stat_format_name() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "hello".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["-c", "%n", "/test.txt"], fs)).await;
        assert_eq!(r.stdout.trim(), "/test.txt");
    }

    #[tokio::test]
    async fn test_stat_format_size() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "hello".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["-c", "%s", "/test.txt"], fs)).await;
        assert_eq!(r.stdout.trim(), "5");
    }

    #[tokio::test]
    async fn test_stat_format_type() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/mydir/file.txt", "content".as_bytes()).await.unwrap();
        let r1 = StatCommand.execute(make_ctx_with_fs(vec!["-c", "%F", "/mydir/file.txt"], fs.clone())).await;
        assert_eq!(r1.stdout.trim(), "regular file");
        let r2 = StatCommand.execute(make_ctx_with_fs(vec!["-c", "%F", "/mydir"], fs)).await;
        assert_eq!(r2.stdout.trim(), "directory");
    }

    #[tokio::test]
    async fn test_stat_format_combined() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "hello world".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["-c", "%n: %s bytes", "/test.txt"], fs)).await;
        assert_eq!(r.stdout.trim(), "/test.txt: 11 bytes");
    }

    #[tokio::test]
    async fn test_stat_help() {
        let r = StatCommand.execute(make_ctx(vec!["--help"])).await;
        assert!(r.stdout.contains("stat"));
        assert!(r.stdout.contains("-c"));
    }

    #[tokio::test]
    async fn test_stat_multiple_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", "aaa".as_bytes()).await.unwrap();
        fs.write_file("/b.txt", "bbbbb".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["/a.txt", "/b.txt"], fs)).await;
        assert!(r.stdout.contains("File: /a.txt"));
        assert!(r.stdout.contains("File: /b.txt"));
    }

    #[tokio::test]
    async fn test_stat_continue_on_error() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/exists.txt", "yes".as_bytes()).await.unwrap();
        let r = StatCommand.execute(make_ctx_with_fs(vec!["/exists.txt", "/missing.txt"], fs)).await;
        assert!(r.stdout.contains("File: /exists.txt"));
        assert!(r.stderr.contains("missing.txt"));
        assert_eq!(r.exit_code, 1);
    }
}
