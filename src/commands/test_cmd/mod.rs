// src/commands/test_cmd/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TestCommand;

#[async_trait]
impl Command for TestCommand {
    fn name(&self) -> &'static str {
        "test"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;

        // 空参数返回 1
        if args.is_empty() {
            return CommandResult::with_exit_code(String::new(), String::new(), 1);
        }

        // 处理 [ ] 语法
        let args: Vec<&str> = if !args.is_empty() && args[0] == "[" {
            if args.last().map(|s| s.as_str()) != Some("]") {
                return CommandResult::error("test: missing ']'\n".to_string());
            }
            args[1..args.len()-1].iter().map(|s| s.as_str()).collect()
        } else {
            args.iter().map(|s| s.as_str()).collect()
        };

        if args.is_empty() {
            return CommandResult::with_exit_code(String::new(), String::new(), 1);
        }

        let result = evaluate_expression(&args, &ctx).await;
        let exit_code = if result { 0 } else { 1 };
        CommandResult::with_exit_code(String::new(), String::new(), exit_code)
    }
}

async fn evaluate_expression(args: &[&str], ctx: &CommandContext) -> bool {
    // 单个参数：非空字符串为真
    if args.len() == 1 {
        return !args[0].is_empty();
    }

    // 处理 ! 取反
    if args[0] == "!" {
        return !Box::pin(evaluate_expression(&args[1..], ctx)).await;
    }

    // 二元操作符
    if args.len() >= 3 {
        // 查找操作符
        for i in 1..args.len() {
            let op = args[i];
            match op {
                // 逻辑操作符
                "-a" => {
                    let left = Box::pin(evaluate_expression(&args[..i], ctx)).await;
                    let right = Box::pin(evaluate_expression(&args[i+1..], ctx)).await;
                    return left && right;
                }
                "-o" => {
                    let left = Box::pin(evaluate_expression(&args[..i], ctx)).await;
                    let right = Box::pin(evaluate_expression(&args[i+1..], ctx)).await;
                    return left || right;
                }
                _ => {}
            }
        }
    }

    // 二元表达式
    if args.len() == 3 {
        let left = args[0];
        let op = args[1];
        let right = args[2];

        match op {
            // 字符串比较
            "=" | "==" => return left == right,
            "!=" => return left != right,

            // 数值比较
            "-eq" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l == r;
            }
            "-ne" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l != r;
            }
            "-lt" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l < r;
            }
            "-le" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l <= r;
            }
            "-gt" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l > r;
            }
            "-ge" => {
                let l: i64 = left.parse().unwrap_or(0);
                let r: i64 = right.parse().unwrap_or(0);
                return l >= r;
            }
            _ => {}
        }
    }

    // 一元表达式
    if args.len() == 2 {
        let op = args[0];
        let operand = args[1];

        match op {
            // 字符串测试
            "-z" => return operand.is_empty(),
            "-n" => return !operand.is_empty(),

            // 文件测试
            "-e" => {
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                return ctx.fs.exists(&path).await;
            }
            "-f" => {
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                if let Ok(stat) = ctx.fs.stat(&path).await {
                    return stat.is_file;
                }
                return false;
            }
            "-d" => {
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                if let Ok(stat) = ctx.fs.stat(&path).await {
                    return stat.is_directory;
                }
                return false;
            }
            "-s" => {
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                if let Ok(stat) = ctx.fs.stat(&path).await {
                    return stat.size > 0;
                }
                return false;
            }
            "-r" | "-w" | "-x" => {
                // 简化：只检查文件是否存在
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                return ctx.fs.exists(&path).await;
            }
            "-L" | "-h" => {
                let path = ctx.fs.resolve_path(&ctx.cwd, operand);
                if let Ok(stat) = ctx.fs.lstat(&path).await {
                    return stat.is_symlink;
                }
                return false;
            }
            _ => {}
        }
    }

    false
}

// 同时提供 [ 命令
pub struct BracketCommand;

#[async_trait]
impl Command for BracketCommand {
    fn name(&self) -> &'static str {
        "["
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        // 检查最后一个参数是否为 ]
        if ctx.args.last().map(|s| s.as_str()) != Some("]") {
            return CommandResult::error("[: missing ']'\n".to_string());
        }

        // 移除 ] 后调用 test 逻辑
        let args: Vec<&str> = ctx.args[..ctx.args.len()-1].iter().map(|s| s.as_str()).collect();

        if args.is_empty() {
            return CommandResult::with_exit_code(String::new(), String::new(), 1);
        }

        let result = evaluate_expression(&args, &ctx).await;
        let exit_code = if result { 0 } else { 1 };
        CommandResult::with_exit_code(String::new(), String::new(), exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
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
    async fn test_empty_args() {
        let ctx = make_ctx(vec![]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_single_arg_nonempty() {
        let ctx = make_ctx(vec!["hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_string_equal() {
        let ctx = make_ctx(vec!["hello", "=", "hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_string_not_equal() {
        let ctx = make_ctx(vec!["hello", "!=", "world"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_eq() {
        let ctx = make_ctx(vec!["5", "-eq", "5"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_lt() {
        let ctx = make_ctx(vec!["3", "-lt", "5"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_z_empty() {
        let ctx = make_ctx(vec!["-z", ""]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_n_nonempty() {
        let ctx = make_ctx(vec!["-n", "hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_file_exists() {
        let ctx = make_ctx_with_files(vec!["-e", "/test.txt"], vec![("/test.txt", "content")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_file_not_exists() {
        let ctx = make_ctx(vec!["-e", "/nonexistent"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_is_file() {
        let ctx = make_ctx_with_files(vec!["-f", "/test.txt"], vec![("/test.txt", "content")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_negation() {
        let ctx = make_ctx(vec!["!", "-z", "hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_and() {
        let ctx = make_ctx(vec!["-n", "a", "-a", "-n", "b"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_or() {
        let ctx = make_ctx(vec!["-z", "a", "-o", "-n", "b"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_single_arg_empty() {
        let ctx = make_ctx(vec![""]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_z_nonempty() {
        let ctx = make_ctx(vec!["-z", "hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_n_empty() {
        let ctx = make_ctx(vec!["-n", ""]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_is_directory() {
        let ctx = make_ctx_with_files(vec!["-d", "/dir"], vec![("/dir/file.txt", "content")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_is_not_directory() {
        let ctx = make_ctx_with_files(vec!["-d", "/file.txt"], vec![("/file.txt", "content")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_file_size_nonempty() {
        let ctx = make_ctx_with_files(vec!["-s", "/file.txt"], vec![("/file.txt", "content")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_file_size_empty() {
        let ctx = make_ctx_with_files(vec!["-s", "/empty.txt"], vec![("/empty.txt", "")]).await;
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_numeric_ne() {
        let ctx = make_ctx(vec!["5", "-ne", "6"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_le() {
        let ctx = make_ctx(vec!["5", "-le", "5"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_gt() {
        let ctx = make_ctx(vec!["5", "-gt", "3"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_ge() {
        let ctx = make_ctx(vec!["5", "-ge", "5"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_and_both_false() {
        let ctx = make_ctx(vec!["-z", "a", "-a", "-z", "b"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_or_both_false() {
        let ctx = make_ctx(vec!["-f", "/nonexistent1", "-o", "-f", "/nonexistent2"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_bracket_with_closing() {
        let ctx = make_ctx(vec!["[", "-n", "hello", "]"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_bracket_missing_closing() {
        let ctx = make_ctx(vec!["[", "-n", "hello"]);
        let cmd = TestCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("missing ']'"));
    }

    #[tokio::test]
    async fn test_bracket_command_with_closing() {
        let ctx = make_ctx(vec!["-f", "/file.txt", "]"]);
        let cmd = BracketCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_bracket_command_missing_closing() {
        let ctx = make_ctx(vec!["-f", "/file.txt"]);
        let cmd = BracketCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("missing ']'"));
    }

    #[tokio::test]
    async fn test_bracket_empty() {
        let ctx = make_ctx(vec!["]"]);
        let cmd = BracketCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }
}
