//! Word Expansion with Glob Handling
//!
//! Provides helper functions for glob/pathname expansion.
//! The main word expansion flow is handled at the interpreter level.

use crate::interpreter::expansion::{has_glob_pattern, unescape_glob_pattern};
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
    // Check if the pattern contains glob characters
    if !has_glob_pattern(pattern, extglob) {
        // No glob characters - return the unescaped pattern
        return Ok(GlobExpansionResult {
            values: vec![unescape_glob_pattern(pattern)],
            quoted: false,
        });
    }

    // Perform glob expansion
    let matches = match glob_pattern(pattern, cwd) {
        Ok(m) => m,
        Err(e) => {
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

/// Perform glob pattern matching.
/// Returns a list of matching file paths.
fn glob_pattern(pattern: &str, cwd: &Path) -> Result<Vec<String>, String> {
    // Use the glob crate for pattern matching
    let full_pattern = if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        format!("{}/{}", cwd.display(), pattern)
    };

    let mut matches = Vec::new();

    // Use glob crate
    match glob::glob(&full_pattern) {
        Ok(paths) => {
            for entry in paths {
                match entry {
                    Ok(path) => {
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

    // Sort matches for consistent output
    matches.sort();

    Ok(matches)
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
}
