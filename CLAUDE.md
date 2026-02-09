# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

just-bash is a sandboxed Bash interpreter written in Rust, designed for AI agents and automated script execution. It implements a full bash parser, interpreter, virtual in-memory filesystem, 70+ Unix commands, network security controls, and a Vercel-compatible Sandbox API.

## Build & Development Commands

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo test                           # Run all tests (675+ unit tests)
cargo test <test_name>               # Run specific test by name
cargo test -- --nocapture            # Show stdout during tests
cargo fmt                            # Format code
cargo clippy                         # Lint
```

## Architecture

The crate exposes both a library (`just_bash`) and a CLI binary (`just-bash`).

### Execution Pipeline

```
Input → parser::parse() → AST → ExecutionEngine::execute_script() → Result
```

### Module Map

- **`parser/`** — Recursive descent parser: lexer → tokenizer → 12 specialized parsers (arithmetic, conditional, compound, expansion, word, command, etc.) → AST
- **`ast/`** — AST node type definitions (`types.rs`, ~30KB) shared between parser and interpreter
- **`interpreter/`** — Execution engine, the largest module (~900KB across 25 files):
  - `execution_engine.rs` — Main execution loop, statement dispatch
  - `word_expansion.rs` — Variable/parameter/command substitution
  - `arithmetic.rs` — Arithmetic expression evaluation
  - `pipeline_execution.rs` — Pipe chains and subshell execution
  - `control_flow.rs` — if/for/while/until/case
  - `builtin_dispatch.rs` — Routes to 30+ builtin commands in `builtins/`
  - `redirections.rs` — I/O redirection handling
  - `expansion/` — 26 modules for different expansion types (brace, tilde, parameter ops, pattern removal, command substitution, etc.)
  - `helpers/` — 25 modules for condition evaluation, file tests, string comparison, etc.
- **`commands/`** — 39 Unix command implementations, each implementing the async `Command` trait. Complex commands have their own subdirectories: `awk/` (14 files), `sed/` (8 files), `curl/` (7 files), `jq/`, `yq/`, `query_engine/` (shared jq/yq engine, 10 files)
- **`fs/`** — Virtual in-memory filesystem (`InMemoryFs`). No real filesystem access. Supports directories, files, symlinks, permissions, and standard /dev, /proc entries
- **`sandbox/`** — Vercel-compatible Sandbox API with execution limits (recursion depth, command count, loop iterations)
- **`network/`** — URL allow-list enforcement, HTTP method restrictions, redirect validation
- **`shell/`** — Glob expansion with extended glob support (`@()`, `*()`, `+()`, `?()`, `!()`)

### Key Design Decisions

- **Async throughout**: Uses Tokio runtime; `block_in_place` bridges sync execution engine code
- **No real filesystem**: All file operations go through the virtual `InMemoryFs`
- **Execution limits**: Configurable recursion depth, command count, and iteration limits prevent runaway scripts
- **Network allow-list**: All HTTP requests must pass URL allow-list validation
- **`InterpreterState`**: Central state object tracking variables, functions, shell options, and execution context — threaded through the interpreter

### Adding a New Command

Implement the async `Command` trait from `src/commands/mod.rs`, then register it in the command dispatch table. See existing commands in `src/commands/` for patterns.
