//! Variable Access
//!
//! Handles variable value retrieval, including:
//! - Special variables ($?, $$, $#, $@, $*, $0)
//! - Array access (${arr[0]}, ${arr[@]}, ${arr[*]})
//! - Positional parameters ($1, $2, ...)
//! - Regular variables
//! - Nameref resolution

use crate::interpreter::helpers::{
    get_array_indices, get_assoc_array_keys, get_ifs_separator, is_nameref, resolve_nameref,
    unquote_key,
};
use crate::interpreter::InterpreterState;
use regex_lite::Regex;

/// Expand simple variable references in a subscript string.
/// This handles patterns like $var and ${var} but not complex expansions.
/// Used to support namerefs pointing to array elements like A[$key].
pub fn expand_simple_vars_in_subscript(state: &InterpreterState, subscript: &str) -> String {
    let braced_re = Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}").unwrap();
    let simple_re = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

    // Replace ${varname} patterns
    let result = braced_re.replace_all(subscript, |caps: &regex_lite::Captures| {
        let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        state.env.get(name).cloned().unwrap_or_default()
    });

    // Replace $varname patterns (must be careful not to match ${})
    let result = simple_re.replace_all(&result, |caps: &regex_lite::Captures| {
        let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        state.env.get(name).cloned().unwrap_or_default()
    });

    result.to_string()
}

/// Array element type - can be indexed by number or string key
#[derive(Debug, Clone)]
pub enum ArrayIndex {
    Numeric(i64),
    String(String),
}

/// Get all elements of an array stored as arrayName_0, arrayName_1, etc.
/// Returns an array of (index/key, value) tuples, sorted by index/key.
/// For associative arrays, uses string keys.
/// Special arrays FUNCNAME, BASH_LINENO, and BASH_SOURCE are handled dynamically from call stack.
pub fn get_array_elements(state: &InterpreterState, array_name: &str) -> Vec<(ArrayIndex, String)> {
    // Handle special call stack arrays
    if array_name == "FUNCNAME" {
        if let Some(ref stack) = state.func_name_stack {
            return stack
                .iter()
                .enumerate()
                .map(|(i, name)| (ArrayIndex::Numeric(i as i64), name.clone()))
                .collect();
        }
        return Vec::new();
    }
    if array_name == "BASH_LINENO" {
        if let Some(ref stack) = state.call_line_stack {
            return stack
                .iter()
                .enumerate()
                .map(|(i, line)| (ArrayIndex::Numeric(i as i64), line.to_string()))
                .collect();
        }
        return Vec::new();
    }
    if array_name == "BASH_SOURCE" {
        if let Some(ref stack) = state.source_stack {
            return stack
                .iter()
                .enumerate()
                .map(|(i, source)| (ArrayIndex::Numeric(i as i64), source.clone()))
                .collect();
        }
        return Vec::new();
    }

    let is_assoc = state
        .associative_arrays
        .as_ref()
        .map(|aa| aa.contains(array_name))
        .unwrap_or(false);

    if is_assoc {
        // For associative arrays, get string keys
        let keys = get_assoc_array_keys(&state.env, array_name);
        return keys
            .into_iter()
            .map(|key| {
                let env_key = format!("{}_{}", array_name, key);
                let value = state.env.get(&env_key).cloned().unwrap_or_default();
                (ArrayIndex::String(key), value)
            })
            .collect();
    }

    // For indexed arrays, get numeric indices
    let indices = get_array_indices(&state.env, array_name);
    indices
        .into_iter()
        .map(|index| {
            let env_key = format!("{}_{}", array_name, index);
            let value = state.env.get(&env_key).cloned().unwrap_or_default();
            (ArrayIndex::Numeric(index as i64), value)
        })
        .collect()
}

/// Check if a variable is an array (has elements stored as name_0, name_1, etc.)
pub fn is_array(state: &InterpreterState, name: &str) -> bool {
    // Handle special call stack arrays - they're only arrays when inside functions
    if name == "FUNCNAME" {
        return state
            .func_name_stack
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    }
    if name == "BASH_LINENO" {
        return state
            .call_line_stack
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    }
    if name == "BASH_SOURCE" {
        return state
            .source_stack
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    }
    // Check if it's an associative array
    if state
        .associative_arrays
        .as_ref()
        .map(|aa| aa.contains(name))
        .unwrap_or(false)
    {
        return !get_assoc_array_keys(&state.env, name).is_empty();
    }
    // Check for indexed array elements
    !get_array_indices(&state.env, name).is_empty()
}

/// Constants for special variables that are always set
const ALWAYS_SET_SPECIAL_VARS: &[&str] = &[
    "?", "$", "#", "_", "-", "0", "PPID", "UID", "EUID", "RANDOM", "SECONDS", "BASH_VERSION", "!",
    "BASHPID", "LINENO",
];

/// Get the value of a variable.
/// Returns the variable value or empty string if not set.
/// Note: This is a simplified synchronous version. For full arithmetic subscript
/// evaluation, use the async version in the interpreter.
pub fn get_variable(state: &InterpreterState, name: &str) -> String {
    // Special variables
    match name {
        "?" => return state.last_exit_code.to_string(),
        "$" => return std::process::id().to_string(),
        "#" => return state.env.get("#").map(|s| s.as_str()).unwrap_or("0").to_string(),
        "@" => return state.env.get("@").map(|s| s.as_str()).unwrap_or("").to_string(),
        "_" => return state.last_arg.clone(),
        "-" => {
            // $- returns current shell option flags
            let mut flags = String::from("h"); // hashall
            if state.options.errexit {
                flags.push('e');
            }
            if state.options.noglob {
                flags.push('f');
            }
            if state.options.nounset {
                flags.push('u');
            }
            if state.options.verbose {
                flags.push('v');
            }
            if state.options.xtrace {
                flags.push('x');
            }
            flags.push('B'); // braceexpand
            if state.options.noclobber {
                flags.push('C');
            }
            flags.push('s'); // stdin reading
            return flags;
        }
        "*" => {
            // $* uses first character of IFS as separator
            let num_params: i32 = state
                .env
                .get("#")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if num_params == 0 {
                return String::new();
            }
            let params: Vec<String> = (1..=num_params)
                .map(|i| state.env.get(&i.to_string()).cloned().unwrap_or_default())
                .collect();
            return params.join(get_ifs_separator(&state.env));
        }
        "0" => return state.env.get("0").map(|s| s.as_str()).unwrap_or("bash").to_string(),
        "PWD" => return state.env.get("PWD").cloned().unwrap_or_default(),
        "OLDPWD" => return state.env.get("OLDPWD").cloned().unwrap_or_default(),
        "PPID" => return std::os::unix::process::parent_id().to_string(),
        "UID" => {
            #[cfg(unix)]
            {
                return unsafe { libc::getuid() }.to_string();
            }
            #[cfg(not(unix))]
            {
                return "0".to_string();
            }
        }
        "EUID" => {
            #[cfg(unix)]
            {
                return unsafe { libc::geteuid() }.to_string();
            }
            #[cfg(not(unix))]
            {
                return "0".to_string();
            }
        }
        "RANDOM" => return (rand::random::<u16>() % 32768).to_string(),
        "SECONDS" => {
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
                .saturating_sub(state.start_time);
            return elapsed.to_string();
        }
        "BASH_VERSION" => return "5.0.0".to_string(),
        "!" => return state.last_background_pid.to_string(),
        "BASHPID" => return state.bash_pid.to_string(),
        "LINENO" => return state.current_line.to_string(),
        "FUNCNAME" => {
            if let Some(ref stack) = state.func_name_stack {
                if let Some(name) = stack.first() {
                    return name.clone();
                }
            }
            return String::new();
        }
        "BASH_LINENO" => {
            if let Some(ref stack) = state.call_line_stack {
                if let Some(&line) = stack.first() {
                    return line.to_string();
                }
            }
            return String::new();
        }
        "BASH_SOURCE" => {
            if let Some(ref stack) = state.source_stack {
                if let Some(source) = stack.first() {
                    return source.clone();
                }
            }
            return String::new();
        }
        _ => {}
    }

    // Check for array subscript: varName[subscript]
    let bracket_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();
    if let Some(caps) = bracket_re.captures(name) {
        let mut array_name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let subscript = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Check if arrayName is a nameref - if so, resolve it
        if is_nameref(state, &array_name) {
            if let Some(resolved) = resolve_nameref(state, &state.env, &array_name, None) {
                if resolved != array_name {
                    // Check if resolved target itself has array subscript
                    if bracket_re.is_match(&resolved) {
                        // Nameref points to an array element - return empty
                        return String::new();
                    }
                    array_name = resolved;
                }
            }
        }

        if subscript == "@" || subscript == "*" {
            // Get all array elements joined with space
            let elements = get_array_elements(state, &array_name);
            if !elements.is_empty() {
                return elements.into_iter().map(|(_, v)| v).collect::<Vec<_>>().join(" ");
            }
            // If no array elements, treat scalar variable as single-element array
            if let Some(scalar_value) = state.env.get(&array_name) {
                return scalar_value.clone();
            }
            return String::new();
        }

        // Handle special call stack arrays with numeric subscript
        if array_name == "FUNCNAME" {
            if let Ok(index) = subscript.parse::<usize>() {
                return state
                    .func_name_stack
                    .as_ref()
                    .and_then(|s| s.get(index))
                    .cloned()
                    .unwrap_or_default();
            }
            return String::new();
        }
        if array_name == "BASH_LINENO" {
            if let Ok(index) = subscript.parse::<usize>() {
                return state
                    .call_line_stack
                    .as_ref()
                    .and_then(|s| s.get(index))
                    .map(|l| l.to_string())
                    .unwrap_or_default();
            }
            return String::new();
        }
        if array_name == "BASH_SOURCE" {
            if let Ok(index) = subscript.parse::<usize>() {
                return state
                    .source_stack
                    .as_ref()
                    .and_then(|s| s.get(index))
                    .cloned()
                    .unwrap_or_default();
            }
            return String::new();
        }

        let is_assoc = state
            .associative_arrays
            .as_ref()
            .map(|aa| aa.contains(&array_name))
            .unwrap_or(false);

        if is_assoc {
            // For associative arrays, use subscript as string key
            let key = expand_simple_vars_in_subscript(state, unquote_key(subscript));
            return state
                .env
                .get(&format!("{}_{}", array_name, key))
                .cloned()
                .unwrap_or_default();
        }

        // Evaluate subscript as numeric index for indexed arrays
        // Note: Full arithmetic evaluation requires async; this is simplified
        let index: i64 = if let Ok(n) = subscript.parse::<i64>() {
            n
        } else {
            // Try to get from environment as simple variable
            state
                .env
                .get(subscript)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };

        // Handle negative indices
        if index < 0 {
            let elements = get_array_elements(state, &array_name);
            if elements.is_empty() {
                return String::new();
            }
            let max_index = elements
                .iter()
                .filter_map(|(idx, _)| match idx {
                    ArrayIndex::Numeric(n) => Some(*n),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let actual_idx = max_index + 1 + index;
            if actual_idx < 0 {
                return String::new();
            }
            return state
                .env
                .get(&format!("{}_{}", array_name, actual_idx))
                .cloned()
                .unwrap_or_default();
        }

        if let Some(value) = state.env.get(&format!("{}_{}", array_name, index)) {
            return value.clone();
        }
        // If array element doesn't exist, check if it's a scalar variable accessed as c[0]
        if index == 0 {
            if let Some(scalar_value) = state.env.get(&array_name) {
                return scalar_value.clone();
            }
        }
        return String::new();
    }

    // Positional parameters ($1, $2, etc.)
    let positional_re = Regex::new(r"^[1-9][0-9]*$").unwrap();
    if positional_re.is_match(name) {
        return state.env.get(name).cloned().unwrap_or_default();
    }

    // Check if this is a nameref - resolve and get target's value
    if is_nameref(state, name) {
        if let Some(resolved) = resolve_nameref(state, &state.env, name, None) {
            if resolved != name {
                return get_variable(state, &resolved);
            }
        }
        return state.env.get(name).cloned().unwrap_or_default();
    }

    // Regular variables
    if let Some(value) = state.env.get(name) {
        return value.clone();
    }

    // Check if plain variable name refers to an array
    if is_array(state, name) {
        // Return the first element (index 0)
        if let Some(first_value) = state.env.get(&format!("{}_0", name)) {
            return first_value.clone();
        }
    }

    String::new()
}

/// Check if a variable is set (exists in the environment).
/// Properly handles array subscripts (e.g., arr[0] -> arr_0).
pub fn is_variable_set(state: &InterpreterState, name: &str) -> bool {
    // Special variables that are always set
    if ALWAYS_SET_SPECIAL_VARS.contains(&name) {
        return true;
    }

    // $@ and $* are considered "set" only if there are positional parameters
    if name == "@" || name == "*" {
        let num_params: i32 = state
            .env
            .get("#")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        return num_params > 0;
    }

    // PWD and OLDPWD are special - they are set unless explicitly unset
    if name == "PWD" || name == "OLDPWD" {
        return state.env.contains_key(name);
    }

    // Check for array subscript: varName[subscript]
    let bracket_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[(.+)\]$").unwrap();
    if let Some(caps) = bracket_re.captures(name) {
        let mut array_name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let subscript = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Check if arrayName is a nameref - if so, resolve it
        if is_nameref(state, &array_name) {
            if let Some(resolved) = resolve_nameref(state, &state.env, &array_name, None) {
                if resolved != array_name {
                    if bracket_re.is_match(&resolved) {
                        // Nameref points to an array element - treat as unset
                        return false;
                    }
                    array_name = resolved;
                }
            }
        }

        // For @ or *, check if array has any elements
        if subscript == "@" || subscript == "*" {
            let elements = get_array_elements(state, &array_name);
            if !elements.is_empty() {
                return true;
            }
            // Also check if scalar variable exists
            return state.env.contains_key(&array_name);
        }

        let is_assoc = state
            .associative_arrays
            .as_ref()
            .map(|aa| aa.contains(&array_name))
            .unwrap_or(false);

        if is_assoc {
            // For associative arrays, use subscript as string key
            let key = unquote_key(subscript);
            return state.env.contains_key(&format!("{}_{}", array_name, key));
        }

        // Evaluate subscript as numeric index
        let index: i64 = if let Ok(n) = subscript.parse::<i64>() {
            n
        } else {
            state
                .env
                .get(subscript)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };

        // Handle negative indices
        if index < 0 {
            let elements = get_array_elements(state, &array_name);
            if elements.is_empty() {
                return false;
            }
            let max_index = elements
                .iter()
                .filter_map(|(idx, _)| match idx {
                    ArrayIndex::Numeric(n) => Some(*n),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let actual_idx = max_index + 1 + index;
            if actual_idx < 0 {
                return false;
            }
            return state
                .env
                .contains_key(&format!("{}_{}", array_name, actual_idx));
        }

        return state.env.contains_key(&format!("{}_{}", array_name, index));
    }

    // Check if this is a nameref - resolve and check target
    if is_nameref(state, name) {
        if let Some(resolved) = resolve_nameref(state, &state.env, name, None) {
            if resolved != name {
                // Recursively check the target
                return is_variable_set(state, &resolved);
            }
        }
        return state.env.contains_key(name);
    }

    // Regular variable - check if scalar value exists
    if state.env.contains_key(name) {
        return true;
    }

    // Check if plain variable name refers to an array
    if is_array(state, name) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        InterpreterState {
            env: HashMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_expand_simple_vars() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());
        state.env.insert("x".to_string(), "123".to_string());

        assert_eq!(expand_simple_vars_in_subscript(&state, "$foo"), "bar");
        assert_eq!(expand_simple_vars_in_subscript(&state, "${foo}"), "bar");
        assert_eq!(expand_simple_vars_in_subscript(&state, "$x+1"), "123+1");
    }

    #[test]
    fn test_is_array_empty() {
        let state = make_state();
        assert!(!is_array(&state, "arr"));
    }

    #[test]
    fn test_is_array_with_elements() {
        let mut state = make_state();
        state.env.insert("arr_0".to_string(), "first".to_string());
        state.env.insert("arr_1".to_string(), "second".to_string());
        assert!(is_array(&state, "arr"));
    }

    #[test]
    fn test_get_array_elements() {
        let mut state = make_state();
        state.env.insert("arr_0".to_string(), "a".to_string());
        state.env.insert("arr_1".to_string(), "b".to_string());
        state.env.insert("arr_2".to_string(), "c".to_string());

        let elements = get_array_elements(&state, "arr");
        assert_eq!(elements.len(), 3);
    }

    #[test]
    fn test_get_variable_simple() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());
        state.env.insert("x".to_string(), "123".to_string());

        assert_eq!(get_variable(&state, "foo"), "bar");
        assert_eq!(get_variable(&state, "x"), "123");
        assert_eq!(get_variable(&state, "nonexistent"), "");
    }

    #[test]
    fn test_get_variable_special() {
        let state = make_state();
        // $? should be last exit code
        assert_eq!(get_variable(&state, "?"), "0");
        // $0 should be "bash" by default
        assert_eq!(get_variable(&state, "0"), "bash");
        // $- should contain shell flags
        let flags = get_variable(&state, "-");
        assert!(flags.contains('h')); // hashall
        assert!(flags.contains('B')); // braceexpand
    }

    #[test]
    fn test_get_variable_array() {
        let mut state = make_state();
        state.env.insert("arr_0".to_string(), "first".to_string());
        state.env.insert("arr_1".to_string(), "second".to_string());
        state.env.insert("arr_2".to_string(), "third".to_string());

        assert_eq!(get_variable(&state, "arr[0]"), "first");
        assert_eq!(get_variable(&state, "arr[1]"), "second");
        assert_eq!(get_variable(&state, "arr[@]"), "first second third");
    }

    #[test]
    fn test_is_variable_set() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());

        assert!(is_variable_set(&state, "foo"));
        assert!(!is_variable_set(&state, "nonexistent"));
        // Special variables are always set
        assert!(is_variable_set(&state, "?"));
        assert!(is_variable_set(&state, "$"));
        assert!(is_variable_set(&state, "LINENO"));
    }
}
