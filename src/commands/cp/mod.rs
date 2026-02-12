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
            exec_fn: None,
            fetch_fn: None,
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
            exec_fn: None,
            fetch_fn: None,
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
            exec_fn: None,
            fetch_fn: None,
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

    #[tokio::test]
    async fn test_cp_preserve_original() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt", "/dest.txt"],
            vec![("/src.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let src_content = fs.read_file("/src.txt").await.unwrap();
        assert_eq!(src_content, "content");
    }

    #[tokio::test]
    async fn test_cp_overwrite_existing() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt", "/dest.txt"],
            vec![("/src.txt", "new content"), ("/dest.txt", "old content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_cp_multiple_files_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"aaa").await.unwrap();
        fs.write_file("/b.txt", b"bbb").await.unwrap();
        fs.mkdir("/dir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/a.txt".to_string(), "/b.txt".to_string(), "/dir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs.read_file("/dir/a.txt").await.unwrap(), "aaa");
        assert_eq!(fs.read_file("/dir/b.txt").await.unwrap(), "bbb");
    }

    #[tokio::test]
    async fn test_cp_multiple_files_to_non_directory() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt", "/nonexistent"],
            vec![("/a.txt", ""), ("/b.txt", "")],
        ).await;
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not a directory"));
    }

    #[tokio::test]
    async fn test_cp_directory_with_recursive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/srcdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-r".to_string(), "/srcdir".to_string(), "/dstdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dstdir/file.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_cp_directory_with_capital_r() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/srcdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-R".to_string(), "/srcdir".to_string(), "/dstdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/dstdir/file.txt").await);
    }

    #[tokio::test]
    async fn test_cp_nested_directories() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/src", &crate::fs::MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/src/a", &crate::fs::MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/src/a/b", &crate::fs::MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/src/a/b/c.txt", b"deep").await.unwrap();
        fs.write_file("/src/root.txt", b"root").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-r".to_string(), "/src".to_string(), "/dst".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs.read_file("/dst/a/b/c.txt").await.unwrap(), "deep");
        assert_eq!(fs.read_file("/dst/root.txt").await.unwrap(), "root");
    }

    #[tokio::test]
    async fn test_cp_recursive_long_flag() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/srcdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["--recursive".to_string(), "/srcdir".to_string(), "/dstdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/dstdir/file.txt").await);
    }

    #[tokio::test]
    async fn test_cp_missing_source() {
        let ctx = make_ctx_with_files(vec!["/missing.txt", "/dst.txt"], vec![]).await;
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_cp_missing_destination() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt"],
            vec![("/src.txt", "")],
        ).await;
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing destination"));
    }

    #[tokio::test]
    async fn test_cp_relative_paths() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/home", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.mkdir("/home/user", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/home/user/src.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["src.txt".to_string(), "dst.txt".to_string()],
            stdin: String::new(),
            cwd: "/home/user".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/home/user/dst.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_cp_verbose() {
        let ctx = make_ctx_with_files(
            vec!["-v", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "content")],
        ).await;
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("/src.txt"));
        assert!(result.stdout.contains("/dest.txt"));
    }

    #[tokio::test]
    async fn test_cp_no_clobber_long_flag() {
        let ctx = make_ctx_with_files(
            vec!["--no-clobber", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "new"), ("/dest.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "old");
    }
}
