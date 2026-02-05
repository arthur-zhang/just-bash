//! declare/typeset - Declare variables and give them attributes
//!
//! Usage:
//!   declare              - List all variables
//!   declare -p           - List all variables (same as no args)
//!   declare NAME=value   - Declare variable with value
//!   declare -a NAME      - Declare indexed array
//!   declare -A NAME      - Declare associative array
//!   declare -r NAME      - Declare readonly variable
//!   declare -x NAME      - Export variable
//!   declare -g NAME      - Declare global variable (inside functions)
//!
//! Also aliased as 'typeset'

use std::collections::HashSet;
use regex_lite::Regex;

use crate::interpreter::builtins::{
    parse_array_elements, parse_assoc_array_literal, list_all_variables, list_associative_arrays,
    list_indexed_arrays, print_all_variables, print_specific_variables, PrintAllFilters, BuiltinResult,
};
use crate::interpreter::helpers::{
    clear_array, expand_tildes_in_value, get_array_indices, is_nameref, is_readonly,
    mark_exported, mark_nameref, mark_nameref_bound, mark_nameref_invalid, mark_readonly,
    resolve_nameref, target_exists, unmark_exported, unmark_nameref,
};
use crate::interpreter::types::InterpreterState;

// ============================================================================
// Variable Attribute Helpers
// ============================================================================

/// Mark a variable as having the integer attribute.
pub fn mark_integer(state: &mut InterpreterState, name: &str) {
    if state.integer_vars.is_none() {
        state.integer_vars = Some(HashSet::new());
    }
    state.integer_vars.as_mut().unwrap().insert(name.to_string());
}

/// Check if a variable has the integer attribute.
pub fn is_integer(state: &InterpreterState, name: &str) -> bool {
    state.integer_vars.as_ref().map_or(false, |v| v.contains(name))
}

/// Mark a variable as having the lowercase attribute.
fn mark_lowercase(state: &mut InterpreterState, name: &str) {
    if state.lowercase_vars.is_none() {
        state.lowercase_vars = Some(HashSet::new());
    }
    state.lowercase_vars.as_mut().unwrap().insert(name.to_string());
    // -l and -u are mutually exclusive; -l clears -u
    if let Some(ref mut upper) = state.uppercase_vars {
        upper.remove(name);
    }
}

/// Check if a variable has the lowercase attribute.
fn is_lowercase(state: &InterpreterState, name: &str) -> bool {
    state.lowercase_vars.as_ref().map_or(false, |v| v.contains(name))
}

/// Mark a variable as having the uppercase attribute.
fn mark_uppercase(state: &mut InterpreterState, name: &str) {
    if state.uppercase_vars.is_none() {
        state.uppercase_vars = Some(HashSet::new());
    }
    state.uppercase_vars.as_mut().unwrap().insert(name.to_string());
    // -l and -u are mutually exclusive; -u clears -l
    if let Some(ref mut lower) = state.lowercase_vars {
        lower.remove(name);
    }
}

/// Check if a variable has the uppercase attribute.
fn is_uppercase(state: &InterpreterState, name: &str) -> bool {
    state.uppercase_vars.as_ref().map_or(false, |v| v.contains(name))
}

/// Apply case transformation based on variable attributes.
/// Returns the transformed value.
pub fn apply_case_transform(state: &InterpreterState, name: &str, value: &str) -> String {
    if is_lowercase(state, name) {
        return value.to_lowercase();
    }
    if is_uppercase(state, name) {
        return value.to_uppercase();
    }
    value.to_string()
}

/// Evaluate a value as arithmetic if the variable has integer attribute.
/// Returns the evaluated string value.
fn evaluate_integer_value(value: &str) -> String {
    // Simple integer evaluation - parse as number, return "0" on failure
    // Full arithmetic evaluation would require the arithmetic parser
    match value.parse::<i64>() {
        Ok(n) => n.to_string(),
        Err(_) => "0".to_string(),
    }
}

/// Parse array assignment syntax: name[index]=value
/// Handles nested brackets like a[a[0]=1]=X
/// Returns None if not an array assignment pattern
fn parse_array_assignment(arg: &str) -> Option<(String, String, String)> {
    // Check for variable name at start
    let name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*").unwrap();
    let name_match = name_re.find(arg)?;
    let name = name_match.as_str().to_string();
    let mut pos = name.len();

    let chars: Vec<char> = arg.chars().collect();

    // Must have [ after name
    if pos >= chars.len() || chars[pos] != '[' {
        return None;
    }

    // Find matching ] using bracket depth tracking
    let mut depth = 0;
    let subscript_start = pos + 1;
    while pos < chars.len() {
        if chars[pos] == '[' {
            depth += 1;
        } else if chars[pos] == ']' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        pos += 1;
    }

    // If depth is not 0, brackets are unbalanced
    if depth != 0 {
        return None;
    }

    let index_expr: String = chars[subscript_start..pos].iter().collect();
    pos += 1; // skip closing ]

    // Must have = after ]
    if pos >= chars.len() || chars[pos] != '=' {
        return None;
    }
    pos += 1; // skip =

    let value: String = chars[pos..].iter().collect();

    Some((name, index_expr, value))
}

// ============================================================================
// Local Scope Helpers
// ============================================================================

/// Mark a variable as being declared at the current call depth.
/// Used for bash-specific unset scoping behavior.
pub fn mark_local_var_depth(state: &mut InterpreterState, name: &str) {
    if state.local_var_depth.is_none() {
        state.local_var_depth = Some(std::collections::HashMap::new());
    }
    state.local_var_depth.as_mut().unwrap().insert(name.to_string(), state.call_depth);
}

// ============================================================================
// Main declare/typeset handler
// ============================================================================

/// Handle the declare/typeset builtin command.
pub fn handle_declare(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse flags
    let mut declare_array = false;
    let mut declare_assoc = false;
    let mut declare_readonly = false;
    let mut declare_export = false;
    let mut print_mode = false;
    let mut declare_nameref = false;
    let mut remove_nameref = false;
    let mut remove_array = false;
    let mut remove_export = false;
    let mut declare_integer = false;
    let mut declare_lowercase = false;
    let mut declare_uppercase = false;
    let mut function_mode = false;
    let mut function_names_only = false;
    let mut declare_global = false;
    let mut processed_args: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-a" {
            declare_array = true;
        } else if arg == "-A" {
            declare_assoc = true;
        } else if arg == "-r" {
            declare_readonly = true;
        } else if arg == "-x" {
            declare_export = true;
        } else if arg == "-p" {
            print_mode = true;
        } else if arg == "-n" {
            declare_nameref = true;
        } else if arg == "+n" {
            remove_nameref = true;
        } else if arg == "+a" {
            remove_array = true;
        } else if arg == "+x" {
            remove_export = true;
        } else if arg == "--" {
            // End of options, rest are arguments
            processed_args.extend(args[i + 1..].iter().cloned());
            break;
        } else if arg.starts_with('+') {
            // Handle + flags that remove attributes
            for flag in arg[1..].chars() {
                match flag {
                    'n' => remove_nameref = true,
                    'a' => remove_array = true,
                    'x' => remove_export = true,
                    'r' | 'i' | 'f' | 'F' => {
                        // +r, +i, +f, +F are accepted but have no effect
                    }
                    _ => {
                        return BuiltinResult {
                            stdout: String::new(),
                            stderr: format!("bash: typeset: +{}: invalid option\n", flag),
                            exit_code: 2,
                        };
                    }
                }
            }
        } else if arg == "-i" {
            declare_integer = true;
        } else if arg == "-l" {
            declare_lowercase = true;
        } else if arg == "-u" {
            declare_uppercase = true;
        } else if arg == "-f" {
            function_mode = true;
        } else if arg == "-F" {
            function_names_only = true;
        } else if arg == "-g" {
            declare_global = true;
        } else if arg.starts_with('-') {
            // Handle combined flags like -ar
            for flag in arg[1..].chars() {
                match flag {
                    'a' => declare_array = true,
                    'A' => declare_assoc = true,
                    'r' => declare_readonly = true,
                    'x' => declare_export = true,
                    'p' => print_mode = true,
                    'n' => declare_nameref = true,
                    'i' => declare_integer = true,
                    'l' => declare_lowercase = true,
                    'u' => declare_uppercase = true,
                    'f' => function_mode = true,
                    'F' => function_names_only = true,
                    'g' => declare_global = true,
                    _ => {
                        return BuiltinResult {
                            stdout: String::new(),
                            stderr: format!("bash: typeset: -{}: invalid option\n", flag),
                            exit_code: 2,
                        };
                    }
                }
            }
        } else {
            processed_args.push(arg.clone());
        }
        i += 1;
    }

    // Determine if we should create local variables (inside a function, without -g flag)
    let is_inside_function = !state.local_scopes.is_empty();
    let create_local = is_inside_function && !declare_global;

    // Handle declare -F (function names only)
    if function_names_only {
        if processed_args.is_empty() {
            // List all function names in sorted order
            let mut func_names: Vec<String> = state.functions.keys().cloned().collect();
            func_names.sort();
            let mut stdout = String::new();
            for name in func_names {
                stdout.push_str(&format!("declare -f {}\n", name));
            }
            return BuiltinResult {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            };
        }
        // With args, check if functions exist and output their names
        let mut all_exist = true;
        let mut stdout = String::new();
        for name in &processed_args {
            if state.functions.contains_key(name) {
                stdout.push_str(&format!("{}\n", name));
            } else {
                all_exist = false;
            }
        }
        return BuiltinResult {
            stdout,
            stderr: String::new(),
            exit_code: if all_exist { 0 } else { 1 },
        };
    }

    // Handle declare -f (function definitions)
    if function_mode {
        if processed_args.is_empty() {
            // List all function definitions
            let mut stdout = String::new();
            let mut func_names: Vec<String> = state.functions.keys().cloned().collect();
            func_names.sort();
            for name in func_names {
                stdout.push_str(&format!("{} ()\n{{\n    # function body\n}}\n", name));
            }
            return BuiltinResult {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            };
        }
        // Check if all specified functions exist
        let all_exist = processed_args.iter().all(|name| state.functions.contains_key(name));
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if all_exist { 0 } else { 1 },
        };
    }

    // Print mode with specific variable names: declare -p varname
    if print_mode && !processed_args.is_empty() {
        return print_specific_variables(state, &processed_args);
    }

    // Print mode without args (declare -p): list all variables with attributes
    if print_mode && processed_args.is_empty() {
        return print_all_variables(state, &PrintAllFilters {
            filter_export: declare_export,
            filter_readonly: declare_readonly,
            filter_nameref: declare_nameref,
            filter_indexed_array: declare_array,
            filter_assoc_array: declare_assoc,
        });
    }

    // Handle declare -A without arguments: list all associative arrays
    if processed_args.is_empty() && declare_assoc && !print_mode {
        return list_associative_arrays(state);
    }

    // Handle declare -a without arguments: list all indexed arrays
    if processed_args.is_empty() && declare_array && !print_mode {
        return list_indexed_arrays(state);
    }

    // No args: list all variables (without -p flag, just print name=value)
    if processed_args.is_empty() && !print_mode {
        return list_all_variables(state);
    }

    // Track errors during processing
    let mut stderr = String::new();
    let mut exit_code = 0;

    // Valid variable name regex
    let valid_name_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    let valid_target_re = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\[.+\])?$").unwrap();

    // Process each argument
    for arg in &processed_args {
        // Check for array assignment: name=(...)
        let array_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)=\((.*)\)$").unwrap();
        if let Some(caps) = array_re.captures(arg) {
            if !remove_array {
                let name = caps.get(1).unwrap().as_str();
                let content = caps.get(2).unwrap().as_str();

                // Check for type conversion errors
                if declare_assoc {
                    let existing_indices = get_array_indices(&state.env, name);
                    if !existing_indices.is_empty() {
                        stderr.push_str(&format!("bash: declare: {}: cannot convert indexed to associative array\n", name));
                        exit_code = 1;
                        continue;
                    }
                }
                if declare_array || (!declare_assoc && !declare_array) {
                    if state.associative_arrays.as_ref().map_or(false, |a| a.contains(name)) {
                        stderr.push_str(&format!("bash: declare: {}: cannot convert associative to indexed array\n", name));
                        exit_code = 1;
                        continue;
                    }
                }

                // Save to local scope before modifying
                if create_local {
                    save_array_to_local_scope(state, name);
                }

                // Track associative array declaration
                if declare_assoc {
                    if state.associative_arrays.is_none() {
                        state.associative_arrays = Some(HashSet::new());
                    }
                    state.associative_arrays.as_mut().unwrap().insert(name.to_string());
                }

                // Clear existing array elements
                clear_array(&mut state.env, name);
                state.env.remove(name);
                state.env.remove(&format!("{}__length", name));

                // Parse and set array elements
                if declare_assoc && content.contains('[') {
                    let entries = parse_assoc_array_literal(content);
                    for (key, raw_value) in entries {
                        let value = expand_tildes_in_value(&state.env, &raw_value);
                        state.env.insert(format!("{}_{}", name, key), value);
                    }
                } else if declare_assoc {
                    // Bare values as alternating key-value pairs
                    let elements = parse_array_elements(content);
                    let mut idx = 0;
                    while idx < elements.len() {
                        let key = &elements[idx];
                        let value = if idx + 1 < elements.len() {
                            expand_tildes_in_value(&state.env, &elements[idx + 1])
                        } else {
                            String::new()
                        };
                        state.env.insert(format!("{}_{}", name, key), value);
                        idx += 2;
                    }
                } else {
                    // Indexed array
                    let elements = parse_array_elements(content);
                    let has_keyed = elements.iter().any(|el| {
                        let keyed_re = Regex::new(r"^\[[^\]]+\]=").unwrap();
                        keyed_re.is_match(el)
                    });
                    if has_keyed {
                        let mut current_index: i64 = 0;
                        let keyed_re = Regex::new(r"^\[([^\]]+)\]=(.*)$").unwrap();
                        for element in &elements {
                            if let Some(caps) = keyed_re.captures(element) {
                                let index_expr = caps.get(1).unwrap().as_str();
                                let raw_value = caps.get(2).unwrap().as_str();
                                let value = expand_tildes_in_value(&state.env, raw_value);
                                let index: i64 = index_expr.parse().unwrap_or(0);
                                state.env.insert(format!("{}_{}", name, index), value);
                                current_index = index + 1;
                            } else {
                                let value = expand_tildes_in_value(&state.env, element);
                                state.env.insert(format!("{}_{}", name, current_index), value);
                                current_index += 1;
                            }
                        }
                    } else {
                        for (idx, element) in elements.iter().enumerate() {
                            state.env.insert(format!("{}_{}", name, idx), element.clone());
                        }
                        state.env.insert(format!("{}__length", name), elements.len().to_string());
                    }
                }

                // Mark as local if inside a function
                if create_local {
                    mark_local_var_depth(state, name);
                }

                if declare_readonly {
                    mark_readonly(state, name);
                }
                if declare_export {
                    mark_exported(state, name);
                }
                continue;
            }
        }

        // Handle nameref removal (+n)
        if remove_nameref {
            let name = if arg.contains('=') {
                &arg[..arg.find('=').unwrap()]
            } else {
                arg.as_str()
            };
            unmark_nameref(state, name);
            if !arg.contains('=') {
                continue;
            }
        }

        // Handle export removal (+x)
        if remove_export {
            let name = if arg.contains('=') {
                &arg[..arg.find('=').unwrap()]
            } else {
                arg.as_str()
            };
            unmark_exported(state, name);
            if !arg.contains('=') {
                continue;
            }
        }

        // Check for array index assignment: name[index]=value
        if let Some((name, index_expr, value)) = parse_array_assignment(arg) {
            // Check if variable is readonly
            if is_readonly(state, &name) {
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: format!("bash: {}: readonly variable\n", name),
                    exit_code: 1,
                };
            }

            // Save to local scope before modifying
            if create_local {
                save_array_to_local_scope(state, &name);
            }

            // Evaluate the index
            let index: i64 = index_expr.parse().unwrap_or(0);

            // Set the array element
            state.env.insert(format!("{}_{}", name, index), value);

            // Update array length if needed
            let current_length: i64 = state.env
                .get(&format!("{}__length", name))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if index >= current_length {
                state.env.insert(format!("{}__length", name), (index + 1).to_string());
            }

            // Mark as local if inside a function
            if create_local {
                mark_local_var_depth(state, &name);
            }

            if declare_readonly {
                mark_readonly(state, &name);
            }
            if declare_export {
                mark_exported(state, &name);
            }
            continue;
        }

        // Check for += append syntax
        let append_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\+=(.*)$").unwrap();
        if let Some(caps) = append_re.captures(arg) {
            let name = caps.get(1).unwrap().as_str();
            let mut append_value = expand_tildes_in_value(&state.env, caps.get(2).unwrap().as_str());

            // Check if variable is readonly
            if is_readonly(state, name) {
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: format!("bash: {}: readonly variable\n", name),
                    exit_code: 1,
                };
            }

            // Save to local scope before modifying
            if create_local {
                save_to_local_scope(state, name);
            }

            // Mark attributes
            if declare_integer {
                mark_integer(state, name);
            }
            if declare_lowercase {
                mark_lowercase(state, name);
            }
            if declare_uppercase {
                mark_uppercase(state, name);
            }

            // Check if this is an array
            let existing_indices = get_array_indices(&state.env, name);
            let is_array = !existing_indices.is_empty()
                || state.associative_arrays.as_ref().map_or(false, |a| a.contains(name));

            if is_integer(state, name) {
                let existing = state.env.get(name).map(|s| s.as_str()).unwrap_or("0");
                let existing_num: i64 = existing.parse().unwrap_or(0);
                let append_num: i64 = evaluate_integer_value(&append_value).parse().unwrap_or(0);
                append_value = (existing_num + append_num).to_string();
                state.env.insert(name.to_string(), append_value);
            } else if is_array {
                // For arrays, append to element 0
                append_value = apply_case_transform(state, name, &append_value);
                let element0_key = format!("{}_0", name);
                let existing = state.env.get(&element0_key).map(|s| s.as_str()).unwrap_or("");
                state.env.insert(element0_key, format!("{}{}", existing, append_value));
            } else {
                // Apply case transformation
                append_value = apply_case_transform(state, name, &append_value);
                let existing = state.env.get(name).map(|s| s.as_str()).unwrap_or("");
                state.env.insert(name.to_string(), format!("{}{}", existing, append_value));
            }

            // Mark as local if inside a function
            if create_local {
                mark_local_var_depth(state, name);
            }

            if declare_readonly {
                mark_readonly(state, name);
            }
            if declare_export {
                mark_exported(state, name);
            }
            if state.options.allexport && !remove_export {
                mark_exported(state, name);
            }
            continue;
        }

        // Check for scalar assignment: name=value
        if arg.contains('=') {
            let eq_idx = arg.find('=').unwrap();
            let name = &arg[..eq_idx];
            let mut value = arg[eq_idx + 1..].to_string();

            // Validate variable name
            if !valid_name_re.is_match(name) {
                stderr.push_str(&format!("bash: typeset: `{}': not a valid identifier\n", name));
                exit_code = 1;
                continue;
            }

            // Check if variable is readonly
            if is_readonly(state, name) {
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: format!("bash: {}: readonly variable\n", name),
                    exit_code: 1,
                };
            }

            // Save to local scope before modifying
            if create_local {
                save_to_local_scope(state, name);
            }

            // For namerefs being declared with a value
            if declare_nameref {
                if !value.is_empty() && !valid_target_re.is_match(&value) {
                    stderr.push_str(&format!("bash: declare: `{}': invalid variable name for name reference\n", value));
                    exit_code = 1;
                    continue;
                }
                state.env.insert(name.to_string(), value.clone());
                mark_nameref(state, name);
                if !value.is_empty() && target_exists(state, &state.env.clone(), &value) {
                    mark_nameref_bound(state, name);
                }
                if create_local {
                    mark_local_var_depth(state, name);
                }
                if declare_readonly {
                    mark_readonly(state, name);
                }
                if declare_export {
                    mark_exported(state, name);
                }
                continue;
            }

            // Mark attributes
            if declare_integer {
                mark_integer(state, name);
            }
            if declare_lowercase {
                mark_lowercase(state, name);
            }
            if declare_uppercase {
                mark_uppercase(state, name);
            }

            // If variable has integer attribute, evaluate as arithmetic
            if is_integer(state, name) {
                value = evaluate_integer_value(&value);
            }

            // Apply case transformation
            value = apply_case_transform(state, name, &value);

            // If this is an existing nameref, write through it
            if is_nameref(state, name) {
                if let Some(resolved) = resolve_nameref(state, &state.env.clone(), name, None) {
                    if resolved != name {
                        state.env.insert(resolved, value);
                    } else {
                        state.env.insert(name.to_string(), value);
                    }
                } else {
                    state.env.insert(name.to_string(), value);
                }
            } else {
                state.env.insert(name.to_string(), value);
            }

            // Mark as local if inside a function
            if create_local {
                mark_local_var_depth(state, name);
            }

            if declare_readonly {
                mark_readonly(state, name);
            }
            if declare_export {
                mark_exported(state, name);
            }
            if state.options.allexport && !remove_export {
                mark_exported(state, name);
            }
        } else {
            // Just declare without value
            let name = arg.as_str();

            // Validate variable name
            if !valid_name_re.is_match(name) {
                stderr.push_str(&format!("bash: typeset: `{}': not a valid identifier\n", name));
                exit_code = 1;
                continue;
            }

            // Save to local scope before modifying
            if create_local {
                if declare_array || declare_assoc {
                    save_array_to_local_scope(state, name);
                } else {
                    save_to_local_scope(state, name);
                }
            }

            // For declare -n without a value, just mark as nameref
            if declare_nameref {
                mark_nameref(state, name);
                let existing_value = state.env.get(name).cloned();
                if let Some(ref val) = existing_value {
                    if !val.is_empty() && !valid_target_re.is_match(val) {
                        mark_nameref_invalid(state, name);
                    } else if target_exists(state, &state.env.clone(), val) {
                        mark_nameref_bound(state, name);
                    }
                }
                if create_local {
                    mark_local_var_depth(state, name);
                }
                if declare_readonly {
                    mark_readonly(state, name);
                }
                if declare_export {
                    mark_exported(state, name);
                }
                continue;
            }

            // Mark attributes
            if declare_integer {
                mark_integer(state, name);
            }
            if declare_lowercase {
                mark_lowercase(state, name);
            }
            if declare_uppercase {
                mark_uppercase(state, name);
            }

            // Track associative array declaration
            if declare_assoc {
                let existing_indices = get_array_indices(&state.env, name);
                if !existing_indices.is_empty() {
                    stderr.push_str(&format!("bash: declare: {}: cannot convert indexed to associative array\n", name));
                    exit_code = 1;
                    continue;
                }
                if state.associative_arrays.is_none() {
                    state.associative_arrays = Some(HashSet::new());
                }
                state.associative_arrays.as_mut().unwrap().insert(name.to_string());
            }

            // Check if any array elements exist
            let has_array_elements = state.env.keys().any(|key| {
                key.starts_with(&format!("{}_", name)) && !key.starts_with(&format!("{}__length", name))
            });
            if !state.env.contains_key(name) && !has_array_elements {
                if declare_array || declare_assoc {
                    state.env.insert(format!("{}__length", name), "0".to_string());
                } else {
                    if state.declared_vars.is_none() {
                        state.declared_vars = Some(HashSet::new());
                    }
                    state.declared_vars.as_mut().unwrap().insert(name.to_string());
                }
            }

            // Mark as local if inside a function
            if create_local {
                mark_local_var_depth(state, name);
            }

            if declare_readonly {
                mark_readonly(state, name);
            }
            if declare_export {
                mark_exported(state, name);
            }
        }
    }

    BuiltinResult {
        stdout: String::new(),
        stderr,
        exit_code,
    }
}

// ============================================================================
// Local scope helper functions
// ============================================================================

/// Save variable to local scope (for restoration when function exits)
fn save_to_local_scope(state: &mut InterpreterState, name: &str) {
    if state.local_scopes.is_empty() {
        return;
    }
    let current_scope = state.local_scopes.last_mut().unwrap();
    if !current_scope.contains_key(name) {
        current_scope.insert(name.to_string(), state.env.get(name).cloned());
    }
}

/// Save array elements to local scope
fn save_array_to_local_scope(state: &mut InterpreterState, name: &str) {
    if state.local_scopes.is_empty() {
        return;
    }
    let current_scope = state.local_scopes.last_mut().unwrap();
    // Save the base variable
    if !current_scope.contains_key(name) {
        current_scope.insert(name.to_string(), state.env.get(name).cloned());
    }
    // Save array elements
    let prefix = format!("{}_", name);
    let keys_to_save: Vec<String> = state.env.keys()
        .filter(|k| k.starts_with(&prefix) && !k.contains("__"))
        .cloned()
        .collect();
    for key in keys_to_save {
        if !current_scope.contains_key(&key) {
            current_scope.insert(key.clone(), state.env.get(&key).cloned());
        }
    }
    // Save length metadata
    let length_key = format!("{}__length", name);
    if state.env.contains_key(&length_key) && !current_scope.contains_key(&length_key) {
        current_scope.insert(length_key.clone(), state.env.get(&length_key).cloned());
    }
}

// ============================================================================
// readonly builtin
// ============================================================================

/// Handle the readonly builtin command.
///
/// Usage:
///   readonly NAME=value   - Declare readonly variable
///   readonly NAME         - Mark existing variable as readonly
pub fn handle_readonly(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse flags
    let mut _declare_array = false;
    let mut _declare_assoc = false;
    let mut _print_mode = false;
    let mut processed_args: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-a" {
            _declare_array = true;
        } else if arg == "-A" {
            _declare_assoc = true;
        } else if arg == "-p" {
            _print_mode = true;
        } else if arg == "--" {
            processed_args.extend(args[i + 1..].iter().cloned());
            break;
        } else if !arg.starts_with('-') {
            processed_args.push(arg.clone());
        }
        i += 1;
    }

    // When called with no args (or just -p), list readonly variables
    if processed_args.is_empty() {
        let mut stdout = String::new();
        let mut readonly_names: Vec<String> = state.readonly_vars
            .as_ref()
            .map_or(Vec::new(), |v| v.iter().cloned().collect());
        readonly_names.sort();
        for name in readonly_names {
            if let Some(value) = state.env.get(&name) {
                let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
                stdout.push_str(&format!("declare -r {}=\"{}\"\n", name, escaped_value));
            }
        }
        return BuiltinResult {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }

    for arg in &processed_args {
        // Check for += append syntax: readonly NAME+=value
        let append_re = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\+=(.*)$").unwrap();
        if let Some(caps) = append_re.captures(arg) {
            let name = caps.get(1).unwrap().as_str();
            let append_value = expand_tildes_in_value(&state.env, caps.get(2).unwrap().as_str());

            // Check if variable is already readonly
            if is_readonly(state, name) {
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: format!("bash: {}: readonly variable\n", name),
                    exit_code: 1,
                };
            }

            // Append to existing value
            let existing = state.env.get(name).map(|s| s.as_str()).unwrap_or("");
            state.env.insert(name.to_string(), format!("{}{}", existing, append_value));
            mark_readonly(state, name);
            continue;
        }

        // Check for scalar assignment: name=value
        if arg.contains('=') {
            let eq_idx = arg.find('=').unwrap();
            let name = &arg[..eq_idx];
            let value = &arg[eq_idx + 1..];

            // Check if variable is already readonly
            if is_readonly(state, name) {
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: format!("bash: {}: readonly variable\n", name),
                    exit_code: 1,
                };
            }

            state.env.insert(name.to_string(), value.to_string());
            mark_readonly(state, name);
        } else {
            // Just mark as readonly
            mark_readonly(state, arg);
        }
    }

    BuiltinResult {
        stdout: String::new(),
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
    fn test_mark_integer() {
        let mut state = make_state();
        assert!(!is_integer(&state, "foo"));
        mark_integer(&mut state, "foo");
        assert!(is_integer(&state, "foo"));
    }

    #[test]
    fn test_apply_case_transform() {
        let mut state = make_state();

        // No transformation
        assert_eq!(apply_case_transform(&state, "foo", "Hello"), "Hello");

        // Lowercase
        mark_lowercase(&mut state, "foo");
        assert_eq!(apply_case_transform(&state, "foo", "Hello"), "hello");

        // Uppercase (clears lowercase)
        mark_uppercase(&mut state, "foo");
        assert_eq!(apply_case_transform(&state, "foo", "Hello"), "HELLO");
    }

    #[test]
    fn test_handle_declare_simple() {
        let mut state = make_state();
        let result = handle_declare(&mut state, &["foo=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("foo"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_handle_declare_readonly() {
        let mut state = make_state();
        let result = handle_declare(&mut state, &["-r".to_string(), "foo=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(is_readonly(&state, "foo"));
    }

    #[test]
    fn test_handle_declare_export() {
        let mut state = make_state();
        let result = handle_declare(&mut state, &["-x".to_string(), "foo=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(state.exported_vars.as_ref().map_or(false, |v| v.contains("foo")));
    }

    #[test]
    fn test_handle_readonly_simple() {
        let mut state = make_state();
        let result = handle_readonly(&mut state, &["foo=bar".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("foo"), Some(&"bar".to_string()));
        assert!(is_readonly(&state, "foo"));
    }

    #[test]
    fn test_handle_readonly_existing() {
        let mut state = make_state();
        state.env.insert("foo".to_string(), "bar".to_string());
        let result = handle_readonly(&mut state, &["foo".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(is_readonly(&state, "foo"));
    }
}
