//! just-bash - A simulated bash environment parser
//!
//! This library provides a complete parser for bash scripts, producing an AST
//! that can be used for interpretation or analysis.

pub mod ast;
pub mod parser;

pub use ast::types::*;
pub use parser::{parse, Parser, ParseException};
