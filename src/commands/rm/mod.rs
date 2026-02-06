// src/commands/rm/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::RmOptions;

pub struct RmCommand;

#[async_trait]
impl Command for RmCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: rm [OPTION]... [FILE]...\n\n\
                 Remove (unlink) the FILE(s).\n\n\
                 Options:\n\
                   -f, --force      ignore nonexistent files and arguments\n\
                   -r, -R, --recursive  remove directories and their contents recursively\n\
                   -v, --verbose    explain what is being done\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut recursive = false;
        let mut force = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-f" | "--force" => force = true,
                "-v" | "--verbose" => verbose = true,
                "-rf" | "-fr" | "-Rf" | "-fR" => {
                    recursive = true;
                    force = true;
                }
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.is_empty() {
            if force {
                return CommandResult::success(String::new());
            }
            return CommandResult::error("rm: missing operand\n".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for path in &paths {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, path);

            // 检查是否为目录
            match ctx.fs.stat(&full_path).await {
                Ok(stat) => {
                    if stat.is_directory && !recursive {
                        stderr.push_str(&format!("rm: cannot remove '{}': Is a directory\n", path));
                        exit_code = 1;
                        continue;
                    }
                }
                Err(_) => {
                    if !force {
                        stderr.push_str(&format!("rm: cannot remove '{}': No such file or directory\n", path));
                        exit_code = 1;
                    }
                    continue;
                }
            }

            let opts = RmOptions { recursive, force };
            match ctx.fs.rm(&full_path, &opts).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("removed '{}'\n", path));
                    }
                }
                Err(e) => {
                    if !force {
                        let msg = format!("{:?}", e);
                        if msg.contains("NotEmpty") {
                            stderr.push_str(&format!("rm: cannot remove '{}': Directory not empty\n", path));
                        } else {
                            stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, msg));
                        }
                        exit_code = 1;
                    }
                }
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx_with_files(args: Vec<&str>, files: Vec<(&str, &str)>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content.as_bytes()).await.unwrap();
        }
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    #[tokio::test]
    async fn test_rm_file() {
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", "content")]).await;
        let fs = ctx.fs.clone();
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/test.txt").await);
    }

    #[tokio::test]
    async fn test_rm_nonexistent() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt"], vec![]).await;
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_rm_force_nonexistent() {
        let ctx = make_ctx_with_files(vec!["-f", "/nonexistent.txt"], vec![]).await;
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_rm_directory_without_r() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/testdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("Is a directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_rm_recursive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/testdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-r".to_string(), "/testdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/testdir").await);
    }
}
