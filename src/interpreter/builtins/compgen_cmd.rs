//! compgen - Generate completion matches
//!
//! Usage:
//!   compgen -v [prefix]         - List variable names (optionally starting with prefix)
//!   compgen -A variable [prefix] - Same as -v
//!   compgen -A function [prefix] - List function names
//!   compgen -e [prefix]          - List exported variable names
//!   compgen -A builtin [prefix]  - List builtin command names
//!   compgen -A keyword [prefix]  - List shell keywords (alias: -k)
//!   compgen -A alias [prefix]    - List alias names
//!   compgen -A shopt [prefix]    - List shopt options
//!   compgen -A helptopic [prefix] - List help topics
//!   compgen -W wordlist [prefix]  - Generate from wordlist
//!   compgen -P prefix             - Prefix to add to completions
//!   compgen -S suffix             - Suffix to add to completions

use crate::interpreter::types::InterpreterState;

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// List of shell keywords (matches bash)
pub const SHELL_KEYWORDS: &[&str] = &[
    "!", "[[", "]]", "case", "do", "done", "elif", "else", "esac", "fi",
    "for", "function", "if", "in", "then", "time", "until", "while", "{", "}",
];

/// List of shell builtins
pub const SHELL_BUILTINS: &[&str] = &[
    ".", ":", "[", "alias", "bg", "bind", "break", "builtin", "caller", "cd",
    "command", "compgen", "complete", "compopt", "continue", "declare", "dirs",
    "disown", "echo", "enable", "eval", "exec", "exit", "export", "false", "fc",
    "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local",
    "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read", "readarray",
    "readonly", "return", "set", "shift", "shopt", "source", "suspend", "test",
    "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias",
    "unset", "wait",
];

/// List of shopt options
pub const SHOPT_OPTIONS: &[&str] = &[
    "autocd", "assoc_expand_once", "cdable_vars", "cdspell", "checkhash",
    "checkjobs", "checkwinsize", "cmdhist", "compat31", "compat32", "compat40",
    "compat41", "compat42", "compat43", "compat44", "complete_fullquote",
    "direxpand", "dirspell", "dotglob", "execfail", "expand_aliases", "extdebug",
    "extglob", "extquote", "failglob", "force_fignore", "globasciiranges",
    "globstar", "gnu_errfmt", "histappend", "histreedit", "histverify",
    "hostcomplete", "huponexit", "inherit_errexit", "interactive_comments",
    "lastpipe", "lithist", "localvar_inherit", "localvar_unset", "login_shell",
    "mailwarn", "no_empty_cmd_completion", "nocaseglob", "nocasematch", "nullglob",
    "progcomp", "progcomp_alias", "promptvars", "restricted_shell", "shift_verbose",
    "sourcepath", "xpg_echo",
];

/// Valid action types for -A option
const VALID_ACTIONS: &[&str] = &[
    "alias", "arrayvar", "binding", "builtin", "command", "directory", "disabled",
    "enabled", "export", "file", "function", "group", "helptopic", "hostname",
    "job", "keyword", "running", "service", "setopt", "shopt", "signal",
    "stopped", "user", "variable",
];

/// Handle the `compgen` builtin command.
///
/// Note: This implementation handles the simpler cases. File/directory completions
/// and command execution (-C, -F) require runtime dependencies.
pub fn handle_compgen(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse options
    let mut action_types: Vec<String> = Vec::new();
    let mut wordlist: Option<String> = None;
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut search_prefix: Option<String> = None;
    let mut exclude_pattern: Option<String> = None;
    let mut processed_args: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-v" {
            action_types.push("variable".to_string());
        } else if arg == "-e" {
            action_types.push("export".to_string());
        } else if arg == "-f" {
            action_types.push("file".to_string());
        } else if arg == "-d" {
            action_types.push("directory".to_string());
        } else if arg == "-k" {
            action_types.push("keyword".to_string());
        } else if arg == "-A" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -A: option requires an argument\n".to_string(), 2);
            }
            let action_type = &args[i];
            if !VALID_ACTIONS.contains(&action_type.as_str()) {
                return (String::new(), format!("compgen: {}: invalid action name\n", action_type), 2);
            }
            action_types.push(action_type.clone());
        } else if arg == "-W" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -W: option requires an argument\n".to_string(), 2);
            }
            wordlist = Some(args[i].clone());
        } else if arg == "-P" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -P: option requires an argument\n".to_string(), 2);
            }
            prefix = args[i].clone();
        } else if arg == "-S" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -S: option requires an argument\n".to_string(), 2);
            }
            suffix = args[i].clone();
        } else if arg == "-X" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -X: option requires an argument\n".to_string(), 2);
            }
            exclude_pattern = Some(args[i].clone());
        } else if arg == "-o" {
            i += 1;
            if i >= args.len() {
                return (String::new(), "compgen: -o: option requires an argument\n".to_string(), 2);
            }
            let opt = &args[i];
            // Validate option
            let valid_opts = ["plusdirs", "dirnames", "default", "filenames", "nospace", "bashdefault", "noquote"];
            if !valid_opts.contains(&opt.as_str()) {
                return (String::new(), format!("compgen: {}: invalid option name\n", opt), 2);
            }
            // These options are mostly for display, not generation
        } else if arg == "-F" || arg == "-C" || arg == "-G" {
            // These require runtime dependencies, skip the argument
            i += 1;
            if i >= args.len() {
                return (String::new(), format!("compgen: {}: option requires an argument\n", arg), 2);
            }
            // Skip - not implemented
        } else if arg == "--" {
            processed_args.extend(args[i + 1..].iter().cloned());
            break;
        } else if !arg.starts_with('-') {
            processed_args.push(arg.clone());
        }
        i += 1;
    }

    // The search prefix is the first non-option argument
    search_prefix = processed_args.first().cloned();

    // Collect completions
    let mut completions: Vec<String> = Vec::new();

    // Handle action types
    for action_type in &action_types {
        match action_type.as_str() {
            "variable" => {
                completions.extend(get_variable_names(state, search_prefix.as_deref()));
            }
            "export" => {
                completions.extend(get_exported_variable_names(state, search_prefix.as_deref()));
            }
            "function" => {
                completions.extend(get_function_names(state, search_prefix.as_deref()));
            }
            "builtin" => {
                completions.extend(get_builtin_names(search_prefix.as_deref()));
            }
            "keyword" => {
                completions.extend(get_keyword_names(search_prefix.as_deref()));
            }
            "alias" => {
                completions.extend(get_alias_names(state, search_prefix.as_deref()));
            }
            "shopt" => {
                completions.extend(get_shopt_names(search_prefix.as_deref()));
            }
            "helptopic" => {
                completions.extend(get_help_topic_names(search_prefix.as_deref()));
            }
            "file" | "directory" | "command" | "user" => {
                // These require filesystem access - not implemented
            }
            _ => {}
        }
    }

    // Handle wordlist
    if let Some(ref wl) = wordlist {
        let words = split_wordlist(state, wl);
        for word in words {
            if search_prefix.is_none() || word.starts_with(search_prefix.as_ref().unwrap()) {
                completions.push(word);
            }
        }
    }

    // Apply -X filter
    if let Some(ref pattern) = exclude_pattern {
        let is_negated = pattern.starts_with('!');
        let pat = if is_negated { &pattern[1..] } else { pattern.as_str() };

        completions.retain(|c| {
            let matches = simple_pattern_match(c, pat);
            if is_negated { matches } else { !matches }
        });
    }

    // If no completions found and we had a search prefix, return exit code 1
    if completions.is_empty() && search_prefix.is_some() {
        return (String::new(), String::new(), 1);
    }

    // Apply prefix/suffix and output
    let output: String = completions
        .iter()
        .map(|c| format!("{}{}{}", prefix, c, suffix))
        .collect::<Vec<_>>()
        .join("\n");

    let output = if output.is_empty() { output } else { format!("{}\n", output) };
    (output, String::new(), 0)
}

/// Get all variable names, optionally filtered by prefix
fn get_variable_names(state: &InterpreterState, prefix: Option<&str>) -> Vec<String> {
    use std::collections::HashSet;
    let mut names: HashSet<String> = HashSet::new();

    for key in state.env.keys() {
        // Skip internal array markers
        if key.contains('_') && is_array_element_key(key) {
            continue;
        }
        if key.ends_with("__length") {
            continue;
        }
        if is_valid_identifier(key) {
            names.insert(key.clone());
        }
    }

    let mut result: Vec<String> = names.into_iter().collect();
    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get exported variable names, optionally filtered by prefix
fn get_exported_variable_names(state: &InterpreterState, prefix: Option<&str>) -> Vec<String> {
    let exported_vars = match &state.exported_vars {
        Some(vars) => vars,
        None => return Vec::new(),
    };

    let mut result: Vec<String> = exported_vars
        .iter()
        .filter(|n| {
            if n.contains('_') && is_array_element_key(n) {
                return false;
            }
            if n.ends_with("__length") {
                return false;
            }
            state.env.contains_key(*n)
        })
        .cloned()
        .collect();

    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get function names, optionally filtered by prefix
fn get_function_names(state: &InterpreterState, prefix: Option<&str>) -> Vec<String> {
    let mut result: Vec<String> = state.functions.keys().cloned().collect();
    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get builtin command names, optionally filtered by prefix
fn get_builtin_names(prefix: Option<&str>) -> Vec<String> {
    let mut result: Vec<String> = SHELL_BUILTINS.iter().map(|s| s.to_string()).collect();
    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get shell keyword names, optionally filtered by prefix
fn get_keyword_names(prefix: Option<&str>) -> Vec<String> {
    let mut result: Vec<String> = SHELL_KEYWORDS.iter().map(|s| s.to_string()).collect();
    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get alias names, optionally filtered by prefix
fn get_alias_names(state: &InterpreterState, prefix: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    // Check aliases map
    if let Some(ref aliases) = state.aliases {
        names.extend(aliases.keys().cloned());
    }

    if let Some(p) = prefix {
        names.retain(|n| n.starts_with(p));
    }
    names.sort();
    names
}

/// Get shopt option names, optionally filtered by prefix
fn get_shopt_names(prefix: Option<&str>) -> Vec<String> {
    let mut result: Vec<String> = SHOPT_OPTIONS.iter().map(|s| s.to_string()).collect();
    if let Some(p) = prefix {
        result.retain(|n| n.starts_with(p));
    }
    result.sort();
    result
}

/// Get help topic names, optionally filtered by prefix
fn get_help_topic_names(prefix: Option<&str>) -> Vec<String> {
    // Help topics are the same as builtins
    get_builtin_names(prefix)
}

/// Split a wordlist string into individual words, respecting IFS
fn split_wordlist(state: &InterpreterState, wordlist: &str) -> Vec<String> {
    let ifs = state.env.get("IFS").map(|s| s.as_str()).unwrap_or(" \t\n");

    if ifs.is_empty() {
        return vec![wordlist.to_string()];
    }

    let ifs_chars: std::collections::HashSet<char> = ifs.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current_word = String::new();
    let mut chars = wordlist.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Backslash escape: the next character is literal
            if let Some(next_ch) = chars.next() {
                current_word.push(next_ch);
            }
        } else if ifs_chars.contains(&ch) {
            // This is an IFS delimiter
            if !current_word.is_empty() {
                words.push(current_word);
                current_word = String::new();
            }
        } else {
            current_word.push(ch);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

/// Simple pattern matching (glob-style)
fn simple_pattern_match(s: &str, pattern: &str) -> bool {
    // Simple implementation: only handle * and ?
    let mut s_chars = s.chars().peekable();
    let mut p_chars = pattern.chars().peekable();

    while let Some(p) = p_chars.next() {
        match p {
            '*' => {
                // Match zero or more characters
                if p_chars.peek().is_none() {
                    return true; // * at end matches everything
                }
                // Try matching the rest of the pattern at each position
                let remaining_pattern: String = p_chars.collect();
                let mut remaining_s = String::new();
                while s_chars.peek().is_some() {
                    remaining_s.push(s_chars.next().unwrap());
                    let test_s: String = remaining_s.chars().rev().collect::<String>()
                        .chars().rev().collect();
                    // This is a simplified check - full glob matching is more complex
                }
                return false;
            }
            '?' => {
                // Match exactly one character
                if s_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match literal character
                if s_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    s_chars.peek().is_none()
}

/// Check if a key looks like an array element key (name_N where N is a number)
fn is_array_element_key(key: &str) -> bool {
    if let Some(underscore_pos) = key.rfind('_') {
        let suffix = &key[underscore_pos + 1..];
        suffix.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Check if a string is a valid shell identifier
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !matches!(first, b'a'..=b'z' | b'A'..=b'Z' | b'_') {
        return false;
    }
    bytes[1..].iter().all(|&b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_names() {
        let result = get_builtin_names(None);
        assert!(result.contains(&"cd".to_string()));
        assert!(result.contains(&"echo".to_string()));
        assert!(result.contains(&"exit".to_string()));
    }

    #[test]
    fn test_get_builtin_names_with_prefix() {
        let result = get_builtin_names(Some("ex"));
        assert!(result.contains(&"exec".to_string()));
        assert!(result.contains(&"exit".to_string()));
        assert!(result.contains(&"export".to_string()));
        assert!(!result.contains(&"cd".to_string()));
    }

    #[test]
    fn test_get_keyword_names() {
        let result = get_keyword_names(None);
        assert!(result.contains(&"if".to_string()));
        assert!(result.contains(&"then".to_string()));
        assert!(result.contains(&"fi".to_string()));
    }

    #[test]
    fn test_get_shopt_names() {
        let result = get_shopt_names(None);
        assert!(result.contains(&"extglob".to_string()));
        assert!(result.contains(&"nullglob".to_string()));
    }

    #[test]
    fn test_get_shopt_names_with_prefix() {
        let result = get_shopt_names(Some("ext"));
        assert!(result.contains(&"extglob".to_string()));
        assert!(result.contains(&"extdebug".to_string()));
        assert!(!result.contains(&"nullglob".to_string()));
    }

    #[test]
    fn test_get_variable_names() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.env.insert("BAR".to_string(), "baz".to_string());
        let result = get_variable_names(&state, None);
        assert!(result.contains(&"FOO".to_string()));
        assert!(result.contains(&"BAR".to_string()));
    }

    #[test]
    fn test_get_variable_names_with_prefix() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        state.env.insert("BAR".to_string(), "baz".to_string());
        let result = get_variable_names(&state, Some("F"));
        assert!(result.contains(&"FOO".to_string()));
        assert!(!result.contains(&"BAR".to_string()));
    }

    #[test]
    fn test_split_wordlist() {
        let state = InterpreterState::default();
        let result = split_wordlist(&state, "one two three");
        assert_eq!(result, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_split_wordlist_with_escape() {
        let state = InterpreterState::default();
        let result = split_wordlist(&state, "one\\ two three");
        assert_eq!(result, vec!["one two", "three"]);
    }

    #[test]
    fn test_handle_compgen_builtin() {
        let mut state = InterpreterState::default();
        let args = vec!["-A".to_string(), "builtin".to_string(), "ec".to_string()];
        let (stdout, stderr, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("echo"));
    }

    #[test]
    fn test_handle_compgen_keyword() {
        let mut state = InterpreterState::default();
        let args = vec!["-k".to_string(), "if".to_string()];
        let (stdout, stderr, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("if"));
    }

    #[test]
    fn test_handle_compgen_wordlist() {
        let mut state = InterpreterState::default();
        let args = vec!["-W".to_string(), "apple banana cherry".to_string(), "b".to_string()];
        let (stdout, stderr, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("banana"));
        assert!(!stdout.contains("apple"));
        assert!(!stdout.contains("cherry"));
    }

    #[test]
    fn test_handle_compgen_prefix_suffix() {
        let mut state = InterpreterState::default();
        let args = vec![
            "-W".to_string(), "foo bar".to_string(),
            "-P".to_string(), "pre_".to_string(),
            "-S".to_string(), "_suf".to_string(),
        ];
        let (stdout, _, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stdout.contains("pre_foo_suf"));
        assert!(stdout.contains("pre_bar_suf"));
    }

    #[test]
    fn test_handle_compgen_invalid_action() {
        let mut state = InterpreterState::default();
        let args = vec!["-A".to_string(), "invalid".to_string()];
        let (_, stderr, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 2);
        assert!(stderr.contains("invalid action name"));
    }

    #[test]
    fn test_handle_compgen_no_match() {
        let mut state = InterpreterState::default();
        let args = vec!["-W".to_string(), "foo bar".to_string(), "xyz".to_string()];
        let (stdout, _, code) = handle_compgen(&mut state, &args);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
    }

    #[test]
    fn test_is_array_element_key() {
        assert!(is_array_element_key("arr_0"));
        assert!(is_array_element_key("arr_123"));
        assert!(!is_array_element_key("arr"));
        assert!(!is_array_element_key("arr_abc"));
        assert!(!is_array_element_key("arr__length"));
    }
}
