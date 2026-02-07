# sed Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a full-featured `sed` stream editor command in Rust, ported from the existing TypeScript implementation.

**Architecture:** The sed command is split into 5 Rust modules mirroring the TS structure: `types.rs` (AST types), `lexer.rs` (tokenizer), `parser.rs` (script parser), `regex_utils.rs` (BRE/ERE conversion), and `executor.rs` (command execution engine). The main `mod.rs` ties them together as `SedCommand` implementing the `Command` trait. All modules live under `src/commands/sed/`.

**Tech Stack:** Rust, async_trait, regex crate, Arc<dyn FileSystem> for file I/O.

---

## Module Dependency Graph

```
mod.rs (SedCommand)
  ├── types.rs (SedAddress, SedCommand enum, SedState, etc.)
  ├── parser.rs (parse_scripts → Vec<SedCmd>)
  │     └── lexer.rs (tokenize → Vec<SedToken>)
  ├── executor.rs (execute_commands, process_content)
  │     ├── types.rs
  │     └── regex_utils.rs (bre_to_ere, normalize_for_js, escape_for_list)
  └── types.rs
```

## Key Design Decisions

1. **Regex:** Use the `regex` crate. BRE patterns are converted to ERE via `bre_to_ere()`, then POSIX classes are expanded. The regex crate uses Rust/Perl syntax, so we convert accordingly.
2. **State:** `SedState` is a mutable struct passed through execution. Hold space, range states, and last pattern persist across lines.
3. **Addresses:** Enum-based: `LineNumber(usize)`, `Last`, `Pattern(String)`, `Step{first, step}`, `RelativeOffset(usize)`.
4. **No file I/O commands initially:** `r/R/w/W/e` commands require filesystem access. We implement them but skip `e` (shell execution) as it needs a shell interpreter reference.
5. **Tests:** We port the most important tests from the 213 TS tests, targeting ~80 Rust tests covering all major features.

---

### Task 1: Create types.rs — AST types and state

**Files:**
- Create: `src/commands/sed/types.rs`

**What to build:**

All the type definitions for the sed AST and execution state. This is a pure data module with no logic.

**Types needed:**

```rust
// Address types
pub enum SedAddress {
    Line(usize),
    Last,                              // $
    Pattern(String),                   // /regex/
    Step { first: usize, step: usize }, // first~step
    RelativeOffset(usize),             // +N
}

pub struct AddressRange {
    pub start: Option<SedAddress>,
    pub end: Option<SedAddress>,
    pub negated: bool,
}

// Command types — one enum variant per sed command
pub enum SedCmd {
    Substitute { address: Option<AddressRange>, pattern: String, replacement: String, global: bool, ignore_case: bool, print_on_match: bool, nth_occurrence: Option<usize>, extended_regex: bool },
    Print { address: Option<AddressRange> },
    PrintFirstLine { address: Option<AddressRange> },
    Delete { address: Option<AddressRange> },
    DeleteFirstLine { address: Option<AddressRange> },
    Append { address: Option<AddressRange>, text: String },
    Insert { address: Option<AddressRange>, text: String },
    Change { address: Option<AddressRange>, text: String },
    Hold { address: Option<AddressRange> },
    HoldAppend { address: Option<AddressRange> },
    Get { address: Option<AddressRange> },
    GetAppend { address: Option<AddressRange> },
    Exchange { address: Option<AddressRange> },
    Next { address: Option<AddressRange> },
    NextAppend { address: Option<AddressRange> },
    Quit { address: Option<AddressRange> },
    QuitSilent { address: Option<AddressRange> },
    Transliterate { address: Option<AddressRange>, source: String, dest: String },
    LineNumber { address: Option<AddressRange> },
    Branch { address: Option<AddressRange>, label: Option<String> },
    BranchOnSubst { address: Option<AddressRange>, label: Option<String> },
    BranchOnNoSubst { address: Option<AddressRange>, label: Option<String> },
    Label { name: String },
    Zap { address: Option<AddressRange> },
    Group { address: Option<AddressRange>, commands: Vec<SedCmd> },
    List { address: Option<AddressRange> },
    PrintFilename { address: Option<AddressRange> },
    Version { address: Option<AddressRange>, min_version: Option<String> },
    ReadFile { address: Option<AddressRange>, filename: String },
    ReadFileLine { address: Option<AddressRange>, filename: String },
    WriteFile { address: Option<AddressRange>, filename: String },
    WriteFirstLine { address: Option<AddressRange>, filename: String },
}

// Execution state
pub struct RangeState {
    pub active: bool,
    pub start_line: Option<usize>,
    pub completed: bool,
}

pub struct SedState {
    pub pattern_space: String,
    pub hold_space: String,
    pub line_number: usize,
    pub total_lines: usize,
    pub deleted: bool,
    pub printed: bool,
    pub quit: bool,
    pub quit_silent: bool,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub append_buffer: Vec<String>,
    pub changed_text: Option<String>,
    pub substitution_made: bool,
    pub line_number_output: Vec<String>,
    pub n_command_output: Vec<String>,
    pub restart_cycle: bool,
    pub in_d_restarted_cycle: bool,
    pub current_filename: Option<String>,
    pub pending_file_reads: Vec<PendingFileRead>,
    pub pending_file_writes: Vec<PendingFileWrite>,
    pub range_states: HashMap<String, RangeState>,
    pub last_pattern: Option<String>,
    pub branch_request: Option<String>,
    pub lines_consumed_in_cycle: usize,
}

pub struct PendingFileRead { pub filename: String, pub whole_file: bool }
pub struct PendingFileWrite { pub filename: String, pub content: String }

pub struct ExecuteContext { pub lines: Vec<String>, pub current_line_index: usize }
```

**Step 1:** Create `src/commands/sed/` directory and write `types.rs` with all types above.
**Step 2:** Verify it compiles: `cargo check`
**Step 3:** Commit: `git add src/commands/sed/types.rs && git commit -m "feat(sed): add AST types and execution state"`

---

### Task 2: Create regex_utils.rs — BRE/ERE conversion and POSIX classes

**Files:**
- Create: `src/commands/sed/regex_utils.rs`

**What to build:**

Port the TypeScript `sed-regex.ts` to Rust. Three functions:

1. `bre_to_ere(pattern: &str) -> String` — Convert BRE to ERE syntax
   - In BRE: `+`, `?`, `|`, `(`, `)` are literal; `\+`, `\?`, `\|`, `\(`, `\)` are special
   - In ERE: those chars are special without backslash
   - Handle bracket expressions `[...]` — copy contents verbatim
   - Expand POSIX classes `[[:alpha:]]` → `[a-zA-Z]`
   - Handle `^` and `$` anchor rules (only special at start/end)
   - Convert `\t`, `\n`, `\r` escape sequences

2. `normalize_for_rust(pattern: &str) -> String` — Normalize for Rust regex crate
   - Convert `{,n}` → `{0,n}` (GNU extension)
   - Handle `\]` inside bracket expressions (Rust regex uses `]` literally at start)

3. `escape_for_list(input: &str) -> String` — For `l` command
   - Show non-printable chars as escape sequences (`\t`, `\n`, `\a`, `\b`, `\f`, `\v`)
   - Show other non-printable as octal `\NNN`
   - End with `$`

**POSIX class mappings:**
```rust
fn posix_class(name: &str) -> Option<&'static str> {
    match name {
        "alnum" => Some("a-zA-Z0-9"),
        "alpha" => Some("a-zA-Z"),
        "ascii" => Some("\\x00-\\x7F"),
        "blank" => Some(" \\t"),
        "cntrl" => Some("\\x00-\\x1F\\x7F"),
        "digit" => Some("0-9"),
        "graph" => Some("!-~"),
        "lower" => Some("a-z"),
        "print" => Some(" -~"),
        "punct" => Some("!-/:-@\\[-`{-~"),
        "space" => Some(" \\t\\n\\r\\x0C\\x0B"),
        "upper" => Some("A-Z"),
        "word" => Some("a-zA-Z0-9_"),
        "xdigit" => Some("0-9A-Fa-f"),
        _ => None,
    }
}
```

**Tests (in same file):**
- `test_bre_to_ere_basic` — `\+` → `+`, `\(` → `(`
- `test_bre_literal_plus` — bare `+` → `\+`
- `test_posix_class_expansion` — `[[:alpha:]]` → `[a-zA-Z]`
- `test_negated_posix_class` — `[^[:digit:]]` → `[^0-9]`
- `test_escape_for_list` — tabs, newlines, backslashes
- `test_normalize_gnu_quantifier` — `{,3}` → `{0,3}`
- `test_bracket_expression_passthrough` — `[abc]` unchanged
- `test_bre_escape_sequences` — `\t` → literal tab, `\n` → literal newline

**Step 1:** Write `regex_utils.rs` with all 3 functions + tests.
**Step 2:** Run tests: `cargo test sed::regex_utils`
**Step 3:** Commit: `git commit -m "feat(sed): add BRE/ERE regex conversion utilities"`

---

### Task 3: Create lexer.rs — Tokenizer for sed scripts

**Files:**
- Create: `src/commands/sed/lexer.rs`

**What to build:**

Port the TypeScript `lexer.ts` to Rust. The lexer tokenizes sed scripts into a stream of tokens.

**Token types:**
```rust
pub enum SedTokenType {
    Number(usize),
    Dollar,
    Pattern(String),
    Step { first: usize, step: usize },
    RelativeOffset(usize),
    LBrace, RBrace, Semicolon, Newline, Comma, Negation,
    Command(char),  // p, d, h, H, g, G, x, n, N, P, D, q, Q, z, =, l, F
    Substitute { pattern: String, replacement: String, flags: String },
    Transliterate { source: String, dest: String },
    LabelDef(String),
    Branch { label: Option<String> },
    BranchOnSubst { label: Option<String> },
    BranchOnNoSubst { label: Option<String> },
    TextCmd { cmd: char, text: String },
    FileRead(String), FileReadLine(String),
    FileWrite(String), FileWriteLine(String),
    Execute(Option<String>),
    Version(Option<String>),
    Eof,
    Error(String),
}
```

**Key methods:**
- `pub fn tokenize(input: &str) -> Vec<SedTokenType>` — Main entry point
- Internal: `next_token()`, `read_number()`, `read_pattern()`, `read_substitute()`, `read_transliterate()`, `read_text_command()`, `read_branch()`, `read_label_def()`, `read_file_command()`, `read_execute()`, `read_version()`

**Important behaviors to port:**
- Context-sensitive tokenization (s command reads pattern/replacement/flags)
- Bracket expression handling in patterns (delimiter inside `[...]` is literal)
- Escape sequences in replacement (`\\`, `\n`, `\t`, `\<newline>`)
- Text commands (`a\`, `i\`, `c\`) with multi-line continuation
- Comment handling (`#` to end of line)
- Custom delimiter support for `s` and `y` commands

**Tests:**
- `test_tokenize_substitute` — `s/foo/bar/g`
- `test_tokenize_custom_delimiter` — `s#foo#bar#`
- `test_tokenize_address_range` — `1,3d`
- `test_tokenize_pattern_address` — `/foo/d`
- `test_tokenize_step_address` — `0~2p`
- `test_tokenize_relative_offset` — `+3`
- `test_tokenize_text_command` — `a\ text`
- `test_tokenize_branch` — `b label`
- `test_tokenize_label` — `:loop`
- `test_tokenize_transliterate` — `y/abc/xyz/`
- `test_tokenize_grouped` — `{ p; d }`

**Step 1:** Write `lexer.rs` with tokenizer + tests.
**Step 2:** Run tests: `cargo test sed::lexer`
**Step 3:** Commit: `git commit -m "feat(sed): add script lexer/tokenizer"`

---

### Task 4: Create parser.rs — Script parser

**Files:**
- Create: `src/commands/sed/parser.rs`

**What to build:**

Port the TypeScript `parser.ts` to Rust. Converts token stream into `Vec<SedCmd>` AST.

**Public API:**
```rust
pub struct ParseResult {
    pub commands: Vec<SedCmd>,
    pub error: Option<String>,
    pub silent_mode: bool,
    pub extended_regex_mode: bool,
}

pub fn parse_scripts(scripts: &[&str], extended_regex: bool) -> ParseResult
```

**Key behaviors:**
- Parse addresses (single, range, negated)
- Parse all command types from tokens
- Handle `#n` comment for silent mode, `#r` for extended regex
- Handle backslash continuation across `-e` arguments
- Validate labels (all branch targets must reference existing labels)
- Handle grouped commands `{ ... }`
- Join multiple scripts with newlines

**Tests:**
- `test_parse_simple_substitute` — `s/foo/bar/`
- `test_parse_address_range` — `1,3d`
- `test_parse_pattern_address` — `/foo/d`
- `test_parse_negated_address` — `2!d`
- `test_parse_group` — `{ p; d }`
- `test_parse_branch_label` — `:loop ... b loop`
- `test_parse_undefined_label_error` — `b nonexistent` → error
- `test_parse_silent_comment` — `#n` sets silent mode
- `test_parse_multiple_scripts` — multiple `-e` args
- `test_parse_text_commands` — `a\ text`, `i\ text`, `c\ text`

**Step 1:** Write `parser.rs` with parser + tests.
**Step 2:** Run tests: `cargo test sed::parser`
**Step 3:** Commit: `git commit -m "feat(sed): add script parser"`

---

### Task 5: Create executor.rs — Command execution engine

**Files:**
- Create: `src/commands/sed/executor.rs`

**What to build:**

Port the TypeScript `executor.ts` to Rust. This is the core execution engine.

**Public API:**
```rust
pub fn create_initial_state(total_lines: usize, filename: Option<&str>, range_states: HashMap<String, RangeState>) -> SedState;
pub fn execute_commands(commands: &[SedCmd], state: &mut SedState, ctx: &mut ExecuteContext) -> usize;
```

**Key functions to implement:**

1. **Address matching:**
   - `matches_address(addr, line_num, total_lines, line, state) -> bool`
   - `is_in_range(range, line_num, total_lines, line, range_states, state) -> bool`
   - `serialize_range(range) -> String` — for range state map keys
   - Handle step addresses, relative offsets, pattern ranges with state tracking

2. **Substitution:**
   - `global_replace(input, regex, replace_fn) -> String` — POSIX-compliant zero-length match handling
   - `process_replacement(replacement, match_text, groups) -> String` — Handle `&`, `\1`-`\9`, `\n`, `\t`, `\r`

3. **Command execution:**
   - `execute_command(cmd, state)` — dispatch to specific command handlers
   - Handle all command types: substitute, print, delete, hold space, branching, etc.
   - `execute_transliterate(input, source, dest) -> String`

4. **Branching:**
   - Build label index for O(1) lookup
   - Handle `b`, `t`, `T` with label resolution
   - Cross-group branching via `branch_request`
   - Iteration limit (10,000) to prevent infinite loops

5. **Special commands:**
   - `n` — print pattern space, read next line (inline via ExecuteContext)
   - `N` — append next line to pattern space
   - `D` — delete first line, restart cycle
   - `q`/`Q` — quit with/without printing

**Tests:**
- `test_substitute_basic` — `s/foo/bar/`
- `test_substitute_global` — `s/foo/bar/g`
- `test_substitute_case_insensitive` — `s/foo/bar/i`
- `test_substitute_nth_occurrence` — `s/foo/bar/2`
- `test_substitute_backreference` — `s/\(foo\)/[\1]/`
- `test_substitute_ampersand` — `s/foo/[&]/`
- `test_delete_command` — `d`
- `test_print_command` — `p`
- `test_hold_space_operations` — `h`, `H`, `g`, `G`, `x`
- `test_append_insert_change` — `a`, `i`, `c`
- `test_branch_unconditional` — `b label`
- `test_branch_on_subst` — `t label`
- `test_branch_on_no_subst` — `T label`
- `test_next_command` — `n`
- `test_next_append_command` — `N`
- `test_delete_first_line` — `D`
- `test_transliterate` — `y/abc/xyz/`
- `test_line_number` — `=`
- `test_quit_commands` — `q`, `Q`
- `test_zap_command` — `z`
- `test_list_command` — `l`
- `test_grouped_commands` — `{ p; d }`
- `test_address_range_matching` — `1,3`, `/start/,/end/`
- `test_negated_address` — `2!d`
- `test_step_address` — `0~2`
- `test_relative_offset` — `/pattern/,+3`
- `test_iteration_limit` — infinite loop protection

**Step 1:** Write `executor.rs` with all execution logic + tests.
**Step 2:** Run tests: `cargo test sed::executor`
**Step 3:** Commit: `git commit -m "feat(sed): add command execution engine"`

---

### Task 6: Create mod.rs — SedCommand entry point and integration

**Files:**
- Create: `src/commands/sed/mod.rs`

**What to build:**

The main `SedCommand` struct implementing the `Command` trait. This ties together parsing and execution.

**Public API:**
```rust
pub struct SedCommand;

#[async_trait]
impl Command for SedCommand {
    fn name(&self) -> &'static str { "sed" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult { ... }
}
```

**Key logic in `execute()`:**

1. **Argument parsing:**
   - `-n`/`--quiet`/`--silent` — suppress auto-print
   - `-e script` — add script (can be repeated)
   - `-f script-file` — read script from file
   - `-i`/`--in-place` — edit files in place
   - `-E`/`-r`/`--regexp-extended` — use ERE
   - First non-option arg without `-e`/`-f` is the script
   - Remaining non-option args are input files

2. **`process_content()` function:**
   - Split content into lines
   - Track trailing newline behavior
   - For each line: create state, execute commands, collect output
   - Handle `D` command cycle restart
   - Handle `n`/`N` inline line consumption
   - Persist hold space, last pattern, range states across lines
   - Process pending file reads/writes
   - Strip trailing newline if input didn't have one

3. **File handling:**
   - Read from stdin if no files
   - Read from files, concatenating with newlines
   - `-` means stdin (consumed once)
   - In-place editing: read file, process, write back

**Integration tests (in same file, using InMemoryFs):**
- `test_basic_substitution` — `echo "hello world" | sed 's/world/rust/'`
- `test_global_substitution` — `s/o/0/g`
- `test_silent_mode` — `-n` with `p`
- `test_line_range` — `1,3d`
- `test_pattern_match` — `/foo/d`
- `test_multiple_expressions` — `-e 's/a/b/' -e 's/c/d/'`
- `test_in_place_editing` — `-i` flag
- `test_extended_regex` — `-E` with `+`, `?`, `|`
- `test_hold_space_workflow` — `h;G` pattern
- `test_append_insert_change` — `a\`, `i\`, `c\`
- `test_branching` — `:label ... b label`
- `test_quit_commands` — `q`, `Q`
- `test_step_address` — `0~2p`
- `test_relative_offset` — `/pattern/,+2d`
- `test_transliterate` — `y/abc/ABC/`
- `test_delete_first_line_cycle` — `N;P;D` sliding window
- `test_empty_script` — passes through input
- `test_no_script_error` — error when no script given
- `test_file_not_found` — error for missing file
- `test_script_file` — `-f` flag
- `test_case_insensitive` — `s/foo/bar/i`
- `test_nth_occurrence` — `s/foo/bar/2`
- `test_backreferences` — `s/\(word\)/[\1]/`
- `test_ampersand_replacement` — `s/foo/[&]/`
- `test_negated_address` — `2!d`
- `test_pattern_range` — `/start/,/end/d`
- `test_last_line_address` — `$d`
- `test_list_command` — `l` with escapes
- `test_line_number_command` — `=`
- `test_zap_command` — `z`
- `test_print_first_line` — `P`
- `test_grouped_commands` — `{ p; d }`
- `test_substitution_tracking` — `t` and `T` commands
- `test_trailing_newline_preservation` — input without trailing newline
- `test_read_file_command` — `r filename`
- `test_write_file_command` — `w filename`
- `test_stdin_marker` — `-` as file argument
- `test_multiple_files` — concatenation behavior
- `test_bre_mode` — BRE patterns (default)
- `test_ere_mode` — ERE patterns with `-E`
- `test_posix_classes` — `[[:alpha:]]`, `[[:digit:]]`

**Step 1:** Write `mod.rs` with SedCommand + process_content + all integration tests.
**Step 2:** Run tests: `cargo test sed`
**Step 3:** Commit: `git commit -m "feat(sed): add SedCommand entry point with integration tests"`

---

### Task 7: Register sed command and update exports

**Files:**
- Modify: `src/commands/mod.rs` — add `pub mod sed;`
- Modify: `src/commands/registry.rs` — add sed to batch B supplement

**Changes:**

In `src/commands/mod.rs`, add:
```rust
pub mod sed;
```

In `src/commands/registry.rs`, add:
```rust
use super::sed::SedCommand;
```
And in `register_batch_b()`:
```rust
registry.register(Box::new(SedCommand));
```

**Step 1:** Update both files.
**Step 2:** Run all tests: `cargo test`
**Step 3:** Commit: `git commit -m "feat(commands): register sed command in batch B"`

---

### Task 8: Update migration roadmap

**Files:**
- Modify: `docs/plans/migration-roadmap.md`

**Changes:**
- Mark sed as completed in the roadmap
- Update test count
- Note that awk is still pending

**Step 1:** Update roadmap.
**Step 2:** Commit: `git commit -m "docs: mark sed command as completed in migration roadmap"`

---

## Execution Notes for Subagents

Each task should be dispatched to a subagent with the following context:

1. **Project location:** `/Users/arthur/conductor/workspaces/just-bash-v1/san-jose`
2. **TypeScript source reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/sed/`
3. **Existing Rust patterns:** See `src/commands/sort/mod.rs` + `src/commands/sort/comparator.rs` for multi-file module pattern
4. **Command trait:** `src/commands/types.rs` — `Command` trait with `name()` and `execute()`
5. **Test pattern:** Use `InMemoryFs` from `crate::fs::InMemoryFs`, create `CommandContext` with args/stdin/cwd/env/fs
6. **Regex crate:** Already in `Cargo.toml` as a dependency

**Critical implementation details:**
- The Rust `regex` crate does NOT support backreferences (`\1`). For substitution backreferences, use `regex::Captures` and manual replacement processing.
- POSIX character classes like `[:alpha:]` must be expanded before passing to the regex crate.
- The `regex` crate uses `(?i)` for case-insensitive, not `/i` flag.
- Zero-length match handling in global replace needs custom implementation (not just `replace_all`).
- `SedState` must be mutable and passed by `&mut` reference through execution.
- Range state tracking uses `HashMap<String, RangeState>` keyed by serialized address range.
