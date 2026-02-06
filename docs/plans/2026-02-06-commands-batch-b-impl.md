# Commands Batch B (Text Processing) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate 7 text processing commands (uniq, cut, nl, tr, paste, join, sort) from TypeScript to Rust, following the same patterns established in Batch A.

**Architecture:** Each command is a struct implementing the async `Command` trait (`src/commands/types.rs`). Commands are registered in `CommandRegistry`. Tests use `InMemoryFs` for filesystem operations. Commands are implemented in order of increasing complexity: uniq → cut → nl → tr → paste → join → sort.

**Tech Stack:** Rust, async-trait, tokio, regex-lite (if needed for tr POSIX classes), InMemoryFs for testing.

**Reference files:**
- Command trait: `src/commands/types.rs`
- Registry: `src/commands/registry.rs`
- Module exports: `src/commands/mod.rs`
- Example command: `src/commands/wc/mod.rs` (good template)
- TS source: `/Users/arthur/PycharmProjects/just-bash/src/commands/<cmd>/`

---

## Task 1: Implement `uniq` command

**Files:**
- Create: `src/commands/uniq/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod uniq;`)

**Step 1: Write the failing tests**

Create `src/commands/uniq/mod.rs` with the test module and a stub struct:

```rust
// src/commands/uniq/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct UniqCommand;

#[async_trait]
impl Command for UniqCommand {
    fn name(&self) -> &'static str { "uniq" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
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
    async fn test_remove_adjacent_duplicates() {
        let ctx = make_ctx_with_files(
            vec!["/input.txt"],
            vec![("/input.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        ).await;
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "aaa\nbbb\nccc\n");
    }

    #[tokio::test]
    async fn test_count_with_c() {
        let ctx = make_ctx_with_files(
            vec!["-c", "/input.txt"],
            vec![("/input.txt", "aaa\naaa\naaa\nbbb\nccc\nccc\n")],
        ).await;
        let r = UniqCommand.execute(ctx).await;
        assert!(r.stdout.contains("3 aaa"));
        assert!(r.stdout.contains("1 bbb"));
        assert!(r.stdout.contains("2 ccc"));
    }

    #[tokio::test]
    async fn test_duplicates_only_with_d() {
        let ctx = make_ctx_with_files(
            vec!["-d", "/input.txt"],
            vec![("/input.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        ).await;
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "aaa\nccc\n");
    }

    #[tokio::test]
    async fn test_unique_only_with_u() {
        let ctx = make_ctx_with_files(
            vec!["-u", "/input.txt"],
            vec![("/input.txt", "aaa\naaa\nbbb\nccc\nccc\n")],
        ).await;
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "bbb\n");
    }

    #[tokio::test]
    async fn test_only_adjacent_duplicates() {
        let ctx = make_ctx(vec![], "aaa\nbbb\naaa\n");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "aaa\nbbb\naaa\n");
    }

    #[tokio::test]
    async fn test_stdin() {
        let ctx = make_ctx(vec![], "hello\nhello\nworld\n");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "hello\nworld\n");
    }

    #[tokio::test]
    async fn test_case_insensitive_with_i() {
        let ctx = make_ctx(vec!["-i"], "Hello\nhello\nWorld\n");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "Hello\nWorld\n");
    }

    #[tokio::test]
    async fn test_no_duplicates() {
        let ctx = make_ctx(vec![], "a\nb\nc\n");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_all_same() {
        let ctx = make_ctx(vec![], "x\nx\nx\n");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "x\n");
    }

    #[tokio::test]
    async fn test_empty_input() {
        let ctx = make_ctx(vec![], "");
        let r = UniqCommand.execute(ctx).await;
        assert_eq!(r.stdout, "");
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let ctx = make_ctx_with_files(vec!["/no.txt"], vec![]).await;
        let r = UniqCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::uniq -- --nocapture 2>&1 | head -30`
Expected: FAIL (not implemented)

**Step 3: Implement `UniqCommand`**

Replace the stub `execute` method with the full implementation. Key logic:
- Parse flags: `-c` (count), `-d` (duplicates only), `-u` (unique only), `-i` (case insensitive)
- Read input from file arg or stdin
- Iterate lines, compare adjacent lines (optionally case-insensitive)
- Output based on flags: count prefix, duplicates only, unique only, or default (deduplicated)

```rust
#[async_trait]
impl Command for UniqCommand {
    fn name(&self) -> &'static str { "uniq" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut count = false;
        let mut duplicates_only = false;
        let mut unique_only = false;
        let mut ignore_case = false;
        let mut files: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-c" | "--count" => count = true,
                "-d" | "--repeated" => duplicates_only = true,
                "-u" | "--unique" => unique_only = true,
                "-i" | "--ignore-case" => ignore_case = true,
                "--help" => {
                    return CommandResult::success(
                        "Usage: uniq [OPTION]... [INPUT [OUTPUT]]\n\
                         Filter adjacent matching lines.\n\n\
                         Options:\n  -c  prefix lines by count\n  \
                         -d  only print duplicate lines\n  \
                         -u  only print unique lines\n  \
                         -i  ignore case\n".to_string()
                    );
                }
                _ if !arg.starts_with('-') => files.push(arg.clone()),
                _ => return CommandResult::error(format!("uniq: unknown option: {}", arg)),
            }
        }

        let input = if let Some(file) = files.first() {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::error(
                    format!("uniq: {}: No such file or directory", file)
                ),
            }
        } else {
            ctx.stdin.clone()
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let lines: Vec<&str> = input.lines().collect();
        let mut output = String::new();
        let mut i = 0;

        while i < lines.len() {
            let current = lines[i];
            let mut cnt = 1;
            while i + cnt < lines.len() {
                let matches = if ignore_case {
                    lines[i + cnt].eq_ignore_ascii_case(current)
                } else {
                    lines[i + cnt] == current
                };
                if matches { cnt += 1; } else { break; }
            }

            let should_print = if duplicates_only {
                cnt > 1
            } else if unique_only {
                cnt == 1
            } else {
                true
            };

            if should_print {
                if count {
                    output.push_str(&format!("{:>7} {}\n", cnt, current));
                } else {
                    output.push_str(current);
                    output.push('\n');
                }
            }
            i += cnt;
        }

        CommandResult::success(output)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::uniq -- --nocapture`
Expected: ALL PASS (11 tests)

**Step 5: Commit**

```bash
git add src/commands/uniq/
git commit -m "feat(commands): implement uniq command with -c/-d/-u/-i options"
```

---

## Task 2: Implement `cut` command

**Files:**
- Create: `src/commands/cut/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod cut;`)

**Step 1: Write the failing tests**

Create `src/commands/cut/mod.rs` with stub and tests:

```rust
// src/commands/cut/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct CutCommand;

#[async_trait]
impl Command for CutCommand {
    fn name(&self) -> &'static str { "cut" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
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
    async fn test_cut_first_field_colon() {
        let ctx = make_ctx(vec!["-d:", "-f1"], "root:x:0:0\nuser:x:1000:1000\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "root\nuser\n");
    }

    #[tokio::test]
    async fn test_cut_multiple_fields() {
        let ctx = make_ctx(vec!["-d:", "-f1,3"], "a:b:c:d\n1:2:3:4\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a:c\n1:3\n");
    }

    #[tokio::test]
    async fn test_cut_field_range() {
        let ctx = make_ctx(vec!["-d:", "-f2-4"], "a:b:c:d:e\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "b:c:d\n");
    }

    #[tokio::test]
    async fn test_cut_csv_comma() {
        let ctx = make_ctx(vec!["-d,", "-f2"], "name,age,city\njohn,30,nyc\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "age\n30\n");
    }

    #[tokio::test]
    async fn test_cut_tab_default() {
        let ctx = make_ctx(vec!["-f1"], "a\tb\tc\n1\t2\t3\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\n1\n");
    }

    #[tokio::test]
    async fn test_cut_characters() {
        let ctx = make_ctx(vec!["-c1-5"], "hello world\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "hello\n");
    }

    #[tokio::test]
    async fn test_cut_specific_chars() {
        let ctx = make_ctx(vec!["-c1,3,5"], "abcdefg\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "ace\n");
    }

    #[tokio::test]
    async fn test_cut_stdin() {
        let ctx = make_ctx(vec!["-d:", "-f1"], "a:b:c\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\n");
    }

    #[tokio::test]
    async fn test_cut_open_range() {
        let ctx = make_ctx(vec!["-d:", "-f3-"], "a:b:c:d:e\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "c:d:e\n");
    }

    #[tokio::test]
    async fn test_cut_file_not_found() {
        let ctx = make_ctx_with_files(vec!["-f1", "/no.txt"], vec![]).await;
        let r = CutCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cut_no_field_or_char() {
        let ctx = make_ctx(vec![], "test\n");
        let r = CutCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cut_only_delimited_s() {
        let ctx = make_ctx(vec!["-d:", "-f1", "-s"], "a:b\nno-delim\nc:d\n");
        let r = CutCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\nc\n");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::cut -- --nocapture 2>&1 | head -30`
Expected: FAIL (not implemented)

**Step 3: Implement `CutCommand`**

Key logic:
- Parse `-c` (characters), `-f` (fields), `-d` (delimiter, default TAB), `-s` (only delimited)
- Parse range specs: `N`, `N-M`, `N-`, `-M`, `N,M,O`
- For `-c`: extract character positions
- For `-f`: split by delimiter, extract fields

```rust
#[derive(Debug, Clone)]
enum CutMode {
    Characters(Vec<(usize, Option<usize>)>),  // (start, end) 1-indexed
    Fields(Vec<(usize, Option<usize>)>),
}

fn parse_range(spec: &str) -> Vec<(usize, Option<usize>)> {
    let mut ranges = Vec::new();
    for part in spec.split(',') {
        if let Some(idx) = part.find('-') {
            let start = part[..idx].parse::<usize>().unwrap_or(1);
            let end = if idx + 1 < part.len() {
                part[idx + 1..].parse::<usize>().ok()
            } else {
                None // open-ended
            };
            ranges.push((start, end));
        } else if let Ok(n) = part.parse::<usize>() {
            ranges.push((n, Some(n)));
        }
    }
    ranges
}

#[async_trait]
impl Command for CutCommand {
    fn name(&self) -> &'static str { "cut" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut mode: Option<CutMode> = None;
        let mut delimiter = '\t';
        let mut only_delimited = false;
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            if arg == "-c" && i + 1 < ctx.args.len() {
                i += 1;
                mode = Some(CutMode::Characters(parse_range(&ctx.args[i])));
            } else if arg.starts_with("-c") {
                mode = Some(CutMode::Characters(parse_range(&arg[2..])));
            } else if arg == "-f" && i + 1 < ctx.args.len() {
                i += 1;
                mode = Some(CutMode::Fields(parse_range(&ctx.args[i])));
            } else if arg.starts_with("-f") {
                mode = Some(CutMode::Fields(parse_range(&arg[2..])));
            } else if arg == "-d" && i + 1 < ctx.args.len() {
                i += 1;
                delimiter = ctx.args[i].chars().next().unwrap_or('\t');
            } else if arg.starts_with("-d") {
                delimiter = arg.chars().nth(2).unwrap_or('\t');
            } else if arg == "-s" || arg == "--only-delimited" {
                only_delimited = true;
            } else if arg == "--help" {
                return CommandResult::success("Usage: cut -c LIST or cut -f LIST [-d DELIM] [-s] [FILE]...\n".to_string());
            } else if !arg.starts_with('-') {
                files.push(arg.clone());
            }
            i += 1;
        }

        let mode = match mode {
            Some(m) => m,
            None => return CommandResult::error("cut: you must specify a list of bytes, characters, or fields\n".to_string()),
        };

        let input = if let Some(file) = files.first() {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::error(format!("cut: {}: No such file or directory\n", file)),
            }
        } else {
            ctx.stdin.clone()
        };

        let mut output = String::new();
        for line in input.lines() {
            match &mode {
                CutMode::Characters(ranges) => {
                    let chars: Vec<char> = line.chars().collect();
                    let mut selected: Vec<char> = Vec::new();
                    for &(start, end) in ranges {
                        let end = end.unwrap_or(chars.len());
                        for idx in start..=end {
                            if idx > 0 && idx <= chars.len() {
                                selected.push(chars[idx - 1]);
                            }
                        }
                    }
                    output.push_str(&selected.into_iter().collect::<String>());
                    output.push('\n');
                }
                CutMode::Fields(ranges) => {
                    if only_delimited && !line.contains(delimiter) {
                        continue;
                    }
                    let fields: Vec<&str> = line.split(delimiter).collect();
                    let mut selected: Vec<&str> = Vec::new();
                    for &(start, end) in ranges {
                        let end = end.unwrap_or(fields.len());
                        for idx in start..=end {
                            if idx > 0 && idx <= fields.len() {
                                selected.push(fields[idx - 1]);
                            }
                        }
                    }
                    output.push_str(&selected.join(&delimiter.to_string()));
                    output.push('\n');
                }
            }
        }

        CommandResult::success(output)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::cut -- --nocapture`
Expected: ALL PASS (12 tests)

**Step 5: Commit**

```bash
git add src/commands/cut/
git commit -m "feat(commands): implement cut command with -c/-f/-d/-s options"
```

---

## Task 3: Implement `nl` command

**Files:**
- Create: `src/commands/nl/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod nl;`)

**Step 1: Write the failing tests**

```rust
// src/commands/nl/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct NlCommand;

#[async_trait]
impl Command for NlCommand {
    fn name(&self) -> &'static str { "nl" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
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
    async fn test_nl_stdin() {
        let ctx = make_ctx(vec![], "line1\nline2\nline3\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains("1") && r.stdout.contains("line1"));
        assert!(r.stdout.contains("2") && r.stdout.contains("line2"));
        assert!(r.stdout.contains("3") && r.stdout.contains("line3"));
    }

    #[tokio::test]
    async fn test_nl_file() {
        let ctx = make_ctx_with_files(
            vec!["/test.txt"],
            vec![("/test.txt", "a\nb\nc\n")],
        ).await;
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains("1") && r.stdout.contains("a"));
    }

    #[tokio::test]
    async fn test_nl_skip_empty_default() {
        let ctx = make_ctx(vec![], "line1\n\nline2\n");
        let r = NlCommand.execute(ctx).await;
        // Default -bt skips empty lines
        let lines: Vec<&str> = r.stdout.lines().collect();
        assert!(lines[0].contains("1") && lines[0].contains("line1"));
        assert!(!lines[1].starts_with(char::is_numeric)); // empty line not numbered
        assert!(lines[2].contains("2") && lines[2].contains("line2"));
    }

    #[tokio::test]
    async fn test_nl_all_lines_ba() {
        let ctx = make_ctx(vec!["-ba"], "line1\n\nline2\n");
        let r = NlCommand.execute(ctx).await;
        let lines: Vec<&str> = r.stdout.lines().collect();
        assert!(lines[0].contains("1"));
        assert!(lines[1].contains("2")); // empty line numbered
        assert!(lines[2].contains("3"));
    }

    #[tokio::test]
    async fn test_nl_no_lines_bn() {
        let ctx = make_ctx(vec!["-bn"], "line1\nline2\n");
        let r = NlCommand.execute(ctx).await;
        // No lines numbered
        assert!(!r.stdout.lines().next().unwrap().starts_with(char::is_numeric));
    }

    #[tokio::test]
    async fn test_nl_left_justify_ln() {
        let ctx = make_ctx(vec!["-nln"], "a\nb\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.starts_with("1"));
    }

    #[tokio::test]
    async fn test_nl_right_zeros_rz() {
        let ctx = make_ctx(vec!["-nrz"], "a\nb\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains("000001"));
    }

    #[tokio::test]
    async fn test_nl_width() {
        let ctx = make_ctx(vec!["-w3"], "a\nb\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.starts_with("  1") || r.stdout.contains("  1"));
    }

    #[tokio::test]
    async fn test_nl_separator() {
        let ctx = make_ctx(vec!["-s:"], "a\nb\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains(":a"));
    }

    #[tokio::test]
    async fn test_nl_start_number() {
        let ctx = make_ctx(vec!["-v10"], "a\nb\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains("10") && r.stdout.contains("a"));
        assert!(r.stdout.contains("11") && r.stdout.contains("b"));
    }

    #[tokio::test]
    async fn test_nl_increment() {
        let ctx = make_ctx(vec!["-i5"], "a\nb\nc\n");
        let r = NlCommand.execute(ctx).await;
        assert!(r.stdout.contains("1") && r.stdout.contains("a"));
        assert!(r.stdout.contains("6") && r.stdout.contains("b"));
        assert!(r.stdout.contains("11") && r.stdout.contains("c"));
    }

    #[tokio::test]
    async fn test_nl_empty_input() {
        let ctx = make_ctx(vec![], "");
        let r = NlCommand.execute(ctx).await;
        assert_eq!(r.stdout, "");
    }

    #[tokio::test]
    async fn test_nl_file_not_found() {
        let ctx = make_ctx_with_files(vec!["/no.txt"], vec![]).await;
        let r = NlCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::nl -- --nocapture 2>&1 | head -30`
Expected: FAIL

**Step 3: Implement `NlCommand`**

Key logic:
- Parse options: `-b` (body style: a/t/n), `-n` (format: ln/rn/rz), `-w` (width), `-s` (separator), `-v` (start), `-i` (increment)
- Process lines: number based on style, format number, apply separator

```rust
#[derive(Clone, Copy, PartialEq)]
enum BodyStyle { All, NonEmpty, None }

#[derive(Clone, Copy)]
enum NumberFormat { Left, Right, RightZero }

#[async_trait]
impl Command for NlCommand {
    fn name(&self) -> &'static str { "nl" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut body_style = BodyStyle::NonEmpty;
        let mut number_format = NumberFormat::Right;
        let mut width: usize = 6;
        let mut separator = "\t".to_string();
        let mut start_num: i64 = 1;
        let mut increment: i64 = 1;
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            if arg == "-b" && i + 1 < ctx.args.len() {
                i += 1;
                body_style = match ctx.args[i].as_str() {
                    "a" => BodyStyle::All,
                    "t" => BodyStyle::NonEmpty,
                    "n" => BodyStyle::None,
                    _ => return CommandResult::error(format!("nl: invalid body style: {}\n", ctx.args[i])),
                };
            } else if arg.starts_with("-b") {
                body_style = match &arg[2..] {
                    "a" => BodyStyle::All,
                    "t" => BodyStyle::NonEmpty,
                    "n" => BodyStyle::None,
                    _ => return CommandResult::error(format!("nl: invalid body style: {}\n", &arg[2..])),
                };
            } else if arg == "-n" && i + 1 < ctx.args.len() {
                i += 1;
                number_format = match ctx.args[i].as_str() {
                    "ln" => NumberFormat::Left,
                    "rn" => NumberFormat::Right,
                    "rz" => NumberFormat::RightZero,
                    _ => return CommandResult::error(format!("nl: invalid number format: {}\n", ctx.args[i])),
                };
            } else if arg.starts_with("-n") {
                number_format = match &arg[2..] {
                    "ln" => NumberFormat::Left,
                    "rn" => NumberFormat::Right,
                    "rz" => NumberFormat::RightZero,
                    _ => return CommandResult::error(format!("nl: invalid number format: {}\n", &arg[2..])),
                };
            } else if arg == "-w" && i + 1 < ctx.args.len() {
                i += 1;
                width = ctx.args[i].parse().unwrap_or(6);
            } else if arg.starts_with("-w") {
                width = arg[2..].parse().unwrap_or(6);
            } else if arg == "-s" && i + 1 < ctx.args.len() {
                i += 1;
                separator = ctx.args[i].clone();
            } else if arg.starts_with("-s") {
                separator = arg[2..].to_string();
            } else if arg == "-v" && i + 1 < ctx.args.len() {
                i += 1;
                start_num = ctx.args[i].parse().unwrap_or(1);
            } else if arg.starts_with("-v") {
                start_num = arg[2..].parse().unwrap_or(1);
            } else if arg == "-i" && i + 1 < ctx.args.len() {
                i += 1;
                increment = ctx.args[i].parse().unwrap_or(1);
            } else if arg.starts_with("-i") {
                increment = arg[2..].parse().unwrap_or(1);
            } else if arg == "--help" {
                return CommandResult::success("Usage: nl [OPTION]... [FILE]...\nNumber lines.\n".to_string());
            } else if !arg.starts_with('-') {
                files.push(arg.clone());
            }
            i += 1;
        }

        let input = if let Some(file) = files.first() {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::error(format!("nl: {}: No such file or directory\n", file)),
            }
        } else {
            ctx.stdin.clone()
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let mut output = String::new();
        let mut line_num = start_num;

        for line in input.lines() {
            let should_number = match body_style {
                BodyStyle::All => true,
                BodyStyle::NonEmpty => !line.is_empty(),
                BodyStyle::None => false,
            };

            if should_number {
                let num_str = match number_format {
                    NumberFormat::Left => format!("{:<width$}", line_num, width = width),
                    NumberFormat::Right => format!("{:>width$}", line_num, width = width),
                    NumberFormat::RightZero => format!("{:0>width$}", line_num, width = width),
                };
                output.push_str(&format!("{}{}{}\n", num_str, separator, line));
                line_num += increment;
            } else {
                output.push_str(&format!("{}{}{}\n", " ".repeat(width), separator, line));
            }
        }

        CommandResult::success(output)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::nl -- --nocapture`
Expected: ALL PASS (13 tests)

**Step 5: Commit**

```bash
git add src/commands/nl/
git commit -m "feat(commands): implement nl command with -b/-n/-w/-s/-v/-i options"
```

---

## Task 4: Implement `tr` command

**Files:**
- Create: `src/commands/tr/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod tr;`)

**Step 1: Write the failing tests**

```rust
// src/commands/tr/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct TrCommand;

#[async_trait]
impl Command for TrCommand {
    fn name(&self) -> &'static str { "tr" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
        }
    }

    #[tokio::test]
    async fn test_tr_lower_to_upper() {
        let ctx = make_ctx(vec!["a-z", "A-Z"], "hello world\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "HELLO WORLD\n");
    }

    #[tokio::test]
    async fn test_tr_upper_to_lower() {
        let ctx = make_ctx(vec!["A-Z", "a-z"], "HELLO WORLD\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "hello world\n");
    }

    #[tokio::test]
    async fn test_tr_delete() {
        let ctx = make_ctx(vec!["-d", "aeiou"], "hello world\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "hll wrld\n");
    }

    #[tokio::test]
    async fn test_tr_delete_newlines() {
        let ctx = make_ctx(vec!["-d", "\n"], "a\nb\nc\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "abc");
    }

    #[tokio::test]
    async fn test_tr_squeeze() {
        let ctx = make_ctx(vec!["-s", "a"], "aaabbbccc\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "abbbccc\n");
    }

    #[tokio::test]
    async fn test_tr_translate_chars() {
        let ctx = make_ctx(vec!["abc", "xyz"], "aabbcc\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "xxyyzz\n");
    }

    #[tokio::test]
    async fn test_tr_space_to_underscore() {
        let ctx = make_ctx(vec![" ", "_"], "hello world\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "hello_world\n");
    }

    #[tokio::test]
    async fn test_tr_char_range() {
        let ctx = make_ctx(vec!["0-9", "X"], "abc123def456\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "abcXXXdefXXX\n");
    }

    #[tokio::test]
    async fn test_tr_delete_digits() {
        let ctx = make_ctx(vec!["-d", "0-9"], "abc123def456\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "abcdef\n");
    }

    #[tokio::test]
    async fn test_tr_missing_operand() {
        let ctx = make_ctx(vec![], "test\n");
        let r = TrCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tr_missing_set2() {
        let ctx = make_ctx(vec!["abc"], "test\n");
        let r = TrCommand.execute(ctx).await;
        // Without -d, need SET2
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tr_shorter_set2() {
        // When SET2 is shorter, last char is repeated
        let ctx = make_ctx(vec!["abc", "x"], "aabbcc\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "xxxxxx\n");
    }

    #[tokio::test]
    async fn test_tr_complement_delete() {
        let ctx = make_ctx(vec!["-cd", "a-z\n"], "Hello123World\n");
        let r = TrCommand.execute(ctx).await;
        assert_eq!(r.stdout, "elloorld\n");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::tr -- --nocapture 2>&1 | head -30`
Expected: FAIL

**Step 3: Implement `TrCommand`**

Key logic:
- Parse flags: `-d` (delete), `-s` (squeeze), `-c` (complement)
- Expand character ranges (a-z, 0-9)
- Build translation map or delete set
- Process stdin character by character

```rust
fn expand_set(set: &str) -> Vec<char> {
    let mut result = Vec::new();
    let chars: Vec<char> = set.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i];
            let end = chars[i + 2];
            for c in start..=end {
                result.push(c);
            }
            i += 3;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

#[async_trait]
impl Command for TrCommand {
    fn name(&self) -> &'static str { "tr" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut delete = false;
        let mut squeeze = false;
        let mut complement = false;
        let mut sets: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-d" | "--delete" => delete = true,
                "-s" | "--squeeze-repeats" => squeeze = true,
                "-c" | "-C" | "--complement" => complement = true,
                "--help" => return CommandResult::success("Usage: tr [OPTION]... SET1 [SET2]\n".to_string()),
                _ if !arg.starts_with('-') => sets.push(arg.clone()),
                _ => {}
            }
        }

        if sets.is_empty() {
            return CommandResult::error("tr: missing operand\n".to_string());
        }

        let set1 = expand_set(&sets[0]);
        let set1_chars: std::collections::HashSet<char> = set1.iter().cloned().collect();

        if delete {
            // Delete mode
            let delete_set: std::collections::HashSet<char> = if complement {
                // Delete everything NOT in set1
                ctx.stdin.chars().filter(|c| !set1_chars.contains(c)).collect()
            } else {
                set1_chars
            };
            let output: String = ctx.stdin.chars().filter(|c| !delete_set.contains(c)).collect();
            return CommandResult::success(output);
        }

        if sets.len() < 2 && !squeeze {
            return CommandResult::error("tr: missing operand after SET1\n".to_string());
        }

        let set2 = if sets.len() > 1 { expand_set(&sets[1]) } else { Vec::new() };

        // Build translation map
        let mut trans_map: std::collections::HashMap<char, char> = std::collections::HashMap::new();
        for (i, &c) in set1.iter().enumerate() {
            let replacement = if i < set2.len() {
                set2[i]
            } else if !set2.is_empty() {
                *set2.last().unwrap()
            } else {
                c
            };
            trans_map.insert(c, replacement);
        }

        let mut output = String::new();
        let mut last_char: Option<char> = None;

        for c in ctx.stdin.chars() {
            let translated = if complement {
                if set1_chars.contains(&c) { c } else { *set2.last().unwrap_or(&c) }
            } else {
                *trans_map.get(&c).unwrap_or(&c)
            };

            if squeeze {
                if Some(translated) == last_char && set1_chars.contains(&c) {
                    continue;
                }
            }
            output.push(translated);
            last_char = Some(translated);
        }

        CommandResult::success(output)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::tr -- --nocapture`
Expected: ALL PASS (13 tests)

**Step 5: Commit**

```bash
git add src/commands/tr/
git commit -m "feat(commands): implement tr command with -d/-s/-c options"
```

---

## Task 5: Implement `paste` command

**Files:**
- Create: `src/commands/paste/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod paste;`)

**Step 1: Write the failing tests**

```rust
// src/commands/paste/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct PasteCommand;

#[async_trait]
impl Command for PasteCommand {
    fn name(&self) -> &'static str { "paste" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx_with_files(args: Vec<&str>, files: Vec<(&str, &str)>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content.as_bytes()).await.unwrap();
        }
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
        }
    }

    #[tokio::test]
    async fn test_paste_two_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\nb\nc\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\ta\n2\tb\n3\tc\n");
    }

    #[tokio::test]
    async fn test_paste_three_files() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt", "/c.txt"],
            vec![("/a.txt", "1\n2\n"), ("/b.txt", "a\nb\n"), ("/c.txt", "x\ny\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\ta\tx\n2\tb\ty\n");
    }

    #[tokio::test]
    async fn test_paste_uneven_lines() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\ta\n2\t\n3\t\n");
    }

    #[tokio::test]
    async fn test_paste_custom_delimiter() {
        let ctx = make_ctx_with_files(
            vec!["-d:", "/a.txt", "/b.txt"],
            vec![("/a.txt", "1\n2\n"), ("/b.txt", "a\nb\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1:a\n2:b\n");
    }

    #[tokio::test]
    async fn test_paste_serial() {
        let ctx = make_ctx_with_files(
            vec!["-s", "/a.txt", "/b.txt"],
            vec![("/a.txt", "1\n2\n3\n"), ("/b.txt", "a\nb\nc\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\t2\t3\na\tb\tc\n");
    }

    #[tokio::test]
    async fn test_paste_stdin() {
        let ctx = make_ctx_with_files(
            vec!["-", "/b.txt"],
            vec![("/b.txt", "a\nb\n")],
            "1\n2\n",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\ta\n2\tb\n");
    }

    #[tokio::test]
    async fn test_paste_no_files_error() {
        let ctx = make_ctx_with_files(vec![], vec![], "").await;
        let r = PasteCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_paste_file_not_found() {
        let ctx = make_ctx_with_files(vec!["/no.txt"], vec![], "").await;
        let r = PasteCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_paste_multiple_delimiters() {
        let ctx = make_ctx_with_files(
            vec!["-d:,", "/a.txt", "/b.txt", "/c.txt"],
            vec![("/a.txt", "1\n2\n"), ("/b.txt", "a\nb\n"), ("/c.txt", "x\ny\n")],
            "",
        ).await;
        let r = PasteCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1:a,x\n2:b,y\n");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::paste -- --nocapture 2>&1 | head -30`
Expected: FAIL

**Step 3: Implement `PasteCommand`**

Key logic:
- Parse `-d` (delimiter), `-s` (serial mode)
- Read all files into line vectors
- Parallel mode: merge corresponding lines with delimiter
- Serial mode: join each file's lines horizontally

```rust
#[async_trait]
impl Command for PasteCommand {
    fn name(&self) -> &'static str { "paste" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut delimiter = "\t".to_string();
        let mut serial = false;
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            if arg == "-d" && i + 1 < ctx.args.len() {
                i += 1;
                delimiter = ctx.args[i].clone();
            } else if arg.starts_with("-d") {
                delimiter = arg[2..].to_string();
            } else if arg == "-s" || arg == "--serial" {
                serial = true;
            } else if arg == "--help" {
                return CommandResult::success("Usage: paste [OPTION]... [FILE]...\n".to_string());
            } else if !arg.starts_with('-') || arg == "-" {
                files.push(arg.clone());
            }
            i += 1;
        }

        if files.is_empty() {
            return CommandResult::error("paste: missing operand\n".to_string());
        }

        // Read all files
        let mut file_lines: Vec<Vec<String>> = Vec::new();
        for file in &files {
            let content = if file == "-" {
                ctx.stdin.clone()
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                match ctx.fs.read_file(&path).await {
                    Ok(c) => c,
                    Err(_) => return CommandResult::error(format!("paste: {}: No such file or directory\n", file)),
                }
            };
            let lines: Vec<String> = content.lines().map(String::from).collect();
            file_lines.push(lines);
        }

        let delim_chars: Vec<char> = delimiter.chars().collect();
        let mut output = String::new();

        if serial {
            // Serial mode: each file on one line
            for lines in &file_lines {
                output.push_str(&lines.join(&delimiter));
                output.push('\n');
            }
        } else {
            // Parallel mode: merge corresponding lines
            let max_lines = file_lines.iter().map(|f| f.len()).max().unwrap_or(0);
            for line_idx in 0..max_lines {
                let mut parts: Vec<&str> = Vec::new();
                for file in &file_lines {
                    parts.push(file.get(line_idx).map(|s| s.as_str()).unwrap_or(""));
                }
                // Join with cycling delimiters
                let mut line = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        let d = delim_chars.get((i - 1) % delim_chars.len()).unwrap_or(&'\t');
                        line.push(*d);
                    }
                    line.push_str(part);
                }
                output.push_str(&line);
                output.push('\n');
            }
        }

        CommandResult::success(output)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::paste -- --nocapture`
Expected: ALL PASS (9 tests)

**Step 5: Commit**

```bash
git add src/commands/paste/
git commit -m "feat(commands): implement paste command with -d/-s options"
```

---

## Task 6: Implement `join` command

**Files:**
- Create: `src/commands/join/mod.rs`
- Modify: `src/commands/mod.rs` (add `pub mod join;`)

**Step 1: Write the failing tests**

```rust
// src/commands/join/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct JoinCommand;

#[async_trait]
impl Command for JoinCommand {
    fn name(&self) -> &'static str { "join" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
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
    async fn test_join_basic() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n2 banana\n3 cherry\n"),
                ("/b.txt", "1 red\n2 yellow\n3 red\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1 apple red"));
        assert!(r.stdout.contains("2 banana yellow"));
        assert!(r.stdout.contains("3 cherry red"));
    }

    #[tokio::test]
    async fn test_join_only_matching() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n2 banana\n"),
                ("/b.txt", "1 red\n3 green\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1 apple red"));
        assert!(!r.stdout.contains("2"));
        assert!(!r.stdout.contains("3"));
    }

    #[tokio::test]
    async fn test_join_custom_field() {
        let ctx = make_ctx_with_files(
            vec!["-1", "2", "-2", "1", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "apple 1\nbanana 2\n"),
                ("/b.txt", "1 red\n2 yellow\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1 apple red"));
        assert!(r.stdout.contains("2 banana yellow"));
    }

    #[tokio::test]
    async fn test_join_custom_separator() {
        let ctx = make_ctx_with_files(
            vec!["-t:", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1:apple\n2:banana\n"),
                ("/b.txt", "1:red\n2:yellow\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1:apple:red"));
    }

    #[tokio::test]
    async fn test_join_left_outer_a1() {
        let ctx = make_ctx_with_files(
            vec!["-a1", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n2 banana\n"),
                ("/b.txt", "1 red\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1 apple red"));
        assert!(r.stdout.contains("2 banana"));
    }

    #[tokio::test]
    async fn test_join_right_outer_a2() {
        let ctx = make_ctx_with_files(
            vec!["-a2", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n"),
                ("/b.txt", "1 red\n2 yellow\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("1 apple red"));
        assert!(r.stdout.contains("2 yellow"));
    }

    #[tokio::test]
    async fn test_join_anti_v1() {
        let ctx = make_ctx_with_files(
            vec!["-v1", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n2 banana\n"),
                ("/b.txt", "1 red\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(!r.stdout.contains("1"));
        assert!(r.stdout.contains("2 banana"));
    }

    #[tokio::test]
    async fn test_join_empty_replacement() {
        let ctx = make_ctx_with_files(
            vec!["-a1", "-e", "EMPTY", "-o", "1.1,1.2,2.2", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n2 banana\n"),
                ("/b.txt", "1 red\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("EMPTY"));
    }

    #[tokio::test]
    async fn test_join_ignore_case() {
        let ctx = make_ctx_with_files(
            vec!["-i", "/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "A apple\nB banana\n"),
                ("/b.txt", "a red\nb yellow\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert!(r.stdout.contains("apple") && r.stdout.contains("red"));
    }

    #[tokio::test]
    async fn test_join_missing_file() {
        let ctx = make_ctx_with_files(vec!["/a.txt"], vec![("/a.txt", "1 a\n")]).await;
        let r = JoinCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_join_no_matches() {
        let ctx = make_ctx_with_files(
            vec!["/a.txt", "/b.txt"],
            vec![
                ("/a.txt", "1 apple\n"),
                ("/b.txt", "2 red\n"),
            ],
        ).await;
        let r = JoinCommand.execute(ctx).await;
        assert_eq!(r.stdout, "");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::join -- --nocapture 2>&1 | head -30`
Expected: FAIL

**Step 3: Implement `JoinCommand`**

Key logic:
- Parse options: `-1`/`-2` (join fields), `-t` (separator), `-a` (outer join), `-v` (anti-join), `-e` (empty replacement), `-o` (output format), `-i` (ignore case)
- Build index of file2 by join key
- Iterate file1, lookup matches, output joined lines
- Handle outer joins and anti-joins

```rust
use std::collections::HashMap as StdHashMap;

struct JoinOptions {
    field1: usize,
    field2: usize,
    separator: Option<char>,
    print_unpairable1: bool,
    print_unpairable2: bool,
    only_unpairable1: bool,
    only_unpairable2: bool,
    empty_string: String,
    output_format: Option<Vec<(usize, usize)>>, // (file_num, field_num)
    ignore_case: bool,
}

impl Default for JoinOptions {
    fn default() -> Self {
        Self {
            field1: 1, field2: 1,
            separator: None,
            print_unpairable1: false, print_unpairable2: false,
            only_unpairable1: false, only_unpairable2: false,
            empty_string: String::new(),
            output_format: None,
            ignore_case: false,
        }
    }
}

fn split_line(line: &str, sep: Option<char>) -> Vec<&str> {
    match sep {
        Some(c) => line.split(c).collect(),
        None => line.split_whitespace().collect(),
    }
}

fn parse_output_format(spec: &str) -> Vec<(usize, usize)> {
    spec.split(',').filter_map(|s| {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 2 {
            Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
        } else {
            None
        }
    }).collect()
}

#[async_trait]
impl Command for JoinCommand {
    fn name(&self) -> &'static str { "join" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut opts = JoinOptions::default();
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "-1" => { i += 1; opts.field1 = ctx.args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1); }
                "-2" => { i += 1; opts.field2 = ctx.args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1); }
                "-t" => { i += 1; opts.separator = ctx.args.get(i).and_then(|s| s.chars().next()); }
                "-a" => { i += 1; match ctx.args.get(i).map(|s| s.as_str()) {
                    Some("1") => opts.print_unpairable1 = true,
                    Some("2") => opts.print_unpairable2 = true,
                    _ => {}
                }}
                "-v" => { i += 1; match ctx.args.get(i).map(|s| s.as_str()) {
                    Some("1") => opts.only_unpairable1 = true,
                    Some("2") => opts.only_unpairable2 = true,
                    _ => {}
                }}
                "-e" => { i += 1; opts.empty_string = ctx.args.get(i).cloned().unwrap_or_default(); }
                "-o" => { i += 1; opts.output_format = ctx.args.get(i).map(|s| parse_output_format(s)); }
                "-i" | "--ignore-case" => opts.ignore_case = true,
                "--help" => return CommandResult::success("Usage: join [OPTION]... FILE1 FILE2\n".to_string()),
                _ if arg.starts_with("-t") => opts.separator = arg.chars().nth(2),
                _ if !arg.starts_with('-') || arg == "-" => files.push(arg.clone()),
                _ => {}
            }
            i += 1;
        }

        if files.len() < 2 {
            return CommandResult::error("join: missing operand\n".to_string());
        }

        // Read files
        let read_file = |file: &str| async {
            if file == "-" {
                Ok(ctx.stdin.clone())
            } else {
                let path = ctx.fs.resolve_path(&ctx.cwd, file);
                ctx.fs.read_file(&path).await.map_err(|_| format!("join: {}: No such file or directory\n", file))
            }
        };

        let content1 = match read_file(&files[0]).await {
            Ok(c) => c,
            Err(e) => return CommandResult::error(e),
        };
        let content2 = match read_file(&files[1]).await {
            Ok(c) => c,
            Err(e) => return CommandResult::error(e),
        };

        // Build index for file2
        let mut file2_index: StdHashMap<String, Vec<Vec<String>>> = StdHashMap::new();
        for line in content2.lines() {
            let fields: Vec<String> = split_line(line, opts.separator).iter().map(|s| s.to_string()).collect();
            if let Some(key) = fields.get(opts.field2 - 1) {
                let key = if opts.ignore_case { key.to_lowercase() } else { key.clone() };
                file2_index.entry(key).or_default().push(fields);
            }
        }

        let sep = opts.separator.map(|c| c.to_string()).unwrap_or(" ".to_string());
        let mut output = String::new();
        let mut matched_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Process file1
        for line in content1.lines() {
            let fields1: Vec<String> = split_line(line, opts.separator).iter().map(|s| s.to_string()).collect();
            let key = fields1.get(opts.field1 - 1).cloned().unwrap_or_default();
            let lookup_key = if opts.ignore_case { key.to_lowercase() } else { key.clone() };

            if let Some(matches) = file2_index.get(&lookup_key) {
                matched_keys.insert(lookup_key.clone());
                if !opts.only_unpairable1 {
                    for fields2 in matches {
                        let line_out = format_output(&key, &fields1, fields2, opts.field1, opts.field2, &opts, &sep);
                        output.push_str(&line_out);
                        output.push('\n');
                    }
                }
            } else if opts.print_unpairable1 || opts.only_unpairable1 {
                let line_out = format_unpairable(&fields1, 1, &opts, &sep);
                output.push_str(&line_out);
                output.push('\n');
            }
        }

        // Handle unpairable from file2
        if opts.print_unpairable2 || opts.only_unpairable2 {
            for line in content2.lines() {
                let fields2: Vec<String> = split_line(line, opts.separator).iter().map(|s| s.to_string()).collect();
                let key = fields2.get(opts.field2 - 1).cloned().unwrap_or_default();
                let lookup_key = if opts.ignore_case { key.to_lowercase() } else { key.clone() };
                if !matched_keys.contains(&lookup_key) {
                    let line_out = format_unpairable(&fields2, 2, &opts, &sep);
                    output.push_str(&line_out);
                    output.push('\n');
                }
            }
        }

        CommandResult::success(output)
    }
}

fn format_output(key: &str, f1: &[String], f2: &[String], jf1: usize, jf2: usize, opts: &JoinOptions, sep: &str) -> String {
    if let Some(fmt) = &opts.output_format {
        fmt.iter().map(|&(file, field)| {
            let fields = if file == 1 { f1 } else { f2 };
            if field == 0 { key.to_string() }
            else { fields.get(field - 1).cloned().unwrap_or_else(|| opts.empty_string.clone()) }
        }).collect::<Vec<_>>().join(sep)
    } else {
        let mut parts = vec![key.to_string()];
        for (i, f) in f1.iter().enumerate() {
            if i + 1 != jf1 { parts.push(f.clone()); }
        }
        for (i, f) in f2.iter().enumerate() {
            if i + 1 != jf2 { parts.push(f.clone()); }
        }
        parts.join(sep)
    }
}

fn format_unpairable(fields: &[String], _file_num: usize, _opts: &JoinOptions, sep: &str) -> String {
    fields.join(sep)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib commands::join -- --nocapture`
Expected: ALL PASS (11 tests)

**Step 5: Commit**

```bash
git add src/commands/join/
git commit -m "feat(commands): implement join command with -1/-2/-t/-a/-v/-e/-o/-i options"
```

---

## Task 7: Implement `sort` command (basic)

**Files:**
- Create: `src/commands/sort/mod.rs`
- Create: `src/commands/sort/comparator.rs`
- Modify: `src/commands/mod.rs` (add `pub mod sort;`)

**Step 1: Write the failing tests for basic sort**

```rust
// src/commands/sort/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

mod comparator;

pub struct SortCommand;

#[async_trait]
impl Command for SortCommand {
    fn name(&self) -> &'static str { "sort" }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::error("not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
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
    async fn test_sort_alphabetical() {
        let ctx = make_ctx(vec![], "banana\napple\ncherry\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn test_sort_reverse() {
        let ctx = make_ctx(vec!["-r"], "a\nb\nc\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "c\nb\na\n");
    }

    #[tokio::test]
    async fn test_sort_numeric() {
        let ctx = make_ctx(vec!["-n"], "10\n2\n1\n20\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "1\n2\n10\n20\n");
    }

    #[tokio::test]
    async fn test_sort_numeric_reverse() {
        let ctx = make_ctx(vec!["-rn"], "10\n2\n1\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "10\n2\n1\n");
    }

    #[tokio::test]
    async fn test_sort_unique() {
        let ctx = make_ctx(vec!["-u"], "b\na\nb\nc\na\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_sort_key_field() {
        let ctx = make_ctx(vec!["-k2"], "a 3\nb 1\nc 2\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "b 1\nc 2\na 3\n");
    }

    #[tokio::test]
    async fn test_sort_stdin() {
        let ctx = make_ctx(vec![], "z\na\nm\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "a\nm\nz\n");
    }

    #[tokio::test]
    async fn test_sort_case_insensitive() {
        let ctx = make_ctx(vec!["-f"], "B\na\nC\n");
        let r = SortCommand.execute(ctx).await;
        // Case-insensitive: a, B, C
        let lines: Vec<&str> = r.stdout.lines().collect();
        assert_eq!(lines[0].to_lowercase(), "a");
        assert_eq!(lines[1].to_lowercase(), "b");
        assert_eq!(lines[2].to_lowercase(), "c");
    }

    #[tokio::test]
    async fn test_sort_file_not_found() {
        let ctx = make_ctx_with_files(vec!["/no.txt"], vec![]).await;
        let r = SortCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_empty() {
        let ctx = make_ctx(vec![], "");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "");
    }

    #[tokio::test]
    async fn test_sort_combined_nr() {
        let ctx = make_ctx(vec!["-nr"], "5\n10\n1\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "10\n5\n1\n");
    }

    #[tokio::test]
    async fn test_sort_key_range() {
        let ctx = make_ctx(vec!["-k1,2"], "a b c\nd e f\ng h i\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_key_numeric_modifier() {
        let ctx = make_ctx(vec!["-k2n"], "a 10\nb 2\nc 1\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "c 1\nb 2\na 10\n");
    }

    #[tokio::test]
    async fn test_sort_custom_delimiter() {
        let ctx = make_ctx(vec!["-t:", "-k2"], "a:3\nb:1\nc:2\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.stdout, "b:1\nc:2\na:3\n");
    }

    #[tokio::test]
    async fn test_sort_check_sorted() {
        let ctx = make_ctx(vec!["-c"], "a\nb\nc\n");
        let r = SortCommand.execute(ctx).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_sort_check_unsorted() {
        let ctx = make_ctx(vec!["-c"], "b\na\nc\n");
        let r = SortCommand.execute(ctx).await;
        assert_ne!(r.exit_code, 0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::sort -- --nocapture 2>&1 | head -30`
Expected: FAIL

**Step 3: Create comparator module**

```rust
// src/commands/sort/comparator.rs
use std::cmp::Ordering;

#[derive(Clone, Default)]
pub struct KeySpec {
    pub start_field: usize,  // 1-indexed
    pub start_char: Option<usize>,
    pub end_field: Option<usize>,
    pub end_char: Option<usize>,
    pub numeric: bool,
    pub reverse: bool,
    pub ignore_case: bool,
    pub ignore_leading: bool,
    pub human_numeric: bool,
    pub version_sort: bool,
    pub dictionary_order: bool,
    pub month_sort: bool,
}

#[derive(Clone, Default)]
pub struct SortOptions {
    pub reverse: bool,
    pub numeric: bool,
    pub unique: bool,
    pub ignore_case: bool,
    pub human_numeric: bool,
    pub version_sort: bool,
    pub dictionary_order: bool,
    pub month_sort: bool,
    pub ignore_leading: bool,
    pub stable: bool,
    pub check: bool,
    pub keys: Vec<KeySpec>,
    pub field_separator: Option<char>,
}

pub fn parse_key_spec(spec: &str) -> KeySpec {
    let mut key = KeySpec::default();
    
    // Parse "F[.C][OPTS][,F[.C][OPTS]]"
    let parts: Vec<&str> = spec.split(',').collect();
    let start_part = parts[0];
    
    // Extract modifiers from end
    let mut pos = start_part.len();
    while pos > 0 {
        let c = start_part.chars().nth(pos - 1).unwrap();
        match c {
            'n' => key.numeric = true,
            'r' => key.reverse = true,
            'f' => key.ignore_case = true,
            'b' => key.ignore_leading = true,
            'h' => key.human_numeric = true,
            'V' => key.version_sort = true,
            'd' => key.dictionary_order = true,
            'M' => key.month_sort = true,
            _ => break,
        }
        pos -= 1;
    }
    
    let field_part = &start_part[..pos];
    if let Some(dot_idx) = field_part.find('.') {
        key.start_field = field_part[..dot_idx].parse().unwrap_or(1);
        key.start_char = field_part[dot_idx + 1..].parse().ok();
    } else {
        key.start_field = field_part.parse().unwrap_or(1);
    }
    
    if parts.len() > 1 {
        let end_part = parts[1];
        // Similar parsing for end
        let mut pos = end_part.len();
        while pos > 0 && end_part.chars().nth(pos - 1).map(|c| c.is_alphabetic()).unwrap_or(false) {
            pos -= 1;
        }
        let field_part = &end_part[..pos];
        if let Some(dot_idx) = field_part.find('.') {
            key.end_field = field_part[..dot_idx].parse().ok();
            key.end_char = field_part[dot_idx + 1..].parse().ok();
        } else {
            key.end_field = field_part.parse().ok();
        }
    }
    
    key
}

pub fn extract_key(line: &str, key: &KeySpec, sep: Option<char>) -> String {
    let fields: Vec<&str> = match sep {
        Some(c) => line.split(c).collect(),
        None => line.split_whitespace().collect(),
    };
    
    let start_idx = key.start_field.saturating_sub(1);
    let end_idx = key.end_field.unwrap_or(key.start_field).saturating_sub(1);
    
    if start_idx >= fields.len() {
        return String::new();
    }
    
    let end_idx = end_idx.min(fields.len() - 1);
    let result: Vec<&str> = fields[start_idx..=end_idx].to_vec();
    
    let joined = result.join(" ");
    
    // Apply character positions if specified
    if let Some(start_char) = key.start_char {
        let chars: Vec<char> = joined.chars().collect();
        let start = start_char.saturating_sub(1);
        if start < chars.len() {
            return chars[start..].iter().collect();
        }
        return String::new();
    }
    
    joined
}

pub fn compare_values(a: &str, b: &str, opts: &SortOptions, key: Option<&KeySpec>) -> Ordering {
    let numeric = key.map(|k| k.numeric).unwrap_or(opts.numeric);
    let ignore_case = key.map(|k| k.ignore_case).unwrap_or(opts.ignore_case);
    let reverse = key.map(|k| k.reverse).unwrap_or(false);
    let human = key.map(|k| k.human_numeric).unwrap_or(opts.human_numeric);
    let version = key.map(|k| k.version_sort).unwrap_or(opts.version_sort);
    let month = key.map(|k| k.month_sort).unwrap_or(opts.month_sort);
    
    let (a, b) = if ignore_case {
        (a.to_lowercase(), b.to_lowercase())
    } else {
        (a.to_string(), b.to_string())
    };
    
    let cmp = if month {
        compare_months(&a, &b)
    } else if human {
        compare_human_sizes(&a, &b)
    } else if version {
        compare_versions(&a, &b)
    } else if numeric {
        let na: f64 = a.trim().parse().unwrap_or(0.0);
        let nb: f64 = b.trim().parse().unwrap_or(0.0);
        na.partial_cmp(&nb).unwrap_or(Ordering::Equal)
    } else {
        a.cmp(&b)
    };
    
    if reverse { cmp.reverse() } else { cmp }
}

fn compare_months(a: &str, b: &str) -> Ordering {
    fn month_num(s: &str) -> i32 {
        match s.trim().to_uppercase().get(..3) {
            Some("JAN") => 1, Some("FEB") => 2, Some("MAR") => 3,
            Some("APR") => 4, Some("MAY") => 5, Some("JUN") => 6,
            Some("JUL") => 7, Some("AUG") => 8, Some("SEP") => 9,
            Some("OCT") => 10, Some("NOV") => 11, Some("DEC") => 12,
            _ => 0,
        }
    }
    month_num(a).cmp(&month_num(b))
}

fn compare_human_sizes(a: &str, b: &str) -> Ordering {
    fn parse_size(s: &str) -> f64 {
        let s = s.trim();
        let mut num_end = 0;
        for (i, c) in s.char_indices() {
            if c.is_ascii_digit() || c == '.' || c == '-' {
                num_end = i + c.len_utf8();
            } else {
                break;
            }
        }
        let num: f64 = s[..num_end].parse().unwrap_or(0.0);
        let suffix = s[num_end..].trim().to_uppercase();
        let mult = match suffix.chars().next() {
            Some('K') => 1024.0,
            Some('M') => 1024.0 * 1024.0,
            Some('G') => 1024.0 * 1024.0 * 1024.0,
            Some('T') => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => 1.0,
        };
        num * mult
    }
    parse_size(a).partial_cmp(&parse_size(b)).unwrap_or(Ordering::Equal)
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let split_version = |s: &str| -> Vec<(bool, String)> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut is_num = false;
        for c in s.chars() {
            let c_is_num = c.is_ascii_digit();
            if !current.is_empty() && c_is_num != is_num {
                parts.push((is_num, std::mem::take(&mut current)));
            }
            current.push(c);
            is_num = c_is_num;
        }
        if !current.is_empty() {
            parts.push((is_num, current));
        }
        parts
    };
    
    let pa = split_version(a);
    let pb = split_version(b);
    
    for (a_part, b_part) in pa.iter().zip(pb.iter()) {
        let cmp = if a_part.0 && b_part.0 {
            // Both numeric
            let na: u64 = a_part.1.parse().unwrap_or(0);
            let nb: u64 = b_part.1.parse().unwrap_or(0);
            na.cmp(&nb)
        } else {
            a_part.1.cmp(&b_part.1)
        };
        if cmp != Ordering::Equal {
            return cmp;
        }
    }
    pa.len().cmp(&pb.len())
}

pub fn create_comparator(opts: &SortOptions) -> impl Fn(&str, &str) -> Ordering + '_ {
    move |a: &str, b: &str| {
        if opts.keys.is_empty() {
            let cmp = compare_values(a, b, opts, None);
            if opts.reverse { cmp.reverse() } else { cmp }
        } else {
            for key in &opts.keys {
                let ka = extract_key(a, key, opts.field_separator);
                let kb = extract_key(b, key, opts.field_separator);
                let cmp = compare_values(&ka, &kb, opts, Some(key));
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            // Fall back to whole line comparison unless stable
            if opts.stable {
                Ordering::Equal
            } else {
                let cmp = a.cmp(b);
                if opts.reverse { cmp.reverse() } else { cmp }
            }
        }
    }
}
```

**Step 4: Implement `SortCommand`**

```rust
// src/commands/sort/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

mod comparator;
use comparator::{SortOptions, create_comparator, parse_key_spec};

pub struct SortCommand;

#[async_trait]
impl Command for SortCommand {
    fn name(&self) -> &'static str { "sort" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut opts = SortOptions::default();
        let mut files: Vec<String> = Vec::new();
        let mut output_file: Option<String> = None;

        let mut i = 0;
        while i < ctx.args.len() {
            let arg = &ctx.args[i];
            match arg.as_str() {
                "-r" | "--reverse" => opts.reverse = true,
                "-n" | "--numeric-sort" => opts.numeric = true,
                "-u" | "--unique" => opts.unique = true,
                "-f" | "--ignore-case" => opts.ignore_case = true,
                "-h" | "--human-numeric-sort" => opts.human_numeric = true,
                "-V" | "--version-sort" => opts.version_sort = true,
                "-d" | "--dictionary-order" => opts.dictionary_order = true,
                "-M" | "--month-sort" => opts.month_sort = true,
                "-b" | "--ignore-leading-blanks" => opts.ignore_leading = true,
                "-s" | "--stable" => opts.stable = true,
                "-c" | "--check" => opts.check = true,
                "-k" | "--key" => {
                    i += 1;
                    if let Some(spec) = ctx.args.get(i) {
                        opts.keys.push(parse_key_spec(spec));
                    }
                }
                "-t" | "--field-separator" => {
                    i += 1;
                    opts.field_separator = ctx.args.get(i).and_then(|s| s.chars().next());
                }
                "-o" | "--output" => {
                    i += 1;
                    output_file = ctx.args.get(i).cloned();
                }
                "--help" => return CommandResult::success(
                    "Usage: sort [OPTION]... [FILE]...\n\
                     Sort lines of text files.\n\n\
                     Options:\n  -r  reverse\n  -n  numeric\n  -u  unique\n  \
                     -f  ignore case\n  -k KEY  sort by key\n  -t SEP  field separator\n  \
                     -c  check sorted\n  -h  human numeric\n  -V  version sort\n  \
                     -M  month sort\n".to_string()
                ),
                _ if arg.starts_with("-k") => {
                    opts.keys.push(parse_key_spec(&arg[2..]));
                }
                _ if arg.starts_with("-t") => {
                    opts.field_separator = arg.chars().nth(2);
                }
                _ if arg.starts_with("-o") => {
                    output_file = Some(arg[2..].to_string());
                }
                _ if arg.starts_with("--key=") => {
                    opts.keys.push(parse_key_spec(&arg[6..]));
                }
                _ if arg.starts_with("--output=") => {
                    output_file = Some(arg[9..].to_string());
                }
                // Handle combined flags like -rn, -nr
                _ if arg.starts_with('-') && arg.len() > 1 && !arg.starts_with("--") => {
                    for c in arg[1..].chars() {
                        match c {
                            'r' => opts.reverse = true,
                            'n' => opts.numeric = true,
                            'u' => opts.unique = true,
                            'f' => opts.ignore_case = true,
                            'h' => opts.human_numeric = true,
                            'V' => opts.version_sort = true,
                            'd' => opts.dictionary_order = true,
                            'M' => opts.month_sort = true,
                            'b' => opts.ignore_leading = true,
                            's' => opts.stable = true,
                            'c' => opts.check = true,
                            _ => {}
                        }
                    }
                }
                _ if !arg.starts_with('-') => files.push(arg.clone()),
                _ => {}
            }
            i += 1;
        }

        let input = if let Some(file) = files.first() {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::error(format!("sort: {}: No such file or directory\n", file)),
            }
        } else {
            ctx.stdin.clone()
        };

        if input.is_empty() {
            return CommandResult::success(String::new());
        }

        let mut lines: Vec<&str> = input.lines().collect();
        let comparator = create_comparator(&opts);

        if opts.check {
            for i in 1..lines.len() {
                if comparator(lines[i - 1], lines[i]) == std::cmp::Ordering::Greater {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("sort: disorder: {}\n", lines[i]),
                        1,
                    );
                }
            }
            return CommandResult::success(String::new());
        }

        lines.sort_by(|a, b| comparator(a, b));

        if opts.unique {
            lines.dedup_by(|a, b| comparator(a, b) == std::cmp::Ordering::Equal);
        }

        let output = lines.join("\n") + "\n";

        if let Some(out_path) = output_file {
            let path = ctx.fs.resolve_path(&ctx.cwd, &out_path);
            if let Err(e) = ctx.fs.write_file(&path, output.as_bytes()).await {
                return CommandResult::error(format!("sort: {}: {:?}\n", out_path, e));
            }
            return CommandResult::success(String::new());
        }

        CommandResult::success(output)
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test --lib commands::sort -- --nocapture`
Expected: ALL PASS (16 tests)

**Step 6: Commit**

```bash
git add src/commands/sort/
git commit -m "feat(commands): implement sort command with -r/-n/-u/-f/-k/-t/-c/-h/-V/-M options"
```

---

## Task 8: Update module exports and registry

**Files:**
- Modify: `src/commands/mod.rs`
- Modify: `src/commands/registry.rs`

**Step 1: Update mod.rs**

Add all new modules:

```rust
// Add to src/commands/mod.rs
pub mod uniq;
pub mod cut;
pub mod nl;
pub mod tr;
pub mod paste;
pub mod join;
pub mod sort;
```

**Step 2: Update registry.rs**

Add batch B registration function:

```rust
// Add imports at top of registry.rs
use super::uniq::UniqCommand;
use super::cut::CutCommand;
use super::nl::NlCommand;
use super::tr::TrCommand;
use super::paste::PasteCommand;
use super::join::JoinCommand;
use super::sort::SortCommand;

/// 注册批次 B 的所有命令
pub fn register_batch_b(registry: &mut CommandRegistry) {
    registry.register(Box::new(UniqCommand));
    registry.register(Box::new(CutCommand));
    registry.register(Box::new(NlCommand));
    registry.register(Box::new(TrCommand));
    registry.register(Box::new(PasteCommand));
    registry.register(Box::new(JoinCommand));
    registry.register(Box::new(SortCommand));
}

/// 创建包含批次 A 和 B 命令的注册表
pub fn create_batch_ab_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    registry
}
```

**Step 3: Update mod.rs exports**

```rust
// Update exports in src/commands/mod.rs
pub use registry::{CommandRegistry, register_batch_a, register_batch_b, create_batch_a_registry, create_batch_ab_registry};
```

**Step 4: Run all tests**

Run: `cargo test --lib`
Expected: ALL PASS (900+ tests including batch A and B)

**Step 5: Commit**

```bash
git add src/commands/mod.rs src/commands/registry.rs
git commit -m "feat(commands): add batch B registration and exports"
```

---

## Task 9: Update migration roadmap

**Files:**
- Modify: `docs/plans/migration-roadmap.md`

**Step 1: Update batch B status**

Mark batch B commands as completed in the roadmap.

**Step 2: Commit**

```bash
git add docs/plans/migration-roadmap.md
git commit -m "docs: mark batch B commands as completed"
```

---

## Summary

This plan implements 7 text processing commands for Batch B:

| Command | Options | Tests | Description |
|---------|---------|-------|-------------|
| uniq | -c/-d/-u/-i | 11 | Remove adjacent duplicates |
| cut | -c/-f/-d/-s | 12 | Extract fields/characters |
| nl | -b/-n/-w/-s/-v/-i | 13 | Number lines |
| tr | -d/-s/-c | 13 | Translate/delete characters |
| paste | -d/-s | 9 | Merge files side-by-side |
| join | -1/-2/-t/-a/-v/-e/-o/-i | 11 | Relational join |
| sort | -r/-n/-u/-f/-k/-t/-c/-h/-V/-M | 16 | Sort lines |

**Total new code:** ~2,500 lines Rust
**Total new tests:** ~85 test cases
**Estimated total tests after completion:** ~920 (834 + 85)
