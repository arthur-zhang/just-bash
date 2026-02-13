pub mod types;
pub mod parser;
pub mod matcher;

use std::collections::VecDeque;
use std::time::SystemTime;

use async_trait::async_trait;
use crate::commands::types::{Command, CommandContext, CommandResult};
use crate::fs::RmOptions;
use types::*;

pub struct FindCommand;

#[async_trait]
impl Command for FindCommand {
    fn name(&self) -> &'static str {
        "find"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        // 1. Parse starting paths (everything before first flag/expression token)
        let mut search_paths: Vec<String> = Vec::new();
        let mut expr_start = 0;

        for (i, arg) in args.iter().enumerate() {
            let s = arg.as_str();
            if s.starts_with('-')
                || s == "("
                || s == "\\("
                || s == ")"
                || s == "\\)"
                || s == "!"
            {
                expr_start = i;
                break;
            }
            search_paths.push(arg.clone());
            expr_start = i + 1;
        }

        if search_paths.is_empty() {
            search_paths.push(".".to_string());
        }

        // 2. Parse expressions from remaining args
        let expr_args: Vec<String> = args[expr_start..].to_vec();
        let (expression, options) = match parser::parse_expressions(&expr_args) {
            Ok((expr, opts)) => (expr, opts),
            Err(e) => {
                return CommandResult::error(format!("{}\n", e));
            }
        };

        // Determine if expression has an explicit action
        let has_action = expression_has_action(&expression);

        let mut all_output = String::new();
        let mut all_stderr = String::new();
        let mut exit_code = 0i32;

        // 3. For each starting path, traverse the directory tree
        for search_path in &search_paths {
            // Normalize trailing slashes
            let search_path_clean = if search_path.len() > 1 && search_path.ends_with('/') {
                &search_path[..search_path.len() - 1]
            } else {
                search_path.as_str()
            };

            let base_path = ctx.fs.resolve_path(&ctx.cwd, search_path_clean);

            // Check if path exists
            if !ctx.fs.exists(&base_path).await {
                all_stderr.push_str(&format!(
                    "find: {}: No such file or directory\n",
                    search_path_clean
                ));
                exit_code = 1;
                continue;
            }

            // Resolve -newer reference file mtime
            let newer_ref_mtime = collect_newer_ref_mtime(&expression, &ctx, &base_path).await;
            // Traversal
            let mut matched_paths: Vec<String> = Vec::new();
            let mut output = String::new();

            if options.depth_first {
                // Depth-first (post-order): process children before parents
                traverse_depth_first(
                    &ctx,
                    &base_path,
                    search_path_clean,
                    &expression,
                    &options,
                    has_action,
                    newer_ref_mtime,
                    &mut matched_paths,
                    &mut output,
                    &mut all_stderr,
                    &mut exit_code,
                ).await;
            } else {
                // BFS (pre-order): process parents before children
                traverse_breadth_first(
                    &ctx,
                    &base_path,
                    search_path_clean,
                    &expression,
                    &options,
                    has_action,
                    newer_ref_mtime,
                    &mut matched_paths,
                    &mut output,
                    &mut all_stderr,
                    &mut exit_code,
                ).await;
            }

            // Handle -delete action
            if expression_has_delete(&expression) {
                // Delete in reverse order (deepest first)
                let mut sorted = matched_paths.clone();
                sorted.sort_by(|a, b| b.len().cmp(&a.len()));
                for file in &sorted {
                    let full_path = ctx.fs.resolve_path(&ctx.cwd, file);
                    match ctx.fs.rm(&full_path, &RmOptions { recursive: false, force: false }).await {
                        Ok(()) => {}
                        Err(e) => {
                            all_stderr.push_str(&format!("find: cannot delete '{}': {}\n", file, e));
                            exit_code = 1;
                        }
                    }
                }
            }
            // Handle -exec action
            if let Some((command_parts, batch)) = expression_get_exec(&expression) {
                if let Some(ref exec_fn) = ctx.exec_fn {
                    if batch {
                        // -exec ... + : execute once with all files
                        let mut cmd_parts: Vec<String> = Vec::new();
                        for part in &command_parts {
                            if part == "{}" {
                                cmd_parts.extend(matched_paths.iter().cloned());
                            } else {
                                cmd_parts.push(part.clone());
                            }
                        }
                        let cmd = cmd_parts.iter()
                            .map(|p| format!("\"{}\"", p))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let result = exec_fn(
                            cmd,
                            String::new(),
                            ctx.cwd.clone(),
                            ctx.env.clone(),
                            ctx.fs.clone(),
                        ).await;
                        output.push_str(&result.stdout);
                        all_stderr.push_str(&result.stderr);
                        if result.exit_code != 0 {
                            exit_code = result.exit_code;
                        }
                    } else {
                        // -exec ... ; : execute for each file
                        for file in &matched_paths {
                            let cmd_with_file: Vec<String> = command_parts.iter()
                                .map(|part| if part == "{}" { file.clone() } else { part.clone() })
                                .collect();
                            let cmd = cmd_with_file.iter()
                                .map(|p| format!("\"{}\"", p))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let result = exec_fn(
                                cmd,
                                String::new(),
                                ctx.cwd.clone(),
                                ctx.env.clone(),
                                ctx.fs.clone(),
                            ).await;
                            output.push_str(&result.stdout);
                            all_stderr.push_str(&result.stderr);
                            if result.exit_code != 0 {
                                exit_code = result.exit_code;
                            }
                        }
                    }
                }
            }

            all_output.push_str(&output);
        }

        CommandResult::with_exit_code(all_output, all_stderr, exit_code)
    }
}
/// Check if expression tree contains any action (print, print0, printf, delete, exec).
fn expression_has_action(expr: &Expression) -> bool {
    match expr {
        Expression::Print | Expression::Print0 | Expression::Printf { .. }
        | Expression::Delete | Expression::Exec { .. } => true,
        Expression::Not(inner) => expression_has_action(inner),
        Expression::And(left, right) | Expression::Or(left, right) => {
            expression_has_action(left) || expression_has_action(right)
        }
        _ => false,
    }
}

/// Check if expression tree contains -delete.
fn expression_has_delete(expr: &Expression) -> bool {
    match expr {
        Expression::Delete => true,
        Expression::Not(inner) => expression_has_delete(inner),
        Expression::And(left, right) | Expression::Or(left, right) => {
            expression_has_delete(left) || expression_has_delete(right)
        }
        _ => false,
    }
}

/// Extract -exec command parts and batch flag if present.
fn expression_get_exec(expr: &Expression) -> Option<(Vec<String>, bool)> {
    match expr {
        Expression::Exec { command, batch } => Some((command.clone(), *batch)),
        Expression::Not(inner) => expression_get_exec(inner),
        Expression::And(left, right) | Expression::Or(left, right) => {
            expression_get_exec(left).or_else(|| expression_get_exec(right))
        }
        _ => None,
    }
}

/// Collect -newer reference file mtime.
async fn collect_newer_ref_mtime(
    expr: &Expression,
    ctx: &CommandContext,
    _base_path: &str,
) -> Option<SystemTime> {
    if let Some(ref_path) = collect_newer_ref_path(expr) {
        let full_path = ctx.fs.resolve_path(&ctx.cwd, &ref_path);
        if let Ok(stat) = ctx.fs.stat(&full_path).await {
            return Some(stat.mtime);
        }
    }
    None
}
fn collect_newer_ref_path(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Newer { reference_path } => Some(reference_path.clone()),
        Expression::Not(inner) => collect_newer_ref_path(inner),
        Expression::And(left, right) | Expression::Or(left, right) => {
            collect_newer_ref_path(left).or_else(|| collect_newer_ref_path(right))
        }
        _ => None,
    }
}

/// Build an EvalContext for a given entry.
fn build_eval_context(
    name: &str,
    path: &str,
    relative_path: &str,
    stat: &crate::fs::FsStat,
    depth: usize,
    is_empty: bool,
    newer_ref_mtime: Option<SystemTime>,
    starting_point: &str,
) -> EvalContext {
    EvalContext {
        name: name.to_string(),
        path: path.to_string(),
        relative_path: relative_path.to_string(),
        is_file: stat.is_file,
        is_directory: stat.is_directory,
        is_symlink: stat.is_symlink,
        size: stat.size,
        mode: stat.mode,
        mtime: stat.mtime,
        depth,
        is_empty,
        newer_ref_mtime,
        starting_point: starting_point.to_string(),
    }
}

/// Compute the relative path for display.
fn compute_relative_path(
    current_path: &str,
    base_path: &str,
    search_path: &str,
) -> String {
    if current_path == base_path {
        search_path.to_string()
    } else if search_path == "." {
        let suffix = if base_path == "/" {
            &current_path[1..]
        } else {
            &current_path[base_path.len() + 1..]
        };
        format!("./{}", suffix)
    } else {
        let suffix = &current_path[base_path.len()..];
        format!("{}{}", search_path, suffix)
    }
}
/// Compute the name (basename) for a path.
fn compute_name(current_path: &str, base_path: &str, search_path: &str) -> String {
    if current_path == base_path {
        // For the starting point, use the last component of search_path
        search_path.rsplit('/').next().unwrap_or(search_path).to_string()
    } else {
        current_path.rsplit('/').next().unwrap_or("").to_string()
    }
}

/// Check if a directory is empty.
async fn check_dir_empty(ctx: &CommandContext, path: &str) -> bool {
    match ctx.fs.readdir(path).await {
        Ok(entries) => entries.is_empty(),
        Err(_) => false,
    }
}

/// Process a single node: evaluate expression, collect output.
fn process_node(
    eval_ctx: &EvalContext,
    expression: &Expression,
    has_action: bool,
    matched_paths: &mut Vec<String>,
    output: &mut String,
) -> (bool, bool) {
    let result = matcher::evaluate(expression, eval_ctx);

    let should_output = if has_action {
        result.printed
    } else {
        result.matches
    };

    if should_output {
        matched_paths.push(eval_ctx.relative_path.clone());
        if !has_action {
            // Default print
            output.push_str(&eval_ctx.relative_path);
            output.push('\n');
        } else {
            output.push_str(&result.output);
        }
    }

    (result.matches, result.pruned)
}
#[allow(clippy::too_many_arguments)]
async fn traverse_breadth_first(
    ctx: &CommandContext,
    base_path: &str,
    search_path: &str,
    expression: &Expression,
    options: &FindOptions,
    has_action: bool,
    newer_ref_mtime: Option<SystemTime>,
    matched_paths: &mut Vec<String>,
    output: &mut String,
    stderr: &mut String,
    exit_code: &mut i32,
) {
    // Queue: (absolute_path, depth)
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((base_path.to_string(), 0));

    while let Some((current_path, depth)) = queue.pop_front() {
        // Check maxdepth
        if let Some(max) = options.max_depth {
            if depth > max {
                continue;
            }
        }

        // Stat the entry
        let stat = match ctx.fs.stat(&current_path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let name = compute_name(&current_path, base_path, search_path);
        let relative_path = compute_relative_path(&current_path, base_path, search_path);

        // Check if empty
        let is_empty = if stat.is_file {
            stat.size == 0
        } else if stat.is_directory {
            check_dir_empty(ctx, &current_path).await
        } else {
            false
        };

        // Build eval context
        let eval_ctx = build_eval_context(
            &name, &current_path, &relative_path, &stat, depth,
            is_empty, newer_ref_mtime, search_path,
        );

        // Check mindepth
        let at_or_beyond_min = options.min_depth.map_or(true, |min| depth >= min);

        let mut pruned = false;
        if at_or_beyond_min {
            let (_matched, p) = process_node(&eval_ctx, expression, has_action, matched_paths, output);
            pruned = p;
        }

        // Descend into directories (unless pruned or at maxdepth)
        if stat.is_directory && !pruned {
            if let Some(max) = options.max_depth {
                if depth >= max {
                    continue;
                }
            }
            match ctx.fs.readdir_with_file_types(&current_path).await {
                Ok(entries) => {
                    for entry in entries {
                        let child_path = if current_path == "/" {
                            format!("/{}", entry.name)
                        } else {
                            format!("{}/{}", current_path, entry.name)
                        };
                        queue.push_back((child_path, depth + 1));
                    }
                }
                Err(e) => {
                    stderr.push_str(&format!("find: {}: {}\n", relative_path, e));
                    *exit_code = 1;
                }
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
async fn traverse_depth_first(
    ctx: &CommandContext,
    base_path: &str,
    search_path: &str,
    expression: &Expression,
    options: &FindOptions,
    has_action: bool,
    newer_ref_mtime: Option<SystemTime>,
    matched_paths: &mut Vec<String>,
    output: &mut String,
    stderr: &mut String,
    exit_code: &mut i32,
) {
    // Iterative post-order DFS using two phases:
    // Phase 1: Discover all nodes (BFS-like) and record parent-child relationships
    // Phase 2: Process in reverse order (children before parents)

    struct NodeInfo {
        path: String,
        depth: usize,
        children_start: usize,
        children_end: usize,
    }

    let mut nodes: Vec<NodeInfo> = Vec::new();
    // Discovery queue
    let mut discover_queue: VecDeque<(String, usize)> = VecDeque::new();
    discover_queue.push_back((base_path.to_string(), 0));

    // Phase 1: Discover all nodes
    while let Some((current_path, depth)) = discover_queue.pop_front() {
        if let Some(max) = options.max_depth {
            if depth > max {
                continue;
            }
        }

        let stat = match ctx.fs.stat(&current_path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let children_start = discover_queue.len();
        if stat.is_directory {
            let can_descend = options.max_depth.map_or(true, |max| depth < max);
            if can_descend {
                match ctx.fs.readdir_with_file_types(&current_path).await {
                    Ok(entries) => {
                        for entry in entries {
                            let child_path = if current_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", current_path, entry.name)
                            };
                            discover_queue.push_back((child_path, depth + 1));
                        }
                    }
                    Err(e) => {
                        let rel = compute_relative_path(&current_path, base_path, search_path);
                        stderr.push_str(&format!("find: {}: {}\n", rel, e));
                        *exit_code = 1;
                    }
                }
            }
        }
        let children_end = discover_queue.len();

        nodes.push(NodeInfo {
            path: current_path,
            depth,
            children_start,
            children_end,
        });
    }

    // Phase 2: Process in reverse order (post-order)
    // Since we discovered in BFS order, reversing gives us children-before-parents
    for node_info in nodes.iter().rev() {
        let stat = match ctx.fs.stat(&node_info.path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let name = compute_name(&node_info.path, base_path, search_path);
        let relative_path = compute_relative_path(&node_info.path, base_path, search_path);

        let is_empty = if stat.is_file {
            stat.size == 0
        } else if stat.is_directory {
            check_dir_empty(ctx, &node_info.path).await
        } else {
            false
        };

        let eval_ctx = build_eval_context(
            &name, &node_info.path, &relative_path, &stat, node_info.depth,
            is_empty, newer_ref_mtime, search_path,
        );

        let at_or_beyond_min = options.min_depth.map_or(true, |min| node_info.depth >= min);
        if at_or_beyond_min {
            process_node(&eval_ctx, expression, has_action, matched_paths, output);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{InMemoryFs, FileSystem, MkdirOptions};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    fn make_ctx(fs: Arc<InMemoryFs>, args: &[&str]) -> CommandContext {
        CommandContext {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    fn sorted_lines(s: &str) -> Vec<&str> {
        let mut lines: Vec<&str> = s.lines().collect();
        lines.sort();
        lines
    }

    async fn setup_basic_fs() -> Arc<InMemoryFs> {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/project", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/project/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/project/docs", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/project/src/main.rs", b"fn main() {}").await.unwrap();
        fs.write_file("/project/src/lib.rs", b"pub mod foo;").await.unwrap();
        fs.write_file("/project/docs/readme.txt", b"Hello").await.unwrap();
        fs.write_file("/project/Cargo.toml", b"[package]").await.unwrap();
        fs
    }

    #[tokio::test]
    async fn test_find_all_files() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project"));
        assert!(lines.contains(&"/project/src"));
        assert!(lines.contains(&"/project/src/main.rs"));
        assert!(lines.contains(&"/project/docs/readme.txt"));
    }

    #[tokio::test]
    async fn test_find_by_name_pattern() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-name", "*.rs"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/project/src/lib.rs"));
        assert!(lines.contains(&"/project/src/main.rs"));
    }
    #[tokio::test]
    async fn test_find_by_type_files_only() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        // Should only contain files, not directories
        for line in &lines {
            assert!(!line.ends_with("/src") && !line.ends_with("/docs") && *line != "/project");
        }
        assert!(lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_by_type_dirs_only() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-type", "d"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project"));
        assert!(lines.contains(&"/project/src"));
        assert!(lines.contains(&"/project/docs"));
        assert!(!lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_with_maxdepth() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-maxdepth", "1"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project"));
        assert!(lines.contains(&"/project/src"));
        assert!(lines.contains(&"/project/docs"));
        assert!(lines.contains(&"/project/Cargo.toml"));
        // Should NOT contain deeper entries
        assert!(!lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_with_mindepth() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-mindepth", "2"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        // Should NOT contain depth 0 or 1
        assert!(!lines.contains(&"/project"));
        assert!(!lines.contains(&"/project/src"));
        // Should contain depth 2
        assert!(lines.contains(&"/project/src/main.rs"));
    }
    #[tokio::test]
    async fn test_find_with_depth_flag() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/d", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/d/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/d/sub/file.txt", b"x").await.unwrap();
        let ctx = make_ctx(fs, &["/d", "-depth"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        // In depth-first, children come before parents
        let file_pos = lines.iter().position(|l| *l == "/d/sub/file.txt").unwrap();
        let sub_pos = lines.iter().position(|l| *l == "/d/sub").unwrap();
        let d_pos = lines.iter().position(|l| *l == "/d").unwrap();
        assert!(file_pos < sub_pos);
        assert!(sub_pos < d_pos);
    }

    #[tokio::test]
    async fn test_find_empty_file() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/empty.txt", b"").await.unwrap();
        fs.write_file("/test/notempty.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-empty", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines, vec!["/test/empty.txt"]);
    }

    #[tokio::test]
    async fn test_find_empty_dir() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/test/emptydir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/test/notemptydir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/notemptydir/file.txt", b"x").await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-empty", "-type", "d"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines, vec!["/test/emptydir"]);
    }

    #[tokio::test]
    async fn test_find_with_size() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/small.txt", b"hi").await.unwrap();
        fs.write_file("/test/big.txt", &vec![b'x'; 2048]).await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-size", "+1k", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines, vec!["/test/big.txt"]);
    }
    #[tokio::test]
    async fn test_find_name_and_type_combined() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-name", "*.rs", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/project/src/lib.rs"));
        assert!(lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_with_or() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-name", "*.rs", "-o", "-name", "*.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project/src/main.rs"));
        assert!(lines.contains(&"/project/docs/readme.txt"));
    }

    #[tokio::test]
    async fn test_find_with_not() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-type", "f", "!", "-name", "*.rs"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project/Cargo.toml"));
        assert!(lines.contains(&"/project/docs/readme.txt"));
        assert!(!lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_with_print0() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/a.txt", b"a").await.unwrap();
        fs.write_file("/test/b.txt", b"b").await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-name", "*.txt", "-print0"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Output should be null-separated
        assert!(result.stdout.contains('\0'));
        let parts: Vec<&str> = result.stdout.split('\0').filter(|s| !s.is_empty()).collect();
        assert_eq!(parts.len(), 2);
    }
    #[tokio::test]
    async fn test_find_with_printf() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/hello.txt", b"hello").await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-name", "hello.txt", "-printf", "%f %s\\n"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello.txt 5\n");
    }

    #[tokio::test]
    async fn test_find_with_prune() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/test/skip", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/skip/hidden.txt", b"x").await.unwrap();
        fs.mkdir("/test/keep", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/keep/visible.txt", b"y").await.unwrap();
        let ctx = make_ctx(fs, &[
            "/test", "-name", "skip", "-prune", "-o", "-type", "f", "-print",
        ]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/test/keep/visible.txt"));
        assert!(!lines.contains(&"/test/skip/hidden.txt"));
    }

    #[tokio::test]
    async fn test_find_with_delete() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/a.txt", b"a").await.unwrap();
        fs.write_file("/test/b.rs", b"b").await.unwrap();
        let ctx = make_ctx(fs.clone(), &["/test", "-name", "*.txt", "-delete"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // a.txt should be deleted
        assert!(!fs.exists("/test/a.txt").await);
        // b.rs should still exist
        assert!(fs.exists("/test/b.rs").await);
    }

    #[tokio::test]
    async fn test_find_with_newer() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/old.txt", b"old").await.unwrap();
        // Set old.txt mtime to 10 days ago
        let old_time = SystemTime::now() - Duration::from_secs(10 * 86400);
        fs.utimes("/test/old.txt", old_time).await.unwrap();
        fs.write_file("/test/ref.txt", b"ref").await.unwrap();
        // Set ref.txt mtime to 5 days ago
        let ref_time = SystemTime::now() - Duration::from_secs(5 * 86400);
        fs.utimes("/test/ref.txt", ref_time).await.unwrap();
        fs.write_file("/test/new.txt", b"new").await.unwrap();
        // new.txt has current mtime (now)

        let ctx = make_ctx(fs, &["/test", "-newer", "/test/ref.txt", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/test/new.txt"));
        assert!(!lines.contains(&"/test/old.txt"));
    }
    #[tokio::test]
    async fn test_find_with_regex() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/test", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/test/file1.txt", b"a").await.unwrap();
        fs.write_file("/test/file2.rs", b"b").await.unwrap();
        fs.write_file("/test/data.json", b"c").await.unwrap();
        let ctx = make_ctx(fs, &["/test", "-regex", ".*\\.txt$"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines, vec!["/test/file1.txt"]);
    }

    #[tokio::test]
    async fn test_find_with_path_pattern() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-path", "*/src/*"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project/src/main.rs"));
        assert!(lines.contains(&"/project/src/lib.rs"));
        assert!(!lines.contains(&"/project/docs/readme.txt"));
    }

    #[tokio::test]
    async fn test_find_multiple_starting_paths() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/a", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/a/file1.txt", b"1").await.unwrap();
        fs.mkdir("/b", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/b/file2.txt", b"2").await.unwrap();
        let ctx = make_ctx(fs, &["/a", "/b", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/a/file1.txt"));
        assert!(lines.contains(&"/b/file2.txt"));
    }

    #[tokio::test]
    async fn test_find_default_path() {
        // When no path given, default to "."
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/cwd", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/cwd/test.txt", b"x").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-name".to_string(), "*.txt".to_string()],
            stdin: String::new(),
            cwd: "/cwd".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_find_nonexistent_path() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx(fs, &["/nonexistent"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_find_no_args_prints_everything() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/cwd", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/cwd/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/cwd/sub/file.txt", b"x").await.unwrap();
        let ctx = CommandContext {
            args: vec![],
            stdin: String::new(),
            cwd: "/cwd".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Should print everything under cwd
        assert!(result.stdout.contains("sub"));
        assert!(result.stdout.contains("file.txt"));
    }

    #[tokio::test]
    async fn test_find_iname_case_insensitive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/README.md", b"").await.unwrap();
        fs.write_file("/dir/readme.txt", b"").await.unwrap();
        fs.write_file("/dir/Readme.rst", b"").await.unwrap();
        fs.write_file("/dir/other.txt", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-iname", "readme*"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"/dir/README.md"));
        assert!(lines.contains(&"/dir/Readme.rst"));
        assert!(lines.contains(&"/dir/readme.txt"));
    }

    #[tokio::test]
    async fn test_find_iname_uppercase_pattern() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/config.json", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-iname", "CONFIG.JSON"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/config.json\n");
    }

    #[tokio::test]
    async fn test_find_ipath_case_insensitive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/Project/SRC", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/Project/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/Project/SRC/file.ts", b"").await.unwrap();
        fs.write_file("/Project/src/other.ts", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/Project", "-ipath", "*src*"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/Project/SRC"));
        assert!(lines.contains(&"/Project/SRC/file.ts"));
        assert!(lines.contains(&"/Project/src"));
        assert!(lines.contains(&"/Project/src/other.ts"));
    }

    #[tokio::test]
    async fn test_find_iregex_case_insensitive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/FILE.TXT", b"").await.unwrap();
        fs.write_file("/dir/file.txt", b"").await.unwrap();
        fs.write_file("/dir/other.js", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-iregex", ".*\\.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/FILE.TXT"));
        assert!(lines.contains(&"/dir/file.txt"));
    }

    #[tokio::test]
    async fn test_find_regex_complex_pattern() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/test1.ts", b"").await.unwrap();
        fs.write_file("/dir/test2.ts", b"").await.unwrap();
        fs.write_file("/dir/test10.ts", b"").await.unwrap();
        fs.write_file("/dir/other.ts", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-regex", ".*/test[0-9]\\.ts"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/test1.ts"));
        assert!(lines.contains(&"/dir/test2.ts"));
    }

    #[tokio::test]
    async fn test_find_maxdepth_0() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-maxdepth", "0"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/project\n");
    }

    #[tokio::test]
    async fn test_find_maxdepth_2_with_name() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-maxdepth", "2", "-name", "*.rs"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        // At depth 2, we should find files in /project/src/ but not deeper
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/project/src/lib.rs"));
        assert!(lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_mindepth_1_type_d() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-mindepth", "1", "-type", "d"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(!lines.contains(&"/project"));
        assert!(lines.contains(&"/project/src"));
        assert!(lines.contains(&"/project/docs"));
    }

    #[tokio::test]
    async fn test_find_mindepth_maxdepth_combined() {
        let fs = setup_basic_fs().await;
        let ctx = make_ctx(fs, &["/project", "-mindepth", "1", "-maxdepth", "1", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project/Cargo.toml"));
        assert!(!lines.contains(&"/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_size_bytes_exact() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/exact.txt", b"12345").await.unwrap();
        fs.write_file("/dir/other.txt", b"1234").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "5c"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/exact.txt\n");
    }

    #[tokio::test]
    async fn test_find_size_less_than_bytes() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/large.txt", &vec![b'x'; 1000]).await.unwrap();
        fs.write_file("/dir/small.txt", b"tiny").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "-100c"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/small.txt\n");
    }

    #[tokio::test]
    async fn test_find_size_greater_than_bytes() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/large.txt", &vec![b'x'; 1000]).await.unwrap();
        fs.write_file("/dir/small.txt", b"tiny").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "+100c"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/large.txt\n");
    }

    #[tokio::test]
    async fn test_find_size_megabytes() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/small.txt", b"tiny").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "-1M"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/small.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_0_today() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/today.txt", b"today").await.unwrap();
        fs.write_file("/dir/old.txt", b"old").await.unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(3 * 86400);
        fs.utimes("/dir/old.txt", old_time).await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-mtime", "0"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/today.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_plus_n() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/recent.txt", b"recent").await.unwrap();
        fs.write_file("/dir/old.txt", b"old").await.unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(10 * 86400);
        fs.utimes("/dir/old.txt", old_time).await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-mtime", "+7"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/old.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_minus_n() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/recent.txt", b"recent").await.unwrap();
        fs.write_file("/dir/old.txt", b"old").await.unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(10 * 86400);
        fs.utimes("/dir/old.txt", old_time).await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-mtime", "-7"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/recent.txt\n");
    }

    #[tokio::test]
    async fn test_find_newer_nonexistent_ref() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-newer", "/nonexistent.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_find_path_with_extension() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/data/pulls", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/data/pulls/1.json", b"{}").await.unwrap();
        fs.write_file("/data/pulls/2.txt", b"").await.unwrap();
        fs.write_file("/data/pulls/readme.md", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/data", "-path", "*/pulls/*.json"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/data/pulls/1.json\n");
    }

    #[tokio::test]
    async fn test_find_path_multiple_segments() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/a/src/lib", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/a/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/a/lib", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/a/src/lib/util.ts", b"").await.unwrap();
        fs.write_file("/a/src/util.ts", b"").await.unwrap();
        fs.write_file("/a/lib/util.ts", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/a", "-path", "*/src/lib/*", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/a/src/lib/util.ts\n");
    }

    #[tokio::test]
    async fn test_find_path_with_dot_prefix() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/project/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/project/lib", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/project/src/index.ts", b"").await.unwrap();
        fs.write_file("/project/src/utils.ts", b"").await.unwrap();
        fs.write_file("/project/lib/index.ts", b"").await.unwrap();
        let ctx = CommandContext {
            args: vec![".", "-path", "./src/*", "-type", "f"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            stdin: String::new(),
            cwd: "/project".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"./src/index.ts"));
        assert!(lines.contains(&"./src/utils.ts"));
    }

    #[tokio::test]
    async fn test_find_prune_multiple_dirs() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/node_modules/pkg", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/.git/objects", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/node_modules/pkg/index.js", b"").await.unwrap();
        fs.write_file("/dir/.git/objects/abc", b"").await.unwrap();
        fs.write_file("/dir/src/main.ts", b"").await.unwrap();
        let ctx = make_ctx(fs, &[
            "/dir", "(", "-name", "node_modules", "-o", "-name", ".git", ")", "-prune", "-o", "-type", "f", "-print",
        ]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/src/main.ts\n");
    }

    #[tokio::test]
    async fn test_find_prune_with_type_d() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/project/dist", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/project/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/project/dist/bundle.js", b"").await.unwrap();
        fs.write_file("/project/src/index.ts", b"").await.unwrap();
        fs.write_file("/project/README.md", b"").await.unwrap();
        let ctx = make_ctx(fs, &[
            "/project", "-type", "d", "-name", "dist", "-prune", "-o", "-type", "f", "-print",
        ]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/project/README.md"));
        assert!(lines.contains(&"/project/src/index.ts"));
        assert!(!lines.contains(&"/project/dist/bundle.js"));
    }

    #[tokio::test]
    async fn test_find_prune_without_print() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/skip", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/keep", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/skip/file.txt", b"").await.unwrap();
        fs.write_file("/dir/keep/file.txt", b"").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "skip", "-prune"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/skip\n");
    }

    #[tokio::test]
    async fn test_find_special_chars_spaces() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file with spaces.txt", b"content").await.unwrap();
        fs.write_file("/dir/normal.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "file with spaces.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/file with spaces.txt\n");
    }

    #[tokio::test]
    async fn test_find_special_chars_wildcard() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file with spaces.txt", b"content").await.unwrap();
        fs.write_file("/dir/another file.txt", b"content").await.unwrap();
        fs.write_file("/dir/normal.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "* *"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/another file.txt"));
        assert!(lines.contains(&"/dir/file with spaces.txt"));
    }

    #[tokio::test]
    async fn test_find_wildcard_question_mark() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/project", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/project/package.json", b"{}").await.unwrap();
        fs.write_file("/project/tsconfig.json", b"{}").await.unwrap();
        let ctx = make_ctx(fs, &["/project", "-name", "???*.json"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/project/package.json"));
        assert!(lines.contains(&"/project/tsconfig.json"));
    }

    #[tokio::test]
    async fn test_find_trailing_slash_in_path() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/project/src", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/project/src/index.ts", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["/project/", "-name", "*.ts"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            stdin: String::new(),
            cwd: "/project".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        };
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/project/src/index.ts\n");
    }

    #[tokio::test]
    async fn test_find_depth_first_multiple_branches() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/a", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/b", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/a/1.txt", b"1").await.unwrap();
        fs.write_file("/dir/b/2.txt", b"2").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-depth", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/a/1.txt"));
        assert!(lines.contains(&"/dir/b/2.txt"));
    }

    #[tokio::test]
    async fn test_find_or_operator_simple() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/file.rs", b"code").await.unwrap();
        fs.write_file("/dir/file.md", b"doc").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "*.txt", "-o", "-name", "*.rs"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/file.txt"));
        assert!(lines.contains(&"/dir/file.rs"));
    }

    #[tokio::test]
    async fn test_find_or_operator_with_type() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/subdir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-o", "-type", "d"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.len() >= 2);
        assert!(lines.contains(&"/dir/file.txt"));
        assert!(lines.contains(&"/dir/subdir"));
    }

    #[tokio::test]
    async fn test_find_and_operator_explicit() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/test.txt", b"content").await.unwrap();
        fs.write_file("/dir/test.rs", b"code").await.unwrap();
        fs.write_file("/dir/other.txt", b"other").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "test*", "-a", "-name", "*.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/test.txt\n");
    }

    #[tokio::test]
    async fn test_find_not_operator_with_name() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/file.rs", b"code").await.unwrap();
        fs.write_file("/dir/file.md", b"doc").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-not", "-name", "*.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/file.rs"));
        assert!(lines.contains(&"/dir/file.md"));
    }

    #[tokio::test]
    async fn test_find_negation_with_exclamation() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/file.rs", b"code").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "!", "-name", "*.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert!(lines.contains(&"/dir"));
        assert!(lines.contains(&"/dir/file.rs"));
        assert!(!lines.contains(&"/dir/file.txt"));
    }

    #[tokio::test]
    async fn test_find_empty_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/empty.txt", b"").await.unwrap();
        fs.write_file("/dir/nonempty.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-empty"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/empty.txt\n");
    }

    #[tokio::test]
    async fn test_find_empty_directories() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/empty", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/nonempty", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/nonempty/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "d", "-empty"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/empty\n");
    }

    #[tokio::test]
    async fn test_find_multiple_paths() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir1", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir2", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir1/file1.txt", b"1").await.unwrap();
        fs.write_file("/dir2/file2.txt", b"2").await.unwrap();
        let ctx = make_ctx(fs, &["/dir1", "/dir2", "-name", "*.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir1/file1.txt"));
        assert!(lines.contains(&"/dir2/file2.txt"));
    }

    #[tokio::test]
    async fn test_find_name_with_brackets() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file1.txt", b"1").await.unwrap();
        fs.write_file("/dir/file2.txt", b"2").await.unwrap();
        fs.write_file("/dir/file3.txt", b"3").await.unwrap();
        fs.write_file("/dir/other.txt", b"x").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "file[12].txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/file1.txt"));
        assert!(lines.contains(&"/dir/file2.txt"));
    }

    #[tokio::test]
    async fn test_find_name_with_negated_brackets() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file1.txt", b"1").await.unwrap();
        fs.write_file("/dir/file2.txt", b"2").await.unwrap();
        fs.write_file("/dir/file3.txt", b"3").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "file[!12].txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/file3.txt\n");
    }

    #[tokio::test]
    async fn test_find_maxdepth_0_only_root() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/sub/nested.txt", b"nested").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-maxdepth", "0"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir\n");
    }

    #[tokio::test]
    async fn test_find_maxdepth_1() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/sub/nested.txt", b"nested").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-maxdepth", "1", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/file.txt\n");
    }

    #[tokio::test]
    async fn test_find_mindepth_1() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/sub/nested.txt", b"nested").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mindepth", "1"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"/dir/file.txt"));
        assert!(lines.contains(&"/dir/sub"));
        assert!(lines.contains(&"/dir/sub/nested.txt"));
    }

    #[tokio::test]
    async fn test_find_mindepth_2() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/sub/nested.txt", b"nested").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mindepth", "2", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/sub/nested.txt\n");
    }

    #[tokio::test]
    async fn test_find_size_zero() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/empty.txt", b"").await.unwrap();
        fs.write_file("/dir/nonempty.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "0"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/empty.txt\n");
    }

    #[tokio::test]
    async fn test_find_size_kilobytes() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        let large_content = vec![b'x'; 2048];
        fs.write_file("/dir/large.txt", &large_content).await.unwrap();
        fs.write_file("/dir/small.txt", b"small").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-size", "+1k"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/large.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_zero() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/recent.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mtime", "0", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/recent.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_negative() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mtime", "-1", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/file.txt\n");
    }

    #[tokio::test]
    async fn test_find_mtime_positive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mtime", "+1", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_find_regex_anchored() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/test.txt", b"content").await.unwrap();
        fs.write_file("/dir/other.txt", b"other").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-regex", ".*/test\\.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/test.txt\n");
    }

    #[tokio::test]
    async fn test_find_regex_with_alternation() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/file.rs", b"code").await.unwrap();
        fs.write_file("/dir/file.md", b"doc").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-regex", ".*\\.(txt|rs)"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/file.txt"));
        assert!(lines.contains(&"/dir/file.rs"));
    }

    #[tokio::test]
    async fn test_find_iregex_readme_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/README.md", b"doc").await.unwrap();
        fs.write_file("/dir/readme.txt", b"doc").await.unwrap();
        fs.write_file("/dir/other.md", b"other").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-iregex", ".*/readme.*"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/README.md"));
        assert!(lines.contains(&"/dir/readme.txt"));
    }

    #[tokio::test]
    async fn test_find_complex_or_and_combination() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/test.txt", b"content").await.unwrap();
        fs.write_file("/dir/test.rs", b"code").await.unwrap();
        fs.write_file("/dir/other.txt", b"other").await.unwrap();
        fs.write_file("/dir/other.rs", b"other").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "test*", "-o", "-name", "*.rs"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 3);
        assert!(lines.contains(&"/dir/test.txt"));
        assert!(lines.contains(&"/dir/test.rs"));
        assert!(lines.contains(&"/dir/other.rs"));
    }

    #[tokio::test]
    async fn test_find_name_case_sensitive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/File.txt", b"content").await.unwrap();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-name", "file.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/file.txt\n");
    }

    #[tokio::test]
    async fn test_find_path_case_sensitive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/Sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.mkdir("/dir/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/Sub/file.txt", b"content").await.unwrap();
        fs.write_file("/dir/sub/file.txt", b"content").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-path", "*/sub/*"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/sub/file.txt\n");
    }

    #[tokio::test]
    async fn test_find_combined_depth_constraints() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/a/b/c", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/1.txt", b"1").await.unwrap();
        fs.write_file("/dir/a/2.txt", b"2").await.unwrap();
        fs.write_file("/dir/a/b/3.txt", b"3").await.unwrap();
        fs.write_file("/dir/a/b/c/4.txt", b"4").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-mindepth", "2", "-maxdepth", "3", "-type", "f"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/a/2.txt"));
        assert!(lines.contains(&"/dir/a/b/3.txt"));
    }

    #[tokio::test]
    async fn test_find_size_with_type_filter() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir/subdir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/small.txt", b"x").await.unwrap();
        let large_content = vec![b'y'; 100];
        fs.write_file("/dir/large.txt", &large_content).await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-type", "f", "-size", "+10c"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/dir/large.txt\n");
    }

    #[tokio::test]
    async fn test_find_regex_digit_pattern() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/file1.txt", b"1").await.unwrap();
        fs.write_file("/dir/file2.txt", b"2").await.unwrap();
        fs.write_file("/dir/fileA.txt", b"a").await.unwrap();
        let ctx = make_ctx(fs, &["/dir", "-regex", ".*/file[0-9]\\.txt"]);
        let cmd = FindCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let lines = sorted_lines(&result.stdout);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"/dir/file1.txt"));
        assert!(lines.contains(&"/dir/file2.txt"));
    }
}
