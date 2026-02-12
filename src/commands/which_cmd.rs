use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct WhichCommand;

const HELP: &str = "which - locate a command

Usage: which [-as] program ...

Options:
  -a         List all instances of executables found
  -s         No output, just return 0 if found, 1 if not
  --help     display this help and exit";

#[async_trait]
impl Command for WhichCommand {
    fn name(&self) -> &'static str {
        "which"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut show_all = false;
        let mut silent = false;
        let mut names = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-a" => show_all = true,
                "-s" => silent = true,
                s if s.starts_with('-') => {
                    for c in s.chars().skip(1) {
                        match c {
                            'a' => show_all = true,
                            's' => silent = true,
                            _ => {}
                        }
                    }
                }
                _ => names.push(arg.clone()),
            }
        }

        if names.is_empty() {
            return CommandResult::with_exit_code(String::new(), String::new(), 1);
        }

        let path_env = ctx.env.get("PATH").cloned().unwrap_or_else(|| "/usr/bin:/bin".to_string());
        let path_dirs: Vec<&str> = path_env.split(':').collect();

        let mut stdout = String::new();
        let mut all_found = true;

        for name in &names {
            let mut found = false;

            for dir in &path_dirs {
                if dir.is_empty() {
                    continue;
                }
                let full_path = format!("{}/{}", dir, name);
                if ctx.fs.exists(&full_path).await {
                    found = true;
                    if !silent {
                        stdout.push_str(&format!("{}\n", full_path));
                    }
                    if !show_all {
                        break;
                    }
                }
            }

            if !found {
                all_found = false;
            }
        }

        CommandResult::with_exit_code(stdout, String::new(), if all_found { 0 } else { 1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    #[tokio::test]
    async fn test_which_no_args() {
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
        let cmd = WhichCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_which_help() {
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
        let cmd = WhichCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("which"));
    }
}
