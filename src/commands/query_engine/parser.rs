use super::ast::*;
use super::lexer::tokenize;
use super::value::Value;

pub fn parse(input: &str) -> Result<AstNode, String> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    let ast = parser.parse()?;
    Ok(ast)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token {
            token_type: TokenType::Eof,
            pos: 0,
        })
    }

    fn peek_ahead(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n)
    }

    fn advance(&mut self) -> Token {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or(Token {
                token_type: TokenType::Eof,
                pos: 0,
            });
        self.pos += 1;
        tok
    }

    fn check(&self, tt: &TokenType) -> bool {
        std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(tt)
    }

    fn check_exact(&self, tt: &TokenType) -> bool {
        self.peek().token_type == *tt
    }

    fn match_exact(&mut self, types: &[TokenType]) -> Option<Token> {
        for tt in types {
            if self.check_exact(tt) {
                return Some(self.advance());
            }
        }
        None
    }

    fn match_token(&mut self, types: &[TokenType]) -> Option<Token> {
        for tt in types {
            if self.check(tt) {
                return Some(self.advance());
            }
        }
        None
    }

    fn expect(&mut self, tt: &TokenType, msg: &str) -> Result<Token, String> {
        if !self.check(tt) {
            return Err(format!(
                "{} at position {}, got {:?}",
                msg,
                self.peek().pos,
                self.peek().token_type
            ));
        }
        Ok(self.advance())
    }

    fn parse(&mut self) -> Result<AstNode, String> {
        let expr = self.parse_expr()?;
        if !self.check_exact(&TokenType::Eof) {
            return Err(format!(
                "Unexpected token {:?} at position {}",
                self.peek().token_type,
                self.peek().pos
            ));
        }
        Ok(expr)
    }

    fn parse_expr(&mut self) -> Result<AstNode, String> {
        self.parse_pipe()
    }


    fn parse_pipe(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_comma()?;
        while self.match_exact(&[TokenType::Pipe]).is_some() {
            let right = self.parse_comma()?;
            left = AstNode::Pipe {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comma(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_var_bind()?;
        while self.match_exact(&[TokenType::Comma]).is_some() {
            let right = self.parse_var_bind()?;
            left = AstNode::Comma {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_var_bind(&mut self) -> Result<AstNode, String> {
        let expr = self.parse_update()?;
        if self.match_exact(&[TokenType::As]).is_some() {
            let pattern = self.parse_pattern()?;
            let mut alternatives: Vec<DestructurePattern> = Vec::new();
            while self.check_exact(&TokenType::Question) {
                if let Some(ahead) = self.peek_ahead(1) {
                    if matches!(ahead.token_type, TokenType::Alt) {
                        self.advance(); // consume ?
                        self.advance(); // consume //
                        alternatives.push(self.parse_pattern()?);
                        continue;
                    }
                }
                break;
            }
            self.expect(&TokenType::Pipe, "Expected '|' after variable binding")?;
            let body = self.parse_expr()?;
            let name = match &pattern {
                DestructurePattern::Var { name } => name.clone(),
                _ => String::new(),
            };
            let pat = match &pattern {
                DestructurePattern::Var { .. } => None,
                _ => Some(pattern),
            };
            let alts = if alternatives.is_empty() {
                None
            } else {
                Some(alternatives)
            };
            return Ok(AstNode::VarBind {
                name,
                value: Box::new(expr),
                body: Box::new(body),
                pattern: pat,
                alternatives: alts,
            });
        }
        Ok(expr)
    }


    fn parse_update(&mut self) -> Result<AstNode, String> {
        let left = self.parse_alt()?;
        let update_types = [
            TokenType::Assign,
            TokenType::UpdateAdd,
            TokenType::UpdateSub,
            TokenType::UpdateMul,
            TokenType::UpdateDiv,
            TokenType::UpdateMod,
            TokenType::UpdateAlt,
            TokenType::UpdatePipe,
        ];
        if let Some(tok) = self.match_exact(&update_types) {
            let op = match tok.token_type {
                TokenType::Assign => UpdateOp::Assign,
                TokenType::UpdateAdd => UpdateOp::AddUpdate,
                TokenType::UpdateSub => UpdateOp::SubUpdate,
                TokenType::UpdateMul => UpdateOp::MulUpdate,
                TokenType::UpdateDiv => UpdateOp::DivUpdate,
                TokenType::UpdateMod => UpdateOp::ModUpdate,
                TokenType::UpdateAlt => UpdateOp::AltUpdate,
                TokenType::UpdatePipe => UpdateOp::PipeUpdate,
                _ => unreachable!(),
            };
            let value = self.parse_var_bind()?;
            return Ok(AstNode::UpdateOp {
                op,
                path: Box::new(left),
                value: Box::new(value),
            });
        }
        Ok(left)
    }

    fn parse_alt(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_or()?;
        while self.match_exact(&[TokenType::Alt]).is_some() {
            let right = self.parse_or()?;
            left = AstNode::BinaryOp {
                op: BinaryOp::Alt,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_and()?;
        while self.match_exact(&[TokenType::Or]).is_some() {
            let right = self.parse_and()?;
            left = AstNode::BinaryOp {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_comparison()?;
        while self.match_exact(&[TokenType::And]).is_some() {
            let right = self.parse_comparison()?;
            left = AstNode::BinaryOp {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_add_sub()?;
        let cmp_types = [
            TokenType::Eq,
            TokenType::Ne,
            TokenType::Lt,
            TokenType::Le,
            TokenType::Gt,
            TokenType::Ge,
        ];
        if let Some(tok) = self.match_exact(&cmp_types) {
            let op = match tok.token_type {
                TokenType::Eq => BinaryOp::Eq,
                TokenType::Ne => BinaryOp::Ne,
                TokenType::Lt => BinaryOp::Lt,
                TokenType::Le => BinaryOp::Le,
                TokenType::Gt => BinaryOp::Gt,
                TokenType::Ge => BinaryOp::Ge,
                _ => unreachable!(),
            };
            let right = self.parse_add_sub()?;
            left = AstNode::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }


    fn parse_add_sub(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_mul_div()?;
        loop {
            if self.match_exact(&[TokenType::Plus]).is_some() {
                let right = self.parse_mul_div()?;
                left = AstNode::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.match_exact(&[TokenType::Minus]).is_some() {
                let right = self.parse_mul_div()?;
                left = AstNode::BinaryOp {
                    op: BinaryOp::Sub,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_unary()?;
        loop {
            if self.match_exact(&[TokenType::Star]).is_some() {
                let right = self.parse_unary()?;
                left = AstNode::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.match_exact(&[TokenType::Slash]).is_some() {
                let right = self.parse_unary()?;
                left = AstNode::BinaryOp {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.match_exact(&[TokenType::Percent]).is_some() {
                let right = self.parse_unary()?;
                left = AstNode::BinaryOp {
                    op: BinaryOp::Mod,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AstNode, String> {
        if self.match_exact(&[TokenType::Minus]).is_some() {
            let operand = self.parse_unary()?;
            return Ok(AstNode::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
            });
        }
        self.parse_postfix()
    }


    fn parse_postfix(&mut self) -> Result<AstNode, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_exact(&[TokenType::Question]).is_some() {
                expr = AstNode::Optional {
                    expr: Box::new(expr),
                };
            } else if self.check_exact(&TokenType::Dot) {
                // Check if next token after dot is ident or string
                let is_field = self.peek_ahead(1).map_or(false, |t| {
                    matches!(
                        t.token_type,
                        TokenType::Ident(_) | TokenType::Str(_)
                    )
                });
                if is_field {
                    self.advance(); // consume dot
                    let token = self.advance();
                    let name = match token.token_type {
                        TokenType::Ident(s) | TokenType::Str(s) => s,
                        _ => unreachable!(),
                    };
                    expr = AstNode::Field {
                        name,
                        base: Some(Box::new(expr)),
                    };
                } else {
                    break;
                }
            } else if self.check_exact(&TokenType::LBracket) {
                self.advance(); // consume [
                if self.match_exact(&[TokenType::RBracket]).is_some() {
                    expr = AstNode::Iterate {
                        base: Some(Box::new(expr)),
                    };
                } else if self.check_exact(&TokenType::Colon) {
                    self.advance(); // consume :
                    let end = if self.check_exact(&TokenType::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(&TokenType::RBracket, "Expected ']'")?;
                    expr = AstNode::Slice {
                        base: Some(Box::new(expr)),
                        start: None,
                        end,
                    };
                } else {
                    let index_expr = self.parse_expr()?;
                    if self.match_exact(&[TokenType::Colon]).is_some() {
                        let end = if self.check_exact(&TokenType::RBracket) {
                            None
                        } else {
                            Some(Box::new(self.parse_expr()?))
                        };
                        self.expect(&TokenType::RBracket, "Expected ']'")?;
                        expr = AstNode::Slice {
                            base: Some(Box::new(expr)),
                            start: Some(Box::new(index_expr)),
                            end,
                        };
                    } else {
                        self.expect(&TokenType::RBracket, "Expected ']'")?;
                        expr = AstNode::Index {
                            base: Some(Box::new(expr)),
                            index: Box::new(index_expr),
                        };
                    }
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }


    fn parse_primary(&mut self) -> Result<AstNode, String> {
        // Recursive descent (..)
        if self.match_exact(&[TokenType::DotDot]).is_some() {
            return Ok(AstNode::Recurse);
        }

        // Identity or field access starting with dot
        if self.match_exact(&[TokenType::Dot]).is_some() {
            // Check for .[] or .[n] or .[n:m]
            if self.check_exact(&TokenType::LBracket) {
                self.advance(); // consume [
                if self.match_exact(&[TokenType::RBracket]).is_some() {
                    return Ok(AstNode::Iterate { base: None });
                }
                if self.check_exact(&TokenType::Colon) {
                    self.advance(); // consume :
                    let end = if self.check_exact(&TokenType::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(&TokenType::RBracket, "Expected ']'")?;
                    return Ok(AstNode::Slice {
                        base: None,
                        start: None,
                        end,
                    });
                }
                let index_expr = self.parse_expr()?;
                if self.match_exact(&[TokenType::Colon]).is_some() {
                    let end = if self.check_exact(&TokenType::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(&TokenType::RBracket, "Expected ']'")?;
                    return Ok(AstNode::Slice {
                        base: None,
                        start: Some(Box::new(index_expr)),
                        end,
                    });
                }
                self.expect(&TokenType::RBracket, "Expected ']'")?;
                return Ok(AstNode::Index {
                    base: None,
                    index: Box::new(index_expr),
                });
            }
            // .field or ."quoted-field"
            if self.check(&TokenType::Ident(String::new()))
                || self.check(&TokenType::Str(String::new()))
            {
                let token = self.advance();
                let name = match token.token_type {
                    TokenType::Ident(s) | TokenType::Str(s) => s,
                    _ => unreachable!(),
                };
                return Ok(AstNode::Field { name, base: None });
            }
            // Just identity
            return Ok(AstNode::Identity);
        }


        // Literals
        if self.match_exact(&[TokenType::True]).is_some() {
            return Ok(AstNode::Literal {
                value: Value::Bool(true),
            });
        }
        if self.match_exact(&[TokenType::False]).is_some() {
            return Ok(AstNode::Literal {
                value: Value::Bool(false),
            });
        }
        if self.match_exact(&[TokenType::Null]).is_some() {
            return Ok(AstNode::Literal {
                value: Value::Null,
            });
        }
        if self.check(&TokenType::Number(0.0)) {
            let tok = self.advance();
            if let TokenType::Number(n) = tok.token_type {
                return Ok(AstNode::Literal {
                    value: Value::Number(n),
                });
            }
        }
        if self.check(&TokenType::Str(String::new())) {
            let tok = self.advance();
            if let TokenType::Str(s) = tok.token_type {
                // Check for string interpolation
                if s.contains("\\(") {
                    return self.parse_string_interpolation(&s);
                }
                return Ok(AstNode::Literal {
                    value: Value::String(s),
                });
            }
        }

        // Array construction
        if self.match_exact(&[TokenType::LBracket]).is_some() {
            if self.match_exact(&[TokenType::RBracket]).is_some() {
                return Ok(AstNode::Array { elements: None });
            }
            let elements = self.parse_expr()?;
            self.expect(&TokenType::RBracket, "Expected ']'")?;
            return Ok(AstNode::Array {
                elements: Some(Box::new(elements)),
            });
        }

        // Object construction
        if self.match_exact(&[TokenType::LBrace]).is_some() {
            return self.parse_object_construction();
        }

        // Parentheses
        if self.match_exact(&[TokenType::LParen]).is_some() {
            let expr = self.parse_expr()?;
            self.expect(&TokenType::RParen, "Expected ')'")?;
            return Ok(AstNode::Paren {
                expr: Box::new(expr),
            });
        }

        // if-then-else
        if self.match_exact(&[TokenType::If]).is_some() {
            return self.parse_if();
        }

        // try-catch
        if self.match_exact(&[TokenType::Try]).is_some() {
            let body = self.parse_postfix()?;
            let catch = if self.match_exact(&[TokenType::Catch]).is_some() {
                Some(Box::new(self.parse_postfix()?))
            } else {
                None
            };
            return Ok(AstNode::Try {
                body: Box::new(body),
                catch,
            });
        }

        // reduce
        if self.match_exact(&[TokenType::Reduce]).is_some() {
            let expr = self.parse_add_sub()?;
            self.expect(&TokenType::As, "Expected 'as' after reduce expression")?;
            let pattern = self.parse_pattern()?;
            self.expect(&TokenType::LParen, "Expected '(' after variable")?;
            let init = self.parse_expr()?;
            self.expect(&TokenType::Semicolon, "Expected ';' after init expression")?;
            let update = self.parse_expr()?;
            self.expect(&TokenType::RParen, "Expected ')' after update expression")?;
            let var_name = match &pattern {
                DestructurePattern::Var { name } => name.clone(),
                _ => String::new(),
            };
            let pat = match &pattern {
                DestructurePattern::Var { .. } => None,
                _ => Some(pattern),
            };
            return Ok(AstNode::Reduce {
                expr: Box::new(expr),
                var_name,
                pattern: pat,
                init: Box::new(init),
                update: Box::new(update),
            });
        }

        // foreach
        if self.match_exact(&[TokenType::Foreach]).is_some() {
            let expr = self.parse_add_sub()?;
            self.expect(&TokenType::As, "Expected 'as' after foreach expression")?;
            let pattern = self.parse_pattern()?;
            self.expect(&TokenType::LParen, "Expected '(' after variable")?;
            let init = self.parse_expr()?;
            self.expect(&TokenType::Semicolon, "Expected ';' after init expression")?;
            let update = self.parse_expr()?;
            let extract = if self.match_exact(&[TokenType::Semicolon]).is_some() {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };
            self.expect(&TokenType::RParen, "Expected ')' after expressions")?;
            let var_name = match &pattern {
                DestructurePattern::Var { name } => name.clone(),
                _ => String::new(),
            };
            let pat = match &pattern {
                DestructurePattern::Var { .. } => None,
                _ => Some(pattern),
            };
            return Ok(AstNode::Foreach {
                expr: Box::new(expr),
                var_name,
                pattern: pat,
                init: Box::new(init),
                update: Box::new(update),
                extract,
            });
        }

        // label $NAME | BODY
        if self.match_exact(&[TokenType::Label]).is_some() {
            let label_tok =
                self.expect(&TokenType::Ident(String::new()), "Expected label name")?;
            let label_name = match label_tok.token_type {
                TokenType::Ident(s) => s,
                _ => unreachable!(),
            };
            if !label_name.starts_with('$') {
                return Err(format!(
                    "Label name must start with $ at position {}",
                    label_tok.pos
                ));
            }
            self.expect(&TokenType::Pipe, "Expected '|' after label name")?;
            let body = self.parse_expr()?;
            return Ok(AstNode::Label {
                name: label_name,
                body: Box::new(body),
            });
        }

        // break $NAME
        if self.match_exact(&[TokenType::Break]).is_some() {
            let break_tok =
                self.expect(&TokenType::Ident(String::new()), "Expected label name to break to")?;
            let break_label = match break_tok.token_type {
                TokenType::Ident(s) => s,
                _ => unreachable!(),
            };
            if !break_label.starts_with('$') {
                return Err(format!(
                    "Break label must start with $ at position {}",
                    break_tok.pos
                ));
            }
            return Ok(AstNode::Break { name: break_label });
        }

        // def NAME(PARAMS): BODY; REST
        if self.match_exact(&[TokenType::Def]).is_some() {
            let name_tok =
                self.expect(&TokenType::Ident(String::new()), "Expected function name after def")?;
            let func_name = match name_tok.token_type {
                TokenType::Ident(s) => s,
                _ => unreachable!(),
            };
            let mut params: Vec<String> = Vec::new();
            if self.match_exact(&[TokenType::LParen]).is_some() {
                if !self.check_exact(&TokenType::RParen) {
                    let first =
                        self.expect(&TokenType::Ident(String::new()), "Expected parameter name")?;
                    if let TokenType::Ident(s) = first.token_type {
                        params.push(s);
                    }
                    while self.match_exact(&[TokenType::Semicolon]).is_some() {
                        let param = self
                            .expect(&TokenType::Ident(String::new()), "Expected parameter name")?;
                        if let TokenType::Ident(s) = param.token_type {
                            params.push(s);
                        }
                    }
                }
                self.expect(&TokenType::RParen, "Expected ')' after parameters")?;
            }
            self.expect(&TokenType::Colon, "Expected ':' after function name")?;
            let func_body = self.parse_expr()?;
            self.expect(&TokenType::Semicolon, "Expected ';' after function body")?;
            let body = self.parse_expr()?;
            return Ok(AstNode::Def {
                name: func_name,
                params,
                func_body: Box::new(func_body),
                body: Box::new(body),
            });
        }

        // not as standalone call
        if self.match_exact(&[TokenType::Not]).is_some() {
            return Ok(AstNode::Call {
                name: "not".to_string(),
                args: vec![],
            });
        }

        // Variable reference or function call
        if self.check(&TokenType::Ident(String::new())) {
            let tok = self.advance();
            let name = match tok.token_type {
                TokenType::Ident(s) => s,
                _ => unreachable!(),
            };
            // Variable reference
            if name.starts_with('$') {
                return Ok(AstNode::VarRef { name });
            }
            // Function call with args
            if self.match_exact(&[TokenType::LParen]).is_some() {
                let mut args: Vec<AstNode> = Vec::new();
                if !self.check_exact(&TokenType::RParen) {
                    args.push(self.parse_expr()?);
                    while self.match_exact(&[TokenType::Semicolon]).is_some() {
                        args.push(self.parse_expr()?);
                    }
                }
                self.expect(&TokenType::RParen, "Expected ')'")?;
                return Ok(AstNode::Call { name, args });
            }
            // Builtin without parens
            return Ok(AstNode::Call {
                name,
                args: vec![],
            });
        }

        Err(format!(
            "Unexpected token {:?} at position {}",
            self.peek().token_type,
            self.peek().pos
        ))
    }


    fn parse_pattern(&mut self) -> Result<DestructurePattern, String> {
        // Array pattern: [$a, $b, ...]
        if self.match_exact(&[TokenType::LBracket]).is_some() {
            let mut elements: Vec<DestructurePattern> = Vec::new();
            if !self.check_exact(&TokenType::RBracket) {
                elements.push(self.parse_pattern()?);
                while self.match_exact(&[TokenType::Comma]).is_some() {
                    if self.check_exact(&TokenType::RBracket) {
                        break;
                    }
                    elements.push(self.parse_pattern()?);
                }
            }
            self.expect(&TokenType::RBracket, "Expected ']' after array pattern")?;
            return Ok(DestructurePattern::Array { elements });
        }

        // Object pattern: {key: $a, $b, ...}
        if self.match_exact(&[TokenType::LBrace]).is_some() {
            let mut fields: Vec<PatternField> = Vec::new();
            if !self.check_exact(&TokenType::RBrace) {
                fields.push(self.parse_pattern_field()?);
                while self.match_exact(&[TokenType::Comma]).is_some() {
                    if self.check_exact(&TokenType::RBrace) {
                        break;
                    }
                    fields.push(self.parse_pattern_field()?);
                }
            }
            self.expect(&TokenType::RBrace, "Expected '}' after object pattern")?;
            return Ok(DestructurePattern::Object { fields });
        }

        // Simple variable: $name
        let tok = self.expect(&TokenType::Ident(String::new()), "Expected variable name in pattern")?;
        let name = match tok.token_type {
            TokenType::Ident(s) => s,
            _ => unreachable!(),
        };
        if !name.starts_with('$') {
            return Err(format!(
                "Variable name must start with $ at position {}",
                tok.pos
            ));
        }
        Ok(DestructurePattern::Var { name })
    }

    fn parse_pattern_field(&mut self) -> Result<PatternField, String> {
        // Computed key: (expr): $pattern
        if self.match_exact(&[TokenType::LParen]).is_some() {
            let key_expr = self.parse_expr()?;
            self.expect(&TokenType::RParen, "Expected ')' after computed key")?;
            self.expect(&TokenType::Colon, "Expected ':' after computed key")?;
            let pattern = self.parse_pattern()?;
            return Ok(PatternField {
                key: PatternKey::Expr(key_expr),
                pattern,
                key_var: None,
            });
        }

        // Check for ident or $var
        if self.check(&TokenType::Ident(String::new())) {
            let tok = self.peek().clone();
            if let TokenType::Ident(ref name) = tok.token_type {
                if name.starts_with('$') {
                    let name = name.clone();
                    self.advance();
                    // Check for $name:pattern
                    if self.match_exact(&[TokenType::Colon]).is_some() {
                        let pattern = self.parse_pattern()?;
                        return Ok(PatternField {
                            key: PatternKey::Ident(name[1..].to_string()),
                            pattern,
                            key_var: Some(name),
                        });
                    }
                    // Shorthand: $foo is equivalent to foo: $foo
                    return Ok(PatternField {
                        key: PatternKey::Ident(name[1..].to_string()),
                        pattern: DestructurePattern::Var { name },
                        key_var: None,
                    });
                }
                // Regular key: name
                let name = name.clone();
                self.advance();
                if self.match_exact(&[TokenType::Colon]).is_some() {
                    let pattern = self.parse_pattern()?;
                    return Ok(PatternField {
                        key: PatternKey::Ident(name),
                        pattern,
                        key_var: None,
                    });
                }
                // Shorthand: key without colon means key: $key
                return Ok(PatternField {
                    key: PatternKey::Ident(name.clone()),
                    pattern: DestructurePattern::Var {
                        name: format!("${}", name),
                    },
                    key_var: None,
                });
            }
        }

        Err(format!(
            "Expected field name in object pattern at position {}",
            self.peek().pos
        ))
    }

    fn parse_object_construction(&mut self) -> Result<AstNode, String> {
        let mut entries: Vec<ObjectEntry> = Vec::new();

        if !self.check_exact(&TokenType::RBrace) {
            loop {
                let key: ObjectKey;
                let value: AstNode;

                // Dynamic key: (expr): value
                if self.match_exact(&[TokenType::LParen]).is_some() {
                    let key_expr = self.parse_expr()?;
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    self.expect(&TokenType::Colon, "Expected ':'")?;
                    value = self.parse_object_value()?;
                    key = ObjectKey::Expr(key_expr);
                } else if self.check(&TokenType::Ident(String::new())) {
                    let tok = self.advance();
                    let ident = match tok.token_type {
                        TokenType::Ident(s) => s,
                        _ => unreachable!(),
                    };
                    if self.match_exact(&[TokenType::Colon]).is_some() {
                        key = ObjectKey::Ident(ident);
                        value = self.parse_object_value()?;
                    } else {
                        // Shorthand: {key} means {key: .key}
                        key = ObjectKey::Ident(ident.clone());
                        value = AstNode::Field {
                            name: ident,
                            base: None,
                        };
                    }
                } else if self.check(&TokenType::Str(String::new())) {
                    let tok = self.advance();
                    let s = match tok.token_type {
                        TokenType::Str(s) => s,
                        _ => unreachable!(),
                    };
                    self.expect(&TokenType::Colon, "Expected ':'")?;
                    value = self.parse_object_value()?;
                    key = ObjectKey::Ident(s);
                } else {
                    return Err(format!(
                        "Expected object key at position {}",
                        self.peek().pos
                    ));
                }

                entries.push(ObjectEntry::KeyValue { key, value });

                if self.match_exact(&[TokenType::Comma]).is_none() {
                    break;
                }
            }
        }

        self.expect(&TokenType::RBrace, "Expected '}'")?;
        Ok(AstNode::Object { entries })
    }

    // Parse object value - allows pipes but stops at comma or rbrace
    fn parse_object_value(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_var_bind()?;
        while self.match_exact(&[TokenType::Pipe]).is_some() {
            let right = self.parse_var_bind()?;
            left = AstNode::Pipe {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_if(&mut self) -> Result<AstNode, String> {
        let cond = self.parse_expr()?;
        self.expect(&TokenType::Then, "Expected 'then'")?;
        let then_branch = self.parse_expr()?;

        let mut elif_branches: Vec<(AstNode, AstNode)> = Vec::new();
        while self.match_exact(&[TokenType::Elif]).is_some() {
            let elif_cond = self.parse_expr()?;
            self.expect(&TokenType::Then, "Expected 'then' after elif")?;
            let elif_then = self.parse_expr()?;
            elif_branches.push((elif_cond, elif_then));
        }

        let else_branch = if self.match_exact(&[TokenType::Else]).is_some() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.expect(&TokenType::End, "Expected 'end'")?;
        Ok(AstNode::Cond {
            cond: Box::new(cond),
            then_branch: Box::new(then_branch),
            elif_branches,
            else_branch,
        })
    }

    fn parse_string_interpolation(&mut self, s: &str) -> Result<AstNode, String> {
        let mut parts: Vec<StringPart> = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '(' {
                if !current.is_empty() {
                    parts.push(StringPart::Literal(current.clone()));
                    current.clear();
                }
                i += 2;
                // Find matching paren
                let mut depth = 1;
                let mut expr_str = String::new();
                while i < chars.len() && depth > 0 {
                    if chars[i] == '(' {
                        depth += 1;
                    } else if chars[i] == ')' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        expr_str.push(chars[i]);
                    }
                    i += 1;
                }
                let tokens = tokenize(&expr_str)?;
                let mut parser = Parser::new(tokens);
                let expr = parser.parse()?;
                parts.push(StringPart::Expr(expr));
            } else {
                current.push(chars[i]);
                i += 1;
            }
        }

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        Ok(AstNode::StringInterp { parts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_identity() {
        let ast = parse(".").unwrap();
        assert!(matches!(ast, AstNode::Identity));
    }

    #[test]
    fn test_parse_field() {
        let ast = parse(".foo").unwrap();
        match ast {
            AstNode::Field { name, base } => {
                assert_eq!(name, "foo");
                assert!(base.is_none());
            }
            other => panic!("Expected Field, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_index() {
        let ast = parse(".[0]").unwrap();
        match ast {
            AstNode::Index { base, index } => {
                assert!(base.is_none());
                match *index {
                    AstNode::Literal { value: Value::Number(n) } => assert_eq!(n, 0.0),
                    other => panic!("Expected Number literal, got {:?}", other),
                }
            }
            other => panic!("Expected Index, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_iterate() {
        let ast = parse(".[]").unwrap();
        match ast {
            AstNode::Iterate { base } => assert!(base.is_none()),
            other => panic!("Expected Iterate, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_slice() {
        let ast = parse(".[1:3]").unwrap();
        match ast {
            AstNode::Slice { base, start, end } => {
                assert!(base.is_none());
                assert!(start.is_some());
                assert!(end.is_some());
            }
            other => panic!("Expected Slice, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_recurse() {
        let ast = parse("..").unwrap();
        assert!(matches!(ast, AstNode::Recurse));
    }

    #[test]
    fn test_parse_pipe() {
        let ast = parse(".foo | .bar").unwrap();
        assert!(matches!(ast, AstNode::Pipe { .. }));
    }

    #[test]
    fn test_parse_comma() {
        let ast = parse(".a, .b").unwrap();
        assert!(matches!(ast, AstNode::Comma { .. }));
    }

    #[test]
    fn test_parse_binary_add() {
        let ast = parse("1 + 2").unwrap();
        match ast {
            AstNode::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Add),
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_binary_eq() {
        let ast = parse("1 == 2").unwrap();
        match ast {
            AstNode::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Eq),
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_binary_and() {
        let ast = parse(".x and .y").unwrap();
        match ast {
            AstNode::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::And),
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_binary_alt() {
        let ast = parse(".x // .y").unwrap();
        match ast {
            AstNode::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Alt),
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_unary_neg() {
        let ast = parse("-1").unwrap();
        match ast {
            AstNode::UnaryOp { op, .. } => assert_eq!(op, UnaryOp::Neg),
            other => panic!("Expected UnaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_not_call() {
        let ast = parse("not").unwrap();
        match ast {
            AstNode::Call { name, args } => {
                assert_eq!(name, "not");
                assert!(args.is_empty());
            }
            other => panic!("Expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_literal_true() {
        let ast = parse("true").unwrap();
        match ast {
            AstNode::Literal { value: Value::Bool(b) } => assert!(b),
            other => panic!("Expected Literal true, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_literal_false() {
        let ast = parse("false").unwrap();
        match ast {
            AstNode::Literal { value: Value::Bool(b) } => assert!(!b),
            other => panic!("Expected Literal false, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_literal_null() {
        let ast = parse("null").unwrap();
        assert!(matches!(ast, AstNode::Literal { value: Value::Null }));
    }

    #[test]
    fn test_parse_literal_string() {
        let ast = parse("\"hello\"").unwrap();
        match ast {
            AstNode::Literal { value: Value::String(s) } => assert_eq!(s, "hello"),
            other => panic!("Expected Literal String, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_literal_number() {
        let ast = parse("42").unwrap();
        match ast {
            AstNode::Literal { value: Value::Number(n) } => assert_eq!(n, 42.0),
            other => panic!("Expected Literal Number, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_array() {
        let ast = parse("[1, 2]").unwrap();
        assert!(matches!(ast, AstNode::Array { elements: Some(_) }));
    }

    #[test]
    fn test_parse_object_key_value() {
        let ast = parse("{a: 1}").unwrap();
        match ast {
            AstNode::Object { entries } => {
                assert_eq!(entries.len(), 1);
                match &entries[0] {
                    ObjectEntry::KeyValue { key, .. } => match key {
                        ObjectKey::Ident(s) => assert_eq!(s, "a"),
                        other => panic!("Expected Ident key, got {:?}", other),
                    },
                }
            }
            other => panic!("Expected Object, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_object_shorthand() {
        let ast = parse("{a}").unwrap();
        match ast {
            AstNode::Object { entries } => {
                assert_eq!(entries.len(), 1);
                match &entries[0] {
                    ObjectEntry::KeyValue { key, value } => {
                        match key {
                            ObjectKey::Ident(s) => assert_eq!(s, "a"),
                            other => panic!("Expected Ident key, got {:?}", other),
                        }
                        // Shorthand: value should be Field { name: "a" }
                        match value {
                            AstNode::Field { name, base } => {
                                assert_eq!(name, "a");
                                assert!(base.is_none());
                            }
                            other => panic!("Expected Field value, got {:?}", other),
                        }
                    }
                }
            }
            other => panic!("Expected Object, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_then_else() {
        let ast = parse("if .x then .y else .z end").unwrap();
        match ast {
            AstNode::Cond {
                else_branch,
                elif_branches,
                ..
            } => {
                assert!(else_branch.is_some());
                assert!(elif_branches.is_empty());
            }
            other => panic!("Expected Cond, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_try_catch() {
        let ast = parse("try .x catch .y").unwrap();
        match ast {
            AstNode::Try { catch, .. } => assert!(catch.is_some()),
            other => panic!("Expected Try, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_optional() {
        let ast = parse(".x?").unwrap();
        assert!(matches!(ast, AstNode::Optional { .. }));
    }

    #[test]
    fn test_parse_var_ref() {
        let ast = parse("$var").unwrap();
        match ast {
            AstNode::VarRef { name } => assert_eq!(name, "$var"),
            other => panic!("Expected VarRef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_var_bind() {
        let ast = parse(".x as $v | .y").unwrap();
        match ast {
            AstNode::VarBind { name, .. } => assert_eq!(name, "$v"),
            other => panic!("Expected VarBind, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_reduce() {
        let ast = parse("reduce .[] as $x (0; . + $x)").unwrap();
        match ast {
            AstNode::Reduce { var_name, .. } => assert_eq!(var_name, "$x"),
            other => panic!("Expected Reduce, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_def() {
        let ast = parse("def f: .; f").unwrap();
        match ast {
            AstNode::Def { name, params, .. } => {
                assert_eq!(name, "f");
                assert!(params.is_empty());
            }
            other => panic!("Expected Def, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_call_no_args() {
        let ast = parse("length").unwrap();
        match ast {
            AstNode::Call { name, args } => {
                assert_eq!(name, "length");
                assert!(args.is_empty());
            }
            other => panic!("Expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_call_with_args() {
        let ast = parse("map(.x)").unwrap();
        match ast {
            AstNode::Call { name, args } => {
                assert_eq!(name, "map");
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_update_assign() {
        let ast = parse(".a = 5").unwrap();
        match ast {
            AstNode::UpdateOp { op, .. } => assert_eq!(op, UpdateOp::Assign),
            other => panic!("Expected UpdateOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_update_pipe() {
        let ast = parse(".a |= . + 1").unwrap();
        match ast {
            AstNode::UpdateOp { op, .. } => assert_eq!(op, UpdateOp::PipeUpdate),
            other => panic!("Expected UpdateOp, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_array() {
        let ast = parse("[]").unwrap();
        match ast {
            AstNode::Array { elements } => assert!(elements.is_none()),
            other => panic!("Expected empty Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_postfix_field_chain() {
        let ast = parse(".a.b.c").unwrap();
        // Should be Field { base: Field { base: Field { base: None, name: "a" }, name: "b" }, name: "c" }
        match ast {
            AstNode::Field { name, base } => {
                assert_eq!(name, "c");
                assert!(base.is_some());
            }
            other => panic!("Expected Field chain, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_postfix_index_on_field() {
        let ast = parse(".foo[0]").unwrap();
        match ast {
            AstNode::Index { base, .. } => {
                assert!(base.is_some());
                match *base.unwrap() {
                    AstNode::Field { name, .. } => assert_eq!(name, "foo"),
                    other => panic!("Expected Field base, got {:?}", other),
                }
            }
            other => panic!("Expected Index, got {:?}", other),
        }
    }
}