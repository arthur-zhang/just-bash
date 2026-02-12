use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::RmOptions;

pub struct RmdirCommand;

const USAGE: &str = "Usage: rmdir [-pv] DIRECTORY...
Remove empty directories.

Options:
  -p, --parents   Remove DIRECTORY and its ancestors
  -v, --verbose   Output a diagnostic for every directory processed
      --help      Display this help and exit";

#[async_trait]
impl Command for RmdirCommand {
    fn name(&self) -> &'static str {
        "rmdir"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut parents = false;
        let mut verbose = false;
        let mut dirs = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", USAGE)),
                "-p" | "--parents" => parents = true,
                "-v" | "--verbose" => verbose = true,
                _ if arg.starts_with('-') => {
                    for c in arg.chars().skip(1) {
                        match c {
                            'p' => parents = true,
                            'v' => verbose = true,
                            _ => return CommandResult::error(format!("rmdir: invalid option -- '{}'\n", c)),
                        }
                    }
                }
                _ => dirs.push(arg.clone()),
            }
        }

        if dirs.is_empty() {
            return CommandResult::error("rmdir: missing operand\n".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for dir in dirs {
            let result = remove_dir(&ctx, &dir, parents, verbose).await;
            stdout.push_str(&result.stdout);
            stderr.push_str(&result.stderr);
            if result.exit_code != 0 {
                exit_code = result.exit_code;
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

async fn remove_dir(ctx: &CommandContext, dir: &str, parents: bool, verbose: bool) -> CommandResult {
    let mut stdout = String::new();
    let full_path = ctx.fs.resolve_path(&ctx.cwd, dir);

    let result = remove_single_dir(ctx, &full_path, dir, verbose).await;
    stdout.push_str(&result.stdout);
    if result.exit_code != 0 {
        return CommandResult::with_exit_code(stdout, result.stderr, result.exit_code);
    }

    if parents {
        let mut current_path = full_path;
        let mut current_dir = dir.to_string();

        loop {
            let parent_path = get_parent_path(&current_path);
            let parent_dir = get_parent_path(&current_dir);

            if parent_path == current_path
                || parent_path == "/"
                || parent_path == "."
                || parent_dir == "."
                || parent_dir.is_empty()
            {
                break;
            }

            let parent_result = remove_single_dir(ctx, &parent_path, &parent_dir, verbose).await;
            stdout.push_str(&parent_result.stdout);

            if parent_result.exit_code != 0 {
                break;
            }

            current_path = parent_path;
            current_dir = parent_dir;
        }
    }

    CommandResult::success(stdout)
}

async fn remove_single_dir(ctx: &CommandContext, full_path: &str, display_path: &str, verbose: bool) -> CommandResult {
    if !ctx.fs.exists(full_path).await {
        return CommandResult::error(format!(
            "rmdir: failed to remove '{}': No such file or directory\n",
            display_path
        ));
    }

    match ctx.fs.stat(full_path).await {
        Ok(stat) => {
            if !stat.is_directory {
                return CommandResult::error(format!(
                    "rmdir: failed to remove '{}': Not a directory\n",
                    display_path
                ));
            }
        }
        Err(e) => {
            return CommandResult::error(format!(
                "rmdir: failed to remove '{}': {}\n",
                display_path, e
            ));
        }
    }

    match ctx.fs.readdir(full_path).await {
        Ok(entries) => {
            if !entries.is_empty() {
                return CommandResult::error(format!(
                    "rmdir: failed to remove '{}': Directory not empty\n",
                    display_path
                ));
            }
        }
        Err(e) => {
            return CommandResult::error(format!(
                "rmdir: failed to remove '{}': {}\n",
                display_path, e
            ));
        }
    }

    if let Err(e) = ctx.fs.rm(full_path, &RmOptions { recursive: false, force: false }).await {
        return CommandResult::error(format!(
            "rmdir: failed to remove '{}': {}\n",
            display_path, e
        ));
    }

    if verbose {
        CommandResult::success(format!("rmdir: removing directory, '{}'\n", display_path))
    } else {
        CommandResult::success(String::new())
    }
}

fn get_parent_path(path: &str) -> String {
    let normalized = path.trim_end_matches('/');

    match normalized.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(pos) => normalized[..pos].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    async fn create_ctx_with_fs() -> (CommandContext, Arc<InMemoryFs>) {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        (ctx, fs)
    }

    #[tokio::test]
    async fn test_rmdir_missing_operand() {
        let (ctx, _) = create_ctx_with_fs().await;
        let cmd = RmdirCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing operand"));
    }

    #[tokio::test]
    async fn test_rmdir_help() {
        let (mut ctx, _) = create_ctx_with_fs().await;
        ctx.args = vec!["--help".to_string()];
        let cmd = RmdirCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("rmdir"));
    }

    #[test]
    fn test_get_parent_path() {
        assert_eq!(get_parent_path("/a/b/c"), "/a/b");
        assert_eq!(get_parent_path("/a"), "/");
        assert_eq!(get_parent_path("a/b"), "a");
        assert_eq!(get_parent_path("a"), ".");
        assert_eq!(get_parent_path("/a/b/c/"), "/a/b");
    }
}
