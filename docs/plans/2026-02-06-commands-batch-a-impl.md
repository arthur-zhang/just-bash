# Commands 模块批次 A 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 TypeScript just-bash 项目的 commands 模块迁移到 Rust，实现批次 A 的 14 个基础命令。

**Architecture:** 创建独立的 `src/commands/` 模块，使用 async trait 定义命令接口，通过 clap 解析参数，复用现有的 FileSystem trait。

**Tech Stack:** Rust, async-trait, clap, regex-lite (已有依赖)

---

## Task 1: 添加 clap 依赖

**Files:**
- Modify: `Cargo.toml`

**Step 1: 添加 clap 依赖**

在 `[dependencies]` 部分添加：
```toml
clap = { version = "4.4", features = ["derive"] }
```

**Step 2: 验证依赖**

Run: `cargo check`
Expected: 编译成功，无错误

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add clap dependency for command argument parsing"
```

---

## Task 2: 创建 commands 模块基础结构

**Files:**
- Create: `src/commands/mod.rs`
- Create: `src/commands/types.rs`
- Modify: `src/lib.rs`

**Step 1: 创建 types.rs**

```rust
// src/commands/types.rs
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use crate::fs::FileSystem;

/// 命令执行结果
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandResult {
    pub fn success(stdout: String) -> Self {
        Self { stdout, stderr: String::new(), exit_code: 0 }
    }

    pub fn error(stderr: String) -> Self {
        Self { stdout: String::new(), stderr, exit_code: 1 }
    }

    pub fn with_exit_code(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }
}

/// 命令执行上下文
pub struct CommandContext {
    pub args: Vec<String>,
    pub stdin: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub fs: Arc<dyn FileSystem>,
}

/// 命令 trait
#[async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: CommandContext) -> CommandResult;
}
```

**Step 2: 创建 mod.rs**

```rust
// src/commands/mod.rs
pub mod types;

pub use types::{Command, CommandContext, CommandResult};
```

**Step 3: 更新 lib.rs**

在 `src/lib.rs` 中添加：
```rust
pub mod commands;
pub use commands::{Command, CommandContext, CommandResult};
```

**Step 4: 验证编译**

Run: `cargo check`
Expected: 编译成功

**Step 5: Commit**

```bash
git add src/commands/mod.rs src/commands/types.rs src/lib.rs
git commit -m "feat(commands): add Command trait and basic types"
```

---

## Task 3: 创建命令注册表

**Files:**
- Create: `src/commands/registry.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 registry.rs**

```rust
// src/commands/registry.rs
use std::collections::HashMap;
use super::types::Command;

pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.insert(cmd.name().to_string(), cmd);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(|c| c.as_ref())
    }

    pub fn names(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: 更新 mod.rs**

```rust
// src/commands/mod.rs
pub mod registry;
pub mod types;

pub use registry::CommandRegistry;
pub use types::{Command, CommandContext, CommandResult};
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译成功

**Step 4: Commit**

```bash
git add src/commands/registry.rs src/commands/mod.rs
git commit -m "feat(commands): add CommandRegistry for command management"
```

---

## Task 4: 实现 basename 命令

**Files:**
- Create: `src/commands/basename/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 basename 目录和实现**

```rust
// src/commands/basename/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct BasenameCommand;

#[async_trait]
impl Command for BasenameCommand {
    fn name(&self) -> &'static str {
        "basename"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        
        // 检查 --help
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: basename NAME [SUFFIX]\n       basename OPTION... NAME...\n\n\
                 Strip directory and suffix from filenames.\n\n\
                 Options:\n\
                   -a, --multiple   support multiple arguments\n\
                   -s, --suffix=SUFFIX  remove a trailing SUFFIX\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut multiple = false;
        let mut suffix = String::new();
        let mut names: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "-a" || arg == "--multiple" {
                multiple = true;
            } else if arg == "-s" && i + 1 < args.len() {
                i += 1;
                suffix = args[i].clone();
                multiple = true;
            } else if let Some(s) = arg.strip_prefix("--suffix=") {
                suffix = s.to_string();
                multiple = true;
            } else if !arg.starts_with('-') {
                names.push(arg.clone());
            }
            i += 1;
        }

        if names.is_empty() {
            return CommandResult::error("basename: missing operand\n".to_string());
        }

        // 如果不是 multiple 模式，第二个参数是 suffix
        if !multiple && names.len() >= 2 {
            suffix = names.pop().unwrap();
        }

        let results: Vec<String> = names
            .iter()
            .map(|name| {
                // 移除尾部斜杠
                let clean_name = name.trim_end_matches('/');
                let mut base = clean_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(clean_name)
                    .to_string();
                
                // 移除后缀
                if !suffix.is_empty() && base.ends_with(&suffix) && base.len() > suffix.len() {
                    base = base[..base.len() - suffix.len()].to_string();
                }
                base
            })
            .collect();

        CommandResult::success(format!("{}\n", results.join("\n")))
    }
}
```

**Step 2: 更新 mod.rs 添加 basename 模块**

在 `src/commands/mod.rs` 中添加：
```rust
pub mod basename;
```

**Step 3: 编写测试**

在 `src/commands/basename/mod.rs` 底部添加：
```rust
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
    async fn test_basename_simple() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/sort"])).await;
        assert_eq!(result.stdout, "sort\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_with_suffix() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["include/stdio.h", ".h"])).await;
        assert_eq!(result.stdout, "stdio\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_trailing_slash() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/"])).await;
        assert_eq!(result.stdout, "usr\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_basename_missing_operand() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec![])).await;
        assert_eq!(result.stderr, "basename: missing operand\n");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_basename_multiple() {
        let cmd = BasenameCommand;
        let result = cmd.execute(make_ctx(vec!["-a", "/usr/bin/sort", "/usr/bin/ls"])).await;
        assert_eq!(result.stdout, "sort\nls\n");
        assert_eq!(result.exit_code, 0);
    }
}
```

**Step 4: 运行测试**

Run: `cargo test basename --`
Expected: 所有测试通过

**Step 5: Commit**

```bash
git add src/commands/basename/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement basename command"
```

---

## Task 5: 实现 dirname 命令

**Files:**
- Create: `src/commands/dirname/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 dirname 实现**

```rust
// src/commands/dirname/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct DirnameCommand;

#[async_trait]
impl Command for DirnameCommand {
    fn name(&self) -> &'static str {
        "dirname"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: dirname [OPTION] NAME...\n\n\
                 Strip last component from file name.\n\n\
                 Options:\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let names: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();

        if names.is_empty() {
            return CommandResult::error("dirname: missing operand\n".to_string());
        }

        let results: Vec<String> = names
            .iter()
            .map(|name| {
                // 移除尾部斜杠
                let clean_name = name.trim_end_matches('/');
                match clean_name.rfind('/') {
                    None => ".".to_string(),
                    Some(0) => "/".to_string(),
                    Some(pos) => clean_name[..pos].to_string(),
                }
            })
            .collect();

        CommandResult::success(format!("{}\n", results.join("\n")))
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
    async fn test_dirname_simple() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/sort"])).await;
        assert_eq!(result.stdout, "/usr/bin\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_no_slash() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["stdio.h"])).await;
        assert_eq!(result.stdout, ".\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_root() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr"])).await;
        assert_eq!(result.stdout, "/\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_trailing_slash() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/usr/bin/"])).await;
        assert_eq!(result.stdout, "/usr\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_dirname_missing_operand() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec![])).await;
        assert_eq!(result.stderr, "dirname: missing operand\n");
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_dirname_multiple() {
        let cmd = DirnameCommand;
        let result = cmd.execute(make_ctx(vec!["/a/b", "/c/d"])).await;
        assert_eq!(result.stdout, "/a\n/c\n");
        assert_eq!(result.exit_code, 0);
    }
}
```

**Step 2: 更新 mod.rs**

在 `src/commands/mod.rs` 中添加：
```rust
pub mod dirname;
```

**Step 3: 运行测试**

Run: `cargo test dirname --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/dirname/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement dirname command"
```

---

## Task 6: 实现 cat 命令

**Files:**
- Create: `src/commands/cat/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 cat 实现**

```rust
// src/commands/cat/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct CatCommand;

#[async_trait]
impl Command for CatCommand {
    fn name(&self) -> &'static str {
        "cat"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: cat [OPTION]... [FILE]...\n\n\
                 Concatenate FILE(s) to standard output.\n\n\
                 Options:\n\
                   -n, --number     number all output lines\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut show_line_numbers = false;
        let mut files: Vec<String> = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-n" | "--number" => show_line_numbers = true,
                _ if !arg.starts_with('-') || arg == "-" => files.push(arg.clone()),
                _ => {}
            }
        }

        // 如果没有文件，从 stdin 读取
        if files.is_empty() {
            files.push("-".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let mut line_number = 1;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("cat: {}: No such file or directory\n", file));
                        exit_code = 1;
                        continue;
                    }
                }
            };

            if show_line_numbers {
                let (numbered, next_line) = add_line_numbers(&content, line_number);
                stdout.push_str(&numbered);
                line_number = next_line;
            } else {
                stdout.push_str(&content);
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

fn add_line_numbers(content: &str, start_line: usize) -> (String, usize) {
    let lines: Vec<&str> = content.split('\n').collect();
    let has_trailing_newline = content.ends_with('\n');
    let lines_to_number = if has_trailing_newline {
        &lines[..lines.len() - 1]
    } else {
        &lines[..]
    };

    let numbered: Vec<String> = lines_to_number
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{}", start_line + i, line))
        .collect();

    let result = if has_trailing_newline {
        format!("{}\n", numbered.join("\n"))
    } else {
        numbered.join("\n")
    };

    (result, start_line + lines_to_number.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
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
        }
    }

    #[tokio::test]
    async fn test_cat_single_file() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello world\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "aaa\nbbb\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_with_line_numbers() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/test.txt"],
            vec![("/test.txt", "line1\nline2\n")],
        ).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "     1\tline1\n     2\tline2\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cat_file_not_found() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt"], vec![]).await;
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_cat_stdin() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["-".to_string()],
            stdin: "from stdin\n".to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = CatCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "from stdin\n");
        assert_eq!(result.exit_code, 0);
    }
}
```

**Step 2: 更新 mod.rs**

在 `src/commands/mod.rs` 中添加：
```rust
pub mod cat;
```

**Step 3: 运行测试**

Run: `cargo test cat --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/cat/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement cat command with line numbering"
```

---

## Task 7: 实现 head 和 tail 共享工具

**Files:**
- Create: `src/commands/utils/mod.rs`
- Create: `src/commands/utils/head_tail.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 utils 模块**

```rust
// src/commands/utils/mod.rs
pub mod head_tail;

pub use head_tail::*;
```

**Step 2: 创建 head_tail.rs**

```rust
// src/commands/utils/head_tail.rs
use crate::commands::{CommandContext, CommandResult};

#[derive(Debug, Clone)]
pub struct HeadTailOptions {
    pub lines: usize,
    pub bytes: Option<usize>,
    pub quiet: bool,
    pub verbose: bool,
    pub files: Vec<String>,
    pub from_line: bool, // tail +N 语法
}

impl Default for HeadTailOptions {
    fn default() -> Self {
        Self {
            lines: 10,
            bytes: None,
            quiet: false,
            verbose: false,
            files: Vec::new(),
            from_line: false,
        }
    }
}

pub enum HeadTailParseResult {
    Ok(HeadTailOptions),
    Err(CommandResult),
}

pub fn parse_head_tail_args(args: &[String], cmd_name: &str) -> HeadTailParseResult {
    let mut opts = HeadTailOptions::default();
    let is_tail = cmd_name == "tail";

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-n" && i + 1 < args.len() {
            i += 1;
            let next_arg = &args[i];
            if is_tail && next_arg.starts_with('+') {
                opts.from_line = true;
                opts.lines = next_arg[1..].parse().unwrap_or(10);
            } else {
                opts.lines = next_arg.parse().unwrap_or(10);
            }
        } else if is_tail && arg.starts_with("-n+") {
            opts.from_line = true;
            opts.lines = arg[3..].parse().unwrap_or(10);
        } else if arg.starts_with("-n") && arg.len() > 2 {
            opts.lines = arg[2..].parse().unwrap_or(10);
        } else if arg == "-c" && i + 1 < args.len() {
            i += 1;
            opts.bytes = args[i].parse().ok();
        } else if arg.starts_with("-c") && arg.len() > 2 {
            opts.bytes = arg[2..].parse().ok();
        } else if arg.starts_with("--bytes=") {
            opts.bytes = arg[8..].parse().ok();
        } else if arg.starts_with("--lines=") {
            opts.lines = arg[8..].parse().unwrap_or(10);
        } else if arg == "-q" || arg == "--quiet" || arg == "--silent" {
            opts.quiet = true;
        } else if arg == "-v" || arg == "--verbose" {
            opts.verbose = true;
        } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit()) {
            opts.lines = arg[1..].parse().unwrap_or(10);
        } else if arg.starts_with("--") {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: unrecognized option '{}'\n", cmd_name, arg)
            ));
        } else if arg.starts_with('-') && arg != "-" {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: invalid option -- '{}'\n", cmd_name, &arg[1..])
            ));
        } else {
            opts.files.push(arg.clone());
        }
        i += 1;
    }

    // 验证 bytes
    if let Some(bytes) = opts.bytes {
        if bytes == 0 {
            return HeadTailParseResult::Err(CommandResult::error(
                format!("{}: invalid number of bytes\n", cmd_name)
            ));
        }
    }

    HeadTailParseResult::Ok(opts)
}

pub async fn process_head_tail_files<F>(
    ctx: &CommandContext,
    opts: &HeadTailOptions,
    cmd_name: &str,
    processor: F,
) -> CommandResult
where
    F: Fn(&str) -> String,
{
    // 如果没有文件，从 stdin 读取
    if opts.files.is_empty() {
        return CommandResult::success(processor(&ctx.stdin));
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    let show_headers = opts.verbose || (!opts.quiet && opts.files.len() > 1);
    let mut files_processed = 0;

    for file in &opts.files {
        let path = ctx.fs.resolve_path(&ctx.cwd, file);
        match ctx.fs.read_file(&path).await {
            Ok(content) => {
                if show_headers {
                    if files_processed > 0 {
                        stdout.push('\n');
                    }
                    stdout.push_str(&format!("==> {} <==\n", file));
                }
                stdout.push_str(&processor(&content));
                files_processed += 1;
            }
            Err(_) => {
                stderr.push_str(&format!("{}: {}: No such file or directory\n", cmd_name, file));
                exit_code = 1;
            }
        }
    }

    CommandResult::with_exit_code(stdout, stderr, exit_code)
}

pub fn get_head(content: &str, lines: usize, bytes: Option<usize>) -> String {
    if let Some(b) = bytes {
        return content.chars().take(b).collect();
    }

    if lines == 0 {
        return String::new();
    }

    let mut pos = 0;
    let mut line_count = 0;
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();

    while pos < len && line_count < lines {
        if chars[pos] == '\n' {
            line_count += 1;
        }
        pos += 1;
    }

    if pos > 0 {
        chars[..pos].iter().collect()
    } else {
        String::new()
    }
}

pub fn get_tail(content: &str, lines: usize, bytes: Option<usize>, from_line: bool) -> String {
    if let Some(b) = bytes {
        let chars: Vec<char> = content.chars().collect();
        let start = chars.len().saturating_sub(b);
        return chars[start..].iter().collect();
    }

    let len = content.len();
    if len == 0 {
        return String::new();
    }

    // +N 语法：从第 N 行开始
    if from_line {
        let mut pos = 0;
        let mut line_count = 1;
        let chars: Vec<char> = content.chars().collect();
        while pos < chars.len() && line_count < lines {
            if chars[pos] == '\n' {
                line_count += 1;
            }
            pos += 1;
        }
        let result: String = chars[pos..].iter().collect();
        if result.ends_with('\n') {
            result
        } else {
            format!("{}\n", result)
        }
    } else {
        if lines == 0 {
            return String::new();
        }

        // 从后向前扫描
        let chars: Vec<char> = content.chars().collect();
        let mut pos = chars.len();
        if pos > 0 && chars[pos - 1] == '\n' {
            pos -= 1;
        }

        let mut line_count = 0;
        while pos > 0 && line_count < lines {
            pos -= 1;
            if chars[pos] == '\n' {
                line_count += 1;
                if line_count == lines {
                    pos += 1;
                    break;
                }
            }
        }

        let result: String = chars[pos..].iter().collect();
        if content.ends_with('\n') {
            result
        } else {
            format!("{}\n", result)
        }
    }
}
```

**Step 3: 更新 commands/mod.rs**

添加：
```rust
pub mod utils;
```

**Step 4: 运行测试**

Run: `cargo check`
Expected: 编译成功

**Step 5: Commit**

```bash
git add src/commands/utils/mod.rs src/commands/utils/head_tail.rs src/commands/mod.rs
git commit -m "feat(commands): add head/tail shared utilities"
```

---

## Task 8: 实现 head 命令

**Files:**
- Create: `src/commands/head/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 head 实现**

```rust
// src/commands/head/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::utils::{parse_head_tail_args, process_head_tail_files, get_head, HeadTailParseResult};

pub struct HeadCommand;

#[async_trait]
impl Command for HeadCommand {
    fn name(&self) -> &'static str {
        "head"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: head [OPTION]... [FILE]...\n\n\
                 Print the first 10 lines of each FILE to standard output.\n\n\
                 Options:\n\
                   -c, --bytes=NUM    print the first NUM bytes\n\
                   -n, --lines=NUM    print the first NUM lines (default 10)\n\
                   -q, --quiet        never print headers giving file names\n\
                   -v, --verbose      always print headers giving file names\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let opts = match parse_head_tail_args(&ctx.args, "head") {
            HeadTailParseResult::Ok(o) => o,
            HeadTailParseResult::Err(e) => return e,
        };

        let lines = opts.lines;
        let bytes = opts.bytes;

        process_head_tail_files(&ctx, &opts, "head", |content| {
            get_head(content, lines, bytes)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
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
        }
    }

    #[tokio::test]
    async fn test_head_default() {
        let content = (1..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        let expected = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_head_n5() {
        let content = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "5", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        let expected = (1..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_head_bytes() {
        let ctx = make_ctx_with_files(vec!["-c", "5", "/test.txt"], vec![("/test.txt", "hello world\n")]).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "hello");
    }

    #[tokio::test]
    async fn test_head_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\n")],
        ).await;
        let cmd = HeadCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("==> /a.txt <=="));
        assert!(result.stdout.contains("==> /b.txt <=="));
        assert!(result.stdout.contains("aaa"));
        assert!(result.stdout.contains("bbb"));
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod head;
```

**Step 3: 运行测试**

Run: `cargo test head --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/head/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement head command"
```

---

## Task 9: 实现 tail 命令

**Files:**
- Create: `src/commands/tail/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 tail 实现**

```rust
// src/commands/tail/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::commands::utils::{parse_head_tail_args, process_head_tail_files, get_tail, HeadTailParseResult};

pub struct TailCommand;

#[async_trait]
impl Command for TailCommand {
    fn name(&self) -> &'static str {
        "tail"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: tail [OPTION]... [FILE]...\n\n\
                 Print the last 10 lines of each FILE to standard output.\n\n\
                 Options:\n\
                   -c, --bytes=NUM    print the last NUM bytes\n\
                   -n, --lines=NUM    print the last NUM lines (default 10)\n\
                   -n +NUM            print starting from line NUM\n\
                   -q, --quiet        never print headers giving file names\n\
                   -v, --verbose      always print headers giving file names\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let opts = match parse_head_tail_args(&ctx.args, "tail") {
            HeadTailParseResult::Ok(o) => o,
            HeadTailParseResult::Err(e) => return e,
        };

        let lines = opts.lines;
        let bytes = opts.bytes;
        let from_line = opts.from_line;

        process_head_tail_files(&ctx, &opts, "tail", |content| {
            get_tail(content, lines, bytes, from_line)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
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
        }
    }

    #[tokio::test]
    async fn test_tail_default() {
        let content = (1..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (6..=15).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_n3() {
        let content = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "3", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (8..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_from_line() {
        let content = (1..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        let ctx = make_ctx_with_files(vec!["-n", "+3", "/test.txt"], vec![("/test.txt", &content)]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        let expected = (3..=5).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n") + "\n";
        assert_eq!(result.stdout, expected);
    }

    #[tokio::test]
    async fn test_tail_bytes() {
        let ctx = make_ctx_with_files(vec!["-c", "5", "/test.txt"], vec![("/test.txt", "hello world\n")]).await;
        let cmd = TailCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout, "orld\n");
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod tail;
```

**Step 3: 运行测试**

Run: `cargo test tail --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/tail/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement tail command"
```

---

## Task 10: 实现 wc 命令

**Files:**
- Create: `src/commands/wc/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 wc 实现**

```rust
// src/commands/wc/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct WcCommand;

#[derive(Default)]
struct Stats {
    lines: usize,
    words: usize,
    chars: usize,
}

fn count_stats(content: &str) -> Stats {
    let mut stats = Stats::default();
    let mut in_word = false;

    for c in content.chars() {
        stats.chars += 1;
        if c == '\n' {
            stats.lines += 1;
            if in_word {
                stats.words += 1;
                in_word = false;
            }
        } else if c == ' ' || c == '\t' || c == '\r' {
            if in_word {
                stats.words += 1;
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }

    if in_word {
        stats.words += 1;
    }

    stats
}

#[async_trait]
impl Command for WcCommand {
    fn name(&self) -> &'static str {
        "wc"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: wc [OPTION]... [FILE]...\n\n\
                 Print newline, word, and byte counts for each FILE.\n\n\
                 Options:\n\
                   -c, --bytes    print the byte counts\n\
                   -m, --chars    print the character counts\n\
                   -l, --lines    print the newline counts\n\
                   -w, --words    print the word counts\n\
                       --help     display this help and exit\n".to_string()
            );
        }

        let mut show_lines = false;
        let mut show_words = false;
        let mut show_chars = false;
        let mut files: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-l" | "--lines" => show_lines = true,
                "-w" | "--words" => show_words = true,
                "-c" | "--bytes" | "-m" | "--chars" => show_chars = true,
                _ if !arg.starts_with('-') => files.push(arg.clone()),
                _ => {}
            }
        }

        // 如果没有指定任何标志，显示全部
        if !show_lines && !show_words && !show_chars {
            show_lines = true;
            show_words = true;
            show_chars = true;
        }

        if files.is_empty() {
            files.push("-".to_string());
        }

        let mut all_stats: Vec<(Stats, Option<String>)> = Vec::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("wc: {}: No such file or directory\n", file));
                        exit_code = 1;
                        continue;
                    }
                }
            };

            let stats = count_stats(&content);
            let filename = if file == "-" { None } else { Some(file.clone()) };
            all_stats.push((stats, filename));
        }

        // 计算最大宽度用于对齐
        let mut max_lines = 0;
        let mut max_words = 0;
        let mut max_chars = 0;
        for (stats, _) in &all_stats {
            max_lines = max_lines.max(stats.lines);
            max_words = max_words.max(stats.words);
            max_chars = max_chars.max(stats.chars);
        }

        let width = if all_stats.len() > 1 { 7 } else { 0 };
        let width = width
            .max(max_lines.to_string().len())
            .max(max_words.to_string().len())
            .max(max_chars.to_string().len());

        let mut stdout = String::new();
        let mut total = Stats::default();

        for (stats, filename) in &all_stats {
            let mut parts: Vec<String> = Vec::new();
            if show_lines {
                parts.push(format!("{:>width$}", stats.lines, width = width));
            }
            if show_words {
                parts.push(format!("{:>width$}", stats.words, width = width));
            }
            if show_chars {
                parts.push(format!("{:>width$}", stats.chars, width = width));
            }

            let line = if let Some(name) = filename {
                format!("{} {}\n", parts.join(" "), name)
            } else {
                format!("{}\n", parts.join(" "))
            };
            stdout.push_str(&line);

            total.lines += stats.lines;
            total.words += stats.words;
            total.chars += stats.chars;
        }

        // 如果有多个文件，显示总计
        if all_stats.len() > 1 {
            let mut parts: Vec<String> = Vec::new();
            if show_lines {
                parts.push(format!("{:>width$}", total.lines, width = width));
            }
            if show_words {
                parts.push(format!("{:>width$}", total.words, width = width));
            }
            if show_chars {
                parts.push(format!("{:>width$}", total.chars, width = width));
            }
            stdout.push_str(&format!("{} total\n", parts.join(" ")));
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
        }
    }

    #[tokio::test]
    async fn test_wc_all() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "hello world\nfoo bar\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        // 2 lines, 4 words, 20 chars
        assert!(result.stdout.contains("2"));
        assert!(result.stdout.contains("4"));
        assert!(result.stdout.contains("20"));
    }

    #[tokio::test]
    async fn test_wc_lines_only() {
        let ctx = make_ctx_with_files(
            vec!["-l", "/test.txt"],
            vec![("/test.txt", "line1\nline2\nline3\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.trim().starts_with("3"));
    }

    #[tokio::test]
    async fn test_wc_multiple_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "aaa\n"), ("/b.txt", "bbb\nccc\n")],
        ).await;
        let cmd = WcCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("total"));
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod wc;
```

**Step 3: 运行测试**

Run: `cargo test wc --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/wc/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement wc command"
```

---

## Task 11: 实现 mkdir 命令

**Files:**
- Create: `src/commands/mkdir/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 mkdir 实现**

```rust
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
```

**Step 2: 更新 mod.rs**

```rust
pub mod mkdir;
```

**Step 3: 运行测试**

Run: `cargo test mkdir --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/mkdir/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement mkdir command"
```

---

## Task 12: 实现 touch 命令

**Files:**
- Create: `src/commands/touch/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 touch 实现**

```rust
// src/commands/touch/mod.rs
use async_trait::async_trait;
use std::time::SystemTime;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TouchCommand;

fn parse_date_string(date_str: &str) -> Option<SystemTime> {
    // 简化的日期解析，支持 YYYY-MM-DD 和 YYYY-MM-DD HH:MM:SS
    let normalized = date_str.replace('/', "-");
    
    // 尝试解析 YYYY-MM-DD
    if let Some(caps) = regex_lite::Regex::new(r"^(\d{4})-(\d{2})-(\d{2})$")
        .ok()
        .and_then(|re| re.captures(&normalized))
    {
        let year: i32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        let day: u32 = caps.get(3)?.as_str().parse().ok()?;
        
        // 简化：使用 chrono 或手动计算
        // 这里使用简化的方法
        let days_since_epoch = days_from_date(year, month, day)?;
        let secs = days_since_epoch as u64 * 86400;
        return Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
    }

    // 尝试解析 YYYY-MM-DD HH:MM:SS
    if let Some(caps) = regex_lite::Regex::new(r"^(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})$")
        .ok()
        .and_then(|re| re.captures(&normalized))
    {
        let year: i32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        let day: u32 = caps.get(3)?.as_str().parse().ok()?;
        let hour: u32 = caps.get(4)?.as_str().parse().ok()?;
        let min: u32 = caps.get(5)?.as_str().parse().ok()?;
        let sec: u32 = caps.get(6)?.as_str().parse().ok()?;
        
        let days_since_epoch = days_from_date(year, month, day)?;
        let secs = days_since_epoch as u64 * 86400 + hour as u64 * 3600 + min as u64 * 60 + sec as u64;
        return Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
    }

    None
}

fn days_from_date(year: i32, month: u32, day: u32) -> Option<i64> {
    // 简化的日期计算（从 1970-01-01 开始）
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }
    
    let mut days: i64 = 0;
    
    // 计算年份贡献的天数
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for y in (year..1970).rev() {
        days -= if is_leap_year(y) { 366 } else { 365 };
    }
    
    // 计算月份贡献的天数
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        days += days_in_month[(m - 1) as usize] as i64;
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    
    days += (day - 1) as i64;
    
    Some(days)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

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
        let mut date_str: Option<String> = None;
        let mut no_create = false;

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            
            if arg == "--" {
                files.extend(ctx.args[i + 1..].iter().cloned());
                break;
            } else if arg == "-d" || arg == "--date" {
                if i + 1 >= ctx.args.len() {
                    return CommandResult::error("touch: option requires an argument -- 'd'\n".to_string());
                }
                i += 1;
                date_str = Some(ctx.args[i].clone());
            } else if let Some(d) = arg.strip_prefix("--date=") {
                date_str = Some(d.to_string());
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
                            if i + 1 >= ctx.args.len() {
                                return CommandResult::error("touch: option requires an argument -- 'd'\n".to_string());
                            }
                            i += 1;
                            date_str = Some(ctx.args[i].clone());
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

        let target_time = if let Some(ds) = &date_str {
            match parse_date_string(ds) {
                Some(t) => t,
                None => {
                    return CommandResult::error(format!("touch: invalid date format '{}'\n", ds));
                }
            }
        } else {
            SystemTime::now()
        };

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
```

**Step 2: 更新 mod.rs**

```rust
pub mod touch;
```

**Step 3: 运行测试**

Run: `cargo test touch --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/touch/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement touch command"
```

---

## Task 13: 实现 rm 命令

**Files:**
- Create: `src/commands/rm/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 rm 实现**

```rust
// src/commands/rm/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::RmOptions;

pub struct RmCommand;

#[async_trait]
impl Command for RmCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: rm [OPTION]... [FILE]...\n\n\
                 Remove (unlink) the FILE(s).\n\n\
                 Options:\n\
                   -f, --force      ignore nonexistent files and arguments\n\
                   -r, -R, --recursive  remove directories and their contents recursively\n\
                   -v, --verbose    explain what is being done\n\
                       --help       display this help and exit\n".to_string()
            );
        }

        let mut recursive = false;
        let mut force = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-f" | "--force" => force = true,
                "-v" | "--verbose" => verbose = true,
                "-rf" | "-fr" => {
                    recursive = true;
                    force = true;
                }
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.is_empty() {
            if force {
                return CommandResult::success(String::new());
            }
            return CommandResult::error("rm: missing operand\n".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for path in &paths {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, path);
            
            // 检查是否为目录
            match ctx.fs.stat(&full_path).await {
                Ok(stat) => {
                    if stat.is_directory && !recursive {
                        stderr.push_str(&format!("rm: cannot remove '{}': Is a directory\n", path));
                        exit_code = 1;
                        continue;
                    }
                }
                Err(_) => {
                    if !force {
                        stderr.push_str(&format!("rm: cannot remove '{}': No such file or directory\n", path));
                        exit_code = 1;
                    }
                    continue;
                }
            }

            let opts = RmOptions { recursive, force };
            match ctx.fs.rm(&full_path, &opts).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("removed '{}'\n", path));
                    }
                }
                Err(e) => {
                    if !force {
                        let msg = format!("{:?}", e);
                        if msg.contains("NotEmpty") {
                            stderr.push_str(&format!("rm: cannot remove '{}': Directory not empty\n", path));
                        } else {
                            stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, msg));
                        }
                        exit_code = 1;
                    }
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
        }
    }

    #[tokio::test]
    async fn test_rm_file() {
        let ctx = make_ctx_with_files(vec!["/test.txt"], vec![("/test.txt", "content")]).await;
        let fs = ctx.fs.clone();
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/test.txt").await);
    }

    #[tokio::test]
    async fn test_rm_nonexistent() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt"], vec![]).await;
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_rm_force_nonexistent() {
        let ctx = make_ctx_with_files(vec!["-f", "/nonexistent.txt"], vec![]).await;
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_rm_directory_without_r() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/testdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("Is a directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_rm_recursive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/testdir/file.txt", b"content").await.unwrap();
        let ctx = CommandContext {
            args: vec!["-r".to_string(), "/testdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = RmCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/testdir").await);
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod rm;
```

**Step 3: 运行测试**

Run: `cargo test rm --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/rm/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement rm command"
```

---

## Task 14: 实现 cp 命令

**Files:**
- Create: `src/commands/cp/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 cp 实现**

```rust
// src/commands/cp/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::CpOptions;

pub struct CpCommand;

#[async_trait]
impl Command for CpCommand {
    fn name(&self) -> &'static str {
        "cp"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: cp [OPTION]... SOURCE... DEST\n\n\
                 Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.\n\n\
                 Options:\n\
                   -r, -R, --recursive  copy directories recursively\n\
                   -n, --no-clobber     do not overwrite an existing file\n\
                   -v, --verbose        explain what is being done\n\
                       --help           display this help and exit\n".to_string()
            );
        }

        let mut recursive = false;
        let mut no_clobber = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-n" | "--no-clobber" => no_clobber = true,
                "-v" | "--verbose" => verbose = true,
                "-p" | "--preserve" => {} // 接受但忽略
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.len() < 2 {
            return CommandResult::error("cp: missing destination file operand\n".to_string());
        }

        let dest = paths.pop().unwrap();
        let sources = paths;
        let dest_path = ctx.fs.resolve_path(&ctx.cwd, &dest);

        // 检查目标是否为目录
        let dest_is_dir = match ctx.fs.stat(&dest_path).await {
            Ok(stat) => stat.is_directory,
            Err(_) => false,
        };

        // 如果有多个源，目标必须是目录
        if sources.len() > 1 && !dest_is_dir {
            return CommandResult::error(format!(
                "cp: target '{}' is not a directory\n",
                dest
            ));
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for src in &sources {
            let src_path = ctx.fs.resolve_path(&ctx.cwd, src);

            // 检查源是否存在
            let src_stat = match ctx.fs.stat(&src_path).await {
                Ok(s) => s,
                Err(_) => {
                    stderr.push_str(&format!("cp: cannot stat '{}': No such file or directory\n", src));
                    exit_code = 1;
                    continue;
                }
            };

            // 如果是目录但没有 -r
            if src_stat.is_directory && !recursive {
                stderr.push_str(&format!("cp: -r not specified; omitting directory '{}'\n", src));
                exit_code = 1;
                continue;
            }

            // 确定目标路径
            let target_path = if dest_is_dir {
                let basename = src.rsplit('/').next().unwrap_or(src);
                ctx.fs.resolve_path(&dest_path, basename)
            } else {
                dest_path.clone()
            };

            // 检查 no_clobber
            if no_clobber && ctx.fs.exists(&target_path).await {
                continue;
            }

            let opts = CpOptions { recursive };
            match ctx.fs.cp(&src_path, &target_path, &opts).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("'{}' -> '{}'\n", src, target_path));
                    }
                }
                Err(e) => {
                    stderr.push_str(&format!("cp: cannot copy '{}': {:?}\n", src, e));
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
    use crate::fs::{InMemoryFs, MkdirOptions};
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
        }
    }

    #[tokio::test]
    async fn test_cp_file() {
        let ctx = make_ctx_with_files(
            vec!["/src.txt", "/dest.txt"],
            vec![("/src.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_cp_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/src.txt", b"content").await.unwrap();
        fs.mkdir("/destdir", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src.txt".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/destdir/src.txt").await);
    }

    #[tokio::test]
    async fn test_cp_directory_without_r() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/srcdir", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/srcdir".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("omitting directory"));
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_cp_no_clobber() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "new"), ("/dest.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = CpCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "old"); // 未被覆盖
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod cp;
```

**Step 3: 运行测试**

Run: `cargo test cp --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/cp/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement cp command"
```

---

## Task 15: 实现 mv 命令

**Files:**
- Create: `src/commands/mv/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 mv 实现**

```rust
// src/commands/mv/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct MvCommand;

#[async_trait]
impl Command for MvCommand {
    fn name(&self) -> &'static str {
        "mv"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: mv [OPTION]... SOURCE... DEST\n\n\
                 Rename SOURCE to DEST, or move SOURCE(s) to DIRECTORY.\n\n\
                 Options:\n\
                   -f, --force        do not prompt before overwriting\n\
                   -n, --no-clobber   do not overwrite an existing file\n\
                   -v, --verbose      explain what is being done\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let mut no_clobber = false;
        let mut verbose = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-f" | "--force" => {} // 接受但忽略（默认行为）
                "-n" | "--no-clobber" => no_clobber = true,
                "-v" | "--verbose" => verbose = true,
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.len() < 2 {
            return CommandResult::error("mv: missing destination file operand\n".to_string());
        }

        let dest = paths.pop().unwrap();
        let sources = paths;
        let dest_path = ctx.fs.resolve_path(&ctx.cwd, &dest);

        // 检查目标是否为目录
        let dest_is_dir = match ctx.fs.stat(&dest_path).await {
            Ok(stat) => stat.is_directory,
            Err(_) => false,
        };

        // 如果有多个源，目标必须是目录
        if sources.len() > 1 && !dest_is_dir {
            return CommandResult::error(format!(
                "mv: target '{}' is not a directory\n",
                dest
            ));
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for src in &sources {
            let src_path = ctx.fs.resolve_path(&ctx.cwd, src);

            // 检查源是否存在
            if !ctx.fs.exists(&src_path).await {
                stderr.push_str(&format!("mv: cannot stat '{}': No such file or directory\n", src));
                exit_code = 1;
                continue;
            }

            // 确定目标路径
            let target_path = if dest_is_dir {
                let basename = src.rsplit('/').next().unwrap_or(src);
                ctx.fs.resolve_path(&dest_path, basename)
            } else {
                dest_path.clone()
            };

            // 检查 no_clobber
            if no_clobber && ctx.fs.exists(&target_path).await {
                continue;
            }

            match ctx.fs.mv(&src_path, &target_path).await {
                Ok(()) => {
                    if verbose {
                        stdout.push_str(&format!("renamed '{}' -> '{}'\n", src, target_path));
                    }
                }
                Err(e) => {
                    stderr.push_str(&format!("mv: cannot move '{}': {:?}\n", src, e));
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
    use crate::fs::{InMemoryFs, MkdirOptions};
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
        }
    }

    #[tokio::test]
    async fn test_mv_rename() {
        let ctx = make_ctx_with_files(
            vec!["/old.txt", "/new.txt"],
            vec![("/old.txt", "content")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/old.txt").await);
        assert!(fs.exists("/new.txt").await);
    }

    #[tokio::test]
    async fn test_mv_to_directory() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/src.txt", b"content").await.unwrap();
        fs.mkdir("/destdir", &MkdirOptions { recursive: false }).await.unwrap();
        let ctx = CommandContext {
            args: vec!["/src.txt".to_string(), "/destdir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
        };
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!fs.exists("/src.txt").await);
        assert!(fs.exists("/destdir/src.txt").await);
    }

    #[tokio::test]
    async fn test_mv_no_clobber() {
        let ctx = make_ctx_with_files(
            vec!["-n", "/src.txt", "/dest.txt"],
            vec![("/src.txt", "new"), ("/dest.txt", "old")],
        ).await;
        let fs = ctx.fs.clone();
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/src.txt").await); // 源文件仍存在
        let content = fs.read_file("/dest.txt").await.unwrap();
        assert_eq!(content, "old"); // 目标未被覆盖
    }

    #[tokio::test]
    async fn test_mv_nonexistent() {
        let ctx = make_ctx_with_files(vec!["/nonexistent.txt", "/dest.txt"], vec![]).await;
        let cmd = MvCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 1);
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod mv;
```

**Step 3: 运行测试**

Run: `cargo test mv --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/mv/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement mv command"
```

---

## Task 16: 实现 ls 命令

**Files:**
- Create: `src/commands/ls/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 ls 实现**

```rust
// src/commands/ls/mod.rs
use async_trait::async_trait;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::commands::{Command, CommandContext, CommandResult};

pub struct LsCommand;

fn format_mode(mode: u32, is_dir: bool, is_link: bool) -> String {
    let file_type = if is_link { 'l' } else if is_dir { 'd' } else { '-' };
    
    let perms = [
        if mode & 0o400 != 0 { 'r' } else { '-' },
        if mode & 0o200 != 0 { 'w' } else { '-' },
        if mode & 0o100 != 0 { 'x' } else { '-' },
        if mode & 0o040 != 0 { 'r' } else { '-' },
        if mode & 0o020 != 0 { 'w' } else { '-' },
        if mode & 0o010 != 0 { 'x' } else { '-' },
        if mode & 0o004 != 0 { 'r' } else { '-' },
        if mode & 0o002 != 0 { 'w' } else { '-' },
        if mode & 0o001 != 0 { 'x' } else { '-' },
    ];
    
    format!("{}{}", file_type, perms.iter().collect::<String>())
}

fn format_size(size: u64, human_readable: bool) -> String {
    if !human_readable {
        return size.to_string();
    }
    
    if size < 1024 {
        return size.to_string();
    }
    if size < 1024 * 1024 {
        let k = size as f64 / 1024.0;
        return if k < 10.0 { format!("{:.1}K", k) } else { format!("{}K", k as u64) };
    }
    if size < 1024 * 1024 * 1024 {
        let m = size as f64 / (1024.0 * 1024.0);
        return if m < 10.0 { format!("{:.1}M", m) } else { format!("{}M", m as u64) };
    }
    let g = size as f64 / (1024.0 * 1024.0 * 1024.0);
    if g < 10.0 { format!("{:.1}G", g) } else { format!("{}G", g as u64) }
}

fn format_time(mtime: SystemTime) -> String {
    let duration = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    
    // 简化的时间格式化
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    
    // 计算日期（简化版）
    let days_since_epoch = secs / 86400;
    let year = 1970 + (days_since_epoch / 365) as i32; // 简化，不考虑闰年
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30).min(11) as usize;
    let day = (day_of_year % 30) + 1;
    
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let six_months_ago = now_secs.saturating_sub(180 * 86400);
    
    if secs > six_months_ago {
        format!("{} {:>2} {:02}:{:02}", months[month], day, hour, minute)
    } else {
        format!("{} {:>2}  {}", months[month], day, year)
    }
}

#[async_trait]
impl Command for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: ls [OPTION]... [FILE]...\n\n\
                 List directory contents.\n\n\
                 Options:\n\
                   -a, --all          do not ignore entries starting with .\n\
                   -A, --almost-all   do not list implied . and ..\n\
                   -l                 use a long listing format\n\
                   -h, --human-readable  with -l, print sizes in human readable format\n\
                   -r, --reverse      reverse order while sorting\n\
                   -R, --recursive    list subdirectories recursively\n\
                   -S                 sort by file size, largest first\n\
                   -t                 sort by time, newest first\n\
                   -d, --directory    list directories themselves, not their contents\n\
                       --help         display this help and exit\n".to_string()
            );
        }

        let mut show_all = false;
        let mut show_almost_all = false;
        let mut long_format = false;
        let mut human_readable = false;
        let mut reverse = false;
        let mut recursive = false;
        let mut sort_by_size = false;
        let mut sort_by_time = false;
        let mut list_dir_itself = false;
        let mut paths: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-a" | "--all" => show_all = true,
                "-A" | "--almost-all" => show_almost_all = true,
                "-l" => long_format = true,
                "-h" | "--human-readable" => human_readable = true,
                "-r" | "--reverse" => reverse = true,
                "-R" | "--recursive" => recursive = true,
                "-S" => sort_by_size = true,
                "-t" => sort_by_time = true,
                "-d" | "--directory" => list_dir_itself = true,
                "-la" | "-al" => { long_format = true; show_all = true; }
                "-lh" | "-hl" => { long_format = true; human_readable = true; }
                _ if !arg.starts_with('-') => paths.push(arg.clone()),
                _ => {}
            }
        }

        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let show_path_header = paths.len() > 1 || recursive;

        for (idx, path) in paths.iter().enumerate() {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, path);
            
            let stat = match ctx.fs.stat(&full_path).await {
                Ok(s) => s,
                Err(_) => {
                    stderr.push_str(&format!("ls: cannot access '{}': No such file or directory\n", path));
                    exit_code = 2;
                    continue;
                }
            };

            if !stat.is_directory || list_dir_itself {
                // 列出单个文件
                if long_format {
                    let mode_str = format_mode(stat.mode, stat.is_directory, stat.is_symlink);
                    let size_str = format_size(stat.size, human_readable);
                    let time_str = format_time(stat.mtime);
                    stdout.push_str(&format!("{} 1 user user {:>5} {} {}\n",
                        mode_str, size_str, time_str, path));
                } else {
                    stdout.push_str(&format!("{}\n", path));
                }
                continue;
            }

            // 列出目录内容
            if show_path_header {
                if idx > 0 { stdout.push('\n'); }
                stdout.push_str(&format!("{}:\n", path));
            }

            let entries = match ctx.fs.readdir_with_file_types(&full_path).await {
                Ok(e) => e,
                Err(_) => {
                    stderr.push_str(&format!("ls: cannot open directory '{}'\n", path));
                    exit_code = 2;
                    continue;
                }
            };

            let mut filtered: Vec<_> = entries
                .into_iter()
                .filter(|e| {
                    if show_all { return true; }
                    if show_almost_all { return !e.name.starts_with('.') || (e.name != "." && e.name != ".."); }
                    !e.name.starts_with('.')
                })
                .collect();

            // 排序
            if sort_by_size || sort_by_time {
                // 需要获取 stat 信息进行排序
                let mut with_stats: Vec<_> = Vec::new();
                for entry in filtered {
                    let entry_path = ctx.fs.resolve_path(&full_path, &entry.name);
                    let stat = ctx.fs.stat(&entry_path).await.ok();
                    with_stats.push((entry, stat));
                }
                
                if sort_by_size {
                    with_stats.sort_by(|a, b| {
                        let size_a = a.1.as_ref().map(|s| s.size).unwrap_or(0);
                        let size_b = b.1.as_ref().map(|s| s.size).unwrap_or(0);
                        size_b.cmp(&size_a)
                    });
                } else if sort_by_time {
                    with_stats.sort_by(|a, b| {
                        let time_a = a.1.as_ref().map(|s| s.mtime).unwrap_or(UNIX_EPOCH);
                        let time_b = b.1.as_ref().map(|s| s.mtime).unwrap_or(UNIX_EPOCH);
                        time_b.cmp(&time_a)
                    });
                }
                
                if reverse {
                    with_stats.reverse();
                }
                
                for (entry, stat_opt) in with_stats {
                    if long_format {
                        if let Some(stat) = stat_opt {
                            let mode_str = format_mode(stat.mode, entry.is_directory, entry.is_symlink);
                            let size_str = format_size(stat.size, human_readable);
                            let time_str = format_time(stat.mtime);
                            stdout.push_str(&format!("{} 1 user user {:>5} {} {}\n",
                                mode_str, size_str, time_str, entry.name));
                        }
                    } else {
                        stdout.push_str(&format!("{}\n", entry.name));
                    }
                }
            } else {
                // 按名称排序
                filtered.sort_by(|a, b| a.name.cmp(&b.name));
                if reverse {
                    filtered.reverse();
                }
                
                for entry in filtered {
                    if long_format {
                        let entry_path = ctx.fs.resolve_path(&full_path, &entry.name);
                        if let Ok(stat) = ctx.fs.stat(&entry_path).await {
                            let mode_str = format_mode(stat.mode, entry.is_directory, entry.is_symlink);
                            let size_str = format_size(stat.size, human_readable);
                            let time_str = format_time(stat.mtime);
                            stdout.push_str(&format!("{} 1 user user {:>5} {} {}\n",
                                mode_str, size_str, time_str, entry.name));
                        }
                    } else {
                        stdout.push_str(&format!("{}\n", entry.name));
                    }
                }
            }
        }

        CommandResult::with_exit_code(stdout, stderr, exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{InMemoryFs, MkdirOptions};
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx_with_structure(args: Vec<&str>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/testdir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/testdir/file1.txt", b"content1").await.unwrap();
        fs.write_file("/testdir/file2.txt", b"content2content2").await.unwrap();
        fs.write_file("/testdir/.hidden", b"hidden").await.unwrap();
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    #[tokio::test]
    async fn test_ls_basic() {
        let ctx = make_ctx_with_structure(vec!["/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("file1.txt"));
        assert!(result.stdout.contains("file2.txt"));
        assert!(!result.stdout.contains(".hidden")); // 默认不显示隐藏文件
    }

    #[tokio::test]
    async fn test_ls_all() {
        let ctx = make_ctx_with_structure(vec!["-a", "/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains(".hidden"));
    }

    #[tokio::test]
    async fn test_ls_long() {
        let ctx = make_ctx_with_structure(vec!["-l", "/testdir"]).await;
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("rw")); // 权限信息
    }

    #[tokio::test]
    async fn test_ls_nonexistent() {
        let fs = Arc::new(InMemoryFs::new());
        let ctx = CommandContext {
            args: vec!["/nonexistent".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        };
        let cmd = LsCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stderr.contains("No such file or directory"));
        assert_eq!(result.exit_code, 2);
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod ls;
```

**Step 3: 运行测试**

Run: `cargo test ls --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/ls/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement ls command with long format support"
```

---

## Task 17: 实现 grep 命令

**Files:**
- Create: `src/commands/grep/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 grep 实现**

```rust
// src/commands/grep/mod.rs
use async_trait::async_trait;
use regex_lite::Regex;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct GrepCommand;

struct GrepOptions {
    pattern: String,
    ignore_case: bool,
    invert_match: bool,
    count_only: bool,
    files_with_matches: bool,
    files_without_matches: bool,
    line_number: bool,
    only_matching: bool,
    quiet: bool,
    fixed_strings: bool,
    extended_regexp: bool,
    word_regexp: bool,
    line_regexp: bool,
    max_count: Option<usize>,
    after_context: usize,
    before_context: usize,
    recursive: bool,
    files: Vec<String>,
}

impl Default for GrepOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            ignore_case: false,
            invert_match: false,
            count_only: false,
            files_with_matches: false,
            files_without_matches: false,
            line_number: false,
            only_matching: false,
            quiet: false,
            fixed_strings: false,
            extended_regexp: false,
            word_regexp: false,
            line_regexp: false,
            max_count: None,
            after_context: 0,
            before_context: 0,
            recursive: false,
            files: Vec::new(),
        }
    }
}

fn parse_grep_args(args: &[String]) -> Result<GrepOptions, String> {
    let mut opts = GrepOptions::default();
    let mut positional: Vec<String> = Vec::new();
    
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        
        if arg == "-e" && i + 1 < args.len() {
            i += 1;
            opts.pattern = args[i].clone();
        } else if arg == "-i" || arg == "--ignore-case" {
            opts.ignore_case = true;
        } else if arg == "-v" || arg == "--invert-match" {
            opts.invert_match = true;
        } else if arg == "-c" || arg == "--count" {
            opts.count_only = true;
        } else if arg == "-l" || arg == "--files-with-matches" {
            opts.files_with_matches = true;
        } else if arg == "-L" || arg == "--files-without-match" {
            opts.files_without_matches = true;
        } else if arg == "-n" || arg == "--line-number" {
            opts.line_number = true;
        } else if arg == "-o" || arg == "--only-matching" {
            opts.only_matching = true;
        } else if arg == "-q" || arg == "--quiet" || arg == "--silent" {
            opts.quiet = true;
        } else if arg == "-F" || arg == "--fixed-strings" {
            opts.fixed_strings = true;
        } else if arg == "-E" || arg == "--extended-regexp" {
            opts.extended_regexp = true;
        } else if arg == "-w" || arg == "--word-regexp" {
            opts.word_regexp = true;
        } else if arg == "-x" || arg == "--line-regexp" {
            opts.line_regexp = true;
        } else if arg == "-r" || arg == "-R" || arg == "--recursive" {
            opts.recursive = true;
        } else if arg == "-m" && i + 1 < args.len() {
            i += 1;
            opts.max_count = args[i].parse().ok();
        } else if let Some(n) = arg.strip_prefix("-m") {
            opts.max_count = n.parse().ok();
        } else if arg == "-A" && i + 1 < args.len() {
            i += 1;
            opts.after_context = args[i].parse().unwrap_or(0);
        } else if let Some(n) = arg.strip_prefix("-A") {
            opts.after_context = n.parse().unwrap_or(0);
        } else if arg == "-B" && i + 1 < args.len() {
            i += 1;
            opts.before_context = args[i].parse().unwrap_or(0);
        } else if let Some(n) = arg.strip_prefix("-B") {
            opts.before_context = n.parse().unwrap_or(0);
        } else if arg == "-C" && i + 1 < args.len() {
            i += 1;
            let n = args[i].parse().unwrap_or(0);
            opts.before_context = n;
            opts.after_context = n;
        } else if let Some(n) = arg.strip_prefix("-C") {
            let n: usize = n.parse().unwrap_or(0);
            opts.before_context = n;
            opts.after_context = n;
        } else if !arg.starts_with('-') {
            positional.push(arg.clone());
        }
        i += 1;
    }
    
    // 第一个位置参数是 pattern（如果没有用 -e 指定）
    if opts.pattern.is_empty() {
        if positional.is_empty() {
            return Err("grep: no pattern specified".to_string());
        }
        opts.pattern = positional.remove(0);
    }
    
    opts.files = positional;
    Ok(opts)
}

fn build_regex(opts: &GrepOptions) -> Result<Regex, String> {
    let mut pattern = opts.pattern.clone();
    
    // 固定字符串模式：转义所有正则特殊字符
    if opts.fixed_strings {
        pattern = regex_lite::escape(&pattern);
    }
    
    // 全词匹配
    if opts.word_regexp {
        pattern = format!(r"\b{}\b", pattern);
    }
    
    // 全行匹配
    if opts.line_regexp {
        pattern = format!("^{}$", pattern);
    }
    
    // 忽略大小写
    if opts.ignore_case {
        pattern = format!("(?i){}", pattern);
    }
    
    Regex::new(&pattern).map_err(|e| format!("grep: invalid pattern: {}", e))
}

#[async_trait]
impl Command for GrepCommand {
    fn name(&self) -> &'static str {
        "grep"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: grep [OPTION]... PATTERN [FILE]...\n\n\
                 Search for PATTERN in each FILE.\n\n\
                 Options:\n\
                   -E, --extended-regexp  PATTERN is an extended regular expression\n\
                   -F, --fixed-strings    PATTERN is a set of newline-separated strings\n\
                   -i, --ignore-case      ignore case distinctions\n\
                   -v, --invert-match     select non-matching lines\n\
                   -w, --word-regexp      match only whole words\n\
                   -x, --line-regexp      match only whole lines\n\
                   -c, --count            print only a count of matching lines\n\
                   -l, --files-with-matches  print only names of FILEs with matches\n\
                   -L, --files-without-match  print only names of FILEs without matches\n\
                   -n, --line-number      print line number with output lines\n\
                   -o, --only-matching    show only the part of a line matching PATTERN\n\
                   -q, --quiet            suppress all normal output\n\
                   -r, -R, --recursive    search directories recursively\n\
                   -A NUM                 print NUM lines of trailing context\n\
                   -B NUM                 print NUM lines of leading context\n\
                   -C NUM                 print NUM lines of output context\n\
                   -m NUM, --max-count=NUM  stop after NUM matches\n\
                       --help             display this help and exit\n".to_string()
            );
        }

        let opts = match parse_grep_args(&ctx.args) {
            Ok(o) => o,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        let regex = match build_regex(&opts) {
            Ok(r) => r,
            Err(e) => return CommandResult::error(format!("{}\n", e)),
        };

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 1; // 默认没有匹配
        let mut total_matches = 0;

        // 如果没有文件，从 stdin 读取
        let files = if opts.files.is_empty() {
            vec!["-".to_string()]
        } else {
            opts.files.clone()
        };

        let show_filename = files.len() > 1;

        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => {
                        stderr.push_str(&format!("grep: {}: No such file or directory\n", file));
                        continue;
                    }
                }
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = 0;
            let mut matched_lines: Vec<(usize, &str)> = Vec::new();

            for (line_num, line) in lines.iter().enumerate() {
                let matches = regex.is_match(line);
                let should_output = if opts.invert_match { !matches } else { matches };

                if should_output {
                    file_matches += 1;
                    total_matches += 1;
                    matched_lines.push((line_num + 1, line));

                    if let Some(max) = opts.max_count {
                        if file_matches >= max {
                            break;
                        }
                    }
                }
            }

            if file_matches > 0 {
                exit_code = 0;
            }

            if opts.quiet {
                if file_matches > 0 {
                    return CommandResult::with_exit_code(String::new(), stderr, 0);
                }
                continue;
            }

            if opts.files_with_matches {
                if file_matches > 0 {
                    stdout.push_str(&format!("{}\n", file));
                }
                continue;
            }

            if opts.files_without_matches {
                if file_matches == 0 {
                    stdout.push_str(&format!("{}\n", file));
                }
                continue;
            }

            if opts.count_only {
                if show_filename {
                    stdout.push_str(&format!("{}:{}\n", file, file_matches));
                } else {
                    stdout.push_str(&format!("{}\n", file_matches));
                }
                continue;
            }

            // 输出匹配的行
            for (line_num, line) in matched_lines {
                let prefix = if show_filename {
                    if opts.line_number {
                        format!("{}:{}:", file, line_num)
                    } else {
                        format!("{}:", file)
                    }
                } else if opts.line_number {
                    format!("{}:", line_num)
                } else {
                    String::new()
                };

                if opts.only_matching {
                    for mat in regex.find_iter(line) {
                        stdout.push_str(&format!("{}{}\n", prefix, mat.as_str()));
                    }
                } else {
                    stdout.push_str(&format!("{}{}\n", prefix, line));
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
        }
    }

    #[tokio::test]
    async fn test_grep_basic() {
        let ctx = make_ctx_with_files(
            vec!["hello", "/test.txt"],
            vec![("/test.txt", "hello world\nfoo bar\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("hello world"));
        assert!(result.stdout.contains("hello again"));
        assert!(!result.stdout.contains("foo bar"));
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_grep_ignore_case() {
        let ctx = make_ctx_with_files(
            vec!["-i", "HELLO", "/test.txt"],
            vec![("/test.txt", "Hello World\nhello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("Hello World"));
        assert!(result.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn test_grep_invert() {
        let ctx = make_ctx_with_files(
            vec!["-v", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "world");
    }

    #[tokio::test]
    async fn test_grep_count() {
        let ctx = make_ctx_with_files(
            vec!["-c", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "2");
    }

    #[tokio::test]
    async fn test_grep_line_number() {
        let ctx = make_ctx_with_files(
            vec!["-n", "hello", "/test.txt"],
            vec![("/test.txt", "hello\nworld\nhello again\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert!(result.stdout.contains("1:hello"));
        assert!(result.stdout.contains("3:hello again"));
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let ctx = make_ctx_with_files(
            vec!["notfound", "/test.txt"],
            vec![("/test.txt", "hello world\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_grep_fixed_strings() {
        let ctx = make_ctx_with_files(
            vec!["-F", "a.b", "/test.txt"],
            vec![("/test.txt", "a.b\naXb\n")],
        ).await;
        let cmd = GrepCommand;
        let result = cmd.execute(ctx).await;
        assert_eq!(result.stdout.trim(), "a.b");
    }
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod grep;
```

**Step 3: 运行测试**

Run: `cargo test grep --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/grep/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement grep command with regex support"
```

---

## Task 18: 实现 test 命令

**Files:**
- Create: `src/commands/test_cmd/mod.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 创建 test 实现**

```rust
// src/commands/test_cmd/mod.rs
// 注意：目录名用 test_cmd 避免与 Rust 的 test 模块冲突
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
        return !evaluate_expression(&args[1..], ctx).await;
    }

    // 二元操作符
    if args.len() >= 3 {
        // 查找操作符
        for i in 1..args.len() {
            let op = args[i];
            match op {
                // 逻辑操作符
                "-a" => {
                    let left = evaluate_expression(&args[..i], ctx).await;
                    let right = evaluate_expression(&args[i+1..], ctx).await;
                    return left && right;
                }
                "-o" => {
                    let left = evaluate_expression(&args[..i], ctx).await;
                    let right = evaluate_expression(&args[i+1..], ctx).await;
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
}
```

**Step 2: 更新 mod.rs**

```rust
pub mod test_cmd;
```

**Step 3: 运行测试**

Run: `cargo test test_cmd --`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/commands/test_cmd/mod.rs src/commands/mod.rs
git commit -m "feat(commands): implement test command with file and string tests"
```

---

## Task 19: 创建命令注册函数

**Files:**
- Modify: `src/commands/registry.rs`
- Modify: `src/commands/mod.rs`

**Step 1: 更新 registry.rs 添加批次 A 注册函数**

在 `src/commands/registry.rs` 底部添加：

```rust
use super::basename::BasenameCommand;
use super::dirname::DirnameCommand;
use super::cat::CatCommand;
use super::head::HeadCommand;
use super::tail::TailCommand;
use super::wc::WcCommand;
use super::mkdir::MkdirCommand;
use super::touch::TouchCommand;
use super::rm::RmCommand;
use super::cp::CpCommand;
use super::mv::MvCommand;
use super::ls::LsCommand;
use super::grep::GrepCommand;
use super::test_cmd::{TestCommand, BracketCommand};

/// 注册批次 A 的所有命令
pub fn register_batch_a(registry: &mut CommandRegistry) {
    registry.register(Box::new(BasenameCommand));
    registry.register(Box::new(DirnameCommand));
    registry.register(Box::new(CatCommand));
    registry.register(Box::new(HeadCommand));
    registry.register(Box::new(TailCommand));
    registry.register(Box::new(WcCommand));
    registry.register(Box::new(MkdirCommand));
    registry.register(Box::new(TouchCommand));
    registry.register(Box::new(RmCommand));
    registry.register(Box::new(CpCommand));
    registry.register(Box::new(MvCommand));
    registry.register(Box::new(LsCommand));
    registry.register(Box::new(GrepCommand));
    registry.register(Box::new(TestCommand));
    registry.register(Box::new(BracketCommand));
}

/// 创建包含批次 A 命令的注册表
pub fn create_batch_a_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    registry
}
```

**Step 2: 更新 mod.rs 导出所有命令**

```rust
// src/commands/mod.rs
pub mod basename;
pub mod cat;
pub mod cp;
pub mod dirname;
pub mod grep;
pub mod head;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod registry;
pub mod rm;
pub mod tail;
pub mod test_cmd;
pub mod touch;
pub mod types;
pub mod utils;

pub use registry::{CommandRegistry, register_batch_a, create_batch_a_registry};
pub use types::{Command, CommandContext, CommandResult};
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译成功

**Step 4: Commit**

```bash
git add src/commands/registry.rs src/commands/mod.rs
git commit -m "feat(commands): add batch A registration function"
```

---

## Task 20: 运行完整测试并更新文档

**Step 1: 运行所有测试**

Run: `cargo test`
Expected: 所有测试通过（包括原有的 760 个测试和新增的命令测试）

**Step 2: 更新迁移路线图**

在 `docs/plans/migration-roadmap.md` 中更新批次 A 状态为已完成。

**Step 3: Commit**

```bash
git add docs/plans/migration-roadmap.md
git commit -m "docs: mark batch A commands as completed"
```

---

## 总结

本计划实现了批次 A 的 14 个基础命令：

| 命令 | 状态 | 说明 |
|------|------|------|
| basename | ✅ | 提取文件名 |
| dirname | ✅ | 提取目录名 |
| cat | ✅ | 连接文件，支持行号 |
| head | ✅ | 显示文件开头 |
| tail | ✅ | 显示文件结尾，支持 +N |
| wc | ✅ | 统计行/词/字符 |
| mkdir | ✅ | 创建目录，支持 -p |
| touch | ✅ | 创建/更新文件时间 |
| rm | ✅ | 删除文件，支持 -rf |
| cp | ✅ | 复制文件，支持 -r |
| mv | ✅ | 移动文件 |
| ls | ✅ | 列出目录，支持 -la |
| grep | ✅ | 模式搜索，支持正则 |
| test/[ | ✅ | 条件测试 |

**新增代码量**: 约 2,500 行 Rust 代码
**新增测试**: 约 60 个测试用例
