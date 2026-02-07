/// AWK Field Operations
///
/// Handles splitting input lines into fields, accessing fields by
/// index ($0, $1, $2, ...), and modifying fields with proper
/// rebuild of $0 and re-splitting semantics.

use regex_lite::Regex;
use crate::commands::awk::context::AwkContext;

/// Split a line into fields based on the field separator.
///
/// When `default_fs` is true (FS = " "), leading and trailing whitespace
/// is trimmed and the line is split on runs of whitespace. Otherwise,
/// the line is split on the compiled regex pattern.
pub fn split_fields(line: &str, field_sep: &Regex, default_fs: bool) -> Vec<String> {
    if line.is_empty() {
        return Vec::new();
    }

    if default_fs {
        // Default FS: trim leading/trailing whitespace, split on runs
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        trimmed.split_whitespace().map(|s| s.to_string()).collect()
    } else {
        // Custom FS: split on the regex pattern
        field_sep.split(line).map(|s| s.to_string()).collect()
    }
}

/// Get a field value by index.
///
/// $0 returns the entire line. $1..$NF return individual fields.
/// Indices beyond NF or negative indices return an empty string.
pub fn get_field(ctx: &AwkContext, index: i64) -> String {
    if index == 0 {
        return ctx.line.clone();
    }
    if index < 0 || index as usize > ctx.fields.len() {
        return String::new();
    }
    ctx.fields
        .get((index - 1) as usize)
        .cloned()
        .unwrap_or_default()
}

/// Set a field value by index.
///
/// Setting $0 re-splits the line into fields. Setting $1+ updates
/// the individual field and rebuilds $0 by joining fields with OFS.
/// Setting a field beyond NF extends the fields array with empty strings.
pub fn set_field(ctx: &mut AwkContext, index: i64, value: &str) {
    if index == 0 {
        // Setting $0 re-splits the line
        ctx.line = value.to_string();
        let default_fs = ctx.fs == " ";
        ctx.fields = split_fields(&ctx.line, &ctx.field_sep, default_fs);
        ctx.nf = ctx.fields.len();
    } else if index > 0 {
        let idx = index as usize;
        // Extend fields array if needed
        while ctx.fields.len() < idx {
            ctx.fields.push(String::new());
        }
        ctx.fields[idx - 1] = value.to_string();
        ctx.nf = ctx.fields.len();
        // Rebuild $0 from fields
        ctx.line = ctx.fields.join(&ctx.ofs);
    }
}

/// Update the context with a new input line.
///
/// Sets $0 to the new line and re-splits into fields, updating NF.
pub fn set_current_line(ctx: &mut AwkContext, new_line: &str) {
    ctx.line = new_line.to_string();
    let default_fs = ctx.fs == " ";
    ctx.fields = split_fields(new_line, &ctx.field_sep, default_fs);
    ctx.nf = ctx.fields.len();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::awk::context::{create_field_sep_regex, AwkContext};

    #[test]
    fn test_split_default_fs_basic() {
        let re = create_field_sep_regex(" ");
        let fields = split_fields("hello world foo", &re, true);
        assert_eq!(fields, vec!["hello", "world", "foo"]);
    }

    #[test]
    fn test_split_default_fs_trims_whitespace() {
        let re = create_field_sep_regex(" ");
        let fields = split_fields("  hello  world  ", &re, true);
        assert_eq!(fields, vec!["hello", "world"]);
    }

    #[test]
    fn test_split_default_fs_tabs_and_spaces() {
        let re = create_field_sep_regex(" ");
        let fields = split_fields("a\tb\t c", &re, true);
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_custom_fs_colon() {
        let re = create_field_sep_regex(":");
        let fields = split_fields("root:x:0:0", &re, false);
        assert_eq!(fields, vec!["root", "x", "0", "0"]);
    }

    #[test]
    fn test_split_custom_fs_regex() {
        let re = create_field_sep_regex("[,;]");
        let fields = split_fields("a,b;c", &re, false);
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_empty_line() {
        let re = create_field_sep_regex(" ");
        let fields = split_fields("", &re, true);
        assert!(fields.is_empty());
    }

    #[test]
    fn test_split_tab_fs() {
        let re = create_field_sep_regex("\t");
        let fields = split_fields("a\tb\tc", &re, false);
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_get_field_zero() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        assert_eq!(get_field(&ctx, 0), "hello world");
    }

    #[test]
    fn test_get_field_one() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        assert_eq!(get_field(&ctx, 1), "hello");
    }

    #[test]
    fn test_get_field_beyond_nf() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        assert_eq!(get_field(&ctx, 5), "");
    }

    #[test]
    fn test_get_field_negative() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        assert_eq!(get_field(&ctx, -1), "");
    }

    #[test]
    fn test_set_field_one_rebuilds_line() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        set_field(&mut ctx, 1, "goodbye");
        assert_eq!(ctx.fields[0], "goodbye");
        assert_eq!(ctx.line, "goodbye world");
    }

    #[test]
    fn test_set_field_zero_resplits() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        set_field(&mut ctx, 0, "a b c");
        assert_eq!(ctx.fields, vec!["a", "b", "c"]);
        assert_eq!(ctx.nf, 3);
    }

    #[test]
    fn test_set_field_beyond_nf_extends() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        set_field(&mut ctx, 5, "extra");
        assert_eq!(ctx.nf, 5);
        assert_eq!(ctx.fields[4], "extra");
        assert_eq!(ctx.fields[2], "");
        assert_eq!(ctx.fields[3], "");
    }

    #[test]
    fn test_set_current_line_updates_nf() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "a b c d");
        assert_eq!(ctx.nf, 4);
        assert_eq!(ctx.fields.len(), 4);
    }

    #[test]
    fn test_empty_line_zero_fields() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "");
        assert_eq!(ctx.nf, 0);
        assert!(ctx.fields.is_empty());
    }
}
