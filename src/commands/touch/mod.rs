// src/commands/touch/mod.rs
use async_trait::async_trait;
use std::time::SystemTime;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TouchCommand;

#[async_trait]
impl Command for TouchCommand {
    fn name(&self) -> &'static str {
        "touch"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: touch [OPTION]... FILE...\n\n\
                 Update the access and modification times of each FILE to the current time.\n\n\
                 Options:\n\
                   -c, --no-create    do not create any files\n\
                   -d, --date=STRING  parse STRING and use it instead of current time\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let mut files: Vec<String> = Vec::new();
        let mut no_create = false;

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];

            if arg == "--" {
                files.extend(ctx.args[i + 1..].iter().cloned());
                break;
            } else if arg == "-d" || arg == "--date" {
                // 跳过日期参数（简化实现，忽略日期）
                i += 1;
            } else if arg.starts_with("--date=") {
                // 忽略日期
            } else if arg == "-c" || arg == "--no-create" {
                no_create = true;
            } else if arg == "-a" || arg == "-m" || arg == "-r" || arg == "-t" {
                // 忽略这些选项
                if arg == "-r" || arg == "-t" {
                    i += 1; // 跳过参数
                }
            } else if arg.starts_with('-') && arg.len() > 1 {
                // 处理组合短选项
                for c in arg[1..].chars() {
                    match c {
                        'c' => no_create = true,
                        'a' | 'm' => {}
                        'd' => {
                            i += 1;
                            break;
                        }
                        _ => {}
                    }
                }
            } else {
                files.push(arg.clone());
            }
            i += 1;
        }

        if files.is_empty() {
            return CommandResult::error("touch: missing file operand\n".to_string());
        }

        let target_time = SystemTime::now();

        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            let exists = ctx.fs.exists(&path).await;

            if !exists {
                if no_create {
                    continue;
                }
                if let Err(e) = ctx.fs.write_file(&path, &[]).await {
                    stderr.push_str(&format!("touch: cannot touch '{}': {:?}\n", file, e));
                    exit_code = 1;
                    continue;
                }
            }

            if let Err(e) = ctx.fs.utimes(&path, target_time).await {
                stderr.push_str(&format!("touch: cannot touch '{}': {:?}\n", file, e));
                exit_code = 1;
            }
        }

        CommandResult::with_exit_code(String::new(), stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_touch_create_file() {
        let ctx = make_ctx(vec!["/newfile.txt"]);
        let fs = ctx.fs.clone();
        let cmd = TouchCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/newfile.txt").await);
    }

    #[tokio::test]
    async fn test_touch_no_create() {
        let ctx = make_ctx(vec!["-c", "/nonexistent.txt"]);
        let fs = ctx.fs.clone();
        let cmd = TouchCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/nonexistent.txt").await);
    }

    #[tokio::test]
    async fn test_touch_missing_operand() {
        let ctx = make_ctx(vec![]);
        let cmd = TouchCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("missing file operand"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_touch_multiple_files() {
        let ctx = make_ctx(vec!["/a.txt", "/b.txt"]);
        let fs = ctx.fs.clone();
        let cmd = TouchCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/a.txt").await);
        assert!(fs.exists("/b.txt").await);
    }
}
