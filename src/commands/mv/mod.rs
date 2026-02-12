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
            exec_fn: None,
            fetch_fn: None,
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
            exec_fn: None,
            fetch_fn: None,
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

    #[tokio::test]
    async fn test_mv_remove_source() {
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
    async fn test_mv_rename_in_same_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/dir/oldname.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["/dir/oldname.txt".to_string(), "/dir/newname.txt".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dir/newname.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_mv_multiple_files_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/a.txt", b"aaa").await.unwrap();
        fs.write_file("/b.txt", b"bbb").await.unwrap();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/a.txt".to_string(), "/b.txt".to_string(), "/dir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs.read_file("/dir/a.txt").await.unwrap(), "aaa");
        assert_eq!(fs.read_file("/dir/b.txt").await.unwrap(), "bbb");
        assert!(!fs.exists("/a.txt").await);
        assert!(!fs.exists("/b.txt").await);
    }

    #[tokio::test]
    async fn test_mv_multiple_files_to_non_directory() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt", "/nonexistent"],
            vec![("/a.txt", ""), ("/b.txt", "")],
        ).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not a directory"));
    }

    #[tokio::test]
    async fn test_mv_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/srcdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["/srcdir".to_string(), "/dstdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dstdir/file.txt").await.unwrap();
        assert_eq!(content, "content");
        assert!(!fs.exists("/srcdir").await);
    }

    #[tokio::test]
    async fn test_mv_nested_directories() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/src/a", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/src/a/b", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/src/a/b/c.txt", b"deep").await.unwrap();
        fs.write_file("/src/root.txt", b"root").await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src".to_string(), "/dst".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs.read_file("/dst/a/b/c.txt").await.unwrap(), "deep");
        assert_eq!(fs.read_file("/dst/root.txt").await.unwrap(), "root");
        assert!(!fs.exists("/src").await);
    }

    #[tokio::test]
    async fn test_mv_overwrite_destination() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "new");
        assert!(!fs.exists("/src.txt").await);
    }

    #[tokio::test]
    async fn test_mv_missing_destination() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt"],
            vec![("/src.txt", "")],
        ).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing destination"));
    }

    #[tokio::test]
    async fn test_mv_relative_paths() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/home", &MkdirOptions { recursive: false }).await.unwrap();
        fs.mkdir("/home/user", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/home/user/old.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["old.txt".to_string(), "new.txt".to_string()],
            stdin: String::new(),
            cwd: "/home/user".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/home/user/new.txt").await.unwrap();
        assert_eq!(content, "content");
        assert!(!fs.exists("/home/user/old.txt").await);
    }

    #[tokio::test]
    async fn test_mv_directory_into_existing_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/src", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/src/file.txt", b"content").await.unwrap();
        fs.mkdir("/dst", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src".to_string(), "/dst/".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dst/src/file.txt").await.unwrap();
        assert_eq!(content, "content");
        assert!(!fs.exists("/src").await);
    }

    #[tokio::test]
    async fn test_mv_force_flag() {
        let ctx = make_ctx_with_files(
            vec!["-f", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stderr, "");
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn test_mv_no_clobber_skip_existing() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stderr, "");
        assert!(fs.exists("/src.txt").await);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "old");
    }

    #[tokio::test]
    async fn test_mv_no_clobber_when_dest_not_exists() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "content");
        assert!(!fs.exists("/src.txt").await);
    }

    #[tokio::test]
    async fn test_mv_verbose() {
        let ctx = make_ctx_with_files(
            vec!["-v", "/old.txt", "/new.txt"],
            vec![("/old.txt", "content")],
        ).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("renamed"));
        assert!(result.stdout.contains("/old.txt"));
        assert!(result.stdout.contains("/new.txt"));
    }

    #[tokio::test]
    async fn test_mv_combined_flags_fv() {
        let ctx = make_ctx_with_files(
            vec!["-f", "-v", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("renamed"));
    }

    #[tokio::test]
    async fn test_mv_no_clobber_long_flag() {
        let ctx = make_ctx_with_files(
            vec!["--no-clobber", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/src.txt").await);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "old");
    }

    #[tokio::test]
    async fn test_mv_verbose_long_flag() {
        let ctx = make_ctx_with_files(
            vec!["--verbose", "/old.txt", "/new.txt"],
            vec![("/old.txt", "content")],
        ).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("renamed"));
    }

    #[tokio::test]
    async fn test_mv_force_long_flag() {
        let ctx = make_ctx_with_files(
            vec!["--force", "/src.txt", "/dst.txt"],
            vec![("/src.txt", "new"), ("/dst.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "new");
    }
}
