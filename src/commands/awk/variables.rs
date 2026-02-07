/// AWK Variable and Array Operations
///
/// Handles reading and writing of built-in variables, user variables,
/// and associative arrays. Built-in variable assignments have side
/// effects (e.g., setting FS recompiles the field separator regex,
/// setting NF truncates or extends the fields array).

use crate::commands::awk::context::{create_field_sep_regex, AwkContext};
use crate::commands::awk::coercion::to_number;

/// Get a variable value by name.
///
/// Dispatches built-in variable names (FS, OFS, ORS, OFMT, NR, NF,
/// FNR, FILENAME, RSTART, RLENGTH, SUBSEP, ARGC) to their context
/// fields. All other names are looked up in the user variables map,
/// returning an empty string if not found.
pub fn get_variable(ctx: &AwkContext, name: &str) -> String {
    match name {
        "FS" => ctx.fs.clone(),
        "OFS" => ctx.ofs.clone(),
        "ORS" => ctx.ors.clone(),
        "OFMT" => ctx.ofmt.clone(),
        "NR" => ctx.nr.to_string(),
        "NF" => ctx.nf.to_string(),
        "FNR" => ctx.fnr.to_string(),
        "FILENAME" => ctx.filename.clone(),
        "RSTART" => ctx.rstart.to_string(),
        "RLENGTH" => ctx.rlength.to_string(),
        "SUBSEP" => ctx.subsep.clone(),
        "ARGC" => ctx.argc.to_string(),
        _ => ctx.vars.get(name).cloned().unwrap_or_default(),
    }
}

/// Set a variable value by name.
///
/// Built-in variables trigger side effects:
/// - FS: recompiles the field separator regex
/// - NF: truncates or extends the fields array and rebuilds $0
/// - NR, FNR, RSTART, RLENGTH: parsed as numbers
/// - OFS, ORS, OFMT, FILENAME, SUBSEP: stored as strings
///
/// All other names are stored in the user variables map.
pub fn set_variable(ctx: &mut AwkContext, name: &str, value: &str) {
    match name {
        "FS" => {
            ctx.fs = value.to_string();
            ctx.field_sep = create_field_sep_regex(value);
        }
        "OFS" => ctx.ofs = value.to_string(),
        "ORS" => ctx.ors = value.to_string(),
        "OFMT" => ctx.ofmt = value.to_string(),
        "NR" => ctx.nr = to_number(value) as usize,
        "NF" => {
            let new_nf = to_number(value) as usize;
            if new_nf < ctx.nf {
                ctx.fields.truncate(new_nf);
                ctx.line = ctx.fields.join(&ctx.ofs);
            } else if new_nf > ctx.nf {
                while ctx.fields.len() < new_nf {
                    ctx.fields.push(String::new());
                }
                ctx.line = ctx.fields.join(&ctx.ofs);
            }
            ctx.nf = new_nf;
        }
        "FNR" => ctx.fnr = to_number(value) as usize,
        "FILENAME" => ctx.filename = value.to_string(),
        "RSTART" => ctx.rstart = to_number(value) as usize,
        "RLENGTH" => ctx.rlength = to_number(value) as i64,
        "SUBSEP" => ctx.subsep = value.to_string(),
        _ => {
            ctx.vars.insert(name.to_string(), value.to_string());
        }
    }
}

/// Resolve an array name through the alias chain.
///
/// When arrays are passed as function parameters, the parameter name
/// is aliased to the original array name. This function follows the
/// alias chain to find the real underlying array name.
pub fn resolve_array_name<'a>(ctx: &'a AwkContext, name: &'a str) -> &'a str {
    let mut resolved = name;
    let mut depth = 0;
    while let Some(alias) = ctx.array_aliases.get(resolved) {
        // Guard against infinite alias loops
        depth += 1;
        if depth > 100 {
            break;
        }
        // We need to convert &String to &str for the next iteration.
        // Since the alias is owned by the HashMap which is borrowed via ctx,
        // the lifetime is valid for 'a.
        resolved = alias.as_str();
    }
    resolved
}

/// Get an array element value.
///
/// Special arrays ARGV and ENVIRON are read from their dedicated maps.
/// Other arrays are resolved through aliases and read from the arrays map.
/// Returns an empty string if the element does not exist.
pub fn get_array_element(ctx: &AwkContext, array: &str, key: &str) -> String {
    if array == "ARGV" {
        return ctx.argv.get(key).cloned().unwrap_or_default();
    }
    if array == "ENVIRON" {
        return ctx.environ.get(key).cloned().unwrap_or_default();
    }
    let resolved = resolve_array_name(ctx, array);
    ctx.arrays
        .get(resolved)
        .and_then(|m| m.get(key))
        .cloned()
        .unwrap_or_default()
}

/// Set an array element value.
///
/// Resolves aliases and creates the array if it does not exist.
pub fn set_array_element(ctx: &mut AwkContext, array: &str, key: &str, value: &str) {
    let resolved = resolve_array_name(ctx, array).to_string();
    ctx.arrays
        .entry(resolved)
        .or_default()
        .insert(key.to_string(), value.to_string());
}

/// Check if an array element exists.
///
/// Special arrays ARGV and ENVIRON are checked in their dedicated maps.
pub fn has_array_element(ctx: &AwkContext, array: &str, key: &str) -> bool {
    if array == "ARGV" {
        return ctx.argv.contains_key(key);
    }
    if array == "ENVIRON" {
        return ctx.environ.contains_key(key);
    }
    let resolved = resolve_array_name(ctx, array);
    ctx.arrays
        .get(resolved)
        .map_or(false, |m| m.contains_key(key))
}

/// Delete a single array element.
pub fn delete_array_element(ctx: &mut AwkContext, array: &str, key: &str) {
    let resolved = resolve_array_name(ctx, array).to_string();
    if let Some(m) = ctx.arrays.get_mut(&resolved) {
        m.remove(key);
    }
}

/// Delete an entire array (all elements).
pub fn delete_array(ctx: &mut AwkContext, array: &str) {
    let resolved = resolve_array_name(ctx, array).to_string();
    ctx.arrays.remove(&resolved);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::awk::context::AwkContext;
    use crate::commands::awk::fields::set_current_line;

    #[test]
    fn test_get_set_user_variable() {
        let mut ctx = AwkContext::new();
        assert_eq!(get_variable(&ctx, "x"), "");
        set_variable(&mut ctx, "x", "hello");
        assert_eq!(get_variable(&ctx, "x"), "hello");
    }

    #[test]
    fn test_get_builtin_variables() {
        let ctx = AwkContext::new();
        assert_eq!(get_variable(&ctx, "FS"), " ");
        assert_eq!(get_variable(&ctx, "OFS"), " ");
        assert_eq!(get_variable(&ctx, "ORS"), "\n");
        assert_eq!(get_variable(&ctx, "OFMT"), "%.6g");
        assert_eq!(get_variable(&ctx, "NR"), "0");
        assert_eq!(get_variable(&ctx, "NF"), "0");
        assert_eq!(get_variable(&ctx, "FNR"), "0");
        assert_eq!(get_variable(&ctx, "FILENAME"), "");
        assert_eq!(get_variable(&ctx, "RSTART"), "0");
        assert_eq!(get_variable(&ctx, "RLENGTH"), "-1");
        assert_eq!(get_variable(&ctx, "SUBSEP"), "\x1c");
        assert_eq!(get_variable(&ctx, "ARGC"), "0");
    }

    #[test]
    fn test_set_fs_triggers_recompilation() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "FS", ":");
        assert_eq!(ctx.fs, ":");
        assert!(ctx.field_sep.is_match(":"));
        assert!(!ctx.field_sep.is_match(" "));
    }

    #[test]
    fn test_set_nf_truncates() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "a b c d e");
        assert_eq!(ctx.nf, 5);
        set_variable(&mut ctx, "NF", "3");
        assert_eq!(ctx.nf, 3);
        assert_eq!(ctx.fields.len(), 3);
        assert_eq!(ctx.fields, vec!["a", "b", "c"]);
        assert_eq!(ctx.line, "a b c");
    }

    #[test]
    fn test_set_nf_extends() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "a b");
        set_variable(&mut ctx, "NF", "5");
        assert_eq!(ctx.nf, 5);
        assert_eq!(ctx.fields.len(), 5);
        assert_eq!(ctx.fields[2], "");
        assert_eq!(ctx.fields[3], "");
        assert_eq!(ctx.fields[4], "");
    }

    #[test]
    fn test_array_crud() {
        let mut ctx = AwkContext::new();
        // Create
        set_array_element(&mut ctx, "arr", "key1", "val1");
        assert_eq!(get_array_element(&ctx, "arr", "key1"), "val1");
        // Has
        assert!(has_array_element(&ctx, "arr", "key1"));
        assert!(!has_array_element(&ctx, "arr", "key2"));
        // Update
        set_array_element(&mut ctx, "arr", "key1", "updated");
        assert_eq!(get_array_element(&ctx, "arr", "key1"), "updated");
        // Delete element
        delete_array_element(&mut ctx, "arr", "key1");
        assert!(!has_array_element(&ctx, "arr", "key1"));
        assert_eq!(get_array_element(&ctx, "arr", "key1"), "");
    }

    #[test]
    fn test_delete_entire_array() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "a", "1");
        set_array_element(&mut ctx, "arr", "b", "2");
        delete_array(&mut ctx, "arr");
        assert!(!has_array_element(&ctx, "arr", "a"));
        assert!(!has_array_element(&ctx, "arr", "b"));
    }

    #[test]
    fn test_array_alias_resolution() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "real_arr", "k", "v");
        ctx.array_aliases
            .insert("alias_arr".to_string(), "real_arr".to_string());
        assert_eq!(get_array_element(&ctx, "alias_arr", "k"), "v");
        set_array_element(&mut ctx, "alias_arr", "k2", "v2");
        assert_eq!(get_array_element(&ctx, "real_arr", "k2"), "v2");
        assert!(has_array_element(&ctx, "alias_arr", "k"));
    }

    #[test]
    fn test_argv_access() {
        let mut ctx = AwkContext::new();
        ctx.argv.insert("0".to_string(), "awk".to_string());
        ctx.argv.insert("1".to_string(), "prog".to_string());
        assert_eq!(get_array_element(&ctx, "ARGV", "0"), "awk");
        assert_eq!(get_array_element(&ctx, "ARGV", "1"), "prog");
        assert_eq!(get_array_element(&ctx, "ARGV", "2"), "");
        assert!(has_array_element(&ctx, "ARGV", "0"));
        assert!(!has_array_element(&ctx, "ARGV", "99"));
    }

    #[test]
    fn test_environ_access() {
        let mut ctx = AwkContext::new();
        ctx.environ
            .insert("HOME".to_string(), "/home/user".to_string());
        assert_eq!(get_array_element(&ctx, "ENVIRON", "HOME"), "/home/user");
        assert_eq!(get_array_element(&ctx, "ENVIRON", "MISSING"), "");
        assert!(has_array_element(&ctx, "ENVIRON", "HOME"));
        assert!(!has_array_element(&ctx, "ENVIRON", "MISSING"));
    }

    #[test]
    fn test_nonexistent_array_element() {
        let ctx = AwkContext::new();
        assert_eq!(get_array_element(&ctx, "noarr", "nokey"), "");
        assert!(!has_array_element(&ctx, "noarr", "nokey"));
    }

    #[test]
    fn test_resolve_array_name_no_alias() {
        let ctx = AwkContext::new();
        assert_eq!(resolve_array_name(&ctx, "arr"), "arr");
    }
}
