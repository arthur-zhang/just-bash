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
}
