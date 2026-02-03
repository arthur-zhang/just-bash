//! Parser module for bash scripts
//!
//! This module contains the lexer and parser for bash scripts.

pub mod types;
pub mod lexer;
pub mod arithmetic_primaries;
pub mod arithmetic_parser;
pub mod word_parser;
pub mod expansion_parser;
pub mod parser_substitution;
pub mod conditional_parser;
pub mod compound_parser;
pub mod command_parser;
pub mod parser;

// Re-exports
pub use types::ParseException;
pub use lexer::{Lexer, Token, TokenType, LexerError};
pub use arithmetic_parser::parse_arithmetic_expression;
pub use parser::{parse, Parser};
