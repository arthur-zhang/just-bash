//! Builtin Commands
//!
//! This module contains implementations of shell builtin commands.

pub mod break_cmd;
pub mod cd_cmd;
pub mod compgen_cmd;
pub mod complete_cmd;
pub mod compopt_cmd;
pub mod continue_cmd;
pub mod declare_array_parsing;
pub mod declare_cmd;
pub mod declare_print;
pub mod dirs_cmd;
pub mod eval_cmd;
pub mod exit_cmd;
pub mod export_cmd;
pub mod getopts_cmd;
pub mod hash_cmd;
pub mod help_cmd;
pub mod let_cmd;
pub mod local_cmd;
pub mod mapfile_cmd;
pub mod read_cmd;
pub mod return_cmd;
pub mod set_cmd;
pub mod shift_cmd;
pub mod shopt_cmd;
pub mod source_cmd;
pub mod unset_cmd;
pub mod variable_assignment;

pub use break_cmd::{handle_break, BuiltinResult};
pub use cd_cmd::handle_cd;
pub use compgen_cmd::{handle_compgen, SHELL_BUILTINS, SHELL_KEYWORDS, SHOPT_OPTIONS};
pub use complete_cmd::handle_complete;
pub use compopt_cmd::handle_compopt;
pub use continue_cmd::handle_continue;
pub use declare_array_parsing::{parse_array_elements, parse_assoc_array_literal};
pub use declare_cmd::{
    apply_case_transform, handle_declare, handle_readonly, is_integer, mark_integer,
    mark_local_var_depth,
};
pub use declare_print::{
    list_all_variables, list_associative_arrays, list_indexed_arrays, print_all_variables,
    print_specific_variables, PrintAllFilters,
};
pub use dirs_cmd::{handle_dirs, handle_popd, handle_pushd};
pub use eval_cmd::{
    handle_eval_parse, parse_eval_args, prepare_eval_stdin, restore_eval_stdin,
    eval_parse_error, EvalCommand,
};
pub use exit_cmd::handle_exit;
pub use export_cmd::handle_export;
pub use getopts_cmd::handle_getopts;
pub use hash_cmd::{handle_hash, hash_add, hash_lookup};
pub use help_cmd::handle_help;
pub use let_cmd::handle_let;
pub use local_cmd::handle_local;
pub use mapfile_cmd::handle_mapfile;
pub use read_cmd::handle_read;
pub use return_cmd::handle_return;
pub use set_cmd::handle_set;
pub use shift_cmd::handle_shift;
pub use shopt_cmd::handle_shopt;
pub use source_cmd::{
    handle_source_parse, parse_source_args, prepare_source_state, restore_source_state,
    resolve_source_paths, source_file_not_found, source_parse_error,
    SourceCommand, SourceSavedState,
};
pub use unset_cmd::handle_unset;
pub use variable_assignment::{
    parse_assignment, set_variable, get_local_var_depth, clear_local_var_depth,
    push_local_var_stack, pop_local_var_stack, clear_local_var_stack_for_scope,
    ParsedAssignment, SetVariableOptions,
};
