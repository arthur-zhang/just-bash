//! SHELLOPTS and BASHOPTS variable helpers.
//!
//! SHELLOPTS is a colon-separated list of enabled shell options from `set -o`.
//! BASHOPTS is a colon-separated list of enabled bash-specific options from `shopt`.

use crate::interpreter::types::{ShellOptions, ShoptOptions};

/// List of shell option names in the order they appear in SHELLOPTS.
/// This matches bash's ordering (alphabetical).
const SHELLOPTS_OPTIONS: &[(&str, fn(&ShellOptions) -> bool)] = &[
    ("allexport", |o| o.allexport),
    ("errexit", |o| o.errexit),
    ("noglob", |o| o.noglob),
    ("noclobber", |o| o.noclobber),
    ("noexec", |o| o.noexec),
    ("nounset", |o| o.nounset),
    ("pipefail", |o| o.pipefail),
    ("posix", |o| o.posix),
    ("verbose", |o| o.verbose),
    ("xtrace", |o| o.xtrace),
];

/// Options that are always enabled in bash (no-op in our implementation but
/// should appear in SHELLOPTS for compatibility).
/// These are in alphabetical order.
const ALWAYS_ON_OPTIONS: &[&str] = &["braceexpand", "hashall", "interactive-comments"];

/// Build the SHELLOPTS string from current shell options.
/// Returns a colon-separated list of enabled options (alphabetically sorted).
/// Includes always-on options like braceexpand, hashall, interactive-comments.
pub fn build_shellopts(options: &ShellOptions) -> String {
    let mut enabled: Vec<&str> = Vec::new();

    // Collect all options with their enabled status
    let mut all_options: Vec<(&str, bool)> = Vec::new();

    // Add always-on options
    for opt in ALWAYS_ON_OPTIONS {
        all_options.push((opt, true));
    }

    // Add dynamic options
    for (name, getter) in SHELLOPTS_OPTIONS {
        all_options.push((name, getter(options)));
    }

    // Sort alphabetically
    all_options.sort_by(|a, b| a.0.cmp(b.0));

    // Collect enabled options
    for (name, is_enabled) in all_options {
        if is_enabled {
            enabled.push(name);
        }
    }

    enabled.join(":")
}

/// List of shopt option names in the order they appear in BASHOPTS.
/// This matches bash's ordering (alphabetical).
const BASHOPTS_OPTIONS: &[(&str, fn(&ShoptOptions) -> bool)] = &[
    ("dotglob", |o| o.dotglob),
    ("expand_aliases", |o| o.expand_aliases),
    ("extglob", |o| o.extglob),
    ("failglob", |o| o.failglob),
    ("globskipdots", |o| o.globskipdots),
    ("globstar", |o| o.globstar),
    ("lastpipe", |o| o.lastpipe),
    ("nocaseglob", |o| o.nocaseglob),
    ("nocasematch", |o| o.nocasematch),
    ("nullglob", |o| o.nullglob),
    ("xpg_echo", |o| o.xpg_echo),
];

/// Build the BASHOPTS string from current shopt options.
/// Returns a colon-separated list of enabled options (alphabetically sorted).
pub fn build_bashopts(shopt_options: &ShoptOptions) -> String {
    let mut enabled: Vec<&str> = Vec::new();

    for (name, getter) in BASHOPTS_OPTIONS {
        if getter(shopt_options) {
            enabled.push(name);
        }
    }

    enabled.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_shellopts_default() {
        let options = ShellOptions::default();
        let result = build_shellopts(&options);
        // Should contain always-on options
        assert!(result.contains("braceexpand"));
        assert!(result.contains("hashall"));
        assert!(result.contains("interactive-comments"));
        // Should not contain disabled options
        assert!(!result.contains("errexit"));
        assert!(!result.contains("xtrace"));
    }

    #[test]
    fn test_build_shellopts_with_errexit() {
        let mut options = ShellOptions::default();
        options.errexit = true;
        let result = build_shellopts(&options);
        assert!(result.contains("errexit"));
    }

    #[test]
    fn test_build_shellopts_alphabetical() {
        let mut options = ShellOptions::default();
        options.errexit = true;
        options.xtrace = true;
        let result = build_shellopts(&options);
        // Check that options are in alphabetical order
        let parts: Vec<&str> = result.split(':').collect();
        let mut sorted = parts.clone();
        sorted.sort();
        assert_eq!(parts, sorted);
    }

    #[test]
    fn test_build_bashopts_default() {
        let options = ShoptOptions::default();
        let result = build_bashopts(&options);
        // globskipdots is true by default
        assert!(result.contains("globskipdots"));
        // Others should be off
        assert!(!result.contains("extglob"));
        assert!(!result.contains("dotglob"));
    }

    #[test]
    fn test_build_bashopts_with_extglob() {
        let mut options = ShoptOptions::default();
        options.extglob = true;
        let result = build_bashopts(&options);
        assert!(result.contains("extglob"));
    }
}
