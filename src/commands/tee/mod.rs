// src/commands/tee/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TeeCommand;

const HELP: &str = "Usage: tee [OPTION]... [FILE]...\n\n\
read from stdin and write to stdout and files\n\n\
Options:\n  -a, --append     append to the given FILEs, do not overwrite\n      --help       display this help and exit\n";

fn resolve_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') { path.to_string() }
    else { format!("{}/{}", cwd.trim_end_matches('/'), path) }
}

#[async_trait]
impl Command for TeeCommand {
    fn name(&self) -> &'static str { "tee" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(HELP.into());
        }

        let mut append = false;
        let mut files: Vec<String> = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-a" | "--append" => append = true,
                _ => files.push(arg.clone()),
            }
        }

        let content = &ctx.stdin;
        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let file_path = resolve_path(&ctx.cwd, file);
            let result = if append {
                ctx.fs.append_file(&file_path, content.as_bytes()).await
            } else {
                ctx.fs.write_file(&file_path, content.as_bytes()).await
            };
            if result.is_err() {
                stderr.push_str(&format!("tee: {}: No such file or directory\n", file));
                exit_code = 1;
            }
        }

        // Pass through to stdout
        CommandResult::with_exit_code(content.clone(), stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: stdin.into(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    fn make_ctx_with_fs(args: Vec<&str>, stdin: &str, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: stdin.into(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    #[tokio::test]
    async fn test_tee_passthrough() {
        let r = TeeCommand.execute(make_ctx(vec![], "hello\n")).await;
        assert_eq!(r.stdout, "hello\n");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tee_write_file() {
        let fs = Arc::new(InMemoryFs::new());
        let r = TeeCommand.execute(make_ctx_with_fs(vec!["output.txt"], "hello\n", fs.clone())).await;
        assert_eq!(r.stdout, "hello\n");
        let content = fs.read_file("/output.txt").await.unwrap();
        assert_eq!(content, "hello\n");
    }

    #[tokio::test]
    async fn test_tee_multiple_files() {
        let fs = Arc::new(InMemoryFs::new());
        let r = TeeCommand.execute(make_ctx_with_fs(vec!["file1.txt", "file2.txt"], "hello\n", fs.clone())).await;
        assert_eq!(r.stdout, "hello\n");
        assert_eq!(fs.read_file("/file1.txt").await.unwrap(), "hello\n");
        assert_eq!(fs.read_file("/file2.txt").await.unwrap(), "hello\n");
    }

    #[tokio::test]
    async fn test_tee_append() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", "existing\n".as_bytes()).await.unwrap();
        let _r = TeeCommand.execute(make_ctx_with_fs(vec!["-a", "/test.txt"], "appended\n", fs.clone())).await;
        let content = fs.read_file("/test.txt").await.unwrap();
        assert_eq!(content, "existing\nappended\n");
    }

    #[tokio::test]
    async fn test_tee_help() {
        let r = TeeCommand.execute(make_ctx(vec!["--help"], "")).await;
        assert!(r.stdout.contains("tee"));
        assert!(r.stdout.contains("stdin"));
    }
}
