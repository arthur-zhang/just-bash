//! Declare Print Mode Functions
//!
//! Handles printing and listing variables for the declare/typeset builtin.

use crate::interpreter::builtins::BuiltinResult;
use crate::interpreter::helpers::{
    get_array_indices, get_assoc_array_keys, is_nameref, quote_array_value, quote_declare_value,
    quote_value,
};
use crate::interpreter::types::InterpreterState;

/// Get the attribute flags string for a variable (e.g., "-r", "-x", "-rx", "--")
/// Order follows bash convention: a/A (array), i (integer), l (lowercase), n (nameref), r (readonly), u (uppercase), x (export)
fn get_variable_flags(state: &InterpreterState, name: &str) -> String {
    let mut flags = String::new();

    // Note: array flags (-a/-A) are handled separately in the caller
    // since they require different output format

    // Integer attribute
    if state
        .integer_vars
        .as_ref()
        .map_or(false, |v| v.contains(name))
    {
        flags.push('i');
    }

    // Lowercase attribute
    if state
        .lowercase_vars
        .as_ref()
        .map_or(false, |v| v.contains(name))
    {
        flags.push('l');
    }

    // Nameref attribute
    if is_nameref(state, name) {
        flags.push('n');
    }

    // Readonly attribute
    if state
        .readonly_vars
        .as_ref()
        .map_or(false, |v| v.contains(name))
    {
        flags.push('r');
    }

    // Uppercase attribute
    if state
        .uppercase_vars
        .as_ref()
        .map_or(false, |v| v.contains(name))
    {
        flags.push('u');
    }

    // Export attribute
    if state
        .exported_vars
        .as_ref()
        .map_or(false, |v| v.contains(name))
    {
        flags.push('x');
    }

    if flags.is_empty() {
        "--".to_string()
    } else {
        format!("-{}", flags)
    }
}

/// Format a value for associative array output in declare -p.
/// Uses the oils/ysh-compatible format:
/// - Simple values (no spaces, no special chars): unquoted
/// - Empty strings or values with spaces/special chars: single-quoted with escaping
fn format_assoc_value(value: &str) -> String {
    // Empty string needs quotes
    if value.is_empty() {
        return "''".to_string();
    }
    // If value contains spaces, single quotes, or other special chars, quote it
    if value.chars().any(|c| c.is_whitespace() || c == '\'' || c == '\\') {
        // Escape single quotes as '\'' (end quote, escaped quote, start quote)
        let escaped = value.replace('\'', "'\\''");
        return format!("'{}'", escaped);
    }
    // Simple value - no quotes needed
    value.to_string()
}

/// Print specific variables with their declarations.
/// Handles: declare -p varname1 varname2 ...
pub fn print_specific_variables(state: &InterpreterState, names: &[String]) -> BuiltinResult {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut any_not_found = false;

    for name in names {
        // Get the variable's attribute flags (for scalar variables)
        let flags = get_variable_flags(state, name);

        // Check if this is an associative array
        let is_assoc = state
            .associative_arrays
            .as_ref()
            .map_or(false, |a| a.contains(name));
        if is_assoc {
            let keys = get_assoc_array_keys(&state.env, name);
            if keys.is_empty() {
                stdout.push_str(&format!("declare -A {}=()\n", name));
            } else {
                let elements: Vec<String> = keys
                    .iter()
                    .map(|key| {
                        let env_key = format!("{}_{}", name, key);
                        let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                        // Format: ['key']=value (single quotes around key)
                        let formatted_value = format_assoc_value(value);
                        format!("['{}']={}", key, formatted_value)
                    })
                    .collect();
                stdout.push_str(&format!("declare -A {}=({})\n", name, elements.join(" ")));
            }
            continue;
        }

        // Check if this is an indexed array (has array elements)
        let array_indices = get_array_indices(&state.env, name);
        if !array_indices.is_empty() {
            let elements: Vec<String> = array_indices
                .iter()
                .map(|index| {
                    let env_key = format!("{}_{}", name, index);
                    let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                    format!("[{}]={}", index, quote_array_value(value))
                })
                .collect();
            stdout.push_str(&format!("declare -a {}=({})\n", name, elements.join(" ")));
            continue;
        }

        // Check if this is an empty array (has __length marker but no elements)
        let length_key = format!("{}__length", name);
        if state.env.contains_key(&length_key) {
            stdout.push_str(&format!("declare -a {}=()\n", name));
            continue;
        }

        // Regular scalar variable
        if let Some(value) = state.env.get(name) {
            // Use $'...' quoting for control characters, double quotes otherwise
            stdout.push_str(&format!(
                "declare {} {}={}\n",
                flags,
                name,
                quote_declare_value(value)
            ));
        } else {
            // Check if variable is declared but unset (via declare or local)
            let is_declared = state
                .declared_vars
                .as_ref()
                .map_or(false, |v| v.contains(name));
            let is_local_var = state
                .local_var_depth
                .as_ref()
                .map_or(false, |v| v.contains_key(name));
            if is_declared || is_local_var {
                // Variable is declared but has no value - output without =""
                stdout.push_str(&format!("declare {} {}\n", flags, name));
            } else {
                // Variable not found - add error to stderr and set flag for exit code 1
                stderr.push_str(&format!("bash: declare: {}: not found\n", name));
                any_not_found = true;
            }
        }
    }

    BuiltinResult {
        stdout,
        stderr,
        exit_code: if any_not_found { 1 } else { 0 },
    }
}

/// Filters for printing all variables
pub struct PrintAllFilters {
    pub filter_export: bool,
    pub filter_readonly: bool,
    pub filter_nameref: bool,
    pub filter_indexed_array: bool,
    pub filter_assoc_array: bool,
}

/// Print all variables with their declarations and attributes.
/// Handles: declare -p (with optional filters like -x, -r, -n, -a, -A)
pub fn print_all_variables(state: &InterpreterState, filters: &PrintAllFilters) -> BuiltinResult {
    let has_filter = filters.filter_export
        || filters.filter_readonly
        || filters.filter_nameref
        || filters.filter_indexed_array
        || filters.filter_assoc_array;

    let mut stdout = String::new();

    // Collect all variable names (excluding internal markers like __length)
    let mut var_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for key in state.env.keys() {
        if key.starts_with("BASH_") {
            continue;
        }
        // For __length markers, extract the base name (for empty arrays)
        if key.ends_with("__length") {
            let base_name = &key[..key.len() - 8];
            var_names.insert(base_name.to_string());
            continue;
        }
        // For array elements (name_index), extract base name
        if let Some(underscore_idx) = key.rfind('_') {
            if underscore_idx > 0 {
                let base_name = &key[..underscore_idx];
                let suffix = &key[underscore_idx + 1..];
                // If suffix is numeric or baseName is an array, it's an array element
                if suffix.parse::<i64>().is_ok()
                    || state
                        .associative_arrays
                        .as_ref()
                        .map_or(false, |a| a.contains(base_name))
                {
                    var_names.insert(base_name.to_string());
                    continue;
                }
            }
        }
        var_names.insert(key.clone());
    }

    // Also include local variables if we're in a function scope
    if let Some(ref local_var_depth) = state.local_var_depth {
        for name in local_var_depth.keys() {
            var_names.insert(name.clone());
        }
    }

    // Include associative array names (for empty associative arrays)
    if let Some(ref assoc_arrays) = state.associative_arrays {
        for name in assoc_arrays {
            var_names.insert(name.clone());
        }
    }

    // Sort and output each variable
    let mut sorted_names: Vec<String> = var_names.into_iter().collect();
    sorted_names.sort();

    for name in sorted_names {
        let flags = get_variable_flags(state, &name);

        // Check if this is an associative array
        let is_assoc = state
            .associative_arrays
            .as_ref()
            .map_or(false, |a| a.contains(&name));

        // Check if this is an indexed array (not associative)
        let array_indices = get_array_indices(&state.env, &name);
        let length_key = format!("{}__length", name);
        let is_indexed_array =
            !is_assoc && (!array_indices.is_empty() || state.env.contains_key(&length_key));

        // Apply filters if set
        if has_filter {
            // If filtering for associative arrays only (-pA)
            if filters.filter_assoc_array && !is_assoc {
                continue;
            }
            // If filtering for indexed arrays only (-pa)
            if filters.filter_indexed_array && !is_indexed_array {
                continue;
            }
            // If filtering for exported only (-px)
            if filters.filter_export
                && !state
                    .exported_vars
                    .as_ref()
                    .map_or(false, |v| v.contains(&name))
            {
                continue;
            }
            // If filtering for readonly only (-pr)
            if filters.filter_readonly
                && !state
                    .readonly_vars
                    .as_ref()
                    .map_or(false, |v| v.contains(&name))
            {
                continue;
            }
            // If filtering for nameref only (-pn)
            if filters.filter_nameref && !is_nameref(state, &name) {
                continue;
            }
        }

        if is_assoc {
            let keys = get_assoc_array_keys(&state.env, &name);
            if keys.is_empty() {
                stdout.push_str(&format!("declare -A {}=()\n", name));
            } else {
                let elements: Vec<String> = keys
                    .iter()
                    .map(|key| {
                        let env_key = format!("{}_{}", name, key);
                        let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                        // Format: ['key']=value (single quotes around key)
                        let formatted_value = format_assoc_value(value);
                        format!("['{}']={}", key, formatted_value)
                    })
                    .collect();
                stdout.push_str(&format!("declare -A {}=({})\n", name, elements.join(" ")));
            }
            continue;
        }

        // Check if this is an indexed array
        if !array_indices.is_empty() {
            let elements: Vec<String> = array_indices
                .iter()
                .map(|index| {
                    let env_key = format!("{}_{}", name, index);
                    let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                    format!("[{}]={}", index, quote_array_value(value))
                })
                .collect();
            stdout.push_str(&format!("declare -a {}=({})\n", name, elements.join(" ")));
            continue;
        }

        // Check if this is an empty array
        if state.env.contains_key(&length_key) {
            stdout.push_str(&format!("declare -a {}=()\n", name));
            continue;
        }

        // Regular scalar variable
        if let Some(value) = state.env.get(&name) {
            stdout.push_str(&format!(
                "declare {} {}={}\n",
                flags,
                name,
                quote_declare_value(value)
            ));
        }
    }

    BuiltinResult {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

/// List all associative arrays.
/// Handles: declare -A (without arguments)
pub fn list_associative_arrays(state: &InterpreterState) -> BuiltinResult {
    let mut stdout = String::new();

    // Get all associative array names and sort them
    let mut assoc_names: Vec<String> = state
        .associative_arrays
        .as_ref()
        .map_or(Vec::new(), |a| a.iter().cloned().collect());
    assoc_names.sort();

    for name in assoc_names {
        let keys = get_assoc_array_keys(&state.env, &name);
        if keys.is_empty() {
            // Empty associative array
            stdout.push_str(&format!("declare -A {}=()\n", name));
        } else {
            // Non-empty associative array: format as (['key']=value ...)
            let elements: Vec<String> = keys
                .iter()
                .map(|key| {
                    let env_key = format!("{}_{}", name, key);
                    let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                    // Format: ['key']=value (single quotes around key)
                    let formatted_value = format_assoc_value(value);
                    format!("['{}']={}", key, formatted_value)
                })
                .collect();
            stdout.push_str(&format!("declare -A {}=({})\n", name, elements.join(" ")));
        }
    }

    BuiltinResult {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

/// List all indexed arrays.
/// Handles: declare -a (without arguments)
pub fn list_indexed_arrays(state: &InterpreterState) -> BuiltinResult {
    let mut stdout = String::new();

    // Find all indexed arrays
    let mut array_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for key in state.env.keys() {
        if key.starts_with("BASH_") {
            continue;
        }
        // Check for __length marker (empty arrays)
        if key.ends_with("__length") {
            let base_name = &key[..key.len() - 8];
            // Make sure it's not an associative array
            if !state
                .associative_arrays
                .as_ref()
                .map_or(false, |a| a.contains(base_name))
            {
                array_names.insert(base_name.to_string());
            }
            continue;
        }
        // Check for numeric index pattern (name_index)
        if let Some(last_underscore) = key.rfind('_') {
            if last_underscore > 0 {
                let base_name = &key[..last_underscore];
                let suffix = &key[last_underscore + 1..];
                // If suffix is numeric, it's an array element
                if suffix.parse::<i64>().is_ok() {
                    // Make sure it's not an associative array
                    if !state
                        .associative_arrays
                        .as_ref()
                        .map_or(false, |a| a.contains(base_name))
                    {
                        array_names.insert(base_name.to_string());
                    }
                }
            }
        }
    }

    // Output each array in sorted order
    let mut sorted_names: Vec<String> = array_names.into_iter().collect();
    sorted_names.sort();

    for name in sorted_names {
        let indices = get_array_indices(&state.env, &name);
        if indices.is_empty() {
            // Empty array
            stdout.push_str(&format!("declare -a {}=()\n", name));
        } else {
            // Non-empty array: format as ([index]="value" ...)
            let elements: Vec<String> = indices
                .iter()
                .map(|index| {
                    let env_key = format!("{}_{}", name, index);
                    let value = state.env.get(&env_key).map(|s| s.as_str()).unwrap_or("");
                    format!("[{}]={}", index, quote_array_value(value))
                })
                .collect();
            stdout.push_str(&format!("declare -a {}=({})\n", name, elements.join(" ")));
        }
    }

    BuiltinResult {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

/// List all variables without print mode (no attributes shown).
/// Handles: declare (without -p and without arguments)
pub fn list_all_variables(state: &InterpreterState) -> BuiltinResult {
    let mut stdout = String::new();

    // Collect all variable names (excluding internal markers)
    let mut var_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for key in state.env.keys() {
        if key.starts_with("BASH_") {
            continue;
        }
        // For __length markers, extract the base name (for arrays)
        if key.ends_with("__length") {
            let base_name = &key[..key.len() - 8];
            var_names.insert(base_name.to_string());
            continue;
        }
        // For array elements (name_index), extract base name
        if let Some(underscore_idx) = key.rfind('_') {
            if underscore_idx > 0 {
                let base_name = &key[..underscore_idx];
                let suffix = &key[underscore_idx + 1..];
                // If suffix is numeric or baseName is an associative array
                if suffix.parse::<i64>().is_ok()
                    || state
                        .associative_arrays
                        .as_ref()
                        .map_or(false, |a| a.contains(base_name))
                {
                    var_names.insert(base_name.to_string());
                    continue;
                }
            }
        }
        var_names.insert(key.clone());
    }

    let mut sorted_names: Vec<String> = var_names.into_iter().collect();
    sorted_names.sort();

    for name in sorted_names {
        // Check if this is an associative array
        let is_assoc = state
            .associative_arrays
            .as_ref()
            .map_or(false, |a| a.contains(&name));
        if is_assoc {
            // Skip associative arrays for simple declare output
            continue;
        }

        // Check if this is an indexed array
        let array_indices = get_array_indices(&state.env, &name);
        let length_key = format!("{}__length", name);
        if !array_indices.is_empty() || state.env.contains_key(&length_key) {
            // Skip indexed arrays for simple declare output
            continue;
        }

        // Regular scalar variable - output as name=value
        if let Some(value) = state.env.get(&name) {
            stdout.push_str(&format!("{}={}\n", name, quote_value(value)));
        }
    }

    BuiltinResult {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_get_variable_flags_none() {
        let state = make_state();
        assert_eq!(get_variable_flags(&state, "foo"), "--");
    }

    #[test]
    fn test_get_variable_flags_readonly() {
        let mut state = make_state();
        state.readonly_vars = Some(std::collections::HashSet::new());
        state.readonly_vars.as_mut().unwrap().insert("foo".to_string());
        assert_eq!(get_variable_flags(&state, "foo"), "-r");
    }

    #[test]
    fn test_get_variable_flags_multiple() {
        let mut state = make_state();
        state.readonly_vars = Some(std::collections::HashSet::new());
        state.readonly_vars.as_mut().unwrap().insert("foo".to_string());
        state.exported_vars = Some(std::collections::HashSet::new());
        state.exported_vars.as_mut().unwrap().insert("foo".to_string());
        assert_eq!(get_variable_flags(&state, "foo"), "-rx");
    }

    #[test]
    fn test_format_assoc_value_empty() {
        assert_eq!(format_assoc_value(""), "''");
    }

    #[test]
    fn test_format_assoc_value_simple() {
        assert_eq!(format_assoc_value("hello"), "hello");
    }

    #[test]
    fn test_format_assoc_value_with_space() {
        assert_eq!(format_assoc_value("hello world"), "'hello world'");
    }

    #[test]
    fn test_format_assoc_value_with_quote() {
        assert_eq!(format_assoc_value("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_print_specific_variables_not_found() {
        let state = make_state();
        let result = print_specific_variables(&state, &["nonexistent".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not found"));
    }

    #[test]
    fn test_print_specific_variables_scalar() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());
        let result = print_specific_variables(&state, &["foo".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("declare -- foo=\"bar\""));
    }

    #[test]
    fn test_list_all_variables() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());
        state.env.insert("baz".to_string(), "qux".to_string());
        let result = list_all_variables(&state);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("foo=bar"));
        assert!(result.stdout.contains("baz=qux"));
    }
}
