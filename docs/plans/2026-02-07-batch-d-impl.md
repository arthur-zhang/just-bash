# Batch D Commands Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement 7 Unix commands (find, xargs, diff, tar, gzip, base64, curl) matching TypeScript logic.

**Architecture:** Each command implements the `Command` trait with `async fn execute(&self, ctx: CommandContext) -> CommandResult`. Complex commands (find, tar) use multi-module structure. Commands needing subprocess execution (xargs, find -exec) or network (curl) use optional callback fields on CommandContext.

**Tech Stack:** Rust, async-trait, flate2 (gzip), similar (diff), tar (archive), base64 (already in deps)

---

## Context for All Tasks

**Existing patterns:**
- Command trait: `src/commands/types.rs` — `name() -> &'static str`, `execute(ctx) -> CommandResult`
- Registry: `src/commands/registry.rs` — `register_batch_X()` functions
- Module exports: `src/commands/mod.rs` — `pub mod X;`
- FileSystem: `src/fs/types.rs` — `read_file`, `read_file_buffer`, `write_file`, `stat`, `lstat`, `readdir`, `readdir_with_file_types`, `exists`, `mkdir`, `rm`
- FsStat: `{ is_file, is_directory, is_symlink, mode: u32, size: u64, mtime: SystemTime }`
- DirentEntry: `{ name, is_file, is_directory, is_symlink }`
- Binary data: `read_file_buffer() -> Vec<u8>`, `write_file(path, &[u8])`
- Tests use `InMemoryFs` from `src/fs/in_memory_fs.rs`
- All paths resolved via `ctx.fs.resolve_path(&ctx.cwd, path)`

**TypeScript reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/`

---

### Task 1: Add Dependencies + Extend CommandContext

**Files:**
- Modify: `Cargo.toml` — add `flate2`, `similar`
- Modify: `src/commands/types.rs` — add optional `exec_fn` and `fetch_fn` callbacks

**Step 1: Add crate dependencies to Cargo.toml**

```toml
flate2 = "1.0"
similar = "2.6"
```

Note: `base64` and `tar` crate are NOT needed — `base64 = "0.22"` already exists; for tar we implement the format inline (the `tar` crate requires real I/O, not virtual FS).

**Step 2: Extend CommandContext with optional exec and fetch callbacks**

In `src/commands/types.rs`, add:

```rust
use std::pin::Pin;
use std::future::Future;

/// Callback for executing shell commands (used by xargs, find -exec)
pub type ExecFn = Arc<dyn Fn(String, String, String, HashMap<String, String>, Arc<dyn FileSystem>)
    -> Pin<Box<dyn Future<Output = CommandResult> + Send>> + Send + Sync>;

/// HTTP response for fetch callback
#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Callback for HTTP requests (used by curl)
pub type FetchFn = Arc<dyn Fn(String, String, HashMap<String, String>, Option<String>)
    -> Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>> + Send + Sync>;
```

Add to CommandContext:
```rust
pub struct CommandContext {
    pub args: Vec<String>,
    pub stdin: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub fs: Arc<dyn FileSystem>,
    pub exec_fn: Option<ExecFn>,   // For xargs, find -exec
    pub fetch_fn: Option<FetchFn>, // For curl
}
```

**Step 3: Fix all existing test code that constructs CommandContext**

Add `exec_fn: None, fetch_fn: None` to every existing CommandContext construction. Use grep to find all occurrences.

**Step 4: Run `cargo test` to verify everything still compiles and passes**

**Step 5: Commit**
```bash
git commit -m "feat(commands): add exec/fetch callbacks to CommandContext, add flate2/similar deps"
```

---

### Task 2: base64 Command

**Files:**
- Create: `src/commands/base64_cmd/mod.rs`
- Test: inline `#[cfg(test)]` module

**Reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/base64/base64.ts` (124 lines)

**Implementation:**

Single-file command implementing:
- `-d, --decode` — decode base64 input
- `-w N, --wrap=N` — wrap encoded lines at N columns (default 76, 0=no wrap)
- Read from files or stdin (use `-` for stdin)
- Use the `base64` crate (already in deps): `base64::engine::general_purpose::STANDARD`
- Binary file reading via `ctx.fs.read_file_buffer()`
- Line wrapping: insert `\n` every N chars in encoded output

**Tests (~15):**
- Encode simple string
- Encode with default wrap (76 chars)
- Encode with `-w 0` (no wrap)
- Encode with custom wrap width
- Decode valid base64
- Decode with whitespace in input
- Decode invalid base64 (error)
- Read from stdin
- Read from file
- Read from multiple files
- Binary data round-trip (encode then decode)
- Empty input

**Commit:** `feat(commands): implement base64 command with -d/-w options`

---

### Task 3: diff Command

**Files:**
- Create: `src/commands/diff_cmd/mod.rs`
- Test: inline `#[cfg(test)]` module

**Reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/diff/diff.ts` (115 lines)

**Implementation:**

Single-file command using `similar` crate for unified diff:
- `-u, --unified` — unified format (default)
- `-q, --brief` — only report whether files differ
- `-s, --report-identical-files` — report when identical
- `-i, --ignore-case` — case-insensitive comparison
- Support `-` for stdin
- Exit codes: 0=identical, 1=different, 2=error
- Use `similar::TextDiff::from_lines()` for diff generation
- Format as unified diff with 3 context lines, `---`/`+++` headers

**Tests (~12):**
- Identical files → exit 0, no output
- Different files → exit 1, unified diff output
- Brief mode (`-q`) → "Files X and Y differ"
- Report identical (`-s`) → "Files X and Y are identical"
- Case-insensitive comparison
- One file from stdin
- Missing file → exit 2, error message
- Empty files comparison
- File vs empty file
- Added/removed lines format check

**Commit:** `feat(commands): implement diff command with -u/-q/-s/-i options`

---

### Task 4: gzip/gunzip/zcat Commands

**Files:**
- Create: `src/commands/gzip/mod.rs`
- Test: inline `#[cfg(test)]` module

**Reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/gzip/gzip.ts` (793 lines)

**Implementation:**

Three commands sharing one module (GzipCommand, GunzipCommand, ZcatCommand):
- `-c, --stdout` — write to stdout, keep original
- `-d, --decompress` — decompress mode
- `-f, --force` — force overwrite
- `-k, --keep` — keep input files
- `-l, --list` — list compressed file info
- `-n, --no-name` / `-N, --name` — name/timestamp handling
- `-q, --quiet` — suppress warnings
- `-r, --recursive` — recursive directory processing
- `-S SUF, --suffix=SUF` — custom suffix (default `.gz`)
- `-t, --test` — test integrity
- `-v, --verbose` — verbose output
- `-1` to `-9` — compression levels
- Use `flate2::write::GzEncoder` / `flate2::read::GzDecoder`
- Binary I/O via `read_file_buffer()` / `write_file()`
- Gzip header parsing for `-l` and `-N` (RFC 1952: magic bytes 0x1f 0x8b, flags, mtime, filename)
- gunzip = gzip -d, zcat = gzip -dc

**Tests (~25):**
- Compress string → produces gzip data
- Decompress gzip data → original string
- Round-trip: compress then decompress
- `-c` flag: output to stdout, keep original file
- `-k` flag: keep input file after compression
- Default: remove input file after compression
- `-d` flag: decompress mode
- `-l` flag: list compressed file info (compressed/uncompressed/ratio/name)
- `-t` flag: test integrity (valid file)
- `-t` flag: test integrity (corrupt file → error)
- `-S .z` custom suffix
- `-1` fast compression
- `-9` best compression
- `-f` force overwrite existing
- Refuse overwrite without `-f`
- gunzip command (implicit -d)
- zcat command (implicit -dc)
- Stdin/stdout piping
- `-v` verbose output format
- `-r` recursive directory processing
- Empty file handling
- Missing file error
- `-n` / `-N` name handling

**Commit:** `feat(commands): implement gzip/gunzip/zcat with compression levels and header parsing`

---

### Task 5: find Command — Types + Parser

**Files:**
- Create: `src/commands/find/types.rs`
- Create: `src/commands/find/parser.rs`
- Create: `src/commands/find/mod.rs` (stub)
- Test: inline `#[cfg(test)]` modules

**Reference:**
- `/Users/arthur/PycharmProjects/just-bash/src/commands/find/types.ts` (68 lines)
- `/Users/arthur/PycharmProjects/just-bash/src/commands/find/parser.ts` (319 lines)

**Implementation — types.rs:**

```rust
pub enum Expression {
    Name { pattern: String, case_insensitive: bool },
    Path { pattern: String, case_insensitive: bool },
    Regex { pattern: String, case_insensitive: bool },
    Type(FileType),
    Empty,
    Mtime { days: i64, comparison: Comparison },
    Newer { reference_path: String },
    Size { value: i64, unit: SizeUnit, comparison: Comparison },
    Perm { mode: u32, match_type: PermMatch },
    Prune,
    Print,
    Print0,
    Printf { format: String },
    Delete,
    Exec { command: Vec<String>, batch: bool }, // batch = + vs ;
    Not(Box<Expression>),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
}

pub enum Comparison { Exact, GreaterThan, LessThan }
pub enum SizeUnit { Bytes, Kilobytes, Megabytes, Gigabytes, Blocks }
pub enum PermMatch { Exact, AllBits, AnyBits }
pub enum FileType { File, Directory, Symlink }

pub struct EvalContext {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mode: u32,
    pub mtime: SystemTime,
    pub depth: usize,
}

pub struct EvalResult {
    pub matches: bool,
    pub pruned: bool,
    pub printed: bool,
}
```

**Implementation — parser.rs:**

Parse find expression arguments into Expression tree:
- Operator precedence: parentheses > NOT > AND > OR
- Implicit AND between adjacent expressions
- Parse all predicates: -name, -iname, -path, -ipath, -regex, -iregex, -type, -empty, -mtime, -newer, -size, -perm, -prune, -print, -print0, -printf, -delete, -exec
- Parse -not/!, -and/-a, -or/-o, parentheses

**Tests — parser (~15):**
- Parse `-name "*.txt"` → Name expression
- Parse `-type f` → Type(File)
- Parse `-name "*.rs" -type f` → And(Name, Type) (implicit AND)
- Parse `-name "*.rs" -o -name "*.toml"` → Or(Name, Name)
- Parse `! -name "*.tmp"` → Not(Name)
- Parse `\( -name "*.rs" -o -name "*.toml" \)` → Or with parens
- Parse `-size +1M` → Size with GreaterThan, Megabytes
- Parse `-mtime -7` → Mtime with LessThan
- Parse `-perm 755` → Perm with Exact
- Parse `-exec grep -l TODO {} ;` → Exec with command
- Parse `-exec grep -l TODO {} +` → Exec with batch=true
- Parse `-maxdepth 3 -mindepth 1`
- Parse `-printf "%f\n"`
- Complex nested expression

**Commit:** `feat(commands): implement find types and expression parser`

---

### Task 6: find Command — Matcher + Main

**Files:**
- Create: `src/commands/find/matcher.rs`
- Modify: `src/commands/find/mod.rs` — full FindCommand implementation
- Test: inline `#[cfg(test)]` modules

**Reference:**
- `/Users/arthur/PycharmProjects/just-bash/src/commands/find/matcher.ts` (795 lines)
- `/Users/arthur/PycharmProjects/just-bash/src/commands/find/find.ts` (1155 lines)

**Implementation — matcher.rs:**

Expression evaluator:
- `evaluate(expr: &Expression, ctx: &EvalContext) -> EvalResult`
- Name matching: glob-style with `*`, `?`, `[...]` (use `glob::Pattern`)
- Path matching: same glob on full path
- Regex matching: use `regex_lite::Regex`
- Type matching: check is_file/is_directory/is_symlink
- Size matching: convert units, compare with +/- prefix semantics
- Mtime matching: compare days since modification
- Perm matching: exact, all-bits (-perm -mode), any-bits (-perm /mode)
- Logical operators: Not, And, Or with short-circuit
- Prune: set pruned flag
- Print/Print0/Printf: set printed flag, format output

**Implementation — mod.rs (FindCommand):**

Main traversal:
- Parse starting paths from args (before first `-` flag)
- Parse expressions via parser
- `-maxdepth N` / `-mindepth N` — depth control
- `-depth` — post-order traversal
- BFS traversal using `readdir_with_file_types` + `stat` as needed
- For each entry: build EvalContext, evaluate expression, collect output
- `-exec` support: use `ctx.exec_fn` callback if available
- `-delete` support: use `ctx.fs.rm()`
- `-printf` format: %f (filename), %h (dirname), %p (path), %P (relative), %s (size), %d (depth), %m (octal mode)
- Default action: `-print` if no print/exec/delete action specified

**Tests — matcher (~15):**
- Name glob matching (*.txt, file?.rs)
- Case-insensitive name matching
- Type matching (file, directory)
- Size comparison (+1k, -1M, exact)
- Mtime comparison
- Permission matching (exact, all-bits, any-bits)
- NOT expression
- AND expression (short-circuit)
- OR expression (short-circuit)
- Prune flag propagation
- Empty file/directory detection

**Tests — find command (~20):**
- Find all files in directory
- Find by name pattern
- Find by type (files only, dirs only)
- Find with -maxdepth
- Find with -mindepth
- Find with -depth (post-order)
- Find with -empty
- Find with -size
- Find with -name and -type combined
- Find with -or
- Find with -not
- Find with -print0 (null separator)
- Find with -printf format
- Find with -prune (skip directory)
- Find with -delete
- Find with -newer
- Find with -regex
- Find with -path pattern
- Multiple starting paths
- Default path (current directory)

**Commit:** `feat(commands): implement find command with expression matching and directory traversal`

---

### Task 7: tar Command — Options + Archive Utilities

**Files:**
- Create: `src/commands/tar/options.rs`
- Create: `src/commands/tar/archive.rs`
- Create: `src/commands/tar/mod.rs` (stub)
- Test: inline `#[cfg(test)]` modules

**Reference:**
- `/Users/arthur/PycharmProjects/just-bash/src/commands/tar/tar-options.ts` (356 lines)
- `/Users/arthur/PycharmProjects/just-bash/src/commands/tar/archive.ts` (476 lines)

**Implementation — options.rs:**

Parse tar command-line arguments:
```rust
pub struct TarOptions {
    pub operation: TarOperation, // Create, Extract, List, Append, Update
    pub file: Option<String>,    // -f archive file
    pub directory: Option<String>, // -C change dir
    pub verbose: bool,           // -v
    pub gzip: bool,              // -z
    pub auto_compress: bool,     // -a
    pub to_stdout: bool,         // -O
    pub keep_old_files: bool,    // -k
    pub touch: bool,             // -m
    pub preserve: bool,          // -p
    pub files_from: Option<String>, // -T
    pub exclude: Vec<String>,    // --exclude
    pub exclude_from: Option<String>, // -X
    pub strip: usize,            // --strip-components
    pub wildcards: bool,         // --wildcards
    pub files: Vec<String>,      // positional file args
}
```

Combined short option parsing (e.g., `-cvzf archive.tar`).

**Implementation — archive.rs:**

Inline tar format implementation (no external `tar` crate — works with virtual FS):

```rust
pub struct TarEntry {
    pub path: String,
    pub content: Vec<u8>,
    pub mode: u32,
    pub size: u64,
    pub mtime: u64,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub link_target: String,
}

// Tar format: 512-byte header blocks + data blocks (padded to 512)
pub fn create_archive(entries: &[TarEntry]) -> Vec<u8>;
pub fn parse_archive(data: &[u8]) -> Result<Vec<TarEntry>, String>;

// Gzip compression/decompression using flate2
pub fn compress_gzip(data: &[u8], level: u32) -> Vec<u8>;
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, String>;

// Magic byte detection
pub fn is_gzip(data: &[u8]) -> bool; // 0x1f 0x8b
```

Tar header format (POSIX ustar):
- 100 bytes: filename
- 8 bytes: mode (octal)
- 8 bytes: uid (octal)
- 8 bytes: gid (octal)
- 12 bytes: size (octal)
- 12 bytes: mtime (octal)
- 8 bytes: checksum
- 1 byte: type flag ('0'=file, '5'=directory, '2'=symlink)
- 100 bytes: link name
- 6 bytes: "ustar" magic
- Remaining: version, uname, gname, prefix

**Tests — options (~10):**
- Parse `-cvzf archive.tar.gz dir/`
- Parse `-xf archive.tar`
- Parse `-tf archive.tar`
- Parse `--create --gzip --file=archive.tar.gz`
- Parse `-C /tmp -xf archive.tar`
- Parse `--exclude="*.log" -cf archive.tar dir/`
- Parse `--strip-components=2 -xf archive.tar`
- Combined short options `-cvf`
- Missing required args error

**Tests — archive (~12):**
- Create archive with single file
- Create archive with directory
- Parse archive → get entries back
- Round-trip: create then parse
- Gzip compress/decompress round-trip
- Create gzipped archive
- Parse gzipped archive
- Tar header checksum validation
- Long filename handling
- Empty archive
- Symlink entry
- File permissions preserved

**Commit:** `feat(commands): implement tar option parsing and archive format utilities`

---

### Task 8: tar Command — Main Implementation

**Files:**
- Modify: `src/commands/tar/mod.rs` — full TarCommand
- Test: inline `#[cfg(test)]` module

**Reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/tar/tar.ts` (1089 lines)

**Implementation:**

TarCommand with operations:
- **Create (-c):** Collect files recursively from FS, build TarEntry list, create archive, optionally gzip, write to file or stdout
- **Extract (-x):** Read archive from file or stdin, parse entries, optionally gunzip, write files to FS with path stripping and exclude filtering
- **List (-t):** Read and parse archive, print entry paths (verbose: mode, size, mtime, path)
- **Append (-r):** Read existing archive, add new entries, write back
- **Update (-u):** Like append but only if file is newer

Features:
- Auto-compress detection from extension (-a): `.tar.gz`/`.tgz` → gzip
- `-C dir` change directory for extraction
- `--exclude` pattern matching (glob-style)
- `-T file` read file list from file
- `-X file` read exclude patterns from file
- `--strip-components=N` strip path prefix on extract
- `-O` extract to stdout
- `-k` don't overwrite existing files
- `-v` verbose output (format: mode, size, date, path)
- `-p` preserve permissions
- Recursive directory collection via `readdir_with_file_types` + `stat`
- Safety limits: max 100MB archive, max 10,000 entries

**Tests (~20):**
- Create tar from single file
- Create tar from directory tree
- Extract tar to filesystem
- Create and extract round-trip
- Create tar.gz (gzip compressed)
- Extract tar.gz
- List archive contents
- List with verbose output
- Auto-compress from .tar.gz extension
- Exclude patterns
- Strip components on extract
- Extract to stdout (-O)
- Keep old files (-k)
- Change directory (-C)
- Append to archive (-r)
- Update archive (-u)
- Files-from (-T)
- Verbose output format
- Missing archive file error
- Empty directory in archive
- Preserve permissions (-p)

**Commit:** `feat(commands): implement tar command with create/extract/list/append/update operations`

---

### Task 9: xargs Command

**Files:**
- Create: `src/commands/xargs/mod.rs`
- Test: inline `#[cfg(test)]` module

**Reference:** `/Users/arthur/PycharmProjects/just-bash/src/commands/xargs/xargs.ts` (207 lines)

**Implementation:**

XargsCommand:
- `-I REPLACE` — replace string mode (one command per item)
- `-d DELIM` — custom delimiter (supports `\n`, `\t`, `\r`, `\0`, `\\`)
- `-n NUM` — max arguments per command invocation
- `-P NUM` — max parallel processes (execute batches concurrently)
- `-0, --null` — null-separated input
- `-t, --verbose` — print commands before execution
- `-r, --no-run-if-empty` — don't run if input is empty

Execution modes:
1. **Replace mode (-I):** For each input item, replace REPLACE in command template, execute
2. **Batch mode (-n):** Group items into batches of N, append to command
3. **Default:** All items appended to single command

Input parsing:
- Default: split on whitespace
- `-d`: split on custom delimiter
- `-0`: split on null bytes

Uses `ctx.exec_fn` to execute commands. If exec_fn is None, format the command that would be executed and return it (for testing without exec).

**Tests (~15):**
- Basic: echo items via xargs (with mock exec)
- Replace mode: `-I {} echo {}`
- Batch mode: `-n 2` groups items
- Null separator: `-0`
- Custom delimiter: `-d ,`
- Delimiter escape sequences: `-d '\n'`
- Verbose mode: `-t` prints commands
- No-run-if-empty: `-r` with empty input
- Multiple items default mode
- Parallel execution: `-P 2`
- Empty input without -r (still runs)
- Argument quoting (special chars)
- Without exec_fn: returns formatted commands

**Commit:** `feat(commands): implement xargs command with -I/-d/-n/-P/-0/-t/-r options`

---

### Task 10: curl Command

**Files:**
- Create: `src/commands/curl/types.rs`
- Create: `src/commands/curl/parse.rs`
- Create: `src/commands/curl/form.rs`
- Create: `src/commands/curl/response_formatting.rs`
- Create: `src/commands/curl/mod.rs`
- Test: inline `#[cfg(test)]` modules

**Reference:**
- `/Users/arthur/PycharmProjects/just-bash/src/commands/curl/` (5 files, ~729 lines)

**Implementation — types.rs:**
```rust
pub struct FormField {
    pub name: String,
    pub value: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

pub struct CurlOptions {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub data: Option<String>,
    pub data_binary: Option<String>,
    pub form_fields: Vec<FormField>,
    pub upload_file: Option<String>,
    pub output_file: Option<String>,
    pub use_remote_name: bool,
    pub head_only: bool,
    pub include_headers: bool,
    pub follow_redirects: bool,
    pub fail_silently: bool,
    pub silent: bool,
    pub show_error: bool,
    pub verbose: bool,
    pub user: Option<String>,
    pub cookie: Option<String>,
    pub cookie_jar: Option<String>,
    pub write_out: Option<String>,
    pub timeout_ms: Option<u64>,
}
```

**Implementation — parse.rs:**
Parse curl command-line arguments into CurlOptions. Handle combined short options, long options with `=`, automatic method switching (-d → POST, -T → PUT).

**Implementation — form.rs:**
- `encode_form_data(input: &str) -> String` — URL encoding
- `parse_form_field(spec: &str) -> FormField` — parse `-F` field specs
- `generate_multipart_body(fields: &[FormField], file_contents: &HashMap<String, String>) -> (String, String)` — returns (body, content_type with boundary)

**Implementation — response_formatting.rs:**
- `format_headers(headers: &HashMap<String, String>) -> String`
- `extract_filename(url: &str) -> String`
- `apply_write_out(format: &str, status: u16, headers: &HashMap<String, String>, url: &str, body_len: usize) -> String`

**Implementation — mod.rs (CurlCommand):**
- URL normalization (add https:// if no protocol)
- Build request from options (method, headers, body)
- Use `ctx.fetch_fn` to make HTTP request
- If fetch_fn is None → error "curl: network not available"
- Process response: headers, body, status
- Output to file (-o) or stdout
- Cookie jar support
- Verbose output (> request, < response)
- Write-out format substitution
- Error codes: 7=connection, 22=HTTP error, 28=timeout

**Tests (~20):**
- Parse simple URL
- Parse `-X POST -d "data" URL`
- Parse `-H "Content-Type: application/json"`
- Parse combined flags `-sSf`
- Parse `-o output.txt`
- Parse `-F "file=@upload.txt"`
- Form field parsing
- Multipart body generation
- URL encoding
- Response header formatting
- Write-out format substitution (%{http_code}, %{size_download})
- Filename extraction from URL
- With mock fetch_fn: GET request
- With mock fetch_fn: POST with data
- With mock fetch_fn: headers included (-i)
- With mock fetch_fn: output to file (-o)
- With mock fetch_fn: follow redirects (-L)
- With mock fetch_fn: fail on HTTP error (-f)
- Without fetch_fn: error message
- Verbose output format
- Basic auth (-u user:pass)

**Commit:** `feat(commands): implement curl command with full option parsing and HTTP support`

---

### Task 11: Registration + Roadmap Update

**Files:**
- Modify: `src/commands/mod.rs` — add module declarations
- Modify: `src/commands/registry.rs` — add `register_batch_d()` and `create_batch_abcd_registry()`
- Modify: `docs/plans/migration-roadmap.md` — update Batch D status

**Step 1: Add module declarations to mod.rs**

```rust
pub mod base64_cmd;
pub mod diff_cmd;
pub mod gzip;
pub mod find;
pub mod tar;
pub mod xargs;
pub mod curl;
```

Update pub use to include `register_batch_d`, `create_batch_abcd_registry`.

**Step 2: Add batch D registration to registry.rs**

```rust
use super::base64_cmd::Base64Command;
use super::diff_cmd::DiffCommand;
use super::gzip::{GzipCommand, GunzipCommand, ZcatCommand};
use super::find::FindCommand;
use super::tar::TarCommand;
use super::xargs::XargsCommand;
use super::curl::CurlCommand;

pub fn register_batch_d(registry: &mut CommandRegistry) {
    registry.register(Box::new(Base64Command));
    registry.register(Box::new(DiffCommand));
    registry.register(Box::new(GzipCommand));
    registry.register(Box::new(GunzipCommand));
    registry.register(Box::new(ZcatCommand));
    registry.register(Box::new(FindCommand));
    registry.register(Box::new(TarCommand));
    registry.register(Box::new(XargsCommand));
    registry.register(Box::new(CurlCommand));
}

pub fn create_batch_abcd_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    registry
}
```

**Step 3: Update migration roadmap**

Mark Batch D as completed with stats.

**Step 4: Run `cargo test` — all tests pass**

**Step 5: Commit**
```bash
git commit -m "feat(commands): add batch D registration and update roadmap"
```
