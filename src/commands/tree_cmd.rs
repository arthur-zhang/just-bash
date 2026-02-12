use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TreeCommand;

const HELP: &str = "tree - list contents of directories in a tree-like format

Usage: tree [OPTION]... [DIRECTORY]...

Options:
  -a          include hidden files
  -d          list directories only
  -L LEVEL    limit depth of directory tree
  -f          print full path prefix for each file
  --help      display this help and exit";

struct TreeOptions {
    show_hidden: bool,
    directories_only: bool,
    max_depth: Option<usize>,
    full_path: bool,
}

struct TreeResult {
    output: String,
    dir_count: usize,
    file_count: usize,
}

#[async_trait]
impl Command for TreeCommand {
    fn name(&self) -> &'static str {
        "tree"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut options = TreeOptions {
            show_hidden: false,
            directories_only: false,
            max_depth: None,
            full_path: false,
        };
        let mut directories = Vec::new();
        let mut i = 0;

        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "-a" => { options.show_hidden = true; i += 1; }
                "-d" => { options.directories_only = true; i += 1; }
                "-f" => { options.full_path = true; i += 1; }
                "-L" => {
                    i += 1;
                    if i < ctx.args.len() {
                        options.max_depth = ctx.args[i].parse().ok();
                    }
                    i += 1;
                }
                "--" => {
                    directories.extend(ctx.args[i + 1..].iter().cloned());
                    break;
                }
                _ => {
                    directories.push(arg.clone());
                    i += 1;
                }
            }
        }

        if directories.is_empty() {
            directories.push(".".to_string());
        }

        let mut total_output = String::new();
        let mut total_dirs = 0usize;
        let mut total_files = 0usize;

        for dir in &directories {
            let result = build_tree(&ctx, dir, &options, "", 0).await;
            total_output.push_str(&result.output);
            total_dirs += result.dir_count;
            total_files += result.file_count;
        }

        total_output.push('\n');
        total_output.push_str(&format!(
            "{} director{}",
            total_dirs,
            if total_dirs == 1 { "y" } else { "ies" }
        ));
        if !options.directories_only {
            total_output.push_str(&format!(
                ", {} file{}",
                total_files,
                if total_files == 1 { "" } else { "s" }
            ));
        }
        total_output.push('\n');

        CommandResult::success(total_output)
    }
}

async fn build_tree(
    ctx: &CommandContext,
    path: &str,
    options: &TreeOptions,
    prefix: &str,
    depth: usize,
) -> TreeResult {
    let mut result = TreeResult {
        output: String::new(),
        dir_count: 0,
        file_count: 0,
    };

    let full_path = ctx.fs.resolve_path(&ctx.cwd, path);

    let stat = match ctx.fs.stat(&full_path).await {
        Ok(s) => s,
        Err(_) => {
            result.output = format!("{} [error opening dir]\n", path);
            return result;
        }
    };

    if !stat.is_directory {
        result.output = format!("{}\n", path);
        result.file_count = 1;
        return result;
    }

    result.output = format!("{}\n", path);

    if let Some(max) = options.max_depth {
        if depth >= max {
            return result;
        }
    }

    let entries = match ctx.fs.readdir(&full_path).await {
        Ok(e) => e,
        Err(_) => return result,
    };

    let mut filtered: Vec<_> = entries
        .into_iter()
        .filter(|e| options.show_hidden || !e.starts_with('.'))
        .collect();
    filtered.sort();

    if options.directories_only {
        let mut dirs_only = Vec::new();
        for entry in &filtered {
            let entry_path = if full_path == "/" {
                format!("/{}", entry)
            } else {
                format!("{}/{}", full_path, entry)
            };
            if let Ok(s) = ctx.fs.stat(&entry_path).await {
                if s.is_directory {
                    dirs_only.push(entry.clone());
                }
            }
        }
        filtered = dirs_only;
    }

    for (idx, entry) in filtered.iter().enumerate() {
        let entry_path = if full_path == "/" {
            format!("/{}", entry)
        } else {
            format!("{}/{}", full_path, entry)
        };
        let is_last = idx == filtered.len() - 1;
        let connector = if is_last { "`-- " } else { "|-- " };
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "|   " });

        let stat = match ctx.fs.stat(&entry_path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let display_name = if options.full_path { &entry_path } else { entry };

        if stat.is_directory {
            result.dir_count += 1;
            result.output.push_str(&format!("{}{}{}\n", prefix, connector, display_name));

            if options.max_depth.is_none() || depth + 1 < options.max_depth.unwrap() {
                let sub = build_tree_recursive(ctx, &entry_path, options, &child_prefix, depth + 1).await;
                result.output.push_str(&sub.output);
                result.dir_count += sub.dir_count;
                result.file_count += sub.file_count;
            }
        } else {
            result.file_count += 1;
            result.output.push_str(&format!("{}{}{}\n", prefix, connector, display_name));
        }
    }

    result
}

async fn build_tree_recursive(
    ctx: &CommandContext,
    path: &str,
    options: &TreeOptions,
    prefix: &str,
    depth: usize,
) -> TreeResult {
    let mut result = TreeResult {
        output: String::new(),
        dir_count: 0,
        file_count: 0,
    };

    if let Some(max) = options.max_depth {
        if depth >= max {
            return result;
        }
    }

    let entries = match ctx.fs.readdir(path).await {
        Ok(e) => e,
        Err(_) => return result,
    };

    let mut filtered: Vec<_> = entries
        .into_iter()
        .filter(|e| options.show_hidden || !e.starts_with('.'))
        .collect();
    filtered.sort();

    if options.directories_only {
        let mut dirs_only = Vec::new();
        for entry in &filtered {
            let entry_path = if path == "/" {
                format!("/{}", entry)
            } else {
                format!("{}/{}", path, entry)
            };
            if let Ok(s) = ctx.fs.stat(&entry_path).await {
                if s.is_directory {
                    dirs_only.push(entry.clone());
                }
            }
        }
        filtered = dirs_only;
    }

    for (idx, entry) in filtered.iter().enumerate() {
        let entry_path = if path == "/" {
            format!("/{}", entry)
        } else {
            format!("{}/{}", path, entry)
        };
        let is_last = idx == filtered.len() - 1;
        let connector = if is_last { "`-- " } else { "|-- " };
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "|   " });

        let stat = match ctx.fs.stat(&entry_path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let display_name = if options.full_path { &entry_path } else { entry };

        if stat.is_directory {
            result.dir_count += 1;
            result.output.push_str(&format!("{}{}{}\n", prefix, connector, display_name));

            let sub = Box::pin(build_tree_recursive(ctx, &entry_path, options, &child_prefix, depth + 1)).await;
            result.output.push_str(&sub.output);
            result.dir_count += sub.dir_count;
            result.file_count += sub.file_count;
        } else {
            result.file_count += 1;
            result.output.push_str(&format!("{}{}{}\n", prefix, connector, display_name));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::{InMemoryFs, FileSystem};

    fn create_ctx(args: Vec<&str>) -> CommandContext {
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
    async fn test_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = TreeCommand.execute(ctx).await;
        assert!(result.stdout.contains("tree"));
        assert!(result.stdout.contains("-a"));
    }

    #[tokio::test]
    async fn test_empty_dir() {
        let mut ctx = create_ctx(vec!["/"]);
        let fs = Arc::new(InMemoryFs::new());
        ctx.fs = fs;
        let result = TreeCommand.execute(ctx).await;
        assert!(result.stdout.contains("/"));
        assert!(result.stdout.contains("director"));
    }

    #[tokio::test]
    async fn test_with_files() {
        let mut ctx = create_ctx(vec!["/"]);
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", b"hello").await.unwrap();
        ctx.fs = fs;
        let result = TreeCommand.execute(ctx).await;
        assert!(result.stdout.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_directories_only() {
        let mut ctx = create_ctx(vec!["-d", "/"]);
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", b"hello").await.unwrap();
        fs.mkdir("/subdir", &Default::default()).await.unwrap();
        ctx.fs = fs;
        let result = TreeCommand.execute(ctx).await;
        assert!(result.stdout.contains("subdir"));
    }
}
