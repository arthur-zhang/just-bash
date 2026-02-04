//! Tilde Expansion
//!
//! Functions for handling tilde (~) expansion in word expansion.

use crate::interpreter::InterpreterState;

/// Apply tilde expansion to a string.
/// Used after brace expansion to handle cases like ~{/src,root} -> ~/src ~root -> /home/user/src /root
/// Only expands ~ at the start of the string followed by / or end of string.
pub fn apply_tilde_expansion(state: &InterpreterState, value: &str) -> String {
    if !value.starts_with('~') {
        return value.to_string();
    }

    // Use HOME if set (even if empty), otherwise fall back to /home/user
    let home = state
        .env
        .get("HOME")
        .map(|s| s.as_str())
        .unwrap_or("/home/user");

    // ~/ or just ~
    if value == "~" || value.starts_with("~/") {
        return format!("{}{}", home, &value[1..]);
    }

    // ~username case: find where the username ends
    // Username chars are alphanumeric, underscore, and hyphen
    let chars: Vec<char> = value.chars().collect();
    let mut i = 1;
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
        i += 1;
    }
    let username: String = chars[1..i].iter().collect();
    let rest: String = chars[i..].iter().collect();

    // Only expand if followed by / or end of string
    if !rest.is_empty() && !rest.starts_with('/') {
        return value.to_string();
    }

    // Only support ~root expansion in sandboxed environment
    if username == "root" {
        return format!("/root{}", rest);
    }

    // Unknown user - keep literal
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state_with_home(home: Option<&str>) -> InterpreterState {
        let mut env = HashMap::new();
        if let Some(h) = home {
            env.insert("HOME".to_string(), h.to_string());
        }
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_tilde_alone() {
        let state = make_state_with_home(Some("/home/user"));
        assert_eq!(apply_tilde_expansion(&state, "~"), "/home/user");
    }

    #[test]
    fn test_tilde_slash() {
        let state = make_state_with_home(Some("/home/user"));
        assert_eq!(apply_tilde_expansion(&state, "~/src"), "/home/user/src");
    }

    #[test]
    fn test_tilde_root() {
        let state = make_state_with_home(Some("/home/user"));
        assert_eq!(apply_tilde_expansion(&state, "~root"), "/root");
        assert_eq!(apply_tilde_expansion(&state, "~root/bin"), "/root/bin");
    }

    #[test]
    fn test_tilde_unknown_user() {
        let state = make_state_with_home(Some("/home/user"));
        assert_eq!(apply_tilde_expansion(&state, "~unknown"), "~unknown");
        assert_eq!(apply_tilde_expansion(&state, "~unknown/dir"), "~unknown/dir");
    }

    #[test]
    fn test_no_tilde() {
        let state = make_state_with_home(Some("/home/user"));
        assert_eq!(apply_tilde_expansion(&state, "/path/to/file"), "/path/to/file");
        assert_eq!(apply_tilde_expansion(&state, "plain"), "plain");
    }

    #[test]
    fn test_tilde_no_home() {
        let state = make_state_with_home(None);
        assert_eq!(apply_tilde_expansion(&state, "~"), "/home/user");
        assert_eq!(apply_tilde_expansion(&state, "~/src"), "/home/user/src");
    }

    #[test]
    fn test_tilde_empty_home() {
        let state = make_state_with_home(Some(""));
        assert_eq!(apply_tilde_expansion(&state, "~"), "");
        assert_eq!(apply_tilde_expansion(&state, "~/src"), "/src");
    }
}
