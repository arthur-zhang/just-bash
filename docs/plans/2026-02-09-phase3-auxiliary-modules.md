# Phase 3: Auxiliary Modules Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate the shell/glob (GlobExpander) and network/ (secure fetch with URL allow-list) modules from TypeScript to Rust, completing Phase 3 of the migration roadmap.

**Architecture:** The GlobExpander is an async struct that works with the virtual `FileSystem` trait to expand glob patterns against the in-memory filesystem. It supports all bash glob options (globstar, dotglob, extglob, nullglob, failglob, globskipdots) and GLOBIGNORE. The network module provides URL allow-list matching and a secure fetch wrapper that enforces access control, wrapping a caller-provided raw fetch function. Both modules are standalone with no new crate dependencies.

**Tech Stack:** Rust, async-trait, regex-lite (already in deps), Arc<dyn FileSystem>

---

## Context

### Existing Rust Code (already migrated)
- `src/interpreter/expansion/glob_escape.rs` — `has_glob_pattern()`, `unescape_glob_pattern()`, `escape_glob_chars()`, `escape_regex_chars()`
- `src/interpreter/expansion/pattern.rs` — `pattern_to_regex()`, `find_matching_paren()`, `split_extglob_alternatives()`, POSIX_CLASSES
- `src/interpreter/expansion/word_glob_expansion.rs` — Uses `glob::glob_with()` (real FS only)
- `src/interpreter/sync_fs_adapter.rs` — Basic `glob()` method using `glob::Pattern` against all virtual FS paths
- `src/commands/find/matcher.rs` — `glob_match()` for find command
- `src/fs/types.rs` — `FileSystem` trait with `readdir()`, `readdir_with_file_types()`, `stat()`, `exists()`, `resolve_path()`, `get_all_paths()`
- `src/commands/types.rs` — `FetchFn`, `FetchResponse` types for curl command

### TypeScript Reference (to migrate)
- `src/shell/glob.ts` (1,047 lines) — `GlobExpander` class with virtual FS glob expansion
- `src/shell/glob-to-regex.ts` (340 lines) — `splitGlobignorePatterns()`, `globignorePatternToRegex()`, `posixClassToRegex()`, `findMatchingParen()`, `splitExtglobAlternatives()`
- `src/utils/glob.ts` (101 lines) — `matchGlob()` utility
- `src/network/types.ts` (119 lines) — `NetworkConfig`, `FetchResult`, error classes
- `src/network/allow-list.ts` (148 lines) — `parseUrl()`, `isUrlAllowed()`, `matchesAllowListEntry()`, `validateAllowList()`
- `src/network/fetch.ts` (189 lines) — `createSecureFetch()` with redirect handling and allow-list enforcement

### Key Design Decisions
1. **GlobExpander is async** — works directly with `Arc<dyn FileSystem>` (the async trait), not the sync adapter
2. **No new crate dependencies** — reuse regex-lite for pattern matching, no reqwest for HTTP
3. **Network module uses callback pattern** — `create_secure_fetch_fn()` wraps a caller-provided raw `FetchFn` with allow-list enforcement, matching the existing `FetchFn` type in `commands/types.rs`
4. **Reuse existing helpers** — POSIX classes from `pattern.rs`, glob escape from `glob_escape.rs`

---

### Task 1: Shell Glob Helpers — GLOBIGNORE and Pattern-to-Regex

**Files:**
- Create: `src/shell/mod.rs`
- Create: `src/shell/glob_helpers.rs`
- Modify: `src/lib.rs` — add `pub mod shell;`

**What to build:**

A helper module with functions needed by the GlobExpander that don't already exist in the Rust codebase. The existing `pattern.rs` already has `pattern_to_regex()`, `find_matching_paren()`, `split_extglob_alternatives()`, and POSIX classes. What's missing:

1. `split_globignore_patterns(globignore: &str) -> Vec<String>` — Split GLOBIGNORE env var on colons, preserving colons inside POSIX character classes like `[[:alnum:]]`
2. `globignore_pattern_to_regex(pattern: &str) -> String` — Convert a GLOBIGNORE pattern to regex where `*` does NOT match `/` (unlike regular glob where `*` matches anything)
3. `glob_to_regex(pattern: &str, extglob: bool) -> String` — Convert a glob pattern to regex for filename matching (anchored with `^...$`). This is for matching individual filenames against patterns (used by GlobExpander.matchPattern). Different from `pattern_to_regex` in `pattern.rs` which is for parameter expansion.

**Reference:** TypeScript `src/shell/glob-to-regex.ts` lines 37-249 and `src/shell/glob.ts` lines 709-949

**Implementation details:**

`split_globignore_patterns`:
- Split on `:` but NOT inside `[...]` character classes
- Handle `[[:alnum:]]` style POSIX classes (colon inside `[:...:]` is not a separator)
- Handle escaped characters `\:`
- Handle `[!...]` and `[^...]` negation

`globignore_pattern_to_regex`:
- `*` → `[^/]*` (does NOT match `/`)
- `?` → `[^/]` (does NOT match `/`)
- `[...]` → character class with POSIX class support
- Escape regex special chars
- Anchor with `^...$`

`glob_to_regex`:
- `*` → `.*` (matches anything including `/` for filename matching)
- `?` → `.`
- `[...]` → character class with POSIX class support, `[!...]` negation
- Extglob: `@(...)`, `*(...)`, `+(...)`, `?(...)`, `!(...)` when enabled
- Handle escaped characters
- Anchor with `^...$`

**Tests (in same file, `#[cfg(test)]`):**

```rust
// split_globignore_patterns tests
- "*.txt:*.log" → ["*.txt", "*.log"]
- "[[:alnum:]]*:*.txt" → ["[[:alnum:]]*", "*.txt"]  // colon inside POSIX class preserved
- "a\\:b:c" → ["a\\:b", "c"]  // escaped colon
- "" → []
- "single" → ["single"]

// globignore_pattern_to_regex tests
- "*" matches "foo" but NOT "dir/foo"
- "*.txt" matches "file.txt" but NOT "dir/file.txt"
- "?" matches "a" but NOT "/"
- "[abc]*" matches "afile" but NOT "dfile"

// glob_to_regex tests
- "*" matches "anything"
- "*.txt" matches "file.txt" but NOT "file.rs"
- "?" matches "a" but NOT "ab"
- "[abc]" matches "a" but NOT "d"
- "[!abc]" matches "d" but NOT "a"
- "[[:digit:]]" matches "5" but NOT "a"
- "@(foo|bar)" matches "foo" and "bar" but NOT "baz" (extglob=true)
- "\\*" matches literal "*"
```

**Step 1:** Create `src/shell/mod.rs` and `src/shell/glob_helpers.rs` with the three functions and all tests.

**Step 2:** Add `pub mod shell;` to `src/lib.rs`.

**Step 3:** Run `cargo test -p just-bash shell::glob_helpers` — all tests pass.

**Step 4:** Commit: `feat(shell): add glob helpers for GLOBIGNORE and pattern-to-regex`

---

### Task 2: GlobExpander Core — Struct, Options, Pattern Matching

**Files:**
- Create: `src/shell/glob_expander.rs`
- Modify: `src/shell/mod.rs` — add `pub mod glob_expander;`

**What to build:**

The `GlobExpander` struct with options and basic pattern matching. This task does NOT include directory walking/expansion — just the struct, configuration, and the `match_pattern()` method.

**Reference:** TypeScript `src/shell/glob.ts` lines 21-137, 706-949

**Implementation:**

```rust
use std::sync::Arc;
use crate::fs::FileSystem;

#[derive(Debug, Clone, Default)]
pub struct GlobOptions {
    pub globstar: bool,
    pub nullglob: bool,
    pub failglob: bool,
    pub dotglob: bool,
    pub extglob: bool,
    pub globskipdots: bool,  // default true in bash >=5.2
}

pub struct GlobExpander {
    fs: Arc<dyn FileSystem>,
    cwd: String,
    globignore_patterns: Vec<String>,
    has_globignore: bool,
    globstar: bool,
    nullglob: bool,
    failglob: bool,
    dotglob: bool,
    extglob: bool,
    globskipdots: bool,
}

impl GlobExpander {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        cwd: String,
        env: Option<&HashMap<String, String>>,
        options: GlobOptions,
    ) -> Self { ... }

    pub fn has_nullglob(&self) -> bool { ... }
    pub fn has_failglob(&self) -> bool { ... }

    /// Check if a string contains glob characters
    pub fn is_glob_pattern(&self, s: &str) -> bool { ... }

    /// Match a filename against a glob pattern
    pub fn match_pattern(&self, name: &str, pattern: &str) -> bool { ... }

    /// Filter results based on GLOBIGNORE and globskipdots
    fn filter_globignore(&self, results: Vec<String>) -> Vec<String> { ... }

    /// Check if ** is a valid complete path segment
    fn is_globstar_valid(&self, pattern: &str) -> bool { ... }
}
```

**Key behaviors:**
- `is_glob_pattern` checks for `*`, `?`, `[...]`, and extglob `@(...)` etc.
- `match_pattern` uses `glob_to_regex` from Task 1 to compile pattern and test against name
- `filter_globignore` filters `.` and `..` when GLOBIGNORE is set or globskipdots is enabled, and filters paths matching GLOBIGNORE patterns
- `is_globstar_valid` checks that `**` appears as a complete path segment (not `d**` or `**y`)
- Constructor parses GLOBIGNORE env var using `split_globignore_patterns`

**Tests:**

```rust
// GlobExpander::new with options
// GlobExpander::is_glob_pattern — *, ?, [...], extglob
// GlobExpander::match_pattern — *, ?, [...], [!...], extglob
// GlobExpander::filter_globignore — filters . and .., GLOBIGNORE patterns
// GlobExpander::is_globstar_valid — "**" valid, "d**" invalid, "**/foo/**" valid
```

**Step 1:** Create `src/shell/glob_expander.rs` with struct, constructor, and all methods + tests.

**Step 2:** Add `pub mod glob_expander;` to `src/shell/mod.rs`.

**Step 3:** Run `cargo test -p just-bash shell::glob_expander` — all tests pass.

**Step 4:** Commit: `feat(shell): add GlobExpander core with pattern matching`

---

### Task 3: GlobExpander Expansion — Simple and Recursive

**Files:**
- Modify: `src/shell/glob_expander.rs` — add `expand()`, `expand_simple()`, `expand_recursive()`, `expand_segments()`, `walk_directory()`, `expand_args()`

**What to build:**

The async expansion methods that walk the virtual filesystem to find matching paths.

**Reference:** TypeScript `src/shell/glob.ts` lines 139-704

**Implementation:**

```rust
impl GlobExpander {
    /// Expand a single glob pattern to matching file paths
    pub async fn expand(&self, pattern: &str) -> Vec<String> { ... }

    /// Expand an array of arguments, replacing glob patterns with matched files
    pub async fn expand_args(&self, args: &[String], quoted_flags: Option<&[bool]>) -> Vec<String> { ... }

    /// Expand a simple glob pattern (no **)
    async fn expand_simple(&self, pattern: &str) -> Vec<String> { ... }

    /// Expand a recursive glob pattern (contains **)
    async fn expand_recursive(&self, pattern: &str) -> Vec<String> { ... }

    /// Recursively expand path segments with glob patterns
    async fn expand_segments(
        &self,
        fs_path: &str,
        result_prefix: &str,
        segments: &[String],
    ) -> Vec<String> { ... }

    /// Recursively walk a directory and collect matching files
    async fn walk_directory(
        &self,
        dir: &str,
        file_pattern: &str,
        results: &mut Vec<String>,
    ) { ... }
}
```

**Key behaviors:**

`expand`:
- If pattern contains `**` and globstar is enabled and `**` is valid segment → `expand_recursive`
- Otherwise replace `**` with `*` and use `expand_simple`
- Apply GLOBIGNORE filtering to results
- Sort results

`expand_simple`:
- Split pattern into path segments
- Find first segment with glob characters
- Build base path (absolute or relative to cwd)
- Call `expand_segments` for remaining segments

`expand_segments`:
- Base case: no segments left → return `[result_prefix]`
- Read directory entries using `fs.readdir_with_file_types()`
- Skip hidden files unless pattern starts with `.` or dotglob is enabled
- When GLOBIGNORE is set, dotglob is implicitly enabled (bash behavior)
- Match entries against current segment pattern
- If more segments remain and entry is directory → recurse
- If no more segments → add to results

`expand_recursive`:
- Split at first `**`
- Get before and after parts
- Walk directory tree, matching file_pattern at each level

`walk_directory`:
- Read directory entries
- Match files against file_pattern, add to results
- Recurse into subdirectories

`expand_args`:
- For each arg, check if quoted or not a glob pattern → keep as-is
- Otherwise expand and collect results
- If no matches, keep original pattern (bash default)

**Tests (using InMemoryFs):**

```rust
// Setup: create InMemoryFs with structure:
//   /home/user/
//   /home/user/file.txt
//   /home/user/file.rs
//   /home/user/data.json
//   /home/user/.hidden
//   /home/user/sub/
//   /home/user/sub/nested.txt
//   /home/user/sub/deep/
//   /home/user/sub/deep/file.txt

// expand("*.txt") from /home/user → ["file.txt"]
// expand("*.rs") from /home/user → ["file.rs"]
// expand("*") from /home/user → ["data.json", "file.rs", "file.txt", "sub"] (sorted, no hidden)
// expand(".*") from /home/user → [".hidden"] (dotfiles with explicit .)
// expand("*") with dotglob → includes ".hidden"
// expand("sub/*.txt") from /home/user → ["sub/nested.txt"]
// expand("**/*.txt") with globstar from /home/user → ["file.txt", "sub/deep/file.txt", "sub/nested.txt"]
// expand("nonexistent*") → ["nonexistent*"] (no match, return pattern)
// expand("nonexistent*") with nullglob → []
// expand("nonexistent*") with failglob → error
// expand("/home/user/*.txt") absolute → ["/home/user/file.txt"]
// expand_args(["hello", "*.txt", "*.rs"], None) → ["hello", "file.txt", "file.rs"]
// expand_args(["*.txt"], Some(&[true])) → ["*.txt"] (quoted, no expansion)
// GLOBIGNORE="*.txt" → expand("*") excludes .txt files
```

**Step 1:** Add all expansion methods and tests to `src/shell/glob_expander.rs`.

**Step 2:** Run `cargo test -p just-bash shell::glob_expander` — all tests pass.

**Step 3:** Commit: `feat(shell): add GlobExpander async expansion with virtual FS`

---

### Task 4: Network Types and Error Definitions

**Files:**
- Create: `src/network/mod.rs`
- Create: `src/network/types.rs`
- Modify: `src/lib.rs` — add `pub mod network;`

**What to build:**

Network configuration types and error types matching the TypeScript `network/types.ts`.

**Reference:** TypeScript `src/network/types.ts` (119 lines)

**Implementation:**

```rust
// src/network/types.rs

use std::fmt;

/// HTTP methods that can be allowed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get, Head, Post, Put, Delete, Patch, Options,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Option<Self> { ... }
    pub fn as_str(&self) -> &'static str { ... }
}

impl fmt::Display for HttpMethod { ... }

/// Configuration for network access
#[derive(Debug, Clone, Default)]
pub struct NetworkConfig {
    /// List of allowed URL prefixes (origin + optional path)
    pub allowed_url_prefixes: Vec<String>,
    /// Allowed HTTP methods (default: GET, HEAD)
    pub allowed_methods: Option<Vec<HttpMethod>>,
    /// Bypass allow-list (DANGEROUS)
    pub dangerously_allow_full_internet_access: bool,
    /// Max redirects (default: 20)
    pub max_redirects: Option<usize>,
    /// Request timeout in ms (default: 30000)
    pub timeout_ms: Option<u64>,
}

/// Result of a network fetch operation
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Network error types
#[derive(Debug, Clone)]
pub enum NetworkError {
    AccessDenied { url: String },
    TooManyRedirects { max: usize },
    RedirectNotAllowed { url: String },
    MethodNotAllowed { method: String, allowed: Vec<String> },
    FetchError { message: String },
}

impl fmt::Display for NetworkError { ... }
impl std::error::Error for NetworkError {}
```

**Tests:**

```rust
// HttpMethod::from_str — "GET" → Get, "post" → Post, "invalid" → None
// HttpMethod::as_str — Get → "GET"
// NetworkConfig::default — empty prefixes, no methods, no bypass
// NetworkError display messages
// FetchResult construction
```

**Step 1:** Create `src/network/mod.rs` and `src/network/types.rs` with all types and tests.

**Step 2:** Add `pub mod network;` to `src/lib.rs`.

**Step 3:** Run `cargo test -p just-bash network::types` — all tests pass.

**Step 4:** Commit: `feat(network): add network types and error definitions`

---

### Task 5: Network Allow-List — URL Matching

**Files:**
- Create: `src/network/allow_list.rs`
- Modify: `src/network/mod.rs` — add `pub mod allow_list;`

**What to build:**

URL allow-list matching logic. Checks if a URL is allowed by comparing against a list of allowed URL prefixes (origin + optional path).

**Reference:** TypeScript `src/network/allow-list.ts` (148 lines)

**Implementation:**

```rust
// src/network/allow_list.rs

use url::Url;  // We'll use simple string parsing, no url crate needed

/// Parsed URL components
struct ParsedUrl {
    origin: String,   // e.g., "https://api.example.com"
    pathname: String,  // e.g., "/v1/users"
}

/// Parse a URL string into components. Returns None if invalid.
fn parse_url(url_string: &str) -> Option<ParsedUrl> { ... }

/// Normalize an allow-list entry for consistent matching.
fn normalize_allow_list_entry(entry: &str) -> Option<(String, String)> { ... }

/// Check if a URL matches an allow-list entry.
/// Rules:
/// 1. Origins must match exactly (case-sensitive)
/// 2. URL path must start with entry's path
/// 3. If entry has no path (or just "/"), all paths allowed
pub fn matches_allow_list_entry(url: &str, allowed_entry: &str) -> bool { ... }

/// Check if a URL is allowed by any entry in the allow-list.
pub fn is_url_allowed(url: &str, allowed_url_prefixes: &[String]) -> bool { ... }

/// Validate allow-list configuration. Returns error messages for invalid entries.
pub fn validate_allow_list(allowed_url_prefixes: &[String]) -> Vec<String> { ... }
```

**URL parsing (no external crate):**
- Parse scheme (http/https only)
- Parse host (required)
- Parse port (optional)
- Build origin as `scheme://host[:port]`
- Extract pathname

**Tests:**

```rust
// parse_url
- "https://api.example.com/v1" → origin="https://api.example.com", pathname="/v1"
- "http://localhost:3000/api" → origin="http://localhost:3000", pathname="/api"
- "not-a-url" → None
- "ftp://example.com" → Some (but validate_allow_list rejects it)

// matches_allow_list_entry
- ("https://api.example.com/v1/users", "https://api.example.com") → true (origin match, all paths)
- ("https://api.example.com/v1/users", "https://api.example.com/v1") → true (path prefix match)
- ("https://api.example.com/v2/users", "https://api.example.com/v1") → false (path mismatch)
- ("https://other.com/v1", "https://api.example.com/v1") → false (origin mismatch)
- ("https://api.example.com/v1", "https://api.example.com/v1/") → false (path doesn't start with /v1/)

// is_url_allowed
- url allowed by first entry → true
- url allowed by second entry → true
- url not allowed by any → false
- empty allow list → false

// validate_allow_list
- ["https://example.com"] → [] (valid)
- ["not-a-url"] → ["Invalid URL..."]
- ["ftp://example.com"] → ["Only http and https..."]
- ["https://example.com?q=1"] → ["Query strings..."]
```

**Step 1:** Create `src/network/allow_list.rs` with all functions and tests.

**Step 2:** Add `pub mod allow_list;` to `src/network/mod.rs`.

**Step 3:** Run `cargo test -p just-bash network::allow_list` — all tests pass.

**Step 4:** Commit: `feat(network): add URL allow-list matching`

---

### Task 6: Network Secure Fetch Wrapper

**Files:**
- Create: `src/network/fetch.rs`
- Modify: `src/network/mod.rs` — add `pub mod fetch;` and public re-exports

**What to build:**

A secure fetch wrapper that enforces the allow-list on every request, including redirect targets. It wraps a caller-provided raw `FetchFn` (the same type used by curl command) with security checks.

**Reference:** TypeScript `src/network/fetch.ts` (189 lines)

**Implementation:**

```rust
// src/network/fetch.rs

use std::sync::Arc;
use std::collections::HashMap;
use crate::commands::types::{FetchFn, FetchResponse};
use crate::network::types::{NetworkConfig, NetworkError, HttpMethod};
use crate::network::allow_list::is_url_allowed;

const DEFAULT_MAX_REDIRECTS: usize = 20;
const DEFAULT_TIMEOUT_MS: u64 = 30000;
const BODYLESS_METHODS: &[&str] = &["GET", "HEAD", "OPTIONS"];
const REDIRECT_CODES: &[u16] = &[301, 302, 303, 307, 308];

/// Options for a single fetch request
#[derive(Debug, Clone, Default)]
pub struct SecureFetchOptions {
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub follow_redirects: Option<bool>,
    pub timeout_ms: Option<u64>,
}

/// Create a secure fetch function that enforces the allow-list.
/// Takes a raw FetchFn (provided by caller) and wraps it with security checks.
/// Returns a new FetchFn that can be used by curl and other commands.
pub fn create_secure_fetch_fn(
    config: NetworkConfig,
    raw_fetch: FetchFn,
) -> FetchFn {
    // Returns Arc<dyn Fn(url, method, headers, body) -> Pin<Box<...>>>
    // The returned function:
    // 1. Checks URL against allow-list
    // 2. Checks HTTP method against allowed methods
    // 3. Calls raw_fetch
    // 4. If redirect, checks redirect target against allow-list
    // 5. Follows redirects up to max_redirects
    ...
}

/// Standalone secure fetch function (for use outside the FetchFn callback pattern)
pub async fn secure_fetch(
    config: &NetworkConfig,
    raw_fetch: &FetchFn,
    url: &str,
    options: SecureFetchOptions,
) -> Result<FetchResponse, NetworkError> {
    let method = options.method.as_deref().unwrap_or("GET").to_uppercase();
    let max_redirects = config.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);
    let follow_redirects = options.follow_redirects.unwrap_or(true);

    // Check URL allowed
    check_url_allowed(&config, url)?;
    // Check method allowed
    check_method_allowed(&config, &method)?;

    let mut current_url = url.to_string();
    let mut redirect_count = 0;

    loop {
        let headers = options.headers.clone().unwrap_or_default();
        let body = if BODYLESS_METHODS.contains(&method.as_str()) {
            None
        } else {
            options.body.clone()
        };

        let response = raw_fetch(
            current_url.clone(),
            method.clone(),
            headers,
            body,
        ).await.map_err(|e| NetworkError::FetchError { message: e })?;

        // Check for redirects
        if REDIRECT_CODES.contains(&response.status) && follow_redirects {
            let location = response.headers.get("location");
            if let Some(location) = location {
                let redirect_url = resolve_redirect_url(&current_url, location);
                check_url_allowed(&config, &redirect_url)?;
                redirect_count += 1;
                if redirect_count > max_redirects {
                    return Err(NetworkError::TooManyRedirects { max: max_redirects });
                }
                current_url = redirect_url;
                continue;
            }
        }

        return Ok(FetchResponse {
            status: response.status,
            headers: response.headers,
            body: response.body,
            url: current_url,
        });
    }
}

fn check_url_allowed(config: &NetworkConfig, url: &str) -> Result<(), NetworkError> { ... }
fn check_method_allowed(config: &NetworkConfig, method: &str) -> Result<(), NetworkError> { ... }
fn resolve_redirect_url(base: &str, location: &str) -> String { ... }
```

**Tests:**

```rust
// Helper: create a mock FetchFn that returns predefined responses

// test_secure_fetch_allowed_url — URL in allow-list → success
// test_secure_fetch_denied_url — URL not in allow-list → AccessDenied error
// test_secure_fetch_full_access — dangerously_allow_full_internet_access → all URLs allowed
// test_secure_fetch_method_not_allowed — POST with default config → MethodNotAllowed
// test_secure_fetch_method_allowed — POST with allowed_methods including POST → success
// test_secure_fetch_redirect_allowed — redirect to allowed URL → follows redirect
// test_secure_fetch_redirect_denied — redirect to non-allowed URL → RedirectNotAllowed
// test_secure_fetch_too_many_redirects — exceed max_redirects → TooManyRedirects
// test_secure_fetch_no_follow_redirects — follow_redirects=false → returns redirect response
// test_secure_fetch_bodyless_method — GET with body → body stripped
// test_create_secure_fetch_fn — wraps raw fetch with security checks
// test_resolve_redirect_url — relative and absolute redirect URLs
```

**Step 1:** Create `src/network/fetch.rs` with all functions and tests.

**Step 2:** Update `src/network/mod.rs` with public re-exports.

**Step 3:** Run `cargo test -p just-bash network::fetch` — all tests pass.

**Step 4:** Commit: `feat(network): add secure fetch wrapper with allow-list enforcement`

---

### Task 7: Module Registration and Roadmap Update

**Files:**
- Modify: `src/lib.rs` — ensure `pub mod shell;` and `pub mod network;` are present
- Modify: `src/shell/mod.rs` — public re-exports
- Modify: `src/network/mod.rs` — public re-exports
- Modify: `docs/plans/migration-roadmap.md` — update Phase 3 status

**What to do:**

1. Ensure `src/lib.rs` has both module declarations
2. Set up clean public API in `src/shell/mod.rs`:
   ```rust
   pub mod glob_helpers;
   pub mod glob_expander;
   pub use glob_expander::{GlobExpander, GlobOptions};
   ```
3. Set up clean public API in `src/network/mod.rs`:
   ```rust
   pub mod types;
   pub mod allow_list;
   pub mod fetch;
   pub use types::{NetworkConfig, NetworkError, FetchResult, HttpMethod};
   pub use allow_list::{is_url_allowed, validate_allow_list};
   pub use fetch::{create_secure_fetch_fn, secure_fetch, SecureFetchOptions};
   ```
4. Run full test suite: `cargo test -p just-bash` — ALL tests pass
5. Update `docs/plans/migration-roadmap.md`:
   - Mark Phase 3.1 (shell/glob) as ✅ completed
   - Mark Phase 3.2 (network/) as ✅ completed
   - Add implementation statistics (files, lines, tests)
   - Update total test count
   - Mark Phase 3 checkbox as complete

**Step 1:** Update module files with re-exports.

**Step 2:** Run `cargo test -p just-bash` — all tests pass (record total count).

**Step 3:** Update migration roadmap.

**Step 4:** Commit: `feat(phase3): complete auxiliary modules — shell/glob and network`


