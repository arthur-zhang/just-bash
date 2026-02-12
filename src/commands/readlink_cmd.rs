use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use std::collections::HashSet;

pub struct ReadlinkCommand;

const HELP: &str = "readlink - print resolved symbolic links or canonical file names

Usage: readlink [OPTIONS] FILE...

Options:
  -f      canonicalize by following every symlink recursively
  --help  display this help and exit";

#[async_trait]
impl Command for ReadlinkCommand {
    fn name(&self) -> &'static str {
        "readlink"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut canonicalize = false;
        let mut files = Vec::new();
        let mut parsing_opts = true;

        for arg in &ctx.args {
            if parsing_opts {
                match arg.as_str() {
                    "--help" => return CommandResult::success(format!("{}\n", HELP)),
                    "-f" | "--canonicalize" => canonicalize = true,
                    "--" => parsing_opts = false,
                    s if s.starts_with('-') => {
                        return CommandResult::error(format!(
                            "readlink: invalid option -- '{}'\n",
                            &s[1..]
                        ));
                    }
                    _ => {
                        parsing_opts = false;
                        files.push(arg.clone());
                    }
                }
            } else {
                files.push(arg.clone());
            }
        }

        if files.is_empty() {
            return CommandResult::error("readlink: missing operand\n".to_string());
        }

        let mut stdout = String::new();
        let mut any_error = false;

        for file in &files {
            let file_path = ctx.fs.resolve_path(&ctx.cwd, file);

            if canonicalize {
                let mut current_path = file_path.clone();
                let mut seen = HashSet::new();

                loop {
                    if seen.contains(&current_path) {
                        break;
                    }
                    seen.insert(current_path.clone());

                    match ctx.fs.readlink(&current_path).await {
                        Ok(target) => {
                            if target.starts_with('/') {
                                current_path = target;
                            } else {
                                let dir = current_path
                                    .rfind('/')
                                    .map(|i| &current_path[..i])
                                    .unwrap_or("/");
                                current_path = ctx.fs.resolve_path(dir, &target);
                            }
                        }
                        Err(_) => break,
                    }
                }
                stdout.push_str(&format!("{}\n", current_path));
            } else {
                match ctx.fs.readlink(&file_path).await {
                    Ok(target) => stdout.push_str(&format!("{}\n", target)),
                    Err(_) => any_error = true,
                }
            }
        }

        CommandResult::with_exit_code(stdout, String::new(), if any_error { 1 } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_readlink_missing_operand() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = ReadlinkCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing operand"));
    }

    #[tokio::test]
    async fn test_readlink_help() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["--help".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = ReadlinkCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("readlink"));
    }
}
