//! GlobExpander Core — Struct, Options, and Pattern Matching
//!
//! Provides the `GlobExpander` struct with configuration options and basic
//! pattern matching. Directory walking and expansion are handled separately
//! (Task 3).

use std::collections::HashMap;
use std::sync::Arc;

use crate::fs::FileSystem;

use super::glob_helpers::{glob_to_regex, globignore_pattern_to_regex, split_globignore_patterns};

/// Options controlling glob expansion behavior.
#[derive(Debug, Clone)]
pub struct GlobOptions {
    pub globstar: bool,
    pub nullglob: bool,
    pub failglob: bool,
    pub dotglob: bool,
    pub extglob: bool,
    /// Default true in bash >=5.2
    pub globskipdots: bool,
}

impl Default for GlobOptions {
    fn default() -> Self {
        Self {
            globstar: false,
            nullglob: false,
            failglob: false,
            dotglob: false,
            extglob: false,
            globskipdots: true, // bash >=5.2 default
        }
    }
}

/// Core glob expander with configuration and pattern matching.
///
/// Holds a reference to the virtual file system, the current working
/// directory, GLOBIGNORE patterns, and all relevant shell options.
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
    ) -> Self {
        let mut globignore_patterns = Vec::new();
        let mut has_globignore = false;
        if let Some(env_map) = env {
            if let Some(globignore) = env_map.get("GLOBIGNORE") {
                if !globignore.is_empty() {
                    has_globignore = true;
                    globignore_patterns = split_globignore_patterns(globignore);
                }
            }
        }
        Self {
            fs,
            cwd,
            globignore_patterns,
            has_globignore,
            globstar: options.globstar,
            nullglob: options.nullglob,
            failglob: options.failglob,
            dotglob: options.dotglob,
            extglob: options.extglob,
            globskipdots: options.globskipdots,
        }
    }

    pub fn has_nullglob(&self) -> bool {
        self.nullglob
    }

    pub fn has_failglob(&self) -> bool {
        self.failglob
    }

    /// Check if a string contains glob characters.
    pub fn is_glob_pattern(&self, s: &str) -> bool {
        if s.contains('*') || s.contains('?') || s.contains('[') {
            return true;
        }
        if self.extglob {
            // Check for @(...), *(...), +(...), ?(...), !(...)
            for i in 0..s.len().saturating_sub(1) {
                let c = s.as_bytes()[i];
                if (c == b'@' || c == b'*' || c == b'+' || c == b'?' || c == b'!')
                    && s.as_bytes()[i + 1] == b'('
                {
                    return true;
                }
            }
        }
        false
    }

    /// Match a filename against a glob pattern.
    pub fn match_pattern(&self, name: &str, pattern: &str) -> bool {
        let regex_str = glob_to_regex(pattern, self.extglob);
        if let Ok(re) = regex_lite::Regex::new(&regex_str) {
            re.is_match(name)
        } else {
            false
        }
    }

    /// Filter results based on GLOBIGNORE and globskipdots.
    pub(crate) fn filter_globignore(&self, results: Vec<String>) -> Vec<String> {
        if !self.has_globignore && !self.globskipdots {
            return results;
        }
        results
            .into_iter()
            .filter(|path| {
                let basename = path.rsplit('/').next().unwrap_or(path);
                // Filter . and .. when GLOBIGNORE is set or globskipdots is enabled
                if (self.has_globignore || self.globskipdots)
                    && (basename == "." || basename == "..")
                {
                    return false;
                }
                // Check GLOBIGNORE patterns
                if self.has_globignore {
                    for ignore_pattern in &self.globignore_patterns {
                        let regex_str = globignore_pattern_to_regex(ignore_pattern);
                        if let Ok(re) = regex_lite::Regex::new(&regex_str) {
                            if re.is_match(path) {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .collect()
    }

    /// Check if `**` is used as a complete path segment.
    pub(crate) fn is_globstar_valid(&self, pattern: &str) -> bool {
        let segments: Vec<&str> = pattern.split('/').collect();
        for segment in segments {
            if segment.contains("**") && segment != "**" {
                return false;
            }
        }
        true
    }

    /// Get effective dotglob (true if dotglob is set OR GLOBIGNORE is set).
    pub(crate) fn effective_dotglob(&self) -> bool {
        self.dotglob || self.has_globignore
    }

    // Getters for expansion methods (Task 3)
    pub(crate) fn fs(&self) -> &Arc<dyn FileSystem> {
        &self.fs
    }

    pub(crate) fn cwd(&self) -> &str {
        &self.cwd
    }

    pub(crate) fn globstar(&self) -> bool {
        self.globstar
    }

    pub(crate) fn dotglob(&self) -> bool {
        self.dotglob
    }

    // =========================================================================
    // Expansion methods (Task 3)
    // =========================================================================

    /// Expand a single glob pattern to matching file paths.
    pub async fn expand(&self, pattern: &str) -> Vec<String> {
        let results = if pattern.contains("**") && self.globstar && self.is_globstar_valid(pattern)
        {
            self.expand_recursive(pattern).await
        } else {
            // When globstar disabled or ** not a valid segment, treat ** as *
            let normalized = pattern.replace("**", "*");
            self.expand_simple(&normalized).await
        };
        // Apply GLOBIGNORE filtering and sort
        let mut filtered = self.filter_globignore(results);
        filtered.sort();
        filtered
    }

    /// Expand an array of arguments, replacing glob patterns with matched files.
    pub async fn expand_args(
        &self,
        args: &[String],
        quoted_flags: Option<&[bool]>,
    ) -> Vec<String> {
        let mut result = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let is_quoted =
                quoted_flags.map_or(false, |flags| flags.get(i).copied().unwrap_or(false));
            if is_quoted || !self.is_glob_pattern(arg) {
                result.push(arg.clone());
            } else {
                let expanded = self.expand(arg).await;
                if expanded.is_empty() {
                    result.push(arg.clone()); // No matches, keep original
                } else {
                    result.extend(expanded);
                }
            }
        }
        result
    }

    /// Expand a simple glob pattern (no **).
    async fn expand_simple(&self, pattern: &str) -> Vec<String> {
        let is_absolute = pattern.starts_with('/');
        let segments: Vec<String> = pattern
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Find first segment with glob characters
        let first_glob_idx = segments.iter().position(|s| self.has_glob_chars(s));
        let first_glob_idx = match first_glob_idx {
            Some(idx) => idx,
            None => return vec![pattern.to_string()], // No glob chars
        };

        // Build base path and result prefix
        let (fs_base_path, result_prefix) = if first_glob_idx == 0 {
            if is_absolute {
                ("/".to_string(), "/".to_string())
            } else {
                (self.cwd.clone(), String::new())
            }
        } else {
            let base_segments: Vec<&str> =
                segments[..first_glob_idx].iter().map(|s| s.as_str()).collect();
            let base = base_segments.join("/");
            if is_absolute {
                (format!("/{}", base), format!("/{}", base))
            } else {
                (self.fs.resolve_path(&self.cwd, &base), base)
            }
        };

        let remaining: Vec<String> = segments[first_glob_idx..].to_vec();
        self.expand_segments(&fs_base_path, &result_prefix, &remaining)
            .await
    }

    /// Recursively expand path segments with glob patterns.
    async fn expand_segments(
        &self,
        fs_path: &str,
        result_prefix: &str,
        segments: &[String],
    ) -> Vec<String> {
        if segments.is_empty() {
            return vec![result_prefix.to_string()];
        }

        let current_segment = &segments[0];
        let remaining = &segments[1..];
        let mut results = Vec::new();

        // Read directory entries
        let entries = match self.fs.readdir_with_file_types(fs_path).await {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        let effective_dotglob = self.dotglob || self.has_globignore;

        for entry in &entries {
            // Skip hidden files unless pattern starts with . or dotglob enabled
            if entry.name.starts_with('.')
                && !current_segment.starts_with('.')
                && !effective_dotglob
            {
                continue;
            }

            if self.match_pattern(&entry.name, current_segment) {
                let new_fs_path = if fs_path == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", fs_path, entry.name)
                };

                let new_result_prefix = if result_prefix.is_empty() {
                    entry.name.clone()
                } else if result_prefix == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", result_prefix, entry.name)
                };

                if remaining.is_empty() {
                    results.push(new_result_prefix);
                } else if entry.is_directory {
                    let sub_results = Box::pin(
                        self.expand_segments(&new_fs_path, &new_result_prefix, remaining),
                    )
                    .await;
                    results.extend(sub_results);
                }
            }
        }

        results
    }

    /// Expand a recursive glob pattern (contains **).
    async fn expand_recursive(&self, pattern: &str) -> Vec<String> {
        let double_star_idx = pattern.find("**").unwrap();
        let before = pattern[..double_star_idx].trim_end_matches('/');
        let before = if before.is_empty() { "." } else { before };
        let after = &pattern[double_star_idx + 2..];
        let file_pattern = after.trim_start_matches('/');

        // If file_pattern contains another **, handle multi-globstar
        if file_pattern.contains("**") && self.is_globstar_valid(file_pattern) {
            let mut results = Vec::new();
            Box::pin(self.walk_directory_multi_globstar(before, file_pattern, &mut results))
                .await;
            results.sort();
            results.dedup();
            return results;
        }

        let mut results = Vec::new();
        self.walk_directory(before, file_pattern, &mut results)
            .await;
        results
    }

    /// Walk directory recursively, matching file_pattern at each level.
    async fn walk_directory(
        &self,
        dir: &str,
        file_pattern: &str,
        results: &mut Vec<String>,
    ) {
        let full_path = self.fs.resolve_path(&self.cwd, dir);

        let entries = match self.fs.readdir_with_file_types(&full_path).await {
            Ok(entries) => entries,
            Err(_) => return,
        };

        let mut dirs = Vec::new();
        for entry in &entries {
            let entry_path = if dir == "." {
                entry.name.clone()
            } else {
                format!("{}/{}", dir, entry.name)
            };

            if entry.is_directory {
                dirs.push(entry_path.clone());
            }

            if !file_pattern.is_empty() && self.match_pattern(&entry.name, file_pattern) {
                results.push(entry_path);
            }
        }

        for dir_path in dirs {
            Box::pin(self.walk_directory(&dir_path, file_pattern, results)).await;
        }
    }

    /// Walk for multi-globstar patterns.
    async fn walk_directory_multi_globstar(
        &self,
        dir: &str,
        sub_pattern: &str,
        results: &mut Vec<String>,
    ) {
        let full_path = self.fs.resolve_path(&self.cwd, dir);

        let entries = match self.fs.readdir_with_file_types(&full_path).await {
            Ok(entries) => entries,
            Err(_) => return,
        };

        let mut dirs = Vec::new();
        for entry in &entries {
            let entry_path = if dir == "." {
                entry.name.clone()
            } else {
                format!("{}/{}", dir, entry.name)
            };
            if entry.is_directory {
                dirs.push(entry_path);
            }
        }

        // From this directory, expand the sub-pattern
        let pattern_from_here = if dir == "." {
            sub_pattern.to_string()
        } else {
            format!("{}/{}", dir, sub_pattern)
        };
        let sub_results = Box::pin(self.expand_recursive(&pattern_from_here)).await;
        results.extend(sub_results);

        // Recurse into subdirectories
        for dir_path in dirs {
            Box::pin(self.walk_directory_multi_globstar(&dir_path, sub_pattern, results)).await;
        }
    }

    /// Check if a string contains glob characters (private helper).
    fn has_glob_chars(&self, s: &str) -> bool {
        self.is_glob_pattern(s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;

    fn make_expander(options: GlobOptions) -> GlobExpander {
        let fs = Arc::new(InMemoryFs::new());
        GlobExpander::new(fs, "/home/user".to_string(), None, options)
    }

    fn make_expander_with_env(
        env: HashMap<String, String>,
        options: GlobOptions,
    ) -> GlobExpander {
        let fs = Arc::new(InMemoryFs::new());
        GlobExpander::new(fs, "/home/user".to_string(), Some(&env), options)
    }

    // -- GlobOptions default --

    #[test]
    fn test_glob_options_default() {
        let opts = GlobOptions::default();
        assert!(!opts.globstar);
        assert!(!opts.nullglob);
        assert!(!opts.failglob);
        assert!(!opts.dotglob);
        assert!(!opts.extglob);
        assert!(opts.globskipdots); // bash >=5.2 default
    }

    // -- GlobExpander::new with default options --

    #[test]
    fn test_new_with_default_options() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.globstar);
        assert!(!expander.nullglob);
        assert!(!expander.failglob);
        assert!(!expander.dotglob);
        assert!(!expander.extglob);
        assert!(expander.globskipdots);
        assert!(!expander.has_globignore);
        assert!(expander.globignore_patterns.is_empty());
        assert_eq!(expander.cwd(), "/home/user");
    }

    // -- GlobExpander::new with GLOBIGNORE env --

    #[test]
    fn test_new_with_globignore_env() {
        let mut env = HashMap::new();
        env.insert("GLOBIGNORE".to_string(), "*.log:*.tmp".to_string());
        let expander = make_expander_with_env(env, GlobOptions::default());
        assert!(expander.has_globignore);
        assert_eq!(expander.globignore_patterns, vec!["*.log", "*.tmp"]);
    }

    #[test]
    fn test_new_with_empty_globignore() {
        let mut env = HashMap::new();
        env.insert("GLOBIGNORE".to_string(), "".to_string());
        let expander = make_expander_with_env(env, GlobOptions::default());
        assert!(!expander.has_globignore);
        assert!(expander.globignore_patterns.is_empty());
    }

    // -- is_glob_pattern --

    #[test]
    fn test_is_glob_pattern_star() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_glob_pattern("*.txt"));
    }

    #[test]
    fn test_is_glob_pattern_question() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_glob_pattern("file?.txt"));
    }

    #[test]
    fn test_is_glob_pattern_bracket() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_glob_pattern("[abc]"));
    }

    #[test]
    fn test_is_glob_pattern_plain_text_returns_false() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.is_glob_pattern("hello"));
        assert!(!expander.is_glob_pattern("file.txt"));
        assert!(!expander.is_glob_pattern("/usr/bin/env"));
    }
    #[test]
    fn test_is_glob_pattern_extglob_at() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.is_glob_pattern("@(foo|bar)"));
    }

    #[test]
    fn test_is_glob_pattern_extglob_star() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.is_glob_pattern("*(ab)"));
    }

    #[test]
    fn test_is_glob_pattern_extglob_plus() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.is_glob_pattern("+(ab)"));
    }

    #[test]
    fn test_is_glob_pattern_extglob_question() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.is_glob_pattern("?(ab)"));
    }

    #[test]
    fn test_is_glob_pattern_extglob_not() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.is_glob_pattern("!(ab)"));
    }

    #[test]
    fn test_is_glob_pattern_extglob_disabled_no_match() {
        // Without extglob, @(foo) is not a glob pattern (no *, ?, [)
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.is_glob_pattern("@(foo)"));
    }

    // -- match_pattern --

    #[test]
    fn test_match_pattern_star() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.match_pattern("file.txt", "*.txt"));
        assert!(!expander.match_pattern("file.rs", "*.txt"));
    }

    #[test]
    fn test_match_pattern_question() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.match_pattern("a", "?"));
        assert!(!expander.match_pattern("ab", "?"));
    }
    #[test]
    fn test_match_pattern_bracket_class() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.match_pattern("a", "[abc]"));
        assert!(!expander.match_pattern("d", "[abc]"));
    }

    #[test]
    fn test_match_pattern_negated_bracket() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.match_pattern("d", "[!abc]"));
        assert!(!expander.match_pattern("a", "[!abc]"));
    }

    #[test]
    fn test_match_pattern_extglob() {
        let mut opts = GlobOptions::default();
        opts.extglob = true;
        let expander = make_expander(opts);
        assert!(expander.match_pattern("foo", "@(foo|bar)"));
        assert!(expander.match_pattern("bar", "@(foo|bar)"));
        assert!(!expander.match_pattern("baz", "@(foo|bar)"));
    }

    // -- filter_globignore --

    #[test]
    fn test_filter_globignore_skips_dots_with_globskipdots() {
        let expander = make_expander(GlobOptions::default()); // globskipdots=true
        let input = vec![
            ".".to_string(),
            "..".to_string(),
            "file.txt".to_string(),
            "dir/..".to_string(),
        ];
        let result = expander.filter_globignore(input);
        assert_eq!(result, vec!["file.txt"]);
    }

    #[test]
    fn test_filter_globignore_filters_patterns() {
        let mut env = HashMap::new();
        env.insert("GLOBIGNORE".to_string(), "*.log:*.tmp".to_string());
        let expander = make_expander_with_env(env, GlobOptions::default());
        let input = vec![
            "file.txt".to_string(),
            "debug.log".to_string(),
            "temp.tmp".to_string(),
            "code.rs".to_string(),
        ];
        let result = expander.filter_globignore(input);
        assert_eq!(result, vec!["file.txt", "code.rs"]);
    }

    #[test]
    fn test_filter_globignore_no_filtering_when_both_disabled() {
        let mut opts = GlobOptions::default();
        opts.globskipdots = false;
        let expander = make_expander(opts);
        let input = vec![
            ".".to_string(),
            "..".to_string(),
            "file.txt".to_string(),
        ];
        let result = expander.filter_globignore(input.clone());
        assert_eq!(result, input);
    }

    // -- is_globstar_valid --

    #[test]
    fn test_is_globstar_valid_double_star_alone() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_globstar_valid("**"));
    }

    #[test]
    fn test_is_globstar_valid_double_star_in_path() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_globstar_valid("**/foo/**"));
    }

    #[test]
    fn test_is_globstar_valid_invalid_mixed() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.is_globstar_valid("d**"));
    }

    #[test]
    fn test_is_globstar_valid_invalid_suffix() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.is_globstar_valid("**x"));
    }

    // -- effective_dotglob --

    #[test]
    fn test_effective_dotglob_with_dotglob_option() {
        let mut opts = GlobOptions::default();
        opts.dotglob = true;
        let expander = make_expander(opts);
        assert!(expander.effective_dotglob());
    }

    #[test]
    fn test_effective_dotglob_with_globignore() {
        let mut env = HashMap::new();
        env.insert("GLOBIGNORE".to_string(), "*.log".to_string());
        let expander = make_expander_with_env(env, GlobOptions::default());
        assert!(expander.effective_dotglob());
    }

    #[test]
    fn test_effective_dotglob_false_by_default() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.effective_dotglob());
    }

    // -- has_nullglob, has_failglob --

    #[test]
    fn test_has_nullglob() {
        let mut opts = GlobOptions::default();
        opts.nullglob = true;
        let expander = make_expander(opts);
        assert!(expander.has_nullglob());
    }

    #[test]
    fn test_has_failglob() {
        let mut opts = GlobOptions::default();
        opts.failglob = true;
        let expander = make_expander(opts);
        assert!(expander.has_failglob());
    }

    #[test]
    fn test_has_nullglob_false_by_default() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.has_nullglob());
    }

    #[test]
    fn test_has_failglob_false_by_default() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.has_failglob());
    }

    // =========================================================================
    // Async expansion tests (Task 3)
    // =========================================================================

    use crate::fs::MkdirOptions;

    /// Helper to create a populated InMemoryFs with a standard directory tree.
    async fn setup_test_fs() -> Arc<InMemoryFs> {
        let fs = Arc::new(InMemoryFs::new());
        // /home/user/file.txt
        // /home/user/file.rs
        // /home/user/data.json
        // /home/user/.hidden
        // /home/user/sub/nested.txt
        // /home/user/sub/deep/file.txt
        fs.mkdir("/home", &MkdirOptions { recursive: true })
            .await
            .unwrap();
        fs.mkdir("/home/user", &MkdirOptions { recursive: false })
            .await
            .unwrap();
        fs.mkdir("/home/user/sub", &MkdirOptions { recursive: false })
            .await
            .unwrap();
        fs.mkdir("/home/user/sub/deep", &MkdirOptions { recursive: false })
            .await
            .unwrap();
        fs.write_file("/home/user/file.txt", b"hello")
            .await
            .unwrap();
        fs.write_file("/home/user/file.rs", b"fn main(){}")
            .await
            .unwrap();
        fs.write_file("/home/user/data.json", b"{}")
            .await
            .unwrap();
        fs.write_file("/home/user/.hidden", b"secret")
            .await
            .unwrap();
        fs.write_file("/home/user/sub/nested.txt", b"nested")
            .await
            .unwrap();
        fs.write_file("/home/user/sub/deep/file.txt", b"deep")
            .await
            .unwrap();
        fs
    }

    fn make_expander_with_fs(
        fs: Arc<InMemoryFs>,
        cwd: &str,
        env: Option<&HashMap<String, String>>,
        options: GlobOptions,
    ) -> GlobExpander {
        GlobExpander::new(fs as Arc<dyn FileSystem>, cwd.to_string(), env, options)
    }

    // -- expand simple patterns --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_star_txt() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("*.txt").await;
        assert_eq!(result, vec!["file.txt"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_star_rs() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("*.rs").await;
        assert_eq!(result, vec!["file.rs"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_star_excludes_hidden() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("*").await;
        // Should NOT include .hidden, should include dirs
        assert_eq!(
            result,
            vec!["data.json", "file.rs", "file.txt", "sub"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_dot_star_matches_hidden() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand(".*").await;
        assert_eq!(result, vec![".hidden"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_star_with_dotglob() {
        let fs = setup_test_fs().await;
        let mut opts = GlobOptions::default();
        opts.dotglob = true;
        let expander = make_expander_with_fs(fs, "/home/user", None, opts);
        let result = expander.expand("*").await;
        // With dotglob, hidden files are included (but . and .. filtered by globskipdots)
        assert!(result.contains(&".hidden".to_string()));
        assert!(result.contains(&"file.txt".to_string()));
        assert!(result.contains(&"data.json".to_string()));
    }

    // -- expand with subdirectory patterns --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_subdir_pattern() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("sub/*.txt").await;
        assert_eq!(result, vec!["sub/nested.txt"]);
    }

    // -- expand recursive (globstar) --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_recursive_globstar() {
        let fs = setup_test_fs().await;
        let mut opts = GlobOptions::default();
        opts.globstar = true;
        let expander = make_expander_with_fs(fs, "/home/user", None, opts);
        let result = expander.expand("**/*.txt").await;
        // Should match at all levels: file.txt, sub/nested.txt, sub/deep/file.txt
        assert_eq!(
            result,
            vec!["file.txt", "sub/deep/file.txt", "sub/nested.txt"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_recursive_without_globstar_treats_as_star() {
        let fs = setup_test_fs().await;
        // globstar is false by default
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("**/*.txt").await;
        // Without globstar, ** is treated as *, so it matches one level only
        // */*.txt matches sub/nested.txt
        assert_eq!(result, vec!["sub/nested.txt"]);
    }

    // -- expand with no matches --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_no_matches_returns_empty() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("nonexistent*").await;
        assert!(result.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_no_matches_with_nullglob() {
        let fs = setup_test_fs().await;
        let mut opts = GlobOptions::default();
        opts.nullglob = true;
        let expander = make_expander_with_fs(fs, "/home/user", None, opts);
        let result = expander.expand("nonexistent*").await;
        assert!(result.is_empty());
    }

    // -- expand absolute paths --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_absolute_path() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("/home/user/*.txt").await;
        assert_eq!(result, vec!["/home/user/file.txt"]);
    }

    // -- expand_args --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_args_mixed() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let args = vec![
            "hello".to_string(),
            "*.txt".to_string(),
            "*.rs".to_string(),
        ];
        let result = expander.expand_args(&args, None).await;
        assert_eq!(
            result,
            vec!["hello", "file.txt", "file.rs"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_args_quoted_no_expansion() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let args = vec!["*.txt".to_string()];
        let quoted = vec![true];
        let result = expander.expand_args(&args, Some(&quoted)).await;
        assert_eq!(result, vec!["*.txt"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_args_no_match_keeps_original() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let args = vec!["nonexistent*.xyz".to_string()];
        let result = expander.expand_args(&args, None).await;
        assert_eq!(result, vec!["nonexistent*.xyz"]);
    }

    // -- GLOBIGNORE filtering --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_with_globignore() {
        let fs = setup_test_fs().await;
        let mut env = HashMap::new();
        env.insert("GLOBIGNORE".to_string(), "*.txt".to_string());
        let expander =
            make_expander_with_fs(fs, "/home/user", Some(&env), GlobOptions::default());
        let result = expander.expand("*").await;
        // *.txt files should be excluded; GLOBIGNORE also enables dotglob
        // so .hidden is included
        assert!(result.contains(&"data.json".to_string()));
        assert!(result.contains(&"file.rs".to_string()));
        assert!(result.contains(&".hidden".to_string()));
        assert!(!result.contains(&"file.txt".to_string()));
    }

    // -- is_globstar_valid additional tests --

    #[test]
    fn test_is_globstar_valid_with_prefix_slash() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.is_globstar_valid("src/**/test"));
    }

    #[test]
    fn test_is_globstar_valid_triple_star_invalid() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.is_globstar_valid("***"));
    }

    // -- expand with question mark pattern --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_question_mark_pattern() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("file.??").await;
        assert_eq!(result, vec!["file.rs"]);
    }

    // -- expand with bracket pattern --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_bracket_pattern() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("[df]*.json").await;
        assert_eq!(result, vec!["data.json"]);
    }

    // -- expand with no glob chars returns pattern as-is --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_no_glob_chars_returns_pattern() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("file.txt").await;
        assert_eq!(result, vec!["file.txt"]);
    }

    // -- expand_args with plain args only --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_args_no_globs() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let args = vec!["hello".to_string(), "world".to_string()];
        let result = expander.expand_args(&args, None).await;
        assert_eq!(result, vec!["hello", "world"]);
    }

    // -- expand sorted results --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_results_are_sorted() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("*").await;
        let mut sorted = result.clone();
        sorted.sort();
        assert_eq!(result, sorted);
    }

    // -- expand deep nested pattern --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_deep_nested_pattern() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let result = expander.expand("sub/deep/*.txt").await;
        assert_eq!(result, vec!["sub/deep/file.txt"]);
    }

    // -- expand_args with partial quoted flags --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_expand_args_partial_quoted_flags() {
        let fs = setup_test_fs().await;
        let expander = make_expander_with_fs(fs, "/home/user", None, GlobOptions::default());
        let args = vec![
            "*.txt".to_string(),
            "*.rs".to_string(),
        ];
        // Only first arg is quoted
        let quoted = vec![true, false];
        let result = expander.expand_args(&args, Some(&quoted)).await;
        assert_eq!(result, vec!["*.txt", "file.rs"]);
    }

    // -- has_glob_chars --

    #[test]
    fn test_has_glob_chars_star() {
        let expander = make_expander(GlobOptions::default());
        assert!(expander.has_glob_chars("*.txt"));
    }

    #[test]
    fn test_has_glob_chars_no_glob() {
        let expander = make_expander(GlobOptions::default());
        assert!(!expander.has_glob_chars("file.txt"));
    }
}
