//! Command Parser
//!
//! Handles parsing of simple commands, redirections, and assignments.

use crate::ast::types::{
    AssignmentNode, HereDocNode, RedirectionNode, RedirectionOperator, RedirectionTarget,
    SimpleCommandNode, WordNode, AST,
};
use crate::parser::lexer::TokenType;
use crate::parser::types::{
    is_invalid_array_token, is_redirection_after_fd_variable, is_redirection_after_number,
    is_redirection_token,
};
use crate::parser::word_parser;

/// Token information for command parsing
#[derive(Debug, Clone)]
pub struct CommandToken {
    pub token_type: TokenType,
    pub value: String,
    pub quoted: bool,
    pub single_quoted: bool,
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// Context for command parsing
pub struct CommandParserContext<'a> {
    pub current: &'a dyn Fn() -> CommandToken,
    pub peek: &'a dyn Fn(isize) -> CommandToken,
    pub advance: &'a dyn Fn() -> CommandToken,
    pub expect: &'a dyn Fn(TokenType) -> CommandToken,
    pub check: &'a dyn Fn(TokenType) -> bool,
    pub check_multi: &'a dyn Fn(&[TokenType]) -> bool,
    pub is_word: &'a dyn Fn() -> bool,
    pub is_statement_end: &'a dyn Fn() -> bool,
    pub skip_newlines: &'a dyn Fn(),
    pub parse_word: &'a dyn Fn() -> WordNode,
    pub parse_word_from_string: &'a dyn Fn(&str, bool, bool, bool) -> WordNode,
    pub add_pending_heredoc:
        &'a dyn Fn(&RedirectionNode, &str, bool, bool),
    pub check_iteration_limit: &'a dyn Fn(),
    pub error: &'a dyn Fn(&str),
}

/// Check if the current token is a redirection
pub fn is_redirection(ctx: &CommandParserContext) -> bool {
    let current_token = (ctx.current)();
    let t = current_token.token_type;

    // Check for number followed by redirection operator
    // Only treat as fd redirection if the number is immediately adjacent to the operator
    if t == TokenType::Number {
        let next_token = (ctx.peek)(1);
        // Check if tokens are adjacent (no space between them)
        if current_token.end != next_token.start {
            return false;
        }
        return is_redirection_after_number(next_token.token_type);
    }

    // Check for FD variable followed by redirection operator
    // e.g., {fd}>file allocates an FD and stores it in variable
    if t == TokenType::FdVariable {
        let next_token = (ctx.peek)(1);
        return is_redirection_after_fd_variable(next_token.token_type);
    }

    is_redirection_token(t)
}

/// Parse a redirection
pub fn parse_redirection(ctx: &CommandParserContext) -> RedirectionNode {
    let mut fd: Option<i32> = None;
    let mut fd_variable: Option<String> = None;

    // Parse optional file descriptor number
    if (ctx.check)(TokenType::Number) {
        fd = Some((ctx.advance)().value.parse().unwrap_or(0));
    }
    // Parse FD variable syntax: {varname}>file
    else if (ctx.check)(TokenType::FdVariable) {
        fd_variable = Some((ctx.advance)().value);
    }

    // Parse operator
    let op_token = (ctx.advance)();
    let operator = word_parser::token_to_redirect_op(op_token.token_type);

    // Handle here-documents
    if op_token.token_type == TokenType::DLess || op_token.token_type == TokenType::DLessDash {
        return parse_heredoc_start(
            ctx,
            operator,
            fd,
            op_token.token_type == TokenType::DLessDash,
        );
    }

    // Parse target
    if !(ctx.is_word)() {
        (ctx.error)("Expected redirection target");
        unreachable!();
    }

    let target = (ctx.parse_word)();
    AST::redirection(operator, RedirectionTarget::Word(target), fd, fd_variable)
}

/// Parse the start of a here-document
fn parse_heredoc_start(
    ctx: &CommandParserContext,
    _operator: RedirectionOperator,
    fd: Option<i32>,
    strip_tabs: bool,
) -> RedirectionNode {
    // Parse delimiter
    if !(ctx.is_word)() {
        (ctx.error)("Expected here-document delimiter");
        unreachable!();
    }

    let delim_token = (ctx.advance)();
    let mut delimiter = delim_token.value.clone();
    let quoted = delim_token.quoted;

    // Remove quotes from delimiter
    if delimiter.starts_with('\'') && delimiter.ends_with('\'') && delimiter.len() >= 2 {
        delimiter = delimiter[1..delimiter.len() - 1].to_string();
    } else if delimiter.starts_with('"') && delimiter.ends_with('"') && delimiter.len() >= 2 {
        delimiter = delimiter[1..delimiter.len() - 1].to_string();
    }

    // Create placeholder redirection
    let heredoc_op = if strip_tabs {
        RedirectionOperator::DLessDash
    } else {
        RedirectionOperator::DLess
    };

    let redirect = AST::redirection(
        heredoc_op,
        RedirectionTarget::HereDoc(HereDocNode {
            delimiter: delimiter.clone(),
            content: AST::word(vec![]),
            strip_tabs,
            quoted,
        }),
        fd,
        None,
    );

    // Register pending here-document
    (ctx.add_pending_heredoc)(&redirect, &delimiter, strip_tabs, quoted);

    redirect
}

/// Parse a simple command
pub fn parse_simple_command(ctx: &CommandParserContext) -> SimpleCommandNode {
    // Capture line number at the start of the command for $LINENO
    let start_line = (ctx.current)().line;

    let mut assignments: Vec<AssignmentNode> = Vec::new();
    let mut name: Option<WordNode> = None;
    let mut args: Vec<WordNode> = Vec::new();
    let mut redirections: Vec<RedirectionNode> = Vec::new();

    // Parse prefix assignments and redirections (they can be interleaved)
    // e.g., FOO=foo >file BAR=bar cmd
    while (ctx.check)(TokenType::AssignmentWord) || is_redirection(ctx) {
        (ctx.check_iteration_limit)();
        if (ctx.check)(TokenType::AssignmentWord) {
            assignments.push(parse_assignment(ctx));
        } else {
            redirections.push(parse_redirection(ctx));
        }
    }

    // Parse command name
    if (ctx.is_word)() {
        name = Some((ctx.parse_word)());
    } else if !assignments.is_empty()
        && ((ctx.check)(TokenType::DBrackStart) || (ctx.check)(TokenType::DParenStart))
    {
        // When we have prefix assignments (e.g., FOO=bar [[ ... ]]), compound command
        // keywords are NOT recognized as keywords - they're treated as command names.
        let token = (ctx.advance)();
        name = Some(AST::word(vec![AST::literal(&token.value)]));
    }

    // Parse arguments and redirections
    // RBRACE (}) can be an argument in command position (e.g., "echo }"), so we handle it specially.
    while (!(ctx.is_statement_end)() || (ctx.check)(TokenType::RBrace))
        && !(ctx.check_multi)(&[TokenType::Pipe, TokenType::PipeAmp])
    {
        (ctx.check_iteration_limit)();

        if is_redirection(ctx) {
            redirections.push(parse_redirection(ctx));
        } else if (ctx.check)(TokenType::RBrace) {
            // } can be an argument like "echo }" - parse it as a word
            let token = (ctx.advance)();
            args.push((ctx.parse_word_from_string)(&token.value, false, false, false));
        } else if (ctx.check)(TokenType::LBrace) {
            // { can be an argument like "type -t {" - parse it as a word
            let token = (ctx.advance)();
            args.push((ctx.parse_word_from_string)(&token.value, false, false, false));
        } else if (ctx.check)(TokenType::DBrackEnd) {
            // ]] can be an argument when [[ is parsed as a regular command
            let token = (ctx.advance)();
            args.push((ctx.parse_word_from_string)(&token.value, false, false, false));
        } else if (ctx.is_word)() {
            args.push((ctx.parse_word)());
        } else if (ctx.check)(TokenType::AssignmentWord) {
            // Assignment words after command name are treated as arguments
            // (for local, export, declare, etc.)
            let token = (ctx.advance)();
            let token_value = token.value.clone();

            // Check if this is an array assignment: name=( or name=(
            let ends_with_eq = token_value.ends_with('=');
            let ends_with_eq_paren = token_value.ends_with("=(");

            if (ends_with_eq || ends_with_eq_paren)
                && (ends_with_eq_paren || (ctx.check)(TokenType::LParen))
            {
                // Parse as array assignment for declare/local/export/typeset/readonly
                let base_name = if ends_with_eq_paren {
                    &token_value[..token_value.len() - 2]
                } else {
                    &token_value[..token_value.len() - 1]
                };
                if !ends_with_eq_paren {
                    (ctx.expect)(TokenType::LParen);
                }
                let elements = parse_array_elements(ctx);
                (ctx.expect)(TokenType::RParen);

                // Build the array assignment string: name=(elem1 elem2 ...)
                let elem_strings: Vec<String> =
                    elements.iter().map(word_parser::word_to_string).collect();
                let array_str = format!("{}=({})", base_name, elem_strings.join(" "));
                args.push((ctx.parse_word_from_string)(&array_str, false, false, false));
            } else {
                args.push((ctx.parse_word_from_string)(
                    &token_value,
                    token.quoted,
                    token.single_quoted,
                    false,
                ));
            }
        } else if (ctx.check)(TokenType::LParen) {
            // Bare ( in argument position is a syntax error (e.g., "echo a(b)")
            (ctx.error)("syntax error near unexpected token `('");
            unreachable!();
        } else {
            break;
        }
    }

    let mut node = AST::simple_command(name, args, assignments, redirections);
    node.line = Some(start_line);
    node
}

/// Parse an assignment
fn parse_assignment(ctx: &CommandParserContext) -> AssignmentNode {
    let token = (ctx.expect)(TokenType::AssignmentWord);
    let value = token.value.clone();

    // Parse VAR=value, VAR+=value, or VAR[subscript]=value, VAR[subscript]+=value
    // Handle nested brackets in subscript: a[a[0]]=value

    // Find the variable name
    let name_end = value
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();

    if name_end == 0 || !value.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
    {
        (ctx.error)(&format!("Invalid assignment: {}", value));
        unreachable!();
    }

    let name = &value[..name_end];
    let mut subscript: Option<String> = None;
    let mut pos = name_end;
    let chars: Vec<char> = value.chars().collect();

    // Check for array subscript with nested brackets
    if chars.get(pos) == Some(&'[') {
        let mut depth = 0;
        let subscript_start = pos + 1;
        while pos < chars.len() {
            if chars[pos] == '[' {
                depth += 1;
            } else if chars[pos] == ']' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            pos += 1;
        }
        if depth != 0 {
            (ctx.error)(&format!("Invalid assignment: {}", value));
            unreachable!();
        }
        subscript = Some(chars[subscript_start..pos].iter().collect());
        pos += 1; // skip closing ]
    }

    // Check for += or =
    let append = chars.get(pos) == Some(&'+');
    if append {
        pos += 1;
    }
    if chars.get(pos) != Some(&'=') {
        (ctx.error)(&format!("Invalid assignment: {}", value));
        unreachable!();
    }
    pos += 1; // skip =

    let value_str: String = chars[pos..].iter().collect();

    // Check for array assignment: VAR=(...)
    if value_str == "(" {
        let elements = parse_array_elements(ctx);
        (ctx.expect)(TokenType::RParen);
        // If subscript is defined, include it in name so runtime can detect the error
        let assign_name = if let Some(ref sub) = subscript {
            format!("{}[{}]", name, sub)
        } else {
            name.to_string()
        };
        return AST::assignment(assign_name, None, append, Some(elements));
    }

    // Check for adjacent LPAREN: a=() with no space
    if value_str.is_empty() && (ctx.check)(TokenType::LParen) {
        let current_token = (ctx.current)();
        // Only allow if LPAREN is immediately after the assignment word
        if token.end == current_token.start {
            (ctx.advance)(); // consume LPAREN
            let elements = parse_array_elements(ctx);
            (ctx.expect)(TokenType::RParen);
            let assign_name = if let Some(ref sub) = subscript {
                format!("{}[{}]", name, sub)
            } else {
                name.to_string()
            };
            return AST::assignment(assign_name, None, append, Some(elements));
        }
        // Space between = and ( is a syntax error - let the parser handle it
    }

    // Regular assignment (may include subscript)
    let word_value = if !value_str.is_empty() {
        Some((ctx.parse_word_from_string)(
            &value_str,
            token.quoted,
            token.single_quoted,
            true, // isAssignment=true allows tilde expansion after :
        ))
    } else {
        None
    };

    // If we have a subscript, embed it in the name (e.g., "a[0]")
    let assign_name = if let Some(ref sub) = subscript {
        format!("{}[{}]", name, sub)
    } else {
        name.to_string()
    };

    AST::assignment(assign_name, word_value, append, None)
}

/// Parse array elements
fn parse_array_elements(ctx: &CommandParserContext) -> Vec<WordNode> {
    let mut elements: Vec<WordNode> = Vec::new();
    (ctx.skip_newlines)();

    while !(ctx.check_multi)(&[TokenType::RParen, TokenType::Eof]) {
        (ctx.check_iteration_limit)();
        if (ctx.is_word)() {
            elements.push((ctx.parse_word)());
        } else if is_invalid_array_token((ctx.current)().token_type) {
            // Invalid tokens inside array literals - throw syntax error
            let current = (ctx.current)();
            (ctx.error)(&format!(
                "syntax error near unexpected token `{}'",
                current.value
            ));
            unreachable!();
        } else {
            // Skip other unexpected tokens to prevent infinite loop
            (ctx.advance)();
        }
        (ctx.skip_newlines)();
    }

    elements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_redirection_token() {
        assert!(is_redirection_token(TokenType::Less));
        assert!(is_redirection_token(TokenType::Great));
        assert!(is_redirection_token(TokenType::DGreat));
        assert!(!is_redirection_token(TokenType::Word));
    }
}
