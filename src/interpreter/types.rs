//! Interpreter Types
//!
//! Type definitions for the bash interpreter state and context.

use std::collections::{HashMap, HashSet};
use crate::FunctionDefNode;

/// Completion specification for a command, set by the `complete` builtin.
#[derive(Debug, Clone, Default)]
pub struct CompletionSpec {
    /// Word list for -W option
    pub wordlist: Option<String>,
    /// Function name for -F option
    pub function: Option<String>,
    /// Command to run for -C option
    pub command: Option<String>,
    /// Completion options (nospace, filenames, etc.)
    pub options: Option<Vec<String>>,
    /// Actions to perform (from -A option)
    pub actions: Option<Vec<String>>,
    /// Whether this is a default completion (-D)
    pub is_default: Option<bool>,
}

/// Shell options (set -e, etc.)
#[derive(Debug, Clone)]
pub struct ShellOptions {
    /// set -e: Exit immediately if a command exits with non-zero status
    pub errexit: bool,
    /// set -o pipefail: Return the exit status of the last (rightmost) command in a pipeline that fails
    pub pipefail: bool,
    /// set -u: Treat unset variables as an error when substituting
    pub nounset: bool,
    /// set -x: Print commands and their arguments as they are executed
    pub xtrace: bool,
    /// set -v: Print shell input lines as they are read (verbose)
    pub verbose: bool,
    /// set -o posix: POSIX mode for stricter compliance
    pub posix: bool,
    /// set -a: Export all variables
    pub allexport: bool,
    /// set -C: Prevent overwriting files with redirection
    pub noclobber: bool,
    /// set -f: Disable filename expansion (globbing)
    pub noglob: bool,
    /// set -n: Read commands but do not execute them (syntax check mode)
    pub noexec: bool,
    /// set -o vi: Use vi-style line editing (mutually exclusive with emacs)
    pub vi: bool,
    /// set -o emacs: Use emacs-style line editing (mutually exclusive with vi)
    pub emacs: bool,
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self {
            errexit: false,
            pipefail: false,
            nounset: false,
            xtrace: false,
            verbose: false,
            posix: false,
            allexport: false,
            noclobber: false,
            noglob: false,
            noexec: false,
            vi: false,
            emacs: false,
        }
    }
}

/// Shopt options (shopt -s, etc.)
#[derive(Debug, Clone)]
pub struct ShoptOptions {
    /// shopt -s extglob: Enable extended globbing patterns @(), *(), +(), ?(), !()
    pub extglob: bool,
    /// shopt -s dotglob: Include dotfiles in glob expansion
    pub dotglob: bool,
    /// shopt -s nullglob: Return empty for non-matching globs instead of literal pattern
    pub nullglob: bool,
    /// shopt -s failglob: Fail if glob pattern has no matches
    pub failglob: bool,
    /// shopt -s globstar: Enable ** recursive glob patterns
    pub globstar: bool,
    /// shopt -s globskipdots: Skip . and .. in glob patterns (default: true in bash >=5.2)
    pub globskipdots: bool,
    /// shopt -s nocaseglob: Case-insensitive glob matching
    pub nocaseglob: bool,
    /// shopt -s nocasematch: Case-insensitive pattern matching in [[ ]] and case
    pub nocasematch: bool,
    /// shopt -s expand_aliases: Enable alias expansion
    pub expand_aliases: bool,
    /// shopt -s lastpipe: Run last command of pipeline in current shell context
    pub lastpipe: bool,
    /// shopt -s xpg_echo: Make echo interpret backslash escapes by default (like echo -e)
    pub xpg_echo: bool,
}

impl Default for ShoptOptions {
    fn default() -> Self {
        Self {
            extglob: false,
            dotglob: false,
            nullglob: false,
            failglob: false,
            globstar: false,
            globskipdots: true, // default: true in bash >=5.2
            nocaseglob: false,
            nocasematch: false,
            expand_aliases: false,
            lastpipe: false,
            xpg_echo: false,
        }
    }
}

// ============================================================================
// Variable Attribute State
// ============================================================================

/// Tracks variable type attributes (declare -i, -l, -u, -n, -a, -A, etc.)
/// and export status. These affect how variables are read, written, and expanded.
#[derive(Debug, Clone, Default)]
pub struct VariableAttributeState {
    /// Set of variable names that are readonly
    pub readonly_vars: Option<HashSet<String>>,
    /// Set of variable names that are associative arrays
    pub associative_arrays: Option<HashSet<String>>,
    /// Set of variable names that are namerefs (declare -n)
    pub namerefs: Option<HashSet<String>>,
    /// Set of nameref variable names that were "bound" to valid targets at creation time.
    pub bound_namerefs: Option<HashSet<String>>,
    /// Set of nameref variable names that were created with an invalid target.
    pub invalid_namerefs: Option<HashSet<String>>,
    /// Set of variable names that have integer attribute (declare -i)
    pub integer_vars: Option<HashSet<String>>,
    /// Set of variable names that have lowercase attribute (declare -l)
    pub lowercase_vars: Option<HashSet<String>>,
    /// Set of variable names that have uppercase attribute (declare -u)
    pub uppercase_vars: Option<HashSet<String>>,
    /// Set of exported variable names
    pub exported_vars: Option<HashSet<String>>,
    /// Set of temporarily exported variable names (for prefix assignments like FOO=bar cmd)
    pub temp_exported_vars: Option<HashSet<String>>,
    /// Stack of sets tracking variables exported within each local scope.
    pub local_exported_vars: Option<Vec<HashSet<String>>>,
    /// Set of variable names that have been declared but not assigned a value
    pub declared_vars: Option<HashSet<String>>,
}

// ============================================================================
// Local Variable Scoping State
// ============================================================================

/// Entry in the local variable stack, tracking saved values for nested local declarations.
#[derive(Debug, Clone)]
pub struct LocalVarStackEntry {
    pub value: Option<String>,
    pub scope_index: usize,
}

/// Tracks the complex local variable scoping machinery.
#[derive(Debug, Clone, Default)]
pub struct LocalScopingState {
    /// Stack of local variable scopes (one Map per function call)
    pub local_scopes: Vec<HashMap<String, Option<String>>>,
    /// Tracks at which call depth each local variable was declared.
    pub local_var_depth: Option<HashMap<String, u32>>,
    /// Stack of saved values for each local variable, supporting bash's localvar-nest behavior.
    pub local_var_stack: Option<HashMap<String, Vec<LocalVarStackEntry>>>,
    /// Map of variable names to scope index where they were fully unset.
    pub fully_unset_locals: Option<HashMap<String, usize>>,
    /// Stack of temporary environment bindings from prefix assignments.
    pub temp_env_bindings: Option<Vec<HashMap<String, Option<String>>>>,
    /// Set of tempenv variable names that have been explicitly written to.
    pub mutated_temp_env_vars: Option<HashSet<String>>,
    /// Set of tempenv variable names that have been accessed.
    pub accessed_temp_env_vars: Option<HashSet<String>>,
}

// ============================================================================
// Call Stack State
// ============================================================================

/// Tracks the function call stack and source file nesting.
#[derive(Debug, Clone)]
pub struct CallStackState {
    /// Function definitions (name -> AST node)
    pub functions: HashMap<String, FunctionDefNode>,
    /// Current function call depth (for recursion limits and local scoping)
    pub call_depth: u32,
    /// Current source script nesting depth (for return in sourced scripts)
    pub source_depth: u32,
    /// Stack of call line numbers for BASH_LINENO
    pub call_line_stack: Option<Vec<u32>>,
    /// Stack of function names for FUNCNAME
    pub func_name_stack: Option<Vec<String>>,
    /// Stack of source files for BASH_SOURCE
    pub source_stack: Option<Vec<String>>,
    /// Current source file context (for function definitions)
    pub current_source: Option<String>,
}

impl Default for CallStackState {
    fn default() -> Self {
        Self {
            functions: HashMap::new(),
            call_depth: 0,
            source_depth: 0,
            call_line_stack: None,
            func_name_stack: None,
            source_stack: None,
            current_source: None,
        }
    }
}

// ============================================================================
// Control Flow State
// ============================================================================

/// Tracks loop nesting and condition context.
#[derive(Debug, Clone, Default)]
pub struct ControlFlowState {
    /// True when executing condition for if/while/until (errexit doesn't apply)
    pub in_condition: bool,
    /// Current loop nesting depth (for break/continue)
    pub loop_depth: u32,
    /// True if this subshell was spawned from within a loop context
    pub parent_has_loop_context: Option<bool>,
    /// True when the last executed statement's exit code is "safe" for errexit purposes
    pub errexit_safe: Option<bool>,
}

// ============================================================================
// Process State
// ============================================================================

/// Tracks process IDs, timing, and execution counts.
#[derive(Debug, Clone)]
pub struct ProcessState {
    /// Total commands executed (for execution limits)
    pub command_count: u64,
    /// Time when shell started (for $SECONDS)
    pub start_time: u64,
    /// PID of last background job (for $!)
    pub last_background_pid: u32,
    /// Current BASHPID (changes in subshells, unlike $$)
    pub bash_pid: u32,
    /// Counter for generating unique virtual PIDs for subshells
    pub next_virtual_pid: u32,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            command_count: 0,
            start_time: 0,
            last_background_pid: 0,
            bash_pid: std::process::id(),
            next_virtual_pid: 1000,
        }
    }
}

// ============================================================================
// I/O State
// ============================================================================

/// Tracks file descriptors and stdin content for I/O operations.
#[derive(Debug, Clone, Default)]
pub struct IOState {
    /// Stdin available for commands in compound commands
    pub group_stdin: Option<String>,
    /// File descriptors for process substitution and here-docs
    pub file_descriptors: Option<HashMap<i32, String>>,
    /// Next available file descriptor for {varname}>file allocation (starts at 10)
    pub next_fd: Option<i32>,
}

// ============================================================================
// Expansion State
// ============================================================================

/// Captures errors that occur during parameter expansion.
#[derive(Debug, Clone, Default)]
pub struct ExpansionState {
    /// Exit code from expansion errors (arithmetic, etc.)
    pub expansion_exit_code: Option<i32>,
    /// Stderr from expansion errors
    pub expansion_stderr: Option<String>,
}

// ============================================================================
// Interpreter State (Composed)
// ============================================================================

/// Complete interpreter state for bash script execution.
#[derive(Debug, Clone)]
pub struct InterpreterState {
    // ---- Core Environment ----
    /// Environment variables (exported to commands)
    pub env: HashMap<String, String>,
    /// Current working directory
    pub cwd: String,
    /// Previous directory (for `cd -`)
    pub previous_dir: String,

    // ---- Execution Tracking ----
    /// Exit code of last executed command
    pub last_exit_code: i32,
    /// Last argument of previous command, for $_ expansion
    pub last_arg: String,
    /// Current line number being executed (for $LINENO)
    pub current_line: u32,

    // ---- Shell Options ----
    /// Shell options (set -e, etc.)
    pub options: ShellOptions,
    /// Shopt options (shopt -s, etc.)
    pub shopt_options: ShoptOptions,

    // ---- Shell Features ----
    /// Completion specifications set by the `complete` builtin
    pub completion_specs: Option<HashMap<String, CompletionSpec>>,
    /// Directory stack for pushd/popd/dirs
    pub directory_stack: Option<Vec<String>>,
    /// Hash table for PATH command lookup caching
    pub hash_table: Option<HashMap<String, String>>,

    // ---- Output Control ----
    /// Suppress verbose mode output (set -v) when inside command substitutions.
    pub suppress_verbose: Option<bool>,

    // ---- Variable Attributes ----
    /// Set of variable names that are readonly
    pub readonly_vars: Option<HashSet<String>>,
    /// Set of variable names that are associative arrays
    pub associative_arrays: Option<HashSet<String>>,
    /// Set of variable names that are namerefs (declare -n)
    pub namerefs: Option<HashSet<String>>,
    /// Set of nameref variable names that were "bound" to valid targets at creation time.
    pub bound_namerefs: Option<HashSet<String>>,
    /// Set of nameref variable names that were created with an invalid target.
    pub invalid_namerefs: Option<HashSet<String>>,
    /// Set of variable names that have integer attribute (declare -i)
    pub integer_vars: Option<HashSet<String>>,
    /// Set of variable names that have lowercase attribute (declare -l)
    pub lowercase_vars: Option<HashSet<String>>,
    /// Set of variable names that have uppercase attribute (declare -u)
    pub uppercase_vars: Option<HashSet<String>>,
    /// Set of exported variable names
    pub exported_vars: Option<HashSet<String>>,
    /// Set of temporarily exported variable names (for prefix assignments like FOO=bar cmd)
    pub temp_exported_vars: Option<HashSet<String>>,
    /// Stack of sets tracking variables exported within each local scope.
    pub local_exported_vars: Option<Vec<HashSet<String>>>,
    /// Set of variable names that have been declared but not assigned a value
    pub declared_vars: Option<HashSet<String>>,

    // ---- Local Scoping ----
    /// Stack of local variable scopes (one Map per function call)
    pub local_scopes: Vec<HashMap<String, Option<String>>>,
    /// Tracks at which call depth each local variable was declared.
    pub local_var_depth: Option<HashMap<String, u32>>,
    /// Stack of saved values for each local variable, supporting bash's localvar-nest behavior.
    pub local_var_stack: Option<HashMap<String, Vec<LocalVarStackEntry>>>,
    /// Map of variable names to scope index where they were fully unset.
    pub fully_unset_locals: Option<HashMap<String, usize>>,
    /// Stack of temporary environment bindings from prefix assignments.
    pub temp_env_bindings: Option<Vec<HashMap<String, Option<String>>>>,
    /// Set of tempenv variable names that have been explicitly written to.
    pub mutated_temp_env_vars: Option<HashSet<String>>,
    /// Set of tempenv variable names that have been accessed.
    pub accessed_temp_env_vars: Option<HashSet<String>>,

    // ---- Call Stack ----
    /// Function definitions (name -> AST node)
    pub functions: HashMap<String, FunctionDefNode>,
    /// Current function call depth (for recursion limits and local scoping)
    pub call_depth: u32,
    /// Current source script nesting depth (for return in sourced scripts)
    pub source_depth: u32,
    /// Stack of call line numbers for BASH_LINENO
    pub call_line_stack: Option<Vec<u32>>,
    /// Stack of function names for FUNCNAME
    pub func_name_stack: Option<Vec<String>>,
    /// Stack of source files for BASH_SOURCE
    pub source_stack: Option<Vec<String>>,
    /// Current source file context (for function definitions)
    pub current_source: Option<String>,

    // ---- Control Flow ----
    /// True when executing condition for if/while/until (errexit doesn't apply)
    pub in_condition: bool,
    /// Current loop nesting depth (for break/continue)
    pub loop_depth: u32,
    /// True if this subshell was spawned from within a loop context
    pub parent_has_loop_context: Option<bool>,
    /// True when the last executed statement's exit code is "safe" for errexit purposes
    pub errexit_safe: Option<bool>,

    // ---- Process ----
    /// Total commands executed (for execution limits)
    pub command_count: u64,
    /// Time when shell started (for $SECONDS)
    pub start_time: u64,
    /// PID of last background job (for $!)
    pub last_background_pid: u32,
    /// Current BASHPID (changes in subshells, unlike $$)
    pub bash_pid: u32,
    /// Counter for generating unique virtual PIDs for subshells
    pub next_virtual_pid: u32,

    // ---- I/O ----
    /// Stdin available for commands in compound commands
    pub group_stdin: Option<String>,
    /// File descriptors for process substitution and here-docs
    pub file_descriptors: Option<HashMap<i32, String>>,
    /// Next available file descriptor for {varname}>file allocation (starts at 10)
    pub next_fd: Option<i32>,

    // ---- Expansion ----
    /// Exit code from expansion errors (arithmetic, etc.)
    pub expansion_exit_code: Option<i32>,
    /// Stderr from expansion errors
    pub expansion_stderr: Option<String>,

    // ---- Aliases ----
    /// Alias definitions (name -> expansion)
    pub aliases: Option<HashMap<String, String>>,
}

impl Default for InterpreterState {
    fn default() -> Self {
        Self {
            env: HashMap::new(),
            cwd: String::from("/"),
            previous_dir: String::new(),
            last_exit_code: 0,
            last_arg: String::new(),
            current_line: 1,
            options: ShellOptions::default(),
            shopt_options: ShoptOptions::default(),
            completion_specs: None,
            directory_stack: None,
            hash_table: None,
            suppress_verbose: None,
            readonly_vars: None,
            associative_arrays: None,
            namerefs: None,
            bound_namerefs: None,
            invalid_namerefs: None,
            integer_vars: None,
            lowercase_vars: None,
            uppercase_vars: None,
            exported_vars: None,
            temp_exported_vars: None,
            local_exported_vars: None,
            declared_vars: None,
            local_scopes: Vec::new(),
            local_var_depth: None,
            local_var_stack: None,
            fully_unset_locals: None,
            temp_env_bindings: None,
            mutated_temp_env_vars: None,
            accessed_temp_env_vars: None,
            functions: HashMap::new(),
            call_depth: 0,
            source_depth: 0,
            call_line_stack: None,
            func_name_stack: None,
            source_stack: None,
            current_source: None,
            in_condition: false,
            loop_depth: 0,
            parent_has_loop_context: None,
            errexit_safe: None,
            command_count: 0,
            start_time: 0,
            last_background_pid: 0,
            bash_pid: std::process::id(),
            next_virtual_pid: 1000,
            group_stdin: None,
            file_descriptors: None,
            next_fd: None,
            expansion_exit_code: None,
            expansion_stderr: None,
            aliases: None,
        }
    }
}

/// Execution result from a command or script.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub env: Option<HashMap<String, String>>,
}

impl ExecResult {
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code, env: None }
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    /// Success result with no output
    pub fn ok() -> Self {
        Self::new(String::new(), String::new(), 0)
    }

    /// Failure result with stderr message
    pub fn failure(stderr: impl Into<String>) -> Self {
        Self::new(String::new(), stderr.into(), 1)
    }

    /// Failure result with stderr message and custom exit code
    pub fn failure_with_code(stderr: impl Into<String>, exit_code: i32) -> Self {
        Self::new(String::new(), stderr.into(), exit_code)
    }
}

impl Default for ExecResult {
    fn default() -> Self {
        Self::ok()
    }
}

/// Execution limits configuration.
#[derive(Debug, Clone)]
pub struct ExecutionLimits {
    /// Maximum recursion depth for function calls
    pub max_recursion_depth: u32,
    /// Maximum number of commands to execute
    pub max_command_count: u64,
    /// Maximum number of loop iterations
    pub max_iterations: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_recursion_depth: 1000,
            max_command_count: 100_000,
            max_iterations: 1_000_000,
        }
    }
}

/// Trace callback type for performance profiling.
pub type TraceCallback = Box<dyn Fn(&str, u64) + Send + Sync>;

/// Command registry type - maps command names to their implementations.
pub type CommandRegistry = HashMap<String, Box<dyn Command + Send + Sync>>;

/// Trait for command implementations.
pub trait Command {
    fn execute(&self, ctx: &mut InterpreterContext, args: &[String], stdin: &str) -> ExecResult;
}

/// Interpreter context passed to commands and helpers.
pub struct InterpreterContext<'a> {
    pub state: &'a mut InterpreterState,
    pub limits: &'a ExecutionLimits,
    // Note: File system and other dependencies will be added as traits
}

impl<'a> InterpreterContext<'a> {
    pub fn new(state: &'a mut InterpreterState, limits: &'a ExecutionLimits) -> Self {
        Self { state, limits }
    }
}
