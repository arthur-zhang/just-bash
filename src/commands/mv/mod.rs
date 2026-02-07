// src/commands/mv/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct MvCommand;

#[async_trait]
impl Command for MvCommand {
    fn name(&self) -> &'static str {
        "mv"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: mv [OPTION]... SOURCE... DEST\n\n\
                 Rename SOURCE to DEST, or move SOURCE(s) to DIRECTORY.\n\n\
                 Options:\n\
                   -f, --force        do not prompt before overwriting\n\
                   -n, --no-clobber   do not overwrite an existing file\n\
                   -v, --verbose      explain what is being done\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let mut no_clobber = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-f" | "--force" => {} // 接受但忽略（默认行为）
                "-n" | "--no-clobber" => no_clobber = true,
                "-v" | "--verbose" => verbose = true,
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.len() < 2 {
            return CommandResult::error("mv: missing destination file operand\n".to_string());
        }

        let dest = paths.pop().unwrap();
        let sources = paths;
        let dest_path = ctx.fs.resolve_path(&ctx.cwd, &dest);

        // 检查目标是否为目录
        let dest_is_dir = match ctx.fs.stat(&dest_path).await {
            Ok(stat) => stat.is_directory,
            Err(_) => false,
        };

        // 如果有多个源，目标必须是目录
        if sources.len() > 1 && !dest_is_dir {
            return CommandResult::error(format!(
                "mv: target '{}' is not a directory\n",
                dest
            ));
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for src in &sources {
            let src_path = ctx.fs.resolve_path(&ctx.cwd, src);

            // 检查源是否存在
            if !ctx.fs.exists(&src_path).await {
                stderr.push_str(&format!("mv: cannot stat '{}': No such file or directory\n", src));
                exit_code = 1;
                continue;
            }

            // 确定目标路径
            let target_path = if dest_is_dir {
                let basename = src.rsplit('/').next().unwrap_or(src);
                ctx.fs.resolve_path(&dest_path, basename)
            } else {
                dest_path.clone()
            };

            // 检查 no_clobber
            if no_clobber && ctx.fs.exists(&target_path).await {
                continue;
            }

            match ctx.fs.mv(&src_path, &target_path).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("renamed '{}' -> '{}'\n", src, target_path));
                    }
                }
                Err(e) => {
                    stderr.push_str(&format!("mv: cannot move '{}': {:?}\n", src, e));
                    exit_code = 1;
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
    async fn test_mv_rename() {
        let ctx = make_ctx_with_files(
            vec!["/old.txt", "/new.txt"],
            vec![("/old.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/old.txt").await);
        assert!(fs.exists("/new.txt").await);
    }

    #[tokio::test]
    async fn test_mv_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/src.txt", b"content").await.unwrap();
        fs.mkdir("/destdir", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src.txt".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/src.txt").await);
        assert!(fs.exists("/destdir/src.txt").await);
    }

    #[tokio::test]
    async fn test_mv_no_clobber() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "new"), ("/dest.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/src.txt").await); // 源文件仍存在
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "old"); // 目标未被覆盖
    }

    #[tokio::test]
    async fn test_mv_nonexistent() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt", "/dest.txt"], vec![]).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }
}
