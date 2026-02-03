//! Abstract Syntax Tree (AST) Types for Bash
//!
//! This module defines the complete AST structure for bash scripts.
//! The design follows the actual bash grammar while being Rust-idiomatic.
//!
//! Architecture:
//!   Input → Lexer → Parser → AST → Expander → Interpreter → Output

pub mod types;
