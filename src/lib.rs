//! just-bash - A simulated bash environment
//!
//! This library provides a complete parser and interpreter for bash scripts.

pub mod ast;
pub mod bash;
pub mod commands;
pub mod fs;
pub mod interpreter;
pub mod network;
pub mod parser;
pub mod shell;
pub mod sandbox;

pub use ast::types::*;
pub use parser::{parse, Parser, ParseException};
pub use bash::Bash;
pub use fs::{FileSystem, InMemoryFs};
pub use commands::{Command, CommandContext, CommandResult};
