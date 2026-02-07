/// AWK Parser
///
/// Recursive descent parser that builds an AST from tokens.
/// Ported from the TypeScript implementation.

use super::lexer::tokenize;
use super::types::{
    AssignOp, AwkExpr, AwkFunctionDef, AwkPattern, AwkProgram, AwkRule, AwkStmt,
    BinaryOp, RedirectInfo, RedirectType, Token, TokenType, UnaryOp,
};

// ─── Parser Struct ───────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ─── Helper Methods ──────────────────────────────────────

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens.last().expect("Token stream should have at least EOF")
        })
    }

    fn advance(&mut self) -> &Token {
        let token = self.current();
        let token_ptr = token as *const Token;
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        unsafe { &*token_ptr }
    }

    fn match_token(&self, types: &[TokenType]) -> bool {
        types.contains(&self.current().token_type)
    }

    fn check(&self, token_type: TokenType) -> bool {
        self.current().token_type == token_type
    }

    fn expect(&mut self, token_type: TokenType) -> Result<&Token, String> {
        if !self.check(token_type.clone()) {
            let tok = self.current();
            return Err(format!(
                "Expected {:?}, got {:?} at line {}:{}",
                token_type, tok.token_type, tok.line, tok.column
            ));
        }
        Ok(self.advance())
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) {
            self.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while self.check(TokenType::Newline) || self.check(TokenType::Semicolon) {
            self.advance();
        }
    }

    fn peek(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).unwrap_or_else(|| {
            self.tokens.last().expect("Token stream should have at least EOF")
        })
    }

    // ─── Program Parsing ─────────────────────────────────────

    fn parse_program(&mut self) -> Result<AwkProgram, String> {
        let mut functions = Vec::new();
        let mut rules = Vec::new();

        self.skip_newlines();

        while !self.check(TokenType::Eof) {
            self.skip_newlines();

            if self.check(TokenType::Eof) {
                break;
            }

            if self.check(TokenType::Function) {
                functions.push(self.parse_function()?);
            } else {
                rules.push(self.parse_rule()?);
            }

            self.skip_terminators();
        }

        Ok(AwkProgram { functions, rules })
    }

    fn parse_function(&mut self) -> Result<AwkFunctionDef, String> {
        self.expect(TokenType::Function)?;
        let name = self.expect(TokenType::Ident)?.value.clone();
        self.expect(TokenType::LParen)?;

        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.expect(TokenType::Ident)?.value.clone());
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.expect(TokenType::Ident)?.value.clone());
            }
        }

        self.expect(TokenType::RParen)?;
        self.skip_newlines();
        let body = self.parse_block()?;

        Ok(AwkFunctionDef { name, params, body })
    }

    fn parse_rule(&mut self) -> Result<AwkRule, String> {
        let mut pattern: Option<AwkPattern> = None;

        // Check for BEGIN/END
        if self.check(TokenType::Begin) {
            self.advance();
            pattern = Some(AwkPattern::Begin);
        } else if self.check(TokenType::End) {
            self.advance();
            pattern = Some(AwkPattern::End);
        } else if self.check(TokenType::LBrace) {
            // No pattern, just action
            pattern = None;
        } else if self.check(TokenType::Regex) {
            // Regex pattern - but check if it's part of a larger expression
            let regex_value = self.advance().value.clone();

            // Check if this regex is followed by && or || (compound pattern)
            if self.check(TokenType::And) || self.check(TokenType::Or) {
                // Convert regex to $0 ~ /regex/ expression and parse as compound expression
                let regex_expr = AwkExpr::BinaryOp {
                    operator: BinaryOp::MatchOp,
                    left: Box::new(AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))),
                    right: Box::new(AwkExpr::RegexLiteral(regex_value)),
                };
                let full_expr = self.parse_logical_or_rest(regex_expr)?;
                pattern = Some(AwkPattern::Expression(full_expr));
            } else {
                let pat = AwkPattern::Regex(regex_value);

                // Check for range pattern
                if self.check(TokenType::Comma) {
                    self.advance();
                    let end_pattern = if self.check(TokenType::Regex) {
                        let end_regex = self.advance().value.clone();
                        AwkPattern::Regex(end_regex)
                    } else {
                        AwkPattern::Expression(self.parse_expression()?)
                    };
                    pattern = Some(AwkPattern::Range {
                        start: Box::new(pat),
                        end: Box::new(end_pattern),
                    });
                } else {
                    pattern = Some(pat);
                }
            }
        } else {
            // Expression pattern
            let expr = self.parse_expression()?;
            let pat = AwkPattern::Expression(expr);

            // Check for range pattern
            if self.check(TokenType::Comma) {
                self.advance();
                let end_pattern = if self.check(TokenType::Regex) {
                    let end_regex = self.advance().value.clone();
                    AwkPattern::Regex(end_regex)
                } else {
                    AwkPattern::Expression(self.parse_expression()?)
                };
                pattern = Some(AwkPattern::Range {
                    start: Box::new(pat),
                    end: Box::new(end_pattern),
                });
            } else {
                pattern = Some(pat);
            }
        }

        self.skip_newlines();

        // Parse action block if present
        let action = if self.check(TokenType::LBrace) {
            self.parse_block()?
        } else {
            // Default action is print $0
            vec![AwkStmt::Print {
                args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                output: None,
            }]
        };

        Ok(AwkRule { pattern, action })
    }

    fn parse_block(&mut self) -> Result<Vec<AwkStmt>, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();

        let mut statements = Vec::new();

        while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
            statements.push(self.parse_statement()?);
            self.skip_terminators();
        }

        self.expect(TokenType::RBrace)?;
        Ok(statements)
    }

    // ─── Statement Parsing ───────────────────────────────────

    fn parse_statement(&mut self) -> Result<AwkStmt, String> {
        // Empty statement (just semicolon or newline before actual statement)
        if self.check(TokenType::Semicolon) || self.check(TokenType::Newline) {
            self.advance();
            return Ok(AwkStmt::Block(vec![]));
        }

        // Block
        if self.check(TokenType::LBrace) {
            let stmts = self.parse_block()?;
            return Ok(AwkStmt::Block(stmts));
        }

        // If statement
        if self.check(TokenType::If) {
            return self.parse_if();
        }

        // While statement
        if self.check(TokenType::While) {
            return self.parse_while();
        }

        // Do-while statement
        if self.check(TokenType::Do) {
            return self.parse_do_while();
        }

        // For statement
        if self.check(TokenType::For) {
            return self.parse_for();
        }

        // Break
        if self.check(TokenType::Break) {
            self.advance();
            return Ok(AwkStmt::Break);
        }

        // Continue
        if self.check(TokenType::Continue) {
            self.advance();
            return Ok(AwkStmt::Continue);
        }

        // Next
        if self.check(TokenType::Next) {
            self.advance();
            return Ok(AwkStmt::Next);
        }

        // Nextfile
        if self.check(TokenType::NextFile) {
            self.advance();
            return Ok(AwkStmt::NextFile);
        }

        // Exit
        if self.check(TokenType::Exit) {
            self.advance();
            let code = if !self.check(TokenType::Newline)
                && !self.check(TokenType::Semicolon)
                && !self.check(TokenType::RBrace)
                && !self.check(TokenType::Eof)
            {
                Some(self.parse_expression()?)
            } else {
                None
            };
            return Ok(AwkStmt::Exit(code));
        }

        // Return
        if self.check(TokenType::Return) {
            self.advance();
            let value = if !self.check(TokenType::Newline)
                && !self.check(TokenType::Semicolon)
                && !self.check(TokenType::RBrace)
                && !self.check(TokenType::Eof)
            {
                Some(self.parse_expression()?)
            } else {
                None
            };
            return Ok(AwkStmt::Return(value));
        }

        // Delete
        if self.check(TokenType::Delete) {
            self.advance();
            let target = self.parse_primary()?;
            return Ok(AwkStmt::Delete { target });
        }

        // Print
        if self.check(TokenType::Print) {
            return self.parse_print_statement();
        }

        // Printf
        if self.check(TokenType::Printf) {
            return self.parse_printf_statement();
        }

        // Expression statement
        let expr = self.parse_expression()?;
        Ok(AwkStmt::ExprStmt(expr))
    }

    fn parse_if(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::If)?;
        self.expect(TokenType::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenType::RParen)?;
        self.skip_newlines();
        let consequent = Box::new(self.parse_statement()?);
        self.skip_terminators();

        let alternate = if self.check(TokenType::Else) {
            self.advance();
            self.skip_newlines();
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        Ok(AwkStmt::If {
            condition,
            consequent,
            alternate,
        })
    }

    fn parse_while(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::While)?;
        self.expect(TokenType::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenType::RParen)?;
        self.skip_newlines();
        let body = Box::new(self.parse_statement()?);

        Ok(AwkStmt::While { condition, body })
    }

    fn parse_do_while(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::Do)?;
        self.skip_newlines();
        let body = Box::new(self.parse_statement()?);
        self.skip_newlines();
        self.expect(TokenType::While)?;
        self.expect(TokenType::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenType::RParen)?;

        Ok(AwkStmt::DoWhile { body, condition })
    }

    fn parse_for(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::For)?;
        self.expect(TokenType::LParen)?;

        // Check for for-in
        if self.check(TokenType::Ident) {
            let var_name = self.advance().value.clone();
            if self.check(TokenType::In) {
                self.advance();
                let array = self.expect(TokenType::Ident)?.value.clone();
                self.expect(TokenType::RParen)?;
                self.skip_newlines();
                let body = Box::new(self.parse_statement()?);
                return Ok(AwkStmt::ForIn {
                    variable: var_name,
                    array,
                    body,
                });
            }
            // Not for-in, backtrack
            self.pos -= 1;
        }

        // C-style for
        let init = if !self.check(TokenType::Semicolon) {
            Some(Box::new(AwkStmt::ExprStmt(self.parse_expression()?)))
        } else {
            None
        };
        self.expect(TokenType::Semicolon)?;

        let condition = if !self.check(TokenType::Semicolon) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.expect(TokenType::Semicolon)?;

        let update = if !self.check(TokenType::RParen) {
            Some(Box::new(AwkStmt::ExprStmt(self.parse_expression()?)))
        } else {
            None
        };
        self.expect(TokenType::RParen)?;
        self.skip_newlines();

        let body = Box::new(self.parse_statement()?);
        Ok(AwkStmt::For {
            init,
            condition,
            update,
            body,
        })
    }

    // ─── Print Statement Parsing ─────────────────────────────

    fn parse_print_statement(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::Print)?;

        let mut args = Vec::new();

        // Check for empty print (print $0)
        if self.check(TokenType::Newline)
            || self.check(TokenType::Semicolon)
            || self.check(TokenType::RBrace)
            || self.check(TokenType::Pipe)
            || self.check(TokenType::Gt)
            || self.check(TokenType::Append)
        {
            args.push(AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0))));
        } else {
            // Parse print arguments - use parse_print_arg to stop before > and >>
            args.push(self.parse_print_arg()?);
            while self.check(TokenType::Comma) {
                self.advance();
                args.push(self.parse_print_arg()?);
            }
        }

        // Check for output redirection
        let output = self.parse_output_redirect()?;

        Ok(AwkStmt::Print { args, output })
    }

    fn parse_printf_statement(&mut self) -> Result<AwkStmt, String> {
        self.expect(TokenType::Printf)?;

        // AWK supports both:
        //   printf format, arg1, arg2
        //   printf(format, arg1, arg2)
        let has_parens = self.check(TokenType::LParen);
        if has_parens {
            self.advance();
            self.skip_newlines();
        }

        let format = if has_parens {
            self.parse_expression()?
        } else {
            self.parse_print_arg()?
        };

        let mut args = Vec::new();
        while self.check(TokenType::Comma) {
            self.advance();
            if has_parens {
                self.skip_newlines();
            }
            args.push(if has_parens {
                self.parse_expression()?
            } else {
                self.parse_print_arg()?
            });
        }

        if has_parens {
            self.skip_newlines();
            self.expect(TokenType::RParen)?;
        }

        // Check for output redirection
        let output = self.parse_output_redirect()?;

        Ok(AwkStmt::Printf {
            format,
            args,
            output,
        })
    }

    fn parse_output_redirect(&mut self) -> Result<Option<RedirectInfo>, String> {
        if self.check(TokenType::Gt) {
            self.advance();
            let target = self.parse_primary()?;
            Ok(Some(RedirectInfo {
                redirect_type: RedirectType::Write,
                target,
            }))
        } else if self.check(TokenType::Append) {
            self.advance();
            let target = self.parse_primary()?;
            Ok(Some(RedirectInfo {
                redirect_type: RedirectType::Append,
                target,
            }))
        } else if self.check(TokenType::Pipe) {
            self.advance();
            let target = self.parse_primary()?;
            Ok(Some(RedirectInfo {
                redirect_type: RedirectType::Pipe,
                target,
            }))
        } else {
            Ok(None)
        }
    }

    /// Parse a print argument - same as expression but treats > and >> at the TOP LEVEL
    /// (not inside ternary) as redirection rather than comparison operators.
    fn parse_print_arg(&mut self) -> Result<AwkExpr, String> {
        let has_ternary = self.look_ahead_for_ternary();

        if has_ternary {
            // Parse as full ternary with regular comparison (> allowed)
            self.parse_print_assignment(true)
        } else {
            // No ternary - parse without > to leave room for redirection
            self.parse_print_assignment(false)
        }
    }

    /// Look ahead to see if there's a ternary ? operator before the next statement terminator.
    fn look_ahead_for_ternary(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;

        while i < self.tokens.len() {
            let token = &self.tokens[i];

            if token.token_type == TokenType::LParen {
                depth += 1;
            }
            if token.token_type == TokenType::RParen {
                depth -= 1;
            }

            // Found ? at top level - it's a ternary
            if token.token_type == TokenType::Question && depth == 0 {
                return true;
            }

            // Statement terminators - stop looking
            if matches!(
                token.token_type,
                TokenType::Newline
                    | TokenType::Semicolon
                    | TokenType::RBrace
                    | TokenType::Comma
                    | TokenType::Pipe
            ) {
                return false;
            }

            i += 1;
        }

        false
    }

    fn parse_print_assignment(&mut self, allow_gt: bool) -> Result<AwkExpr, String> {
        let expr = if allow_gt {
            self.parse_ternary()?
        } else {
            self.parse_print_or()?
        };

        if self.match_token(&[
            TokenType::Assign,
            TokenType::PlusAssign,
            TokenType::MinusAssign,
            TokenType::StarAssign,
            TokenType::SlashAssign,
            TokenType::PercentAssign,
            TokenType::CaretAssign,
        ]) {
            let op_value = self.advance().value.clone();
            let value = self.parse_print_assignment(allow_gt)?;

            let operator = match op_value.as_str() {
                "=" => AssignOp::Assign,
                "+=" => AssignOp::AddAssign,
                "-=" => AssignOp::SubAssign,
                "*=" => AssignOp::MulAssign,
                "/=" => AssignOp::DivAssign,
                "%=" => AssignOp::ModAssign,
                "^=" => AssignOp::PowAssign,
                _ => return Err(format!("Unknown assignment operator: {}", op_value)),
            };

            return Ok(AwkExpr::Assignment {
                operator,
                target: Box::new(expr),
                value: Box::new(value),
            });
        }

        Ok(expr)
    }

    fn parse_print_or(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_print_and()?;
        while self.check(TokenType::Or) {
            self.advance();
            let right = self.parse_print_and()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_print_and(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_print_in()?;
        while self.check(TokenType::And) {
            self.advance();
            let right = self.parse_print_in()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_print_in(&mut self) -> Result<AwkExpr, String> {
        let left = self.parse_print_concatenation()?;

        if self.check(TokenType::In) {
            self.advance();
            let array = self.expect(TokenType::Ident)?.value.clone();
            return Ok(AwkExpr::InExpr {
                key: Box::new(left),
                array,
            });
        }

        Ok(left)
    }

    fn parse_print_concatenation(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_print_match()?;

        while self.can_start_expression() && !self.is_print_concat_terminator() {
            let right = self.parse_print_match()?;
            left = AwkExpr::Concatenation {
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_print_match(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_print_comparison()?;

        while self.match_token(&[TokenType::Match, TokenType::NotMatch]) {
            let op = if self.advance().token_type == TokenType::Match {
                BinaryOp::MatchOp
            } else {
                BinaryOp::NotMatchOp
            };
            let right = self.parse_print_comparison()?;
            left = AwkExpr::BinaryOp {
                operator: op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Like parse_comparison but doesn't consume > and >> (for print redirection)
    fn parse_print_comparison(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_add_sub()?;

        // Only handle <, <=, >=, ==, != - NOT > or >> (those are redirection)
        while self.match_token(&[
            TokenType::Lt,
            TokenType::Le,
            TokenType::Ge,
            TokenType::Eq,
            TokenType::Ne,
        ]) {
            let op_value = self.advance().value.clone();
            let right = self.parse_add_sub()?;
            let operator = match op_value.as_str() {
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::Le,
                ">=" => BinaryOp::Ge,
                "==" => BinaryOp::Eq,
                "!=" => BinaryOp::Ne,
                _ => return Err(format!("Unknown comparison operator: {}", op_value)),
            };
            left = AwkExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn is_print_concat_terminator(&self) -> bool {
        self.match_token(&[
            TokenType::And,
            TokenType::Or,
            TokenType::Question,
            TokenType::Assign,
            TokenType::PlusAssign,
            TokenType::MinusAssign,
            TokenType::StarAssign,
            TokenType::SlashAssign,
            TokenType::PercentAssign,
            TokenType::CaretAssign,
            TokenType::Comma,
            TokenType::Semicolon,
            TokenType::Newline,
            TokenType::RBrace,
            TokenType::RParen,
            TokenType::RBracket,
            TokenType::Colon,
            TokenType::Pipe,
            TokenType::Append,
            TokenType::Gt, // > is redirection in print context
            TokenType::In,
        ])
    }

    // ─── Expression Parsing (Precedence Climbing) ────────────

    fn parse_expression(&mut self) -> Result<AwkExpr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<AwkExpr, String> {
        let expr = self.parse_ternary()?;

        if self.match_token(&[
            TokenType::Assign,
            TokenType::PlusAssign,
            TokenType::MinusAssign,
            TokenType::StarAssign,
            TokenType::SlashAssign,
            TokenType::PercentAssign,
            TokenType::CaretAssign,
        ]) {
            let op_value = self.advance().value.clone();
            let value = self.parse_assignment()?; // Right associative

            let operator = match op_value.as_str() {
                "=" => AssignOp::Assign,
                "+=" => AssignOp::AddAssign,
                "-=" => AssignOp::SubAssign,
                "*=" => AssignOp::MulAssign,
                "/=" => AssignOp::DivAssign,
                "%=" => AssignOp::ModAssign,
                "^=" => AssignOp::PowAssign,
                _ => return Err(format!("Unknown assignment operator: {}", op_value)),
            };

            return Ok(AwkExpr::Assignment {
                operator,
                target: Box::new(expr),
                value: Box::new(value),
            });
        }

        Ok(expr)
    }

    fn parse_ternary(&mut self) -> Result<AwkExpr, String> {
        let mut expr = self.parse_pipe_getline()?;

        if self.check(TokenType::Question) {
            self.advance();
            let consequent = self.parse_expression()?;
            self.expect(TokenType::Colon)?;
            let alternate = self.parse_expression()?;
            expr = AwkExpr::Ternary {
                condition: Box::new(expr),
                consequent: Box::new(consequent),
                alternate: Box::new(alternate),
            };
        }

        Ok(expr)
    }

    /// Parse command pipe getline: "cmd" | getline [var]
    fn parse_pipe_getline(&mut self) -> Result<AwkExpr, String> {
        let left = self.parse_or()?;

        // Check for: expr | getline [var]
        if self.check(TokenType::Pipe) {
            self.advance();
            if !self.check(TokenType::Getline) {
                return Err("Expected 'getline' after '|' in expression context".to_string());
            }
            self.advance(); // consume 'getline'

            let variable = if self.check(TokenType::Ident) {
                Some(self.advance().value.clone())
            } else {
                None
            };

            return Ok(AwkExpr::Getline {
                command: Some(Box::new(left)),
                variable,
                file: None,
            });
        }

        Ok(left)
    }

    fn parse_or(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_and()?;

        while self.check(TokenType::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Continue parsing a logical OR/AND expression from a given left-hand side.
    fn parse_logical_or_rest(&mut self, left: AwkExpr) -> Result<AwkExpr, String> {
        let mut left = self.parse_logical_and_rest(left)?;

        while self.check(TokenType::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_logical_and_rest(&mut self, mut left: AwkExpr) -> Result<AwkExpr, String> {
        while self.check(TokenType::And) {
            self.advance();
            let right = self.parse_in()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_in()?;

        while self.check(TokenType::And) {
            self.advance();
            let right = self.parse_in()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_in(&mut self) -> Result<AwkExpr, String> {
        let left = self.parse_concatenation()?;

        if self.check(TokenType::In) {
            self.advance();
            let array = self.expect(TokenType::Ident)?.value.clone();
            return Ok(AwkExpr::InExpr {
                key: Box::new(left),
                array,
            });
        }

        Ok(left)
    }

    fn parse_concatenation(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_match()?;

        while self.can_start_expression() && !self.is_concat_terminator() {
            let right = self.parse_match()?;
            left = AwkExpr::Concatenation {
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_match(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_comparison()?;

        while self.match_token(&[TokenType::Match, TokenType::NotMatch]) {
            let op = if self.advance().token_type == TokenType::Match {
                BinaryOp::MatchOp
            } else {
                BinaryOp::NotMatchOp
            };
            let right = self.parse_comparison()?;
            left = AwkExpr::BinaryOp {
                operator: op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_add_sub()?;

        while self.match_token(&[
            TokenType::Lt,
            TokenType::Le,
            TokenType::Gt,
            TokenType::Ge,
            TokenType::Eq,
            TokenType::Ne,
        ]) {
            let op_value = self.advance().value.clone();
            let right = self.parse_add_sub()?;
            let operator = match op_value.as_str() {
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::Le,
                ">" => BinaryOp::Gt,
                ">=" => BinaryOp::Ge,
                "==" => BinaryOp::Eq,
                "!=" => BinaryOp::Ne,
                _ => return Err(format!("Unknown comparison operator: {}", op_value)),
            };
            left = AwkExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn can_start_expression(&self) -> bool {
        self.match_token(&[
            TokenType::Number,
            TokenType::String,
            TokenType::Ident,
            TokenType::Dollar,
            TokenType::LParen,
            TokenType::Not,
            TokenType::Minus,
            TokenType::Plus,
            TokenType::Increment,
            TokenType::Decrement,
        ])
    }

    fn is_concat_terminator(&self) -> bool {
        self.match_token(&[
            TokenType::And,
            TokenType::Or,
            TokenType::Question,
            TokenType::Assign,
            TokenType::PlusAssign,
            TokenType::MinusAssign,
            TokenType::StarAssign,
            TokenType::SlashAssign,
            TokenType::PercentAssign,
            TokenType::CaretAssign,
            TokenType::Comma,
            TokenType::Semicolon,
            TokenType::Newline,
            TokenType::RBrace,
            TokenType::RParen,
            TokenType::RBracket,
            TokenType::Colon,
            TokenType::Pipe,
            TokenType::Append,
            TokenType::In,
        ])
    }

    fn parse_add_sub(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_mul_div()?;

        while self.match_token(&[TokenType::Plus, TokenType::Minus]) {
            let op_value = self.advance().value.clone();
            let right = self.parse_mul_div()?;
            let operator = if op_value == "+" {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            left = AwkExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_unary()?;

        while self.match_token(&[TokenType::Star, TokenType::Slash, TokenType::Percent]) {
            let op_value = self.advance().value.clone();
            let right = self.parse_unary()?;
            let operator = match op_value.as_str() {
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                "%" => BinaryOp::Mod,
                "**" => BinaryOp::Pow, // ** is tokenized as Caret but with value "**"
                _ => return Err(format!("Unknown multiplicative operator: {}", op_value)),
            };
            left = AwkExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AwkExpr, String> {
        // Prefix increment/decrement
        if self.check(TokenType::Increment) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(AwkExpr::PreIncrement(Box::new(operand)));
        }

        if self.check(TokenType::Decrement) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(AwkExpr::PreDecrement(Box::new(operand)));
        }

        // Unary operators (-, +, !)
        if self.match_token(&[TokenType::Not, TokenType::Minus, TokenType::Plus]) {
            let op_value = self.advance().value.clone();
            let operand = self.parse_unary()?;
            let operator = match op_value.as_str() {
                "!" => UnaryOp::Not,
                "-" => UnaryOp::Neg,
                "+" => UnaryOp::Pos,
                _ => return Err(format!("Unknown unary operator: {}", op_value)),
            };
            return Ok(AwkExpr::UnaryOp {
                operator,
                operand: Box::new(operand),
            });
        }

        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_postfix()?;

        if self.check(TokenType::Caret) {
            self.advance();
            // Exponent is right-associative
            let right = self.parse_power()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<AwkExpr, String> {
        let expr = self.parse_primary()?;

        // Postfix increment/decrement
        if self.check(TokenType::Increment) {
            self.advance();
            return Ok(AwkExpr::PostIncrement(Box::new(expr)));
        }

        if self.check(TokenType::Decrement) {
            self.advance();
            return Ok(AwkExpr::PostDecrement(Box::new(expr)));
        }

        Ok(expr)
    }

    /// Parse a field index expression. This is like parse_unary but does NOT allow
    /// postfix operators, so that $i++ parses as ($i)++ rather than $(i++).
    fn parse_field_index(&mut self) -> Result<AwkExpr, String> {
        // Prefix increment/decrement for field index
        if self.check(TokenType::Increment) {
            self.advance();
            let operand = self.parse_field_index()?;
            return Ok(AwkExpr::PreIncrement(Box::new(operand)));
        }

        if self.check(TokenType::Decrement) {
            self.advance();
            let operand = self.parse_field_index()?;
            return Ok(AwkExpr::PreDecrement(Box::new(operand)));
        }

        // Unary operators (-, +, !)
        if self.match_token(&[TokenType::Not, TokenType::Minus, TokenType::Plus]) {
            let op_value = self.advance().value.clone();
            let operand = self.parse_field_index()?;
            let operator = match op_value.as_str() {
                "!" => UnaryOp::Not,
                "-" => UnaryOp::Neg,
                "+" => UnaryOp::Pos,
                _ => return Err(format!("Unknown unary operator: {}", op_value)),
            };
            return Ok(AwkExpr::UnaryOp {
                operator,
                operand: Box::new(operand),
            });
        }

        self.parse_field_index_power()
    }

    fn parse_field_index_power(&mut self) -> Result<AwkExpr, String> {
        let mut left = self.parse_field_index_primary()?;

        if self.check(TokenType::Caret) {
            self.advance();
            let right = self.parse_field_index_power()?;
            left = AwkExpr::BinaryOp {
                operator: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse primary expression for field index - like parse_primary but returns
    /// without checking for postfix operators
    fn parse_field_index_primary(&mut self) -> Result<AwkExpr, String> {
        // Number literal
        if self.check(TokenType::Number) {
            let value: f64 = self.advance().value.parse().unwrap_or(0.0);
            return Ok(AwkExpr::NumberLiteral(value));
        }

        // String literal
        if self.check(TokenType::String) {
            let value = self.advance().value.clone();
            return Ok(AwkExpr::StringLiteral(value));
        }

        // Nested field reference
        if self.check(TokenType::Dollar) {
            self.advance();
            let index = self.parse_field_index()?;
            return Ok(AwkExpr::FieldRef(Box::new(index)));
        }

        // Parenthesized expression - allows full expression inside
        if self.check(TokenType::LParen) {
            self.advance();
            let expr = self.parse_expression()?;
            self.expect(TokenType::RParen)?;
            return Ok(expr);
        }

        // Variable or function call
        if self.check(TokenType::Ident) {
            let name = self.advance().value.clone();

            // Check for function call
            if self.check(TokenType::LParen) {
                self.advance();
                let mut args = Vec::new();
                if !self.check(TokenType::RParen) {
                    args.push(self.parse_expression()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        args.push(self.parse_expression()?);
                    }
                }
                self.expect(TokenType::RParen)?;
                return Ok(AwkExpr::FunctionCall { name, args });
            }

            // Check for array access
            if self.check(TokenType::LBracket) {
                self.advance();
                let key = self.parse_array_key()?;
                self.expect(TokenType::RBracket)?;
                return Ok(AwkExpr::ArrayAccess {
                    array: name,
                    key: Box::new(key),
                });
            }

            return Ok(AwkExpr::Variable(name));
        }

        Err(format!(
            "Unexpected token in field index: {:?} at line {}:{}",
            self.current().token_type,
            self.current().line,
            self.current().column
        ))
    }

    fn parse_primary(&mut self) -> Result<AwkExpr, String> {
        // Number literal
        if self.check(TokenType::Number) {
            let value: f64 = self.advance().value.parse().unwrap_or(0.0);
            return Ok(AwkExpr::NumberLiteral(value));
        }

        // String literal
        if self.check(TokenType::String) {
            let value = self.advance().value.clone();
            return Ok(AwkExpr::StringLiteral(value));
        }

        // Regex literal
        if self.check(TokenType::Regex) {
            let pattern = self.advance().value.clone();
            return Ok(AwkExpr::RegexLiteral(pattern));
        }

        // Field reference - the index can be any expression, but postfix ++ and --
        // should apply to the field, not the index. So $i++ means ($i)++, not $(i++).
        if self.check(TokenType::Dollar) {
            self.advance();
            let index = self.parse_field_index()?;
            return Ok(AwkExpr::FieldRef(Box::new(index)));
        }

        // Parenthesized expression or tuple (for multi-dimensional 'in' operator)
        if self.check(TokenType::LParen) {
            self.advance();
            let first = self.parse_expression()?;

            // Check for comma tuple: (expr, expr, ...)
            if self.check(TokenType::Comma) {
                let mut elements = vec![first];
                while self.check(TokenType::Comma) {
                    self.advance();
                    elements.push(self.parse_expression()?);
                }
                self.expect(TokenType::RParen)?;
                return Ok(AwkExpr::Tuple(elements));
            }

            self.expect(TokenType::RParen)?;
            return Ok(first);
        }

        // Getline
        if self.check(TokenType::Getline) {
            self.advance();
            let mut variable = None;
            let mut file = None;

            if self.check(TokenType::Ident) {
                variable = Some(self.advance().value.clone());
            }

            if self.check(TokenType::Lt) {
                self.advance();
                file = Some(Box::new(self.parse_primary()?));
            }

            return Ok(AwkExpr::Getline {
                variable,
                file,
                command: None,
            });
        }

        // Identifier (variable or function call)
        if self.check(TokenType::Ident) {
            let name = self.advance().value.clone();

            // Function call
            if self.check(TokenType::LParen) {
                self.advance();
                let mut args = Vec::new();
                self.skip_newlines();

                if !self.check(TokenType::RParen) {
                    args.push(self.parse_expression()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        self.skip_newlines();
                        args.push(self.parse_expression()?);
                    }
                }
                self.skip_newlines();
                self.expect(TokenType::RParen)?;
                return Ok(AwkExpr::FunctionCall { name, args });
            }

            // Array access
            if self.check(TokenType::LBracket) {
                self.advance();
                let key = self.parse_array_key()?;
                self.expect(TokenType::RBracket)?;
                return Ok(AwkExpr::ArrayAccess {
                    array: name,
                    key: Box::new(key),
                });
            }

            // Simple variable
            return Ok(AwkExpr::Variable(name));
        }

        Err(format!(
            "Unexpected token: {:?} at line {}:{}",
            self.current().token_type,
            self.current().line,
            self.current().column
        ))
    }

    /// Parse array key, handling multi-dimensional arrays: a[1,2,3]
    fn parse_array_key(&mut self) -> Result<AwkExpr, String> {
        let first = self.parse_expression()?;

        // Handle multi-dimensional array syntax: a[1,2,3] -> concatenate with SUBSEP
        if self.check(TokenType::Comma) {
            let mut keys = vec![first];
            while self.check(TokenType::Comma) {
                self.advance();
                keys.push(self.parse_expression()?);
            }

            // Build concatenation: key1 SUBSEP key2 SUBSEP key3 ...
            let mut result = keys.remove(0);
            for key in keys {
                // Concatenate with SUBSEP
                result = AwkExpr::Concatenation {
                    left: Box::new(AwkExpr::Concatenation {
                        left: Box::new(result),
                        right: Box::new(AwkExpr::Variable("SUBSEP".to_string())),
                    }),
                    right: Box::new(key),
                };
            }
            return Ok(result);
        }

        Ok(first)
    }
}

// ─── Public API ──────────────────────────────────────────────

/// Parse AWK source code into an AST.
///
/// This is the main entry point for the parser. It tokenizes the input
/// and then parses the token stream into an AwkProgram.
pub fn parse(input: &str) -> Result<AwkProgram, String> {
    let tokens = tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_print_field_zero() {
        // { print $0 }
        let program = parse("{ print $0 }").unwrap();
        assert_eq!(program.rules.len(), 1);
        assert!(program.rules[0].pattern.is_none());
        assert_eq!(program.rules[0].action.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { args, output } => {
                assert_eq!(args.len(), 1);
                assert!(output.is_none());
                match &args[0] {
                    AwkExpr::FieldRef(idx) => match idx.as_ref() {
                        AwkExpr::NumberLiteral(n) => assert_eq!(*n, 0.0),
                        _ => panic!("Expected NumberLiteral"),
                    },
                    _ => panic!("Expected FieldRef"),
                }
            }
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_begin_end() {
        // BEGIN { x=0 } END { print x }
        let program = parse("BEGIN { x=0 } END { print x }").unwrap();
        assert_eq!(program.rules.len(), 2);

        match &program.rules[0].pattern {
            Some(AwkPattern::Begin) => {}
            _ => panic!("Expected BEGIN pattern"),
        }

        match &program.rules[1].pattern {
            Some(AwkPattern::End) => {}
            _ => panic!("Expected END pattern"),
        }
    }

    #[test]
    fn test_parse_regex_pattern() {
        // /foo/ { print }
        let program = parse("/foo/ { print }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].pattern {
            Some(AwkPattern::Regex(pat)) => assert_eq!(pat, "foo"),
            _ => panic!("Expected Regex pattern"),
        }
    }

    #[test]
    fn test_parse_expression_pattern() {
        // NR > 5 { print }
        let program = parse("NR > 5 { print }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].pattern {
            Some(AwkPattern::Expression(expr)) => match expr {
                AwkExpr::BinaryOp { operator, .. } => {
                    assert_eq!(*operator, BinaryOp::Gt);
                }
                _ => panic!("Expected BinaryOp"),
            },
            _ => panic!("Expected Expression pattern"),
        }
    }

    #[test]
    fn test_parse_range_pattern() {
        // /start/,/end/ { print }
        let program = parse("/start/,/end/ { print }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].pattern {
            Some(AwkPattern::Range { start, end }) => {
                match start.as_ref() {
                    AwkPattern::Regex(pat) => assert_eq!(pat, "start"),
                    _ => panic!("Expected Regex start pattern"),
                }
                match end.as_ref() {
                    AwkPattern::Regex(pat) => assert_eq!(pat, "end"),
                    _ => panic!("Expected Regex end pattern"),
                }
            }
            _ => panic!("Expected Range pattern"),
        }
    }

    #[test]
    fn test_parse_if_else() {
        // { if (x > 0) print "pos"; else print "neg" }
        let program = parse(r#"{ if (x > 0) print "pos"; else print "neg" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::If {
                condition,
                consequent,
                alternate,
            } => {
                match condition {
                    AwkExpr::BinaryOp { operator, .. } => {
                        assert_eq!(*operator, BinaryOp::Gt);
                    }
                    _ => panic!("Expected BinaryOp condition"),
                }
                match consequent.as_ref() {
                    AwkStmt::Print { .. } => {}
                    _ => panic!("Expected Print consequent"),
                }
                assert!(alternate.is_some());
            }
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_parse_while_loop() {
        // { while (i < 10) i++ }
        let program = parse("{ while (i < 10) i++ }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::While { condition, body } => {
                match condition {
                    AwkExpr::BinaryOp { operator, .. } => {
                        assert_eq!(*operator, BinaryOp::Lt);
                    }
                    _ => panic!("Expected BinaryOp condition"),
                }
                match body.as_ref() {
                    AwkStmt::ExprStmt(AwkExpr::PostIncrement(_)) => {}
                    _ => panic!("Expected PostIncrement body"),
                }
            }
            _ => panic!("Expected While statement"),
        }
    }

    #[test]
    fn test_parse_for_loop() {
        // { for (i=0; i<10; i++) print i }
        let program = parse("{ for (i=0; i<10; i++) print i }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::For {
                init,
                condition,
                update,
                body,
            } => {
                assert!(init.is_some());
                assert!(condition.is_some());
                assert!(update.is_some());
                match body.as_ref() {
                    AwkStmt::Print { .. } => {}
                    _ => panic!("Expected Print body"),
                }
            }
            _ => panic!("Expected For statement"),
        }
    }

    #[test]
    fn test_parse_for_in() {
        // { for (k in arr) print k }
        let program = parse("{ for (k in arr) print k }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ForIn {
                variable,
                array,
                body,
            } => {
                assert_eq!(variable, "k");
                assert_eq!(array, "arr");
                match body.as_ref() {
                    AwkStmt::Print { .. } => {}
                    _ => panic!("Expected Print body"),
                }
            }
            _ => panic!("Expected ForIn statement"),
        }
    }

    #[test]
    fn test_parse_function_definition() {
        // function add(a, b) { return a + b }
        let program = parse("function add(a, b) { return a + b }").unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "add");
        assert_eq!(program.functions[0].params, vec!["a", "b"]);
        assert_eq!(program.functions[0].body.len(), 1);
        match &program.functions[0].body[0] {
            AwkStmt::Return(Some(expr)) => match expr {
                AwkExpr::BinaryOp { operator, .. } => {
                    assert_eq!(*operator, BinaryOp::Add);
                }
                _ => panic!("Expected BinaryOp"),
            },
            _ => panic!("Expected Return statement"),
        }
    }

    #[test]
    fn test_parse_assignment_operators() {
        // { x = 1; x += 2; x -= 1; x *= 3; x /= 2; x %= 5; x ^= 2 }
        let program =
            parse("{ x = 1; x += 2; x -= 1; x *= 3; x /= 2; x %= 5; x ^= 2 }").unwrap();
        assert_eq!(program.rules.len(), 1);
        assert_eq!(program.rules[0].action.len(), 7);

        let expected_ops = vec![
            AssignOp::Assign,
            AssignOp::AddAssign,
            AssignOp::SubAssign,
            AssignOp::MulAssign,
            AssignOp::DivAssign,
            AssignOp::ModAssign,
            AssignOp::PowAssign,
        ];

        for (i, stmt) in program.rules[0].action.iter().enumerate() {
            match stmt {
                AwkStmt::ExprStmt(AwkExpr::Assignment { operator, .. }) => {
                    assert_eq!(*operator, expected_ops[i]);
                }
                _ => panic!("Expected Assignment at index {}", i),
            }
        }
    }

    #[test]
    fn test_parse_ternary() {
        // { x = a > b ? a : b }
        let program = parse("{ x = a > b ? a : b }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Assignment { value, .. }) => match value.as_ref() {
                AwkExpr::Ternary {
                    condition,
                    consequent,
                    alternate,
                } => {
                    match condition.as_ref() {
                        AwkExpr::BinaryOp { operator, .. } => {
                            assert_eq!(*operator, BinaryOp::Gt);
                        }
                        _ => panic!("Expected BinaryOp condition"),
                    }
                    match consequent.as_ref() {
                        AwkExpr::Variable(name) => assert_eq!(name, "a"),
                        _ => panic!("Expected Variable consequent"),
                    }
                    match alternate.as_ref() {
                        AwkExpr::Variable(name) => assert_eq!(name, "b"),
                        _ => panic!("Expected Variable alternate"),
                    }
                }
                _ => panic!("Expected Ternary"),
            },
            _ => panic!("Expected Assignment"),
        }
    }

    #[test]
    fn test_parse_print_redirect_write() {
        // { print "hello" > "file.txt" }
        let program = parse(r#"{ print "hello" > "file.txt" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { args, output } => {
                assert_eq!(args.len(), 1);
                match output {
                    Some(RedirectInfo {
                        redirect_type,
                        target,
                    }) => {
                        assert_eq!(*redirect_type, RedirectType::Write);
                        match target {
                            AwkExpr::StringLiteral(s) => assert_eq!(s, "file.txt"),
                            _ => panic!("Expected StringLiteral target"),
                        }
                    }
                    None => panic!("Expected redirect"),
                }
            }
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_print_redirect_append() {
        // { print "hello" >> "file.txt" }
        let program = parse(r#"{ print "hello" >> "file.txt" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { output, .. } => match output {
                Some(RedirectInfo { redirect_type, .. }) => {
                    assert_eq!(*redirect_type, RedirectType::Append);
                }
                None => panic!("Expected redirect"),
            },
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_print_redirect_pipe() {
        // { print "hello" | "cmd" }
        let program = parse(r#"{ print "hello" | "cmd" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { output, .. } => match output {
                Some(RedirectInfo { redirect_type, .. }) => {
                    assert_eq!(*redirect_type, RedirectType::Pipe);
                }
                None => panic!("Expected redirect"),
            },
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_printf() {
        // { printf "%s %d\n", name, age }
        let program = parse(r#"{ printf "%s %d\n", name, age }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Printf { format, args, .. } => {
                match format {
                    AwkExpr::StringLiteral(s) => assert_eq!(s, "%s %d\n"),
                    _ => panic!("Expected StringLiteral format"),
                }
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Printf statement"),
        }
    }

    #[test]
    fn test_parse_delete() {
        // { delete arr[key] }
        let program = parse("{ delete arr[key] }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Delete { target } => match target {
                AwkExpr::ArrayAccess { array, .. } => {
                    assert_eq!(array, "arr");
                }
                _ => panic!("Expected ArrayAccess"),
            },
            _ => panic!("Expected Delete statement"),
        }
    }

    #[test]
    fn test_parse_getline_plain() {
        // { getline }
        let program = parse("{ getline }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Getline {
                variable,
                file,
                command,
            }) => {
                assert!(variable.is_none());
                assert!(file.is_none());
                assert!(command.is_none());
            }
            _ => panic!("Expected Getline expression"),
        }
    }

    #[test]
    fn test_parse_getline_into_var() {
        // { getline line }
        let program = parse("{ getline line }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Getline { variable, .. }) => {
                assert_eq!(variable.as_ref().unwrap(), "line");
            }
            _ => panic!("Expected Getline expression"),
        }
    }

    #[test]
    fn test_parse_getline_from_file() {
        // { getline < "file.txt" }
        let program = parse(r#"{ getline < "file.txt" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Getline { file, .. }) => {
                assert!(file.is_some());
                match file.as_ref().unwrap().as_ref() {
                    AwkExpr::StringLiteral(s) => assert_eq!(s, "file.txt"),
                    _ => panic!("Expected StringLiteral file"),
                }
            }
            _ => panic!("Expected Getline expression"),
        }
    }

    #[test]
    fn test_parse_multi_dim_array() {
        // { a[1,2] = 3 }
        let program = parse("{ a[1,2] = 3 }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Assignment { target, .. }) => match target.as_ref() {
                AwkExpr::ArrayAccess { array, key } => {
                    assert_eq!(array, "a");
                    // Key should be a concatenation with SUBSEP
                    match key.as_ref() {
                        AwkExpr::Concatenation { .. } => {}
                        _ => panic!("Expected Concatenation key for multi-dim array"),
                    }
                }
                _ => panic!("Expected ArrayAccess"),
            },
            _ => panic!("Expected Assignment"),
        }
    }

    #[test]
    fn test_parse_default_action() {
        // /pattern/ (no braces - default action is print $0)
        let program = parse("/pattern/").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].pattern {
            Some(AwkPattern::Regex(pat)) => assert_eq!(pat, "pattern"),
            _ => panic!("Expected Regex pattern"),
        }
        // Default action should be print $0
        assert_eq!(program.rules[0].action.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { args, .. } => {
                assert_eq!(args.len(), 1);
                match &args[0] {
                    AwkExpr::FieldRef(idx) => match idx.as_ref() {
                        AwkExpr::NumberLiteral(n) => assert_eq!(*n, 0.0),
                        _ => panic!("Expected NumberLiteral"),
                    },
                    _ => panic!("Expected FieldRef"),
                }
            }
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_nested_blocks() {
        // { if (x) { if (y) { print } } }
        let program = parse("{ if (x) { if (y) { print } } }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::If { consequent, .. } => match consequent.as_ref() {
                AwkStmt::Block(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    match &stmts[0] {
                        AwkStmt::If { .. } => {}
                        _ => panic!("Expected nested If"),
                    }
                }
                _ => panic!("Expected Block"),
            },
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_parse_do_while() {
        // { do { i++ } while (i < 10) }
        let program = parse("{ do { i++ } while (i < 10) }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::DoWhile { body, condition } => {
                match body.as_ref() {
                    AwkStmt::Block(stmts) => {
                        assert_eq!(stmts.len(), 1);
                    }
                    _ => panic!("Expected Block body"),
                }
                match condition {
                    AwkExpr::BinaryOp { operator, .. } => {
                        assert_eq!(*operator, BinaryOp::Lt);
                    }
                    _ => panic!("Expected BinaryOp condition"),
                }
            }
            _ => panic!("Expected DoWhile statement"),
        }
    }

    #[test]
    fn test_parse_concatenation() {
        // { x = "a" "b" }
        let program = parse(r#"{ x = "a" "b" }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Assignment { value, .. }) => match value.as_ref() {
                AwkExpr::Concatenation { left, right } => {
                    match left.as_ref() {
                        AwkExpr::StringLiteral(s) => assert_eq!(s, "a"),
                        _ => panic!("Expected StringLiteral left"),
                    }
                    match right.as_ref() {
                        AwkExpr::StringLiteral(s) => assert_eq!(s, "b"),
                        _ => panic!("Expected StringLiteral right"),
                    }
                }
                _ => panic!("Expected Concatenation"),
            },
            _ => panic!("Expected Assignment"),
        }
    }

    #[test]
    fn test_parse_field_postfix() {
        // { $i++ } should parse as ($i)++ not $(i++)
        let program = parse("{ $i++ }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::PostIncrement(operand)) => match operand.as_ref() {
                AwkExpr::FieldRef(idx) => match idx.as_ref() {
                    AwkExpr::Variable(name) => assert_eq!(name, "i"),
                    _ => panic!("Expected Variable index"),
                },
                _ => panic!("Expected FieldRef"),
            },
            _ => panic!("Expected PostIncrement"),
        }
    }

    #[test]
    fn test_parse_empty_print() {
        // { print } should be print $0
        let program = parse("{ print }").unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Print { args, .. } => {
                assert_eq!(args.len(), 1);
                match &args[0] {
                    AwkExpr::FieldRef(idx) => match idx.as_ref() {
                        AwkExpr::NumberLiteral(n) => assert_eq!(*n, 0.0),
                        _ => panic!("Expected NumberLiteral"),
                    },
                    _ => panic!("Expected FieldRef"),
                }
            }
            _ => panic!("Expected Print statement"),
        }
    }

    #[test]
    fn test_parse_printf_parens() {
        // { printf("%s", x) }
        let program = parse(r#"{ printf("%s", x) }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::Printf { format, args, .. } => {
                match format {
                    AwkExpr::StringLiteral(s) => assert_eq!(s, "%s"),
                    _ => panic!("Expected StringLiteral format"),
                }
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected Printf statement"),
        }
    }

    #[test]
    fn test_parse_pipe_getline() {
        // { "cmd" | getline var }
        let program = parse(r#"{ "cmd" | getline var }"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        match &program.rules[0].action[0] {
            AwkStmt::ExprStmt(AwkExpr::Getline {
                command,
                variable,
                file,
            }) => {
                assert!(command.is_some());
                match command.as_ref().unwrap().as_ref() {
                    AwkExpr::StringLiteral(s) => assert_eq!(s, "cmd"),
                    _ => panic!("Expected StringLiteral command"),
                }
                assert_eq!(variable.as_ref().unwrap(), "var");
                assert!(file.is_none());
            }
            _ => panic!("Expected Getline expression"),
        }
    }
}
