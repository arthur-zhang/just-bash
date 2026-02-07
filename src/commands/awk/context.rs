/// AWK Runtime Context
///
/// Holds all mutable state for AWK program execution, including
/// built-in variables, fields, user variables, arrays, control flow
/// flags, and execution limits.

use std::collections::{HashMap, HashSet};
use regex_lite::Regex;
use crate::commands::awk::types::AwkFunctionDef;

const DEFAULT_MAX_ITERATIONS: usize = 10000;
const DEFAULT_MAX_RECURSION_DEPTH: usize = 100;

/// Create a compiled regex for the given field separator string.
///
/// The default FS (" ") maps to a whitespace-run regex `\s+`.
/// Single-character separators are escaped for literal matching.
/// Multi-character separators are treated as regex patterns.
pub fn create_field_sep_regex(fs: &str) -> Regex {
    if fs == " " {
        Regex::new(r"\s+").unwrap()
    } else if fs.len() == 1 {
        // Single character: escape for literal match
        let escaped = regex_lite::escape(fs);
        Regex::new(&escaped).unwrap()
    } else {
        // Multi-character: treat as regex pattern
        Regex::new(fs).unwrap_or_else(|_| {
            let escaped = regex_lite::escape(fs);
            Regex::new(&escaped).unwrap()
        })
    }
}

/// The runtime context for an AWK program execution.
pub struct AwkContext {
    // Built-in variables
    pub fs: String,
    pub ofs: String,
    pub ors: String,
    pub ofmt: String,
    pub nr: usize,
    pub nf: usize,
    pub fnr: usize,
    pub filename: String,
    pub rstart: usize,
    pub rlength: i64,
    pub subsep: String,
    pub argc: usize,
    pub argv: HashMap<String, String>,
    pub environ: HashMap<String, String>,

    // Current line state
    pub fields: Vec<String>,
    pub line: String,
    pub field_sep: Regex,

    // User data
    pub vars: HashMap<String, String>,
    pub arrays: HashMap<String, HashMap<String, String>>,
    pub array_aliases: HashMap<String, String>,
    pub functions: HashMap<String, AwkFunctionDef>,

    // Getline support
    pub lines: Option<Vec<String>>,
    pub line_index: Option<usize>,

    // Execution limits
    pub max_iterations: usize,
    pub max_recursion_depth: usize,
    pub current_recursion_depth: usize,

    // Control flow
    pub exit_code: i32,
    pub should_exit: bool,
    pub should_next: bool,
    pub should_next_file: bool,
    pub loop_break: bool,
    pub loop_continue: bool,
    pub return_value: Option<String>,
    pub has_return: bool,
    pub in_end_block: bool,

    // I/O
    pub output: String,
    pub opened_files: HashSet<String>,
}

impl AwkContext {
    /// Create a new context with default settings (FS = " ").
    pub fn new() -> Self {
        Self::with_fs(" ")
    }

    /// Create a new context with a custom field separator.
    pub fn with_fs(fs: &str) -> Self {
        let field_sep = create_field_sep_regex(fs);
        AwkContext {
            fs: fs.to_string(),
            ofs: " ".to_string(),
            ors: "\n".to_string(),
            ofmt: "%.6g".to_string(),
            nr: 0,
            nf: 0,
            fnr: 0,
            filename: String::new(),
            rstart: 0,
            rlength: -1,
            subsep: "\x1c".to_string(),
            argc: 0,
            argv: HashMap::new(),
            environ: HashMap::new(),

            fields: Vec::new(),
            line: String::new(),
            field_sep,

            vars: HashMap::new(),
            arrays: HashMap::new(),
            array_aliases: HashMap::new(),
            functions: HashMap::new(),

            lines: None,
            line_index: None,

            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
            current_recursion_depth: 0,

            exit_code: 0,
            should_exit: false,
            should_next: false,
            should_next_file: false,
            loop_break: false,
            loop_continue: false,
            return_value: None,
            has_return: false,
            in_end_block: false,

            output: String::new(),
            opened_files: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_context_builtin_vars() {
        let ctx = AwkContext::new();
        assert_eq!(ctx.fs, " ");
        assert_eq!(ctx.ofs, " ");
        assert_eq!(ctx.ors, "\n");
        assert_eq!(ctx.ofmt, "%.6g");
        assert_eq!(ctx.nr, 0);
        assert_eq!(ctx.nf, 0);
        assert_eq!(ctx.fnr, 0);
        assert_eq!(ctx.filename, "");
        assert_eq!(ctx.rstart, 0);
        assert_eq!(ctx.rlength, -1);
        assert_eq!(ctx.subsep, "\x1c");
        assert_eq!(ctx.argc, 0);
    }

    #[test]
    fn test_custom_fs_context() {
        let ctx = AwkContext::with_fs(":");
        assert_eq!(ctx.fs, ":");
        // FS regex should match ":"
        assert!(ctx.field_sep.is_match(":"));
        assert!(!ctx.field_sep.is_match(" "));
    }

    #[test]
    fn test_control_flow_defaults() {
        let ctx = AwkContext::new();
        assert!(!ctx.should_exit);
        assert!(!ctx.should_next);
        assert!(!ctx.should_next_file);
        assert!(!ctx.loop_break);
        assert!(!ctx.loop_continue);
        assert!(!ctx.has_return);
        assert!(!ctx.in_end_block);
        assert!(ctx.return_value.is_none());
        assert_eq!(ctx.exit_code, 0);
    }

    #[test]
    fn test_execution_limits_defaults() {
        let ctx = AwkContext::new();
        assert_eq!(ctx.max_iterations, 10000);
        assert_eq!(ctx.max_recursion_depth, 100);
        assert_eq!(ctx.current_recursion_depth, 0);
    }

    #[test]
    fn test_output_buffer_starts_empty() {
        let ctx = AwkContext::new();
        assert_eq!(ctx.output, "");
        assert!(ctx.opened_files.is_empty());
    }

    #[test]
    fn test_collections_start_empty() {
        let ctx = AwkContext::new();
        assert!(ctx.fields.is_empty());
        assert_eq!(ctx.line, "");
        assert!(ctx.vars.is_empty());
        assert!(ctx.arrays.is_empty());
        assert!(ctx.array_aliases.is_empty());
        assert!(ctx.functions.is_empty());
        assert!(ctx.argv.is_empty());
        assert!(ctx.environ.is_empty());
    }

    #[test]
    fn test_getline_support_defaults() {
        let ctx = AwkContext::new();
        assert!(ctx.lines.is_none());
        assert!(ctx.line_index.is_none());
    }

    #[test]
    fn test_create_field_sep_regex_default() {
        let re = create_field_sep_regex(" ");
        assert!(re.is_match("  "));
        assert!(re.is_match("\t"));
    }

    #[test]
    fn test_create_field_sep_regex_colon() {
        let re = create_field_sep_regex(":");
        assert!(re.is_match(":"));
        assert!(!re.is_match(" "));
    }

    #[test]
    fn test_create_field_sep_regex_pattern() {
        let re = create_field_sep_regex("[,;]");
        assert!(re.is_match(","));
        assert!(re.is_match(";"));
        assert!(!re.is_match(":"));
    }
}
