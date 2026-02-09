# Phase 4: Interface Layer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the Sandbox API (Vercel-compatible) and CLI binary for the just-bash Rust project.

**Architecture:** The Sandbox module wraps `Bash` in a Vercel-compatible API with `SandboxCommand` for async execution tracking. The CLI binary uses `clap` for argument parsing and reads scripts from `-c`, file, or stdin. Both modules build on the existing `Bash` struct and `InMemoryFs`.

**Tech Stack:** Rust, tokio, clap (already in Cargo.toml), serde_json (already in Cargo.toml), base64 (already in Cargo.toml)

---

## Overview

| Task | Module | Description |
|------|--------|-------------|
| 1 | sandbox/types.rs | Sandbox types and SandboxCommand |
| 2 | sandbox/sandbox.rs | Sandbox struct with create/runCommand/writeFiles/readFile |
| 3 | sandbox/mod.rs + tests | Module registration and integration tests |
| 4 | cli (main.rs) | CLI binary with clap, -c/file/stdin, --json |
| 5 | Roadmap update | Update migration-roadmap.md |

---

### Task 1: Sandbox Types and SandboxCommand

**Files:**
- Create: `src/sandbox/types.rs`
- Create: `src/sandbox/mod.rs`

**Context:**
- TypeScript source: `jakarta/src/sandbox/Command.ts` (92 lines)
- Rust `ExecResult` is at `src/interpreter/types.rs:497` — has `stdout`, `stderr`, `exit_code`, `env`
- Rust `Bash` is at `src/bash.rs` — has `exec()`, `read_file()`, `write_file()`, `get_cwd()`
- Rust `ExecutionLimits` is at `src/interpreter/types.rs:538`
- Rust `NetworkConfig` is at `src/network/types.rs`

**What to implement in `src/sandbox/types.rs`:**

```rust
use std::collections::HashMap;
use crate::interpreter::types::ExecResult;

/// Output message type (stdout or stderr)
#[derive(Debug, Clone, PartialEq)]
pub enum OutputType {
    Stdout,
    Stderr,
}

/// A single output message from command execution.
#[derive(Debug, Clone)]
pub struct OutputMessage {
    pub output_type: OutputType,
    pub data: String,
}

/// Options for creating a Sandbox.
#[derive(Debug, Default)]
pub struct SandboxOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub max_call_depth: Option<u32>,
    pub max_command_count: Option<u64>,
    pub max_loop_iterations: Option<u64>,
}

/// Options for running a command.
#[derive(Debug, Default)]
pub struct RunCommandOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

/// Input for writing files. Values are either plain strings or
/// `{ content, encoding }` objects.
#[derive(Debug, Clone)]
pub enum FileContent {
    Text(String),
    Encoded { content: String, encoding: FileEncoding },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileEncoding {
    Utf8,
    Base64,
}

/// Result of a completed command execution.
#[derive(Debug, Clone)]
pub struct SandboxCommand {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl SandboxCommand {
    pub fn from_exec_result(result: &ExecResult) -> Self {
        Self {
            exit_code: result.exit_code,
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
        }
    }

    /// Get combined stdout + stderr output.
    pub fn output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Get output messages as a Vec.
    pub fn logs(&self) -> Vec<OutputMessage> {
        let mut messages = Vec::new();
        if !self.stdout.is_empty() {
            messages.push(OutputMessage {
                output_type: OutputType::Stdout,
                data: self.stdout.clone(),
            });
        }
        if !self.stderr.is_empty() {
            messages.push(OutputMessage {
                output_type: OutputType::Stderr,
                data: self.stderr.clone(),
            });
        }
        messages
    }
}
```

**Tests (in `src/sandbox/types.rs`):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_command_from_exec_result() { ... }

    #[test]
    fn test_sandbox_command_output_combined() { ... }

    #[test]
    fn test_sandbox_command_logs_both() { ... }

    #[test]
    fn test_sandbox_command_logs_empty() { ... }

    #[test]
    fn test_sandbox_options_default() { ... }

    #[test]
    fn test_file_content_variants() { ... }

    #[test]
    fn test_file_encoding_equality() { ... }

    #[test]
    fn test_output_type_equality() { ... }
}
```

Write 8 tests covering: `from_exec_result` mapping, `output()` concatenation, `logs()` with both/empty, defaults, enum variants.

**Step 1:** Create `src/sandbox/mod.rs` with `pub mod types;` and re-exports.
**Step 2:** Write the types and tests in `src/sandbox/types.rs`.
**Step 3:** Add `pub mod sandbox;` to `src/lib.rs`.
**Step 4:** Run `cargo test sandbox::types` — expect all 8 tests pass.
**Step 5:** Commit.

---

### Task 2: Sandbox Struct

**Files:**
- Create: `src/sandbox/sandbox.rs`
- Modify: `src/sandbox/mod.rs` — add `pub mod sandbox;`

**Context:**
- TypeScript source: `jakarta/src/sandbox/Sandbox.ts` (137 lines)
- `Bash::new(BashOptions)` at `src/bash.rs:45` — async constructor
- `Bash::exec(&mut self, script, options)` at `src/bash.rs:110`
- `Bash::read_file(&self, path)` at `src/bash.rs:175`
- `Bash::write_file(&self, path, content)` at `src/bash.rs:181`
- `BashOptions` at `src/bash.rs:14` — `env`, `cwd`, `fs`, `limits`
- `ExecOptions` at `src/bash.rs:27` — `env`, `cwd`, `raw_script`
- `ExecutionLimits` at `src/interpreter/types.rs:538` — `max_recursion_depth`, `max_command_count`, `max_iterations`
- No OverlayFs in Rust yet — use `InMemoryFs` only (OverlayFs is out of scope for this phase)
- base64 crate already in Cargo.toml for decoding base64 file content

**What to implement in `src/sandbox/sandbox.rs`:**

```rust
use std::collections::HashMap;
use crate::bash::{Bash, BashOptions, ExecOptions};
use crate::interpreter::types::ExecutionLimits;
use crate::fs::MkdirOptions;
use super::types::*;

pub struct Sandbox {
    bash: Bash,
}

impl Sandbox {
    /// Create a new Sandbox with the given options.
    pub async fn create(opts: Option<SandboxOptions>) -> Self {
        let opts = opts.unwrap_or_default();
        let limits = ExecutionLimits {
            max_recursion_depth: opts.max_call_depth.unwrap_or(1000),
            max_command_count: opts.max_command_count.unwrap_or(100_000),
            max_iterations: opts.max_loop_iterations.unwrap_or(1_000_000),
        };
        let bash = Bash::new(BashOptions {
            env: opts.env,
            cwd: opts.cwd,
            fs: None,
            limits: Some(limits),
        }).await;
        Self { bash }
    }

    /// Execute a command in the sandbox.
    pub async fn run_command(
        &mut self,
        cmd: &str,
        opts: Option<RunCommandOptions>,
    ) -> SandboxCommand {
        let exec_opts = opts.map(|o| ExecOptions {
            env: o.env,
            cwd: o.cwd,
            raw_script: false,
        });
        let result = self.bash.exec(cmd, exec_opts).await;
        SandboxCommand::from_exec_result(&result)
    }

    /// Write multiple files to the sandbox filesystem.
    /// Parent directories are created automatically.
    pub async fn write_files(
        &mut self,
        files: HashMap<String, FileContent>,
    ) -> Result<(), String> {
        for (path, content) in &files {
            let data = match content {
                FileContent::Text(s) => s.clone(),
                FileContent::Encoded { content: c, encoding } => {
                    match encoding {
                        FileEncoding::Base64 => {
                            use base64::Engine;
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(c)
                                .map_err(|e| format!("base64 decode error: {}", e))?;
                            String::from_utf8(bytes)
                                .map_err(|e| format!("utf-8 decode error: {}", e))?
                        }
                        FileEncoding::Utf8 => c.clone(),
                    }
                }
            };
            // Ensure parent directory exists
            if let Some(last_slash) = path.rfind('/') {
                let parent = if last_slash == 0 { "/" } else { &path[..last_slash] };
                if parent != "/" {
                    let _ = self.bash.fs.mkdir(parent, &MkdirOptions { recursive: true }).await;
                }
            }
            self.bash.write_file(path, &data).await
                .map_err(|e| format!("write error for {}: {}", path, e))?;
        }
        Ok(())
    }

    /// Read a file from the sandbox filesystem.
    pub async fn read_file(
        &self,
        path: &str,
        encoding: Option<FileEncoding>,
    ) -> Result<String, String> {
        let content = self.bash.read_file(path).await
            .map_err(|e| format!("read error: {}", e))?;
        match encoding {
            Some(FileEncoding::Base64) => {
                use base64::Engine;
                Ok(base64::engine::general_purpose::STANDARD.encode(content.as_bytes()))
            }
            _ => Ok(content),
        }
    }

    /// Create a directory in the sandbox.
    pub async fn mkdir(
        &self,
        path: &str,
        recursive: bool,
    ) -> Result<(), String> {
        self.bash.fs.mkdir(path, &MkdirOptions { recursive }).await
            .map_err(|e| format!("mkdir error: {}", e))
    }

    /// Get current working directory.
    pub fn get_cwd(&self) -> &str {
        self.bash.get_cwd()
    }
}
```

**Tests (in `src/sandbox/sandbox.rs`):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_create_default() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_create_with_options() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_echo() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_with_cwd() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_with_env() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_exit_code() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_and_read_files() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_files_base64() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_files_creates_parent_dirs() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_read_file_base64_encoding() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_read_file_not_found() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_mkdir() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_mkdir_recursive() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_multiple_commands_share_state() { ... }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_get_cwd() { ... }
}
```

Write 15 tests covering: create default/with-options, run_command (echo, cwd, env, exit code), write_files (text, base64, parent dirs), read_file (normal, base64, not found), mkdir, state sharing, get_cwd.

**Step 1:** Add `pub mod sandbox;` to `src/sandbox/mod.rs` and add re-exports.
**Step 2:** Write `src/sandbox/sandbox.rs` with struct and all methods.
**Step 3:** Write all 15 tests.
**Step 4:** Run `cargo test sandbox::sandbox` — expect all 15 tests pass.
**Step 5:** Commit.

---

### Task 3: Sandbox Module Registration and Integration Tests

**Files:**
- Modify: `src/sandbox/mod.rs` — finalize re-exports
- Modify: `src/lib.rs` — verify `pub mod sandbox;` and add re-exports

**What to do:**

1. Ensure `src/sandbox/mod.rs` has complete re-exports:
```rust
pub mod types;
pub mod sandbox;

pub use types::{SandboxOptions, SandboxCommand, RunCommandOptions, FileContent, FileEncoding, OutputMessage, OutputType};
pub use sandbox::Sandbox;
```

2. Add to `src/lib.rs` re-exports:
```rust
pub use sandbox::Sandbox;
```

3. Run full test suite: `cargo test` — all tests pass.
4. Commit.

---

### Task 4: CLI Binary (main.rs)

**Files:**
- Modify: `src/main.rs` — replace placeholder with full CLI

**Context:**
- TypeScript source: `jakarta/src/cli/just-bash.ts` (329 lines)
- `clap` crate already in Cargo.toml with `derive` feature
- `serde_json` already in Cargo.toml for `--json` output
- `tokio` already in Cargo.toml with `rt-multi-thread` and `macros`
- Binary target already configured in Cargo.toml: `[[bin]] name = "just-bash" path = "src/main.rs"`
- No OverlayFs in Rust — CLI uses InMemoryFs (real filesystem overlay is out of scope)
- The CLI should support: `-c <script>`, script file (read from InMemoryFs), stdin, `--json`, `-e`/`--errexit`, `--cwd`, `--help`, `--version`

**What to implement in `src/main.rs`:**

```rust
use clap::Parser;
use std::io::Read;
use just_bash::bash::{Bash, BashOptions};

#[derive(Parser)]
#[command(name = "just-bash")]
#[command(about = "A secure bash environment for AI agents")]
#[command(version)]
struct Cli {
    /// Execute the script from command line argument
    #[arg(short = 'c')]
    script: Option<String>,

    /// Exit immediately if a command exits with non-zero status
    #[arg(short = 'e', long = "errexit")]
    errexit: bool,

    /// Working directory within the sandbox
    #[arg(long = "cwd")]
    cwd: Option<String>,

    /// Output results as JSON (stdout, stderr, exitCode)
    #[arg(long = "json")]
    json: bool,

    /// Script file to execute
    #[arg()]
    script_file: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Determine script source: -c, file, or stdin
    let script = if let Some(s) = cli.script {
        s
    } else if let Some(ref file) = cli.script_file {
        match std::fs::read_to_string(file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Error: Cannot read script file: {}: {}", file, e);
                std::process::exit(1);
            }
        }
    } else if atty::is(atty::Stream::Stdin) {
        // No script provided and stdin is a TTY — show help
        // clap will handle --help, but if no args at all, print usage
        eprintln!("Error: No script provided. Use -c 'script', provide a script file, or pipe via stdin.");
        std::process::exit(1);
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).unwrap_or_default();
        buf
    };

    if script.trim().is_empty() {
        if cli.json {
            println!("{}", serde_json::json!({"stdout": "", "stderr": "", "exitCode": 0}));
        }
        std::process::exit(0);
    }

    let mut bash = Bash::new(BashOptions {
        cwd: cli.cwd,
        ..Default::default()
    }).await;

    // Prepend set -e if errexit
    let final_script = if cli.errexit {
        format!("set -e\n{}", script)
    } else {
        script
    };

    let result = bash.exec(&final_script, None).await;

    if cli.json {
        println!("{}", serde_json::json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exitCode": result.exit_code,
        }));
    } else {
        if !result.stdout.is_empty() {
            print!("{}", result.stdout);
        }
        if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
        }
    }

    std::process::exit(result.exit_code);
}
```

**Note on stdin TTY detection:** Instead of adding the `atty` crate, we can use `std::io::IsTerminal` (stable since Rust 1.70). Replace `atty::is(atty::Stream::Stdin)` with `std::io::stdin().is_terminal()`.

**Step 1:** Replace `src/main.rs` with the full CLI implementation.
**Step 2:** Run `cargo build` — expect success.
**Step 3:** Test manually:
  - `echo 'echo hello' | cargo run --` → prints `hello`
  - `cargo run -- -c 'echo world'` → prints `world`
  - `cargo run -- -c 'echo test' --json` → prints JSON
  - `cargo run -- -c 'false' -e` → exits with code 1
**Step 4:** Commit.

---

### Task 5: Roadmap Update

**Files:**
- Modify: `docs/plans/migration-roadmap.md`

**What to do:**

1. Update Phase 4 section with completion details:
   - sandbox/types.rs — types and SandboxCommand
   - sandbox/sandbox.rs — Sandbox struct
   - main.rs — CLI binary
2. Update test count table.
3. Mark Phase 4 as complete in the progress checklist.
4. Run `cargo test` one final time to confirm total test count.
5. Commit.

---

## Verification

After all tasks are complete:
- `cargo test` — all tests pass
- `cargo build` — binary builds successfully
- `echo 'echo hello' | cargo run --` — prints `hello`
- `cargo run -- -c 'for i in 1 2 3; do echo $i; done' --json` — prints JSON output
