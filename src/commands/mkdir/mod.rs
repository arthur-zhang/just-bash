// src/commands/mkdir/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::MkdirOptions;

pub struct MkdirCommand;

#[async_trait]
impl Command for MkdirCommand {
    fn name(&self) -> &'static str {
        "mkdir"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: mkdir [OPTION]... DIRECTORY...\n\n\
                 Create the DIRECTORY(ies), if they do not already exist.\n\n\
                 Options:\n\
                   -p, --parents    no error if existing, make parent directories as needed\n\
                   -v, --verbose    print a message for each created directory\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut recursive = false;
        let mut verbose = false;
        let mut dirs: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-p" | "--parents" => recursive = true,
                "-v" | "--verbose" => verbose = true,
                _ if !arg.starts_with('-') => dirs.push(arg.clone()),
                _ => {}
            }
        }

        if dirs.is_empty() {
            return CommandResult::error("mkdir: missing operand\n".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for dir in &dirs {
            let path = ctx.fs.resolve_path(&ctx.cwd, dir);
            let opts = MkdirOptions { recursive };

            match ctx.fs.mkdir(&path, &opts).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("mkdir: created directory '{}'\n", dir));
                    }
                }
                Err(e) => {
                    let msg = format!("{:?}", e);
                    if msg.contains("NotFound") {
                        stderr.push_str(&format!(
                            "mkdir: cannot create directory '{}': No such file or directory\n",
                            dir
                        ));
                    } else if msg.contains("AlreadyExists") {
                        stderr.push_str(&format!(
                            "mkdir: cannot create directory '{}': File exists\n",
                            dir
                        ));
                    } else {
                        stderr.push_str(&format!("mkdir: cannot create directory '{}': {}\n", dir, msg));
                    }
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
        }
    }

    #[tokio::test]
    async fn test_mkdir_simple() {
        let ctx = make_ctx(vec!["/newdir"]);
        let cmd = MkdirCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_mkdir_recursive() {
        let ctx = make_ctx(vec!["-p", "/a/b/c"]);
        let cmd = MkdirCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_mkdir_verbose() {
        let ctx = make_ctx(vec!["-v", "/newdir"]);
        let cmd = MkdirCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("created directory"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_mkdir_missing_operand() {
        let ctx = make_ctx(vec![]);
        let cmd = MkdirCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("missing operand"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_mkdir_no_parent() {
        let ctx = make_ctx(vec!["/nonexistent/dir"]);
        let cmd = MkdirCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }
}
