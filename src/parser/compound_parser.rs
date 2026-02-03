//! Compound Command Parser
//!
//! Handles parsing of compound commands: if, for, while, until, case, subshell, group.

use crate::ast::types::{
    ArithmeticExpressionNode, CaseItemNode, CaseNode, CaseTerminator, CStyleForNode, ForNode,
    GroupNode, IfClause, IfNode, RedirectionNode, StatementNode, SubshellNode, UntilNode,
    WhileNode, WordNode, AST,
};
use crate::parser::arithmetic_parser::parse_arithmetic_expression;
use crate::parser::lexer::{Token, TokenType};

/// Token information for compound parsing
#[derive(Debug, Clone)]
pub struct CompoundToken {
    pub token_type: TokenType,
    pub value: String,
    pub line: usize,
}

/// Context for compound command parsing
pub struct CompoundParserContext<'a> {
    pub check: &'a dyn Fn(TokenType) -> bool,
    pub check_multi: &'a dyn Fn(&[TokenType]) -> bool,
    pub advance: &'a dyn Fn() -> CompoundToken,
    pub expect: &'a dyn Fn(TokenType) -> CompoundToken,
    pub is_word: &'a dyn Fn() -> bool,
    pub peek: &'a dyn Fn(isize) -> CompoundToken,
    pub skip_newlines: &'a dyn Fn(),
    pub skip_separators: &'a dyn Fn(bool),
    pub parse_compound_list: &'a dyn Fn() -> Vec<StatementNode>,
    pub parse_word: &'a dyn Fn() -> WordNode,
    pub parse_statement: &'a dyn Fn() -> Option<StatementNode>,
    pub parse_optional_redirections: &'a dyn Fn() -> Vec<RedirectionNode>,
    pub get_pos: &'a dyn Fn() -> usize,
    pub check_iteration_limit: &'a dyn Fn(),
    pub error: &'a dyn Fn(&str),
}

/// Options for parsing compound commands
#[derive(Default)]
pub struct ParseOptions {
    pub skip_redirections: bool,
}

/// Parse an if statement
pub fn parse_if(ctx: &CompoundParserContext, options: &ParseOptions) -> IfNode {
    (ctx.expect)(TokenType::If);
    let mut clauses: Vec<IfClause> = Vec::new();

    // Parse if condition
    let condition = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::Then);
    let body = (ctx.parse_compound_list)();

    // Empty body is a syntax error in bash
    if body.is_empty() {
        let next_tok = if (ctx.check)(TokenType::Fi) {
            "fi"
        } else if (ctx.check)(TokenType::Else) {
            "else"
        } else if (ctx.check)(TokenType::Elif) {
            "elif"
        } else {
            "fi"
        };
        (ctx.error)(&format!("syntax error near unexpected token `{}'", next_tok));
        unreachable!();
    }
    clauses.push(IfClause { condition, body });

    // Parse elif clauses
    while (ctx.check)(TokenType::Elif) {
        (ctx.advance)();
        let elif_condition = (ctx.parse_compound_list)();
        (ctx.expect)(TokenType::Then);
        let elif_body = (ctx.parse_compound_list)();

        // Empty elif body is a syntax error
        if elif_body.is_empty() {
            let next_tok = if (ctx.check)(TokenType::Fi) {
                "fi"
            } else if (ctx.check)(TokenType::Else) {
                "else"
            } else if (ctx.check)(TokenType::Elif) {
                "elif"
            } else {
                "fi"
            };
            (ctx.error)(&format!("syntax error near unexpected token `{}'", next_tok));
            unreachable!();
        }
        clauses.push(IfClause {
            condition: elif_condition,
            body: elif_body,
        });
    }

    // Parse else clause
    let mut else_body: Option<Vec<StatementNode>> = None;
    if (ctx.check)(TokenType::Else) {
        (ctx.advance)();
        let body = (ctx.parse_compound_list)();
        // Empty else body is a syntax error
        if body.is_empty() {
            (ctx.error)("syntax error near unexpected token `fi'");
            unreachable!();
        }
        else_body = Some(body);
    }

    (ctx.expect)(TokenType::Fi);

    // Parse optional redirections (unless skipped for function body)
    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::if_node(clauses, else_body, redirections)
}

/// Parse a for loop (regular or C-style)
pub fn parse_for(
    ctx: &CompoundParserContext,
    options: &ParseOptions,
) -> ForOrCStyleFor {
    let for_token = (ctx.expect)(TokenType::For);

    // Check for C-style for: for (( ... ))
    if (ctx.check)(TokenType::DParenStart) {
        return ForOrCStyleFor::CStyle(parse_c_style_for(ctx, options, Some(for_token.line)));
    }

    // Regular for: for VAR in WORDS
    // The variable can be NAME, IN, or even invalid names like "i.j"
    // Invalid names are validated at runtime to match bash behavior
    if !(ctx.is_word)() {
        (ctx.error)("Expected variable name in for loop");
        unreachable!();
    }
    let var_token = (ctx.advance)();
    let variable = var_token.value;

    let mut words: Option<Vec<WordNode>> = None;

    // Check for 'in' keyword
    (ctx.skip_newlines)();
    if (ctx.check)(TokenType::In) {
        (ctx.advance)();
        let mut word_list = Vec::new();

        // Parse words until ; or newline
        while !(ctx.check_multi)(&[
            TokenType::Semicolon,
            TokenType::Newline,
            TokenType::Do,
            TokenType::Eof,
        ]) {
            if (ctx.is_word)() {
                word_list.push((ctx.parse_word)());
            } else {
                break;
            }
        }
        words = Some(word_list);
    }

    // Skip separator
    if (ctx.check)(TokenType::Semicolon) {
        (ctx.advance)();
    }
    (ctx.skip_newlines)();

    (ctx.expect)(TokenType::Do);
    let body = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::Done);

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    ForOrCStyleFor::Regular(AST::for_node(variable, words, body, redirections))
}

/// Result type for parse_for which can return either a regular for or C-style for
pub enum ForOrCStyleFor {
    Regular(ForNode),
    CStyle(CStyleForNode),
}

/// Parse a C-style for loop
fn parse_c_style_for(
    ctx: &CompoundParserContext,
    options: &ParseOptions,
    start_line: Option<usize>,
) -> CStyleForNode {
    (ctx.expect)(TokenType::DParenStart);

    // Parse init; cond; step
    // This is a simplified parser - we read until ; or ))
    let mut init: Option<ArithmeticExpressionNode> = None;
    let mut condition: Option<ArithmeticExpressionNode> = None;
    let mut update: Option<ArithmeticExpressionNode> = None;

    let mut parts = vec![String::new(), String::new(), String::new()];
    let mut part_idx = 0;
    let mut depth = 0;

    // Read until ))
    while !(ctx.check_multi)(&[TokenType::DParenEnd, TokenType::Eof]) {
        let token = (ctx.advance)();
        if token.token_type == TokenType::Semicolon && depth == 0 {
            part_idx += 1;
            if part_idx > 2 {
                break;
            }
        } else {
            if token.value == "(" {
                depth += 1;
            }
            if token.value == ")" {
                depth -= 1;
            }
            parts[part_idx].push_str(&token.value);
        }
    }

    (ctx.expect)(TokenType::DParenEnd);

    if !parts[0].trim().is_empty() {
        init = Some(parse_arithmetic_expression(parts[0].trim()));
    }
    if !parts[1].trim().is_empty() {
        condition = Some(parse_arithmetic_expression(parts[1].trim()));
    }
    if !parts[2].trim().is_empty() {
        update = Some(parse_arithmetic_expression(parts[2].trim()));
    }

    (ctx.skip_newlines)();
    if (ctx.check)(TokenType::Semicolon) {
        (ctx.advance)();
    }
    (ctx.skip_newlines)();

    // Accept either do...done or { } for body (bash allows both)
    let body = if (ctx.check)(TokenType::LBrace) {
        (ctx.advance)();
        let body = (ctx.parse_compound_list)();
        (ctx.expect)(TokenType::RBrace);
        body
    } else {
        (ctx.expect)(TokenType::Do);
        let body = (ctx.parse_compound_list)();
        (ctx.expect)(TokenType::Done);
        body
    };

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    CStyleForNode {
        init,
        condition,
        update,
        body,
        redirections,
        line: start_line,
    }
}

/// Parse a while loop
pub fn parse_while(ctx: &CompoundParserContext, options: &ParseOptions) -> WhileNode {
    (ctx.expect)(TokenType::While);
    let condition = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::Do);
    let body = (ctx.parse_compound_list)();

    // Empty body is a syntax error in bash
    if body.is_empty() {
        (ctx.error)("syntax error near unexpected token `done'");
        unreachable!();
    }
    (ctx.expect)(TokenType::Done);

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::while_node(condition, body, redirections)
}

/// Parse an until loop
pub fn parse_until(ctx: &CompoundParserContext, options: &ParseOptions) -> UntilNode {
    (ctx.expect)(TokenType::Until);
    let condition = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::Do);
    let body = (ctx.parse_compound_list)();

    // Empty body is a syntax error in bash
    if body.is_empty() {
        (ctx.error)("syntax error near unexpected token `done'");
        unreachable!();
    }
    (ctx.expect)(TokenType::Done);

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::until_node(condition, body, redirections)
}

/// Parse a case statement
pub fn parse_case(ctx: &CompoundParserContext, options: &ParseOptions) -> CaseNode {
    (ctx.expect)(TokenType::Case);

    if !(ctx.is_word)() {
        (ctx.error)("Expected word after 'case'");
        unreachable!();
    }
    let word = (ctx.parse_word)();

    (ctx.skip_newlines)();
    (ctx.expect)(TokenType::In);
    (ctx.skip_newlines)();

    let mut items: Vec<CaseItemNode> = Vec::new();

    // Parse case items
    while !(ctx.check_multi)(&[TokenType::Esac, TokenType::Eof]) {
        (ctx.check_iteration_limit)();
        let pos_before = (ctx.get_pos)();

        if let Some(item) = parse_case_item(ctx) {
            items.push(item);
        }
        (ctx.skip_newlines)();

        // Safety: if we didn't advance and didn't get an item, break to prevent infinite loop
        if (ctx.get_pos)() == pos_before {
            break;
        }
    }

    (ctx.expect)(TokenType::Esac);

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::case_node(word, items, redirections)
}

/// Parse a single case item
fn parse_case_item(ctx: &CompoundParserContext) -> Option<CaseItemNode> {
    // Skip optional (
    if (ctx.check)(TokenType::LParen) {
        (ctx.advance)();
    }

    let mut patterns: Vec<WordNode> = Vec::new();

    // Parse patterns separated by |
    while (ctx.is_word)() {
        patterns.push((ctx.parse_word)());

        if (ctx.check)(TokenType::Pipe) {
            (ctx.advance)();
        } else {
            break;
        }
    }

    if patterns.is_empty() {
        return None;
    }

    // Expect )
    (ctx.expect)(TokenType::RParen);
    (ctx.skip_newlines)();

    // Parse body
    let mut body: Vec<StatementNode> = Vec::new();
    while !(ctx.check_multi)(&[
        TokenType::DSemi,
        TokenType::SemiAnd,
        TokenType::SemiSemiAnd,
        TokenType::Esac,
        TokenType::Eof,
    ]) {
        (ctx.check_iteration_limit)();

        // Check if we're looking at the start of another case pattern (word followed by ))
        // This handles the syntax error case of empty actions like: a) b) echo A ;;
        if (ctx.is_word)() && (ctx.peek)(1).token_type == TokenType::RParen {
            // This looks like another case pattern starting without a terminator
            // This is a syntax error in bash
            (ctx.error)("syntax error near unexpected token `)'");
            unreachable!();
        }
        // Also check for optional ( before pattern
        if (ctx.check)(TokenType::LParen) && (ctx.peek)(1).token_type == TokenType::Word {
            let next_val = (ctx.peek)(1).value.clone();
            (ctx.error)(&format!("syntax error near unexpected token `{}'", next_val));
            unreachable!();
        }

        let pos_before = (ctx.get_pos)();
        if let Some(stmt) = (ctx.parse_statement)() {
            body.push(stmt);
        }
        // Don't skip case terminators (;;, ;&, ;;&) - we need to see them
        (ctx.skip_separators)(false);

        // If we didn't advance and didn't get a statement, break to avoid infinite loop
        if (ctx.get_pos)() == pos_before {
            break;
        }
    }

    // Parse terminator
    let terminator = if (ctx.check)(TokenType::DSemi) {
        (ctx.advance)();
        CaseTerminator::DoubleSemi
    } else if (ctx.check)(TokenType::SemiAnd) {
        (ctx.advance)();
        CaseTerminator::SemiAnd
    } else if (ctx.check)(TokenType::SemiSemiAnd) {
        (ctx.advance)();
        CaseTerminator::SemiSemiAnd
    } else {
        CaseTerminator::DoubleSemi
    };

    Some(AST::case_item(patterns, body, terminator))
}

/// Parse a subshell
pub fn parse_subshell(ctx: &CompoundParserContext, options: &ParseOptions) -> SubshellNode {
    (ctx.expect)(TokenType::LParen);

    let body = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::RParen);

    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::subshell(body, redirections)
}

/// Parse a command group
pub fn parse_group(ctx: &CompoundParserContext, options: &ParseOptions) -> GroupNode {
    (ctx.expect)(TokenType::LBrace);
    let body = (ctx.parse_compound_list)();
    (ctx.expect)(TokenType::RBrace);

    // For function bodies, redirections are parsed by the function definition, not the group
    let redirections = if options.skip_redirections {
        Vec::new()
    } else {
        (ctx.parse_optional_redirections)()
    };

    AST::group(body, redirections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_options_default() {
        let options = ParseOptions::default();
        assert!(!options.skip_redirections);
    }
}
