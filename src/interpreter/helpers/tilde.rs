//! Tilde expansion helper functions.
//!
//! Handles ~ expansion in assignment contexts.

use std::collections::HashMap;

/// Expand tildes in assignment values (PATH-like expansion)
/// - ~ at start expands to HOME
/// - ~ after : expands to HOME (for PATH-like values)
/// - ~username expands to user's home (only root supported)
pub fn expand_tildes_in_value(env: &HashMap<String, String>, value: &str) -> String {
    let home = env.get("HOME").map(|s| s.as_str()).unwrap_or("/home/user");

    // Split by : to handle PATH-like values
    let parts: Vec<&str> = value.split(':').collect();
    let expanded: Vec<String> = parts
        .iter()
        .map(|part| expand_tilde_part(home, part))
        .collect();

    expanded.join(":")
}

/// Expand a single tilde part.
fn expand_tilde_part(home: &str, part: &str) -> String {
    if part == "~" {
        return home.to_string();
    }
    if part == "~root" {
        return "/root".to_string();
    }
    if let Some(rest) = part.strip_prefix("~/") {
        return format!("{}/{}", home, rest);
    }
    if let Some(rest) = part.strip_prefix("~root/") {
        return format!("/root/{}", rest);
    }
    // ~otheruser stays literal (can't verify user exists)
    part.to_string()
}

/// Expand a single tilde at the start of a string.
pub fn expand_tilde(env: &HashMap<String, String>, value: &str) -> String {
    let home = env.get("HOME").map(|s| s.as_str()).unwrap_or("/home/user");
    expand_tilde_part(home, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/testuser".to_string());
        env
    }

    #[test]
    fn test_expand_tilde_simple() {
        let env = make_env();
        assert_eq!(expand_tilde(&env, "~"), "/home/testuser");
        assert_eq!(expand_tilde(&env, "~/bin"), "/home/testuser/bin");
        assert_eq!(expand_tilde(&env, "~root"), "/root");
        assert_eq!(expand_tilde(&env, "~root/bin"), "/root/bin");
        assert_eq!(expand_tilde(&env, "~other"), "~other");
        assert_eq!(expand_tilde(&env, "/usr/bin"), "/usr/bin");
    }

    #[test]
    fn test_expand_tildes_in_value_path() {
        let env = make_env();
        assert_eq!(
            expand_tildes_in_value(&env, "~/bin:~root/bin:/usr/bin"),
            "/home/testuser/bin:/root/bin:/usr/bin"
        );
    }

    #[test]
    fn test_expand_tildes_in_value_single() {
        let env = make_env();
        assert_eq!(expand_tildes_in_value(&env, "~"), "/home/testuser");
        assert_eq!(expand_tildes_in_value(&env, "~/Documents"), "/home/testuser/Documents");
    }

    #[test]
    fn test_expand_tildes_no_home() {
        let env = HashMap::new();
        assert_eq!(expand_tilde(&env, "~"), "/home/user");
        assert_eq!(expand_tilde(&env, "~/bin"), "/home/user/bin");
    }
}
