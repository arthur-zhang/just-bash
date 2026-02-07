// src/commands/cp/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::CpOptions;

pub struct CpCommand;

#[async_trait]
impl Command for CpCommand {
    fn name(&self) -> &'static str {
        "cp"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: cp [OPTION]... SOURCE... DEST\n\n\
                 Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.\n\n\
                 Options:\n\
                   -r, -R, --recursive  copy directories recursively\n\
                   -n, --no-clobber     do not overwrite an existing file\n\
                   -v, --verbose        explain what is being done\n\
                       --help           display this help and exit\n".to_string()
            );
        }

        let mut recursive = false;
        let mut no_clobber = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-n" | "--no-clobber" => no_clobber = true,
                "-v" | "--verbose" => verbose = true,
                "-p" | "--preserve" => {} // 接受但忽略
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.len() < 2 {
            return CommandResult::error("cp: missing destination file operand\n".to_string());
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
                "cp: target '{}' is not a directory\n",
                dest
            ));
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for src in &sources {
            let src_path = ctx.fs.resolve_path(&ctx.cwd, src);

            // 检查源是否存在
            let src_stat = match ctx.fs.stat(&src_path).await {
                Ok(s) => s,
                Err(_) => {
                    stderr.push_str(&format!("cp: cannot stat '{}': No such file or directory\n", src));
                    exit_code = 1;
                    continue;
                }
            };

            // 如果是目录但没有 -r
            if src_stat.is_directory && !recursive {
                stderr.push_str(&format!("cp: -r not specified; omitting directory '{}'\n", src));
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

            let opts = CpOptions { recursive };
            match ctx.fs.cp(&src_path, &target_path, &opts).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("'{}' -> '{}'\n", src, target_path));
                    }
                }
                Err(e) => {
                    stderr.push_str(&format!("cp: cannot copy '{}': {:?}\n", src, e));
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
    use crate::fs::{InMemoryFs, FileSystem};
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
    async fn test_cp_file() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt", "/dest.txt"],
            vec![("/src.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_cp_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/src.txt", b"content").await.unwrap();
        fs.mkdir("/destdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src.txt".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/destdir/src.txt").await);
    }

    #[tokio::test]
    async fn test_cp_directory_without_r() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/srcdir".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("omitting directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_cp_no_clobber() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "new"), ("/dest.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "old"); // 未被覆盖
    }
}
