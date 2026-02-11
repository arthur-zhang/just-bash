// src/commands/ln/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::types::RmOptions;

pub struct LnCommand;

#[async_trait]
impl Command for LnCommand {
    fn name(&self) -> &'static str {
        "ln"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: ln [OPTIONS] TARGET LINK_NAME\n\n\
                 Make links between files.\n\n\
                 Options:\n\
                   -s      create a symbolic link instead of a hard link\n\
                   -f      remove existing destination files\n\
                   -n      treat LINK_NAME as a normal file if it is a symbolic link to a directory\n\
                   -v      print name of each linked file\n\
                       --help display this help and exit\n".to_string()
            );
        }

        let mut symbolic = false;
        let mut force = false;
        let mut verbose = false;
        let mut arg_idx = 0;

        // Parse options
        while arg_idx < args.len() && args[arg_idx].starts_with('-') {
            let arg = &args[arg_idx];
            match arg.as_str() {
                "-s" | "--symbolic" => {
                    symbolic = true;
                    arg_idx += 1;
                }
                "-f" | "--force" => {
                    force = true;
                    arg_idx += 1;
                }
                "-v" | "--verbose" => {
                    verbose = true;
                    arg_idx += 1;
                }
                "-n" | "--no-dereference" => {
                    // Accept but don't implement special behavior
                    arg_idx += 1;
                }
                "--" => {
                    arg_idx += 1;
                    break;
                }
                _ => {
                    // Check for combined short flags like -sf, -sfv, etc.
                    let flag_chars: Vec<char> = arg[1..].chars().collect();
                    let all_valid = flag_chars.iter().all(|c| "sfvn".contains(*c));
                    if all_valid && !flag_chars.is_empty() {
                        if flag_chars.contains(&'s') { symbolic = true; }
                        if flag_chars.contains(&'f') { force = true; }
                        if flag_chars.contains(&'v') { verbose = true; }
                        arg_idx += 1;
                    } else {
                        return CommandResult::with_exit_code(
                            String::new(),
                            format!("ln: invalid option -- '{}'\n", &arg[1..]),
                            1,
                        );
                    }
                }
            }
        }

        let remaining: Vec<&String> = args[arg_idx..].iter().collect();

        if remaining.len() < 2 {
            return CommandResult::error("ln: missing file operand\n".to_string());
        }

        let target = &remaining[0];
        let link_name = &remaining[1];
        let link_path = ctx.fs.resolve_path(&ctx.cwd, link_name);

        // Check if link already exists
        if ctx.fs.exists(&link_path).await {
            if force {
                if let Err(_) = ctx.fs.rm(&link_path, &RmOptions { force: true, recursive: false }).await {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("ln: cannot remove '{}': Permission denied\n", link_name),
                        1,
                    );
                }
            } else {
                let link_type = if symbolic { "symbolic " } else { "" };
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("ln: failed to create {}link '{}': File exists\n", link_type, link_name),
                    1,
                );
            }
        }

        if symbolic {
            // Create symbolic link
            // For symlinks, the target is stored as-is (can be relative or absolute)
            if let Err(e) = ctx.fs.symlink(target, &link_path).await {
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("ln: {}\n", e),
                    1,
                );
            }
        } else {
            // Create hard link
            let target_path = ctx.fs.resolve_path(&ctx.cwd, target);
            // Check that target exists
            if !ctx.fs.exists(&target_path).await {
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("ln: failed to access '{}': No such file or directory\n", target),
                    1,
                );
            }
            if let Err(e) = ctx.fs.link(&target_path, &link_path).await {
                let msg = format!("{}", e);
                if msg.contains("EPERM") {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("ln: '{}': hard link not allowed for directory\n", target),
                        1,
                    );
                }
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("ln: {}\n", msg),
                    1,
                );
            }
        }

        let stdout = if verbose {
            format!("'{}' -> '{}'\n", link_name, target)
        } else {
            String::new()
        };
        CommandResult::with_exit_code(stdout, String::new(), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
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
    async fn test_ln_symbolic() {
        let ctx = make_ctx_with_files(
            vec!["-s", "/target.txt", "/link.txt"],
            vec![("/target.txt", "hello world\n")],
        ).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ln_error_exists() {
        let ctx = make_ctx_with_files(
            vec!["-s", "/target.txt", "/link.txt"],
            vec![("/target.txt", "hello\n"), ("/link.txt", "existing\n")],
        ).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("File exists"));
    }

    #[tokio::test]
    async fn test_ln_missing_operand() {
        let ctx = make_ctx_with_files(vec![], vec![]).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing file operand"));
    }

    #[tokio::test]
    async fn test_ln_help() {
        let ctx = make_ctx_with_files(vec!["--help"], vec![]).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("ln"));
        assert!(result.stdout.contains("link"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ln_hard_link() {
        let ctx = make_ctx_with_files(
            vec!["/original.txt", "/hardlink.txt"],
            vec![("/original.txt", "hello world\n")],
        ).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ln_hard_link_missing_target() {
        let ctx = make_ctx_with_files(
            vec!["/nonexistent.txt", "/link.txt"],
            vec![],
        ).await;
        let cmd = LnCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file"));
    }
}
