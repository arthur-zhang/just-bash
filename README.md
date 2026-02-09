# just-bash

A secure, sandboxed Bash interpreter written in Rust — designed for AI agents and automated script execution.

[中文文档](README.zh-CN.md)

## Overview

just-bash is a complete Bash parser, interpreter, and execution environment implemented in Rust. It features a virtual in-memory filesystem, 70+ Unix command implementations, network security controls, and a Vercel-compatible Sandbox API.

## Features

- **Full Bash Parser** — Lexer and recursive descent parser for bash syntax
- **Execution Engine** — Variable expansion, control flow, pipes, redirections, subshells
- **Virtual Filesystem** — In-memory filesystem with directories, files, symlinks, and permissions
- **70+ Unix Commands** — cat, grep, sed, awk, jq, yq, curl, find, tar, and more
- **jq-compatible Query Engine** — 150+ builtin functions for JSON processing
- **Network Security** — URL allow-list for controlled network access
- **Sandbox API** — Vercel-compatible interface for programmatic execution
- **CLI Binary** — Run bash scripts from the command line with JSON output support

## Getting Started

```bash
# Build
cargo build --release

# Run a script
./target/release/just-bash -c 'echo "Hello, World!"'

# Run from file
./target/release/just-bash script.sh

# JSON output mode
./target/release/just-bash -c 'echo hello' --json
```

## CLI Options

| Option | Description |
|--------|-------------|
| `-c <script>` | Execute script from argument |
| `<file>` | Execute script from file |
| `--json` | Output as JSON (stdout, stderr, exitCode) |
| `--cwd <path>` | Set working directory |
| `-e, --errexit` | Exit on first error |

## Usage as Library

```rust
use just_bash::bash::{Bash, BashOptions};

#[tokio::main]
async fn main() {
    let mut bash = Bash::new(BashOptions::default()).await;
    let result = bash.exec("echo 'Hello'", None).await;
    println!("{}", result.stdout);
}
```

## Usage with Sandbox API

```rust
use just_bash::sandbox::{Sandbox, SandboxOptions};

#[tokio::main]
async fn main() {
    let mut sandbox = Sandbox::create(None).await;
    let result = sandbox.run_command("echo 'test'", None).await;
    println!("Exit code: {}", result.exit_code);
}
```

## Supported Commands

| Category | Commands |
|----------|----------|
| File Operations | `cat`, `head`, `tail`, `wc`, `ls`, `mkdir`, `rm`, `cp`, `mv`, `touch`, `basename`, `dirname`, `grep`, `test`/`[` |
| Text Processing | `uniq`, `cut`, `nl`, `tr`, `paste`, `join`, `sort`, `sed`, `awk` |
| Data Processing | `jq`, `yq` |
| Advanced Tools | `base64`, `diff`, `gzip`/`gunzip`/`zcat`, `find`, `tar`, `xargs`, `curl` |
| Builtins | `cd`, `echo`, `printf`, `read`, `export`, `declare`, `local`, `set`, `eval`, `source`, and more |

## License

MIT
