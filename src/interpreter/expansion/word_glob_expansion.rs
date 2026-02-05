//! Word Expansion with Glob Handling
//!
//! Provides helper functions for glob/pathname expansion.
//! The main word expansion flow is handled at the interpreter level.
//!
//! This module handles:
//! - Glob pattern expansion
//! - Brace expansion result handling
//! - Word splitting and glob expansion coordination

use crate::interpreter::expansion::{has_glob_pattern, unescape_glob_pattern};
use crate::interpreter::helpers::{get_ifs, split_by_ifs_for_expansion};
use crate::interpreter::InterpreterState;
use std::fs;
use std::path::Path;

/// Result of glob expansion.
#[derive(Debug, Clone)]
pub struct GlobExpansionResult {
    /// The expanded values (file paths or original pattern if no match)
    pub values: Vec<String>,
    /// Whether the result should be treated as quoted (no further splitting)
    pub quoted: bool,
}

/// Expand a glob pattern to matching file paths.
/// If no matches are found, returns the original pattern (with glob escapes removed).
/// If failglob is enabled and no matches are found, returns an error.
pub fn expand_glob_pattern(
    pattern: &str,
    cwd: &Path,
    failglob: bool,
    nullglob: bool,
    extglob: bool,
) -> Result<GlobExpansionResult, String> {
    expand_glob_pattern_impl(pattern, cwd, failglob, nullglob, extglob, false, false)
}

/// Expand a glob pattern with full options support.
/// Supports dotglob (match files starting with .) and globstar (** pattern).
pub fn expand_glob_pattern_with_options(
    pattern: &str,
    cwd: &Path,
    options: &WordExpansionOptions,
) -> Result<GlobExpansionResult, String> {
    if options.noglob {
        return Ok(GlobExpansionResult {
            values: vec![unescape_glob_pattern(pattern)],
            quoted: false,
        });
    }
    expand_glob_pattern_impl(
        pattern,
        cwd,
        options.failglob,
        options.nullglob,
        options.extglob,
        options.dotglob,
        options.globstar,
    )
}

/// Internal implementation of glob pattern expansion with all options.
fn expand_glob_pattern_impl(
    pattern: &str,
    cwd: &Path,
    failglob: bool,
    nullglob: bool,
    extglob: bool,
    dotglob: bool,
    globstar: bool,
) -> Result<GlobExpansionResult, String> {
    // Check if the pattern contains glob characters
    if !has_glob_pattern(pattern, extglob) {
        // No glob characters - return the unescaped pattern
        return Ok(GlobExpansionResult {
            values: vec![unescape_glob_pattern(pattern)],
            quoted: false,
        });
    }

    // Perform glob expansion
    let matches = match glob_pattern_with_options(pattern, cwd, dotglob, globstar) {
        Ok(m) => m,
        Err(_e) => {
            if failglob {
                return Err(format!("no match: {}", pattern));
            }
            // Return the original pattern on error
            return Ok(GlobExpansionResult {
                values: vec![unescape_glob_pattern(pattern)],
                quoted: false,
            });
        }
    };

    if matches.is_empty() {
        if failglob {
            return Err(format!("no match: {}", pattern));
        }
        if nullglob {
            return Ok(GlobExpansionResult {
                values: vec![],
                quoted: false,
            });
        }
        // Return the original pattern
        return Ok(GlobExpansionResult {
            values: vec![unescape_glob_pattern(pattern)],
            quoted: false,
        });
    }

    Ok(GlobExpansionResult {
        values: matches,
        quoted: false,
    })
}

/// Perform glob pattern matching with options.
/// Returns a list of matching file paths.
fn glob_pattern_with_options(
    pattern: &str,
    cwd: &Path,
    dotglob: bool,
    globstar: bool,
) -> Result<Vec<String>, String> {
    // Handle globstar (**) pattern expansion
    let expanded_pattern = if globstar && pattern.contains("**") {
        pattern.to_string()
    } else {
        // If globstar is disabled, treat ** as * (single directory level)
        pattern.replace("**", "*")
    };

    // Use the glob crate for pattern matching
    let full_pattern = if expanded_pattern.starts_with('/') {
        expanded_pattern.clone()
    } else {
        format!("{}/{}", cwd.display(), expanded_pattern)
    };

    let mut matches = Vec::new();

    // Use glob crate with MatchOptions
    let glob_options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: !globstar,
        require_literal_leading_dot: !dotglob,
    };

    match glob::glob_with(&full_pattern, glob_options) {
        Ok(paths) => {
            for entry in paths {
                match entry {
                    Ok(path) => {
                        // Skip . and .. entries (globskipdots behavior)
                        if let Some(file_name) = path.file_name() {
                            let name = file_name.to_string_lossy();
                            if name == "." || name == ".." {
                                continue;
                            }
                        }

                        let path_str = if pattern.starts_with('/') {
                            path.display().to_string()
                        } else {
                            // Return relative path
                            path.strip_prefix(cwd)
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|_| path.display().to_string())
                        };
                        matches.push(path_str);
                    }
                    Err(_) => continue,
                }
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    // Sort matches for consistent output (bash sorts glob results)
    matches.sort();

    Ok(matches)
}

/// Legacy glob pattern matching for backward compatibility.
#[allow(dead_code)]
fn glob_pattern(pattern: &str, cwd: &Path) -> Result<Vec<String>, String> {
    glob_pattern_with_options(pattern, cwd, false, false)
}

/// Check if a word should be subject to glob expansion.
/// Returns false if the word is entirely quoted.
pub fn should_glob_expand(is_quoted: bool, noglob: bool) -> bool {
    !is_quoted && !noglob
}

/// Split a value by IFS and expand each resulting word as a glob pattern.
pub fn split_and_glob_expand(
    values: &[String],
    cwd: &Path,
    failglob: bool,
    nullglob: bool,
    noglob: bool,
    extglob: bool,
) -> Result<Vec<String>, String> {
    if noglob {
        return Ok(values.to_vec());
    }

    let mut result = Vec::new();
    for value in values {
        let expanded = expand_glob_pattern(value, cwd, failglob, nullglob, extglob)?;
        result.extend(expanded.values);
    }
    Ok(result)
}

/// Options for word expansion.
#[derive(Debug, Clone, Default)]
pub struct WordExpansionOptions {
    pub failglob: bool,
    pub nullglob: bool,
    pub noglob: bool,
    pub extglob: bool,
    pub globstar: bool,
    pub dotglob: bool,
}

impl WordExpansionOptions {
    /// Create options from interpreter state.
    pub fn from_state(state: &InterpreterState) -> Self {
        Self {
            failglob: state.shopt_options.failglob,
            nullglob: state.shopt_options.nullglob,
            noglob: state.options.noglob,
            extglob: state.shopt_options.extglob,
            globstar: state.shopt_options.globstar,
            dotglob: state.shopt_options.dotglob,
        }
    }
}

/// Handle brace expansion results by applying glob expansion to each result.
pub fn handle_brace_expansion_results(
    brace_expanded: &[String],
    cwd: &Path,
    options: &WordExpansionOptions,
) -> Result<GlobExpansionResult, String> {
    if options.noglob {
        return Ok(GlobExpansionResult {
            values: brace_expanded.to_vec(),
            quoted: false,
        });
    }

    let mut result = Vec::new();
    for value in brace_expanded {
        let expanded = expand_glob_pattern(
            value,
            cwd,
            options.failglob,
            options.nullglob,
            options.extglob,
        )?;
        result.extend(expanded.values);
    }

    Ok(GlobExpansionResult {
        values: result,
        quoted: false,
    })
}

/// Perform word splitting on a value and then glob expand each word.
pub fn split_and_glob_expand_with_state(
    value: &str,
    state: &InterpreterState,
    cwd: &Path,
) -> Result<Vec<String>, String> {
    let options = WordExpansionOptions::from_state(state);
    let ifs = get_ifs(&state.env);
    let words = split_by_ifs_for_expansion(value, ifs);

    if options.noglob {
        return Ok(words);
    }

    let mut result = Vec::new();
    for word in words {
        let expanded = expand_glob_pattern(
            &word,
            cwd,
            options.failglob,
            options.nullglob,
            options.extglob,
        )?;
        result.extend(expanded.values);
    }

    Ok(result)
}

/// Expand multiple values with glob expansion.
pub fn expand_values_with_glob(
    values: &[String],
    cwd: &Path,
    options: &WordExpansionOptions,
) -> Result<Vec<String>, String> {
    if options.noglob {
        return Ok(values.to_vec());
    }

    let mut result = Vec::new();
    for value in values {
        if has_glob_pattern(value, options.extglob) {
            let expanded = expand_glob_pattern(
                value,
                cwd,
                options.failglob,
                options.nullglob,
                options.extglob,
            )?;
            result.extend(expanded.values);
        } else {
            result.push(unescape_glob_pattern(value));
        }
    }

    Ok(result)
}

/// Apply glob expansion to a list of values.
/// Each value is checked for glob patterns and expanded if necessary.
/// Corresponds to TypeScript's `applyGlobToValues`.
pub fn apply_glob_to_values(
    values: &[String],
    cwd: &Path,
    options: &WordExpansionOptions,
) -> Result<GlobExpansionResult, String> {
    if options.noglob {
        return Ok(GlobExpansionResult {
            values: values.to_vec(),
            quoted: false,
        });
    }

    let mut expanded_values = Vec::new();
    for value in values {
        if has_glob_pattern(value, options.extglob) {
            let result = expand_glob_pattern_with_options(value, cwd, options)?;
            expanded_values.extend(result.values);
        } else {
            expanded_values.push(unescape_glob_pattern(value));
        }
    }

    Ok(GlobExpansionResult {
        values: expanded_values,
        quoted: false,
    })
}

/// Handle final glob expansion after word expansion.
/// This is used when the main expansion is complete and we need to apply
/// final glob processing based on whether the word contains glob parts.
/// Corresponds to TypeScript's `handleFinalGlobExpansion`.
pub fn handle_final_glob_expansion(
    value: &str,
    glob_pattern_str: Option<&str>,
    has_glob_parts: bool,
    has_quoted: bool,
    cwd: &Path,
    options: &WordExpansionOptions,
    ifs_chars: &str,
) -> Result<GlobExpansionResult, String> {
    // Handle explicit glob parts in the word
    if !options.noglob && has_glob_parts {
        if let Some(pattern) = glob_pattern_str {
            if has_glob_pattern(pattern, options.extglob) {
                let result = expand_glob_pattern_with_options(pattern, cwd, options)?;
                if !result.values.is_empty() && result.values[0] != pattern {
                    return Ok(result);
                }
                if result.values.is_empty() {
                    return Ok(GlobExpansionResult {
                        values: vec![],
                        quoted: false,
                    });
                }
            }
        }

        // Unescape and potentially split by IFS
        let unescaped = unescape_glob_pattern(value);
        if !ifs_chars.is_empty() {
            let split_values = split_by_ifs_for_expansion(&unescaped, ifs_chars);
            return Ok(GlobExpansionResult {
                values: split_values,
                quoted: false,
            });
        }
        return Ok(GlobExpansionResult {
            values: vec![unescaped],
            quoted: false,
        });
    }

    // Handle unquoted values with glob patterns
    if !has_quoted && !options.noglob && has_glob_pattern(value, options.extglob) {
        if let Some(pattern) = glob_pattern_str {
            if has_glob_pattern(pattern, options.extglob) {
                let result = expand_glob_pattern_with_options(pattern, cwd, options)?;
                if !result.values.is_empty() && result.values[0] != pattern {
                    return Ok(result);
                }
            }
        }
    }

    // Handle empty unquoted values
    if value.is_empty() && !has_quoted {
        return Ok(GlobExpansionResult {
            values: vec![],
            quoted: false,
        });
    }

    // Handle glob parts in unquoted context
    if has_glob_parts && !has_quoted {
        let unescaped = unescape_glob_pattern(value);
        if !ifs_chars.is_empty() {
            let split_values = split_by_ifs_for_expansion(&unescaped, ifs_chars);
            return Ok(GlobExpansionResult {
                values: split_values,
                quoted: false,
            });
        }
        return Ok(GlobExpansionResult {
            values: vec![unescaped],
            quoted: false,
        });
    }

    Ok(GlobExpansionResult {
        values: vec![value.to_string()],
        quoted: has_quoted,
    })
}

/// Filter criteria for glob matches.
#[derive(Debug, Clone, Default)]
pub struct GlobFilterCriteria {
    /// Only include directories
    pub directories_only: bool,
    /// Only include regular files
    pub files_only: bool,
    /// Only include executable files
    pub executables_only: bool,
    /// Exclude patterns (glob patterns to exclude)
    pub exclude_patterns: Vec<String>,
}

/// Filter glob matches based on criteria.
/// Returns only matches that satisfy all specified criteria.
pub fn filter_glob_matches(
    matches: &[String],
    cwd: &Path,
    criteria: &GlobFilterCriteria,
) -> Vec<String> {
    matches
        .iter()
        .filter(|m| {
            let path = if m.starts_with('/') {
                std::path::PathBuf::from(m)
            } else {
                cwd.join(m)
            };

            // Check directory filter
            if criteria.directories_only {
                if let Ok(metadata) = fs::metadata(&path) {
                    if !metadata.is_dir() {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Check file filter
            if criteria.files_only {
                if let Ok(metadata) = fs::metadata(&path) {
                    if !metadata.is_file() {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Check executable filter (Unix only)
            #[cfg(unix)]
            if criteria.executables_only {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&path) {
                    let mode = metadata.permissions().mode();
                    if mode & 0o111 == 0 {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Check exclude patterns
            for exclude_pattern in &criteria.exclude_patterns {
                // Simple wildcard matching for exclude patterns
                if matches_simple_pattern(m, exclude_pattern) {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}

/// Simple pattern matching for exclude patterns.
/// Supports * as wildcard for any sequence of characters.
fn matches_simple_pattern(value: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if !pattern.contains('*') {
        return value == pattern;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return true;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found_pos) = value[pos..].find(part) {
            // First part must be at the beginning (unless pattern starts with *)
            if i == 0 && !pattern.starts_with('*') && found_pos != 0 {
                return false;
            }
            pos += found_pos + part.len();
        } else {
            return false;
        }
    }

    // Last part must be at the end (unless pattern ends with *)
    if !pattern.ends_with('*') {
        if let Some(last_part) = parts.last() {
            if !last_part.is_empty() && !value.ends_with(last_part) {
                return false;
            }
        }
    }

    true
}

/// Expand glob pattern and return matches, applying options from interpreter state.
pub fn expand_glob_with_state(
    pattern: &str,
    state: &InterpreterState,
    cwd: &Path,
) -> Result<GlobExpansionResult, String> {
    let options = WordExpansionOptions::from_state(state);
    expand_glob_pattern_with_options(pattern, cwd, &options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_should_glob_expand() {
        assert!(should_glob_expand(false, false));
        assert!(!should_glob_expand(true, false));
        assert!(!should_glob_expand(false, true));
        assert!(!should_glob_expand(true, true));
    }

    #[test]
    fn test_expand_glob_no_pattern() {
        let cwd = env::current_dir().unwrap();
        let result = expand_glob_pattern("hello", &cwd, false, false, false).unwrap();
        assert_eq!(result.values, vec!["hello"]);
    }

    #[test]
    fn test_expand_glob_escaped() {
        let cwd = env::current_dir().unwrap();
        // Escaped glob characters should not trigger expansion
        let result = expand_glob_pattern("hello\\*world", &cwd, false, false, false).unwrap();
        assert_eq!(result.values, vec!["hello*world"]);
    }

    #[test]
    fn test_word_expansion_options_default() {
        let options = WordExpansionOptions::default();
        assert!(!options.failglob);
        assert!(!options.nullglob);
        assert!(!options.noglob);
        assert!(!options.extglob);
        assert!(!options.globstar);
        assert!(!options.dotglob);
    }

    #[test]
    fn test_expand_glob_pattern_with_options_noglob() {
        let cwd = env::current_dir().unwrap();
        let mut options = WordExpansionOptions::default();
        options.noglob = true;

        let result = expand_glob_pattern_with_options("*.rs", &cwd, &options).unwrap();
        assert_eq!(result.values, vec!["*.rs"]);
    }

    #[test]
    fn test_apply_glob_to_values_noglob() {
        let cwd = env::current_dir().unwrap();
        let mut options = WordExpansionOptions::default();
        options.noglob = true;

        let values = vec!["*.rs".to_string(), "hello".to_string()];
        let result = apply_glob_to_values(&values, &cwd, &options).unwrap();
        assert_eq!(result.values, values);
    }

    #[test]
    fn test_apply_glob_to_values_no_patterns() {
        let cwd = env::current_dir().unwrap();
        let options = WordExpansionOptions::default();

        let values = vec!["hello".to_string(), "world".to_string()];
        let result = apply_glob_to_values(&values, &cwd, &options).unwrap();
        assert_eq!(result.values, values);
    }

    #[test]
    fn test_handle_final_glob_expansion_empty_unquoted() {
        let cwd = env::current_dir().unwrap();
        let options = WordExpansionOptions::default();

        let result =
            handle_final_glob_expansion("", None, false, false, &cwd, &options, " \t\n").unwrap();
        assert!(result.values.is_empty());
    }

    #[test]
    fn test_handle_final_glob_expansion_quoted() {
        let cwd = env::current_dir().unwrap();
        let options = WordExpansionOptions::default();

        let result =
            handle_final_glob_expansion("hello", None, false, true, &cwd, &options, " \t\n")
                .unwrap();
        assert_eq!(result.values, vec!["hello"]);
        assert!(result.quoted);
    }

    #[test]
    fn test_matches_simple_pattern() {
        assert!(matches_simple_pattern("hello", "hello"));
        assert!(!matches_simple_pattern("hello", "world"));

        // Wildcard tests
        assert!(matches_simple_pattern("hello.txt", "*.txt"));
        assert!(!matches_simple_pattern("hello.rs", "*.txt"));
        assert!(matches_simple_pattern("test_file.txt", "test*"));
        assert!(matches_simple_pattern("test_file.txt", "*file*"));
        assert!(matches_simple_pattern("abc", "*"));
        assert!(matches_simple_pattern("", ""));
        assert!(!matches_simple_pattern("hello", ""));
    }

    #[test]
    fn test_glob_filter_criteria_default() {
        let criteria = GlobFilterCriteria::default();
        assert!(!criteria.directories_only);
        assert!(!criteria.files_only);
        assert!(!criteria.executables_only);
        assert!(criteria.exclude_patterns.is_empty());
    }

    #[test]
    fn test_filter_glob_matches_exclude() {
        let cwd = env::current_dir().unwrap();
        let criteria = GlobFilterCriteria {
            exclude_patterns: vec!["*.bak".to_string()],
            ..Default::default()
        };

        let matches = vec![
            "file.txt".to_string(),
            "file.bak".to_string(),
            "other.rs".to_string(),
        ];

        let filtered = filter_glob_matches(&matches, &cwd, &criteria);
        assert_eq!(filtered, vec!["file.txt", "other.rs"]);
    }

    #[test]
    fn test_nullglob_option() {
        let cwd = env::current_dir().unwrap();
        // Use a pattern that won't match anything
        let result =
            expand_glob_pattern("nonexistent_xyz_*.qqq", &cwd, false, true, false).unwrap();
        assert!(result.values.is_empty());
    }

    #[test]
    fn test_failglob_option() {
        let cwd = env::current_dir().unwrap();
        // Use a pattern that won't match anything
        let result = expand_glob_pattern("nonexistent_xyz_*.qqq", &cwd, true, false, false);
        assert!(result.is_err());
    }
}
