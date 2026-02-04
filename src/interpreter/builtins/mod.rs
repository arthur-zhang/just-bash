//! Builtin Commands
//!
//! This module contains implementations of shell builtin commands.

pub mod break_cmd;
pub mod complete_cmd;
pub mod compopt_cmd;
pub mod continue_cmd;
pub mod declare_array_parsing;
pub mod declare_print;
pub mod exit_cmd;
pub mod export_cmd;
pub mod getopts_cmd;
pub mod help_cmd;
pub mod mapfile_cmd;
pub mod read_cmd;
pub mod return_cmd;
pub mod set_cmd;
pub mod shift_cmd;
pub mod shopt_cmd;

pub use break_cmd::{handle_break, BuiltinResult};
pub use complete_cmd::handle_complete;
pub use compopt_cmd::handle_compopt;
pub use continue_cmd::handle_continue;
pub use declare_array_parsing::{parse_array_elements, parse_assoc_array_literal};
pub use declare_print::{
    list_all_variables, list_associative_arrays, list_indexed_arrays, print_all_variables,
    print_specific_variables, PrintAllFilters,
};
pub use exit_cmd::handle_exit;
pub use export_cmd::handle_export;
pub use getopts_cmd::handle_getopts;
pub use help_cmd::handle_help;
pub use mapfile_cmd::handle_mapfile;
pub use read_cmd::handle_read;
pub use return_cmd::handle_return;
pub use set_cmd::handle_set;
pub use shift_cmd::handle_shift;
pub use shopt_cmd::handle_shopt;
