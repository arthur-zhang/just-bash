//! just-bash - A simulated bash environment
//!
//! This library provides a complete parser and interpreter for bash scripts.

pub mod ast;
pub mod bash;
pub mod fs;
pub mod interpreter;
pub mod parser;

pub use ast::types::*;
pub use parser::{parse, Parser, ParseException};
pub use bash::Bash;
pub use fs::{FileSystem, InMemoryFs};
