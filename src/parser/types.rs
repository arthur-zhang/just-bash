//! Parser Types and Constants
//!
//! Shared types, interfaces, and constants used across parser modules.

use std::fmt;
use thiserror::Error;
use crate::parser::lexer::{Token, TokenType};

// Parser limits to prevent hangs and resource exhaustion
pub const MAX_INPUT_SIZE: usize = 1_000_000; // 1MB max input
pub const MAX_TOKENS: usize = 100_000; // Max tokens to parse
pub const MAX_PARSE_ITERATIONS: usize = 1_000_000; // Max iterations in parsing loops
pub const MAX_PARSER_DEPTH: usize = 200; // Max recursion depth for nested constructs

/// Check if a token type is a redirection token
pub fn is_redirection_token(t: TokenType) -> bool {
    matches!(
        t,
        TokenType::Less
            | TokenType::Great
            | TokenType::DLess
            | TokenType::DGreat
            | TokenType::LessAnd
            | TokenType::GreatAnd
            | TokenType::LessGreat
            | TokenType::DLessDash
            | TokenType::Clobber
            | TokenType::TLess
            | TokenType::AndGreat
            | TokenType::AndDGreat
    )
}

/// Check if a token type can follow a number in a redirection
pub fn is_redirection_after_number(t: TokenType) -> bool {
    matches!(
        t,
        TokenType::Less
            | TokenType::Great
            | TokenType::DLess
            | TokenType::DGreat
            | TokenType::LessAnd
            | TokenType::GreatAnd
            | TokenType::LessGreat
            | TokenType::DLessDash
            | TokenType::Clobber
            | TokenType::TLess
    )
}

/// Check if a token type can follow an FD variable in a redirection
pub fn is_redirection_after_fd_variable(t: TokenType) -> bool {
    matches!(
        t,
        TokenType::Less
            | TokenType::Great
            | TokenType::DLess
            | TokenType::DGreat
            | TokenType::LessAnd
            | TokenType::GreatAnd
            | TokenType::LessGreat
            | TokenType::DLessDash
            | TokenType::Clobber
            | TokenType::TLess
            | TokenType::AndGreat
            | TokenType::AndDGreat
    )
}

/// Tokens that are invalid inside array literals
pub fn is_invalid_array_token(t: TokenType) -> bool {
    matches!(
        t,
        TokenType::Amp
            | TokenType::Pipe
            | TokenType::PipeAmp
            | TokenType::Semicolon
            | TokenType::AndAnd
            | TokenType::OrOr
            | TokenType::DSemi
            | TokenType::SemiAnd
            | TokenType::SemiSemiAnd
    )
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub token: Option<Token>,
}

#[derive(Debug, Error)]
pub struct ParseException {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub token: Option<Token>,
}

impl fmt::Display for ParseException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse error at {}:{}: {}", self.line, self.column, self.message)
    }
}

impl ParseException {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
            token: None,
        }
    }

    pub fn with_token(message: impl Into<String>, line: usize, column: usize, token: Token) -> Self {
        Self {
            message: message.into(),
            line,
            column,
            token: Some(token),
        }
    }
}
