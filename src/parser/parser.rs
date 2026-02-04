//! Recursive Descent Parser for Bash Scripts
//!
//! This parser consumes tokens from the lexer and produces an AST.
//! It follows the bash grammar structure for correctness.
//!
//! Grammar (simplified):
//!   script       ::= statement*
//!   statement    ::= pipeline ((&&|'||') pipeline)*  [&]
//!   pipeline     ::= [!] command (| command)*
//!   command      ::= simple_command | compound_command | function_def
//!   simple_cmd   ::= (assignment)* [word] (word)* (redirection)*
//!   compound_cmd ::= if | for | while | until | case | subshell | group | (( | [[

use crate::ast::types::{
    AST, ArithmeticExpressionNode, CommandNode, CompoundCommandNode, DeferredError,
    PipelineNode, RedirectionNode, ScriptNode, StatementNode, StatementOperator, WordNode,
    WordPart,
};
use crate::parser::lexer::{Lexer, Token, TokenType};
use crate::parser::parser_substitution::{
    parse_backtick_substitution_from_string, parse_command_substitution_from_string,
    ErrorFn, ParserFactory,
};
use crate::parser::types::ParseException;
use crate::parser::word_parser::parse_arith_expr_from_string;
use crate::parser::expansion_parser::{parse_word_parts, ExpansionContext};
use crate::parser::conditional_parser::{parse_conditional_expression, CondParserContext, CondToken};
use std::cell::RefCell;

// Constants for limits
pub const MAX_INPUT_SIZE: usize = 10_000_000;
pub const MAX_TOKENS: usize = 100_000;
pub const MAX_PARSE_ITERATIONS: usize = 1_000_000;

/// Pending heredoc information
#[derive(Debug, Clone)]
struct PendingHeredoc {
    redirect_idx: usize,
    delimiter: String,
    strip_tabs: bool,
    quoted: bool,
}

// Independent helper functions for expansion parsing (don't require Parser self)

/// Parse command substitution from a string - standalone version for ExpansionContext
fn standalone_parse_command_substitution(value: &str, start: usize) -> (Option<WordPart>, usize) {
    let create_parser: ParserFactory = |input| {
        let mut p = Parser::new();
        p.parse(input).unwrap_or_else(|_| AST::script(vec![]))
    };
    let error: ErrorFn = |msg| {
        eprintln!("Parse error: {}", msg);
    };

    let (part, end_idx) = parse_command_substitution_from_string(value, start, create_parser, error);
    (Some(WordPart::CommandSubstitution(part)), end_idx)
}

/// Parse backtick substitution from a string - standalone version for ExpansionContext
fn standalone_parse_backtick_substitution(value: &str, start: usize, in_double_quotes: bool) -> (WordPart, usize) {
    let create_parser: ParserFactory = |input| {
        let mut p = Parser::new();
        p.parse(input).unwrap_or_else(|_| AST::script(vec![]))
    };
    let error: ErrorFn = |msg| {
        eprintln!("Parse error: {}", msg);
    };

    let (part, end_idx) = parse_backtick_substitution_from_string(value, start, in_double_quotes, create_parser, error);
    (WordPart::CommandSubstitution(part), end_idx)
}

/// Parse arithmetic expansion from a string - standalone version for ExpansionContext
fn standalone_parse_arithmetic_expansion(value: &str, start: usize) -> (Option<WordPart>, usize) {
    let chars: Vec<char> = value.chars().collect();
    let expr_start = start + 3;
    let mut arith_depth = 1;
    let mut paren_depth = 0;
    let mut i = expr_start;

    while i < value.len().saturating_sub(1) && arith_depth > 0 {
        if i + 3 <= value.len() && &value[i..i+3] == "$((" {
            arith_depth += 1;
            i += 3;
        } else if i + 2 <= value.len() && &value[i..i+2] == "$(" {
            paren_depth += 1;
            i += 2;
        } else if i + 2 <= value.len() && &value[i..i+2] == "))" {
            if paren_depth > 0 {
                paren_depth -= 1;
                i += 1;
            } else {
                arith_depth -= 1;
                if arith_depth > 0 {
                    i += 2;
                }
            }
        } else if chars.get(i) == Some(&'(') {
            paren_depth += 1;
            i += 1;
        } else if chars.get(i) == Some(&')') {
            if paren_depth > 0 {
                paren_depth -= 1;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let expr_str: String = value.chars().skip(expr_start).take(i - expr_start).collect();
    let expression = parse_arith_expr_from_string(&expr_str);

    (
        Some(WordPart::ArithmeticExpansion(crate::ast::types::ArithmeticExpansionPart { expression })),
        i + 2,
    )
}

/// Check if $(( is a subshell - standalone version for ExpansionContext
fn standalone_is_dollar_dparen_subshell(value: &str, start: usize) -> bool {
    crate::parser::parser_substitution::is_dollar_dparen_subshell(value, start)
}

/// Create an ExpansionContext with the standalone parsing functions
fn create_expansion_context<'a>() -> ExpansionContext<'a> {
    ExpansionContext {
        parse_command_substitution: &standalone_parse_command_substitution,
        parse_backtick_substitution: &standalone_parse_backtick_substitution,
        parse_arithmetic_expansion: &standalone_parse_arithmetic_expansion,
        is_dollar_dparen_subshell: &standalone_is_dollar_dparen_subshell,
        report_error: &|msg| eprintln!("Parse error: {}", msg),
    }
}

/// Main parser struct
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pending_heredocs: Vec<PendingHeredoc>,
    pending_redirections: Vec<RedirectionNode>,
    parse_iterations: usize,
    input: String,
}

impl Parser {
    /// Create a new parser instance
    pub fn new() -> Self {
        Parser {
            tokens: Vec::new(),
            pos: 0,
            pending_heredocs: Vec::new(),
            pending_redirections: Vec::new(),
            parse_iterations: 0,
            input: String::new(),
        }
    }

    /// Get the raw input string being parsed.
    pub fn get_input(&self) -> &str {
        &self.input
    }

    /// Check parse iteration limit to prevent infinite loops
    pub fn check_iteration_limit(&mut self) -> Result<(), ParseException> {
        self.parse_iterations += 1;
        if self.parse_iterations > MAX_PARSE_ITERATIONS {
            return Err(ParseException::new(
                "Maximum parse iterations exceeded (possible infinite loop)",
                self.current().line,
                self.current().column,
            ));
        }
        Ok(())
    }

    /// Parse a bash script string
    pub fn parse(&mut self, input: &str) -> Result<ScriptNode, ParseException> {
        // Check input size limit
        if input.len() > MAX_INPUT_SIZE {
            return Err(ParseException::new(
                &format!("Input too large: {} bytes exceeds limit of {}", input.len(), MAX_INPUT_SIZE),
                1,
                1,
            ));
        }

        self.input = input.to_string();
        let mut lexer = Lexer::new(input);
        self.tokens = lexer.tokenize().map_err(|e| ParseException::new(&e.message, e.line, e.column))?;

        // Check token count limit
        if self.tokens.len() > MAX_TOKENS {
            return Err(ParseException::new(
                &format!("Too many tokens: {} exceeds limit of {}", self.tokens.len(), MAX_TOKENS),
                1,
                1,
            ));
        }

        self.pos = 0;
        self.pending_heredocs = Vec::new();
        self.pending_redirections = Vec::new();
        self.parse_iterations = 0;

        self.parse_script()
    }

    /// Parse from pre-tokenized input
    pub fn parse_tokens(&mut self, tokens: Vec<Token>) -> Result<ScriptNode, ParseException> {
        self.tokens = tokens;
        self.pos = 0;
        self.pending_heredocs = Vec::new();
        self.pending_redirections = Vec::new();
        self.parse_iterations = 0;

        self.parse_script()
    }

    // ===========================================================================
    // HELPER METHODS
    // ===========================================================================

    fn current(&self) -> Token {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].clone()
        } else if !self.tokens.is_empty() {
            self.tokens[self.tokens.len() - 1].clone()
        } else {
            Token {
                token_type: TokenType::Eof,
                value: String::new(),
                line: 1,
                column: 1,
                start: 0,
                end: 0,
                quoted: false,
                single_quoted: false,
            }
        }
    }

    fn peek(&self, offset: usize) -> Token {
        let idx = self.pos + offset;
        if idx < self.tokens.len() {
            self.tokens[idx].clone()
        } else if !self.tokens.is_empty() {
            self.tokens[self.tokens.len() - 1].clone()
        } else {
            Token {
                token_type: TokenType::Eof,
                value: String::new(),
                line: 1,
                column: 1,
                start: 0,
                end: 0,
                quoted: false,
                single_quoted: false,
            }
        }
    }

    fn advance(&mut self) -> Token {
        let token = self.current();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn get_pos(&self) -> usize {
        self.pos
    }

    fn check(&self, types: &[TokenType]) -> bool {
        let current_type = self.tokens.get(self.pos).map(|t| &t.token_type);
        types.iter().any(|t| current_type == Some(t))
    }

    fn expect(&mut self, token_type: TokenType, message: Option<&str>) -> Result<Token, ParseException> {
        if self.check(&[token_type]) {
            Ok(self.advance())
        } else {
            let token = self.current();
            Err(ParseException::new(
                message.unwrap_or(&format!("Expected {:?}, got {:?}", token_type, token.token_type)),
                token.line,
                token.column,
            ))
        }
    }

    fn error(&self, message: &str) -> Result<(), ParseException> {
        let token = self.current();
        Err(ParseException::new(message, token.line, token.column))
    }

    fn skip_newlines(&mut self) {
        while self.check(&[TokenType::Newline, TokenType::Comment]) {
            if self.check(&[TokenType::Newline]) {
                self.advance();
                self.process_heredocs();
            } else {
                self.advance();
            }
        }
    }

    fn skip_separators(&mut self, include_case_terminators: bool) {
        loop {
            if self.check(&[TokenType::Newline]) {
                self.advance();
                self.process_heredocs();
                continue;
            }
            if self.check(&[TokenType::Semicolon, TokenType::Comment]) {
                self.advance();
                continue;
            }
            if include_case_terminators && self.check(&[TokenType::DSemi, TokenType::SemiAnd, TokenType::SemiSemiAnd]) {
                self.advance();
                continue;
            }
            break;
        }
    }

    fn add_pending_heredoc(
        &mut self,
        redirect_idx: usize,
        delimiter: String,
        strip_tabs: bool,
        quoted: bool,
    ) {
        self.pending_heredocs.push(PendingHeredoc {
            redirect_idx,
            delimiter,
            strip_tabs,
            quoted,
        });
    }

    fn process_heredocs(&mut self) {
        // Process pending here-documents
        let heredocs = std::mem::take(&mut self.pending_heredocs);
        for heredoc in heredocs {
            if self.check(&[TokenType::HeredocContent]) {
                let content = self.advance();
                let content_word = if heredoc.quoted {
                    AST::word(vec![AST::literal(&content.value)])
                } else {
                    self.parse_word_from_string(&content.value, false, false, false, true)
                };

                if let Some(redirection) = self.pending_redirections.get_mut(heredoc.redirect_idx) {
                    redirection.target = crate::ast::types::RedirectionTarget::HereDoc(
                        AST::here_doc(
                            &heredoc.delimiter,
                            content_word,
                            heredoc.strip_tabs,
                            heredoc.quoted,
                        ),
                    );
                }
            }
        }
    }

    fn is_statement_end(&self) -> bool {
        self.check(&[
            TokenType::Eof,
            TokenType::Newline,
            TokenType::Semicolon,
            TokenType::Amp,
            TokenType::AndAnd,
            TokenType::OrOr,
            TokenType::RParen,
            TokenType::RBrace,
            TokenType::DSemi,
            TokenType::SemiAnd,
            TokenType::SemiSemiAnd,
        ])
    }

    fn is_command_start(&self) -> bool {
        let t = self.current().token_type;
        matches!(
            t,
            TokenType::Word
                | TokenType::Name
                | TokenType::Number
                | TokenType::AssignmentWord
                | TokenType::If
                | TokenType::For
                | TokenType::While
                | TokenType::Until
                | TokenType::Case
                | TokenType::LParen
                | TokenType::LBrace
                | TokenType::DParenStart
                | TokenType::DBrackStart
                | TokenType::Function
                | TokenType::Bang
                | TokenType::Time
                | TokenType::In
                | TokenType::Less
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

    // ===========================================================================
    // SCRIPT PARSING
    // ===========================================================================

    fn parse_script(&mut self) -> Result<ScriptNode, ParseException> {
        let mut statements = Vec::new();
        let max_iterations = 10000;
        let mut iterations = 0;

        self.skip_newlines();

        while !self.check(&[TokenType::Eof]) {
            iterations += 1;
            if iterations > max_iterations {
                return Err(ParseException::new(&format!("Parser stuck: too many iterations (>{}))", max_iterations), self.current().line, self.current().column));
            }

            // Check for unexpected tokens at statement start
            if let Some(deferred_error_stmt) = self.check_unexpected_token()? {
                statements.push(deferred_error_stmt);
                self.skip_separators(false);
                continue;
            }

            let pos_before = self.pos;
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
            // Don't skip case terminators at script level
            self.skip_separators(false);

            // Check for case terminators at script level - syntax errors
            if self.check(&[TokenType::DSemi, TokenType::SemiAnd, TokenType::SemiSemiAnd]) {
                return Err(ParseException::new(&format!("syntax error near unexpected token `{}`", self.current().value), self.current().line, self.current().column));
            }

            // Safety: if we didn't advance, force advance
            if self.pos == pos_before && !self.check(&[TokenType::Eof]) {
                self.advance();
            }
        }

        Ok(AST::script(statements))
    }

    fn check_unexpected_token(&mut self) -> Result<Option<StatementNode>, ParseException> {
        let t = self.current().token_type;
        let v = self.current().value.clone();

        // Check for unexpected reserved words
        if matches!(
            t,
            TokenType::Do
                | TokenType::Done
                | TokenType::Then
                | TokenType::Else
                | TokenType::Elif
                | TokenType::Fi
                | TokenType::Esac
        ) {
            return Err(ParseException::new(&format!("syntax error near unexpected token `{}`", v), self.current().line, self.current().column));
        }

        // Check for unexpected closing braces/parens - deferred errors
        if t == TokenType::RBrace || t == TokenType::RParen {
            let error_msg = format!("syntax error near unexpected token `{}`", v);
            self.advance(); // Consume the token
            return Ok(Some(AST::statement(
                vec![AST::pipeline(vec![CommandNode::Simple(AST::simple_command(None, vec![], vec![], vec![]))], false, false, false, None)],
                vec![],
                false,
                Some(DeferredError {
                    message: error_msg.clone(),
                    token: v,
                }),
                None,
            )));
        }

        // Check for case terminators
        if matches!(t, TokenType::DSemi | TokenType::SemiAnd | TokenType::SemiSemiAnd) {
            return Err(ParseException::new(&format!("syntax error near unexpected token `{}`", v), self.current().line, self.current().column));
        }

        // Check for bare semicolon
        if t == TokenType::Semicolon {
            return Err(ParseException::new(&format!("syntax error near unexpected token `{}`", v), self.current().line, self.current().column));
        }

        // Check for pipe at statement start
        if t == TokenType::Pipe || t == TokenType::PipeAmp {
            return Err(ParseException::new(&format!("syntax error near unexpected token `{}`", v), self.current().line, self.current().column));
        }

        Ok(None)
    }

    // ===========================================================================
    // STATEMENT PARSING
    // ===========================================================================

    pub fn parse_statement(&mut self) -> Result<Option<StatementNode>, ParseException> {
        self.skip_newlines();

        if !self.is_command_start() {
            return Ok(None);
        }

        let start_offset = self.current().start;

        let mut pipelines = Vec::new();
        let mut operators = Vec::new();
        let mut background = false;

        // Parse first pipeline
        let first_pipeline = self.parse_pipeline()?;
        pipelines.push(first_pipeline);

        // Parse additional pipelines connected by && or ||
        while self.check(&[TokenType::AndAnd, TokenType::OrOr]) {
            let op = self.advance();
            operators.push(if op.token_type == TokenType::AndAnd {
                StatementOperator::And
            } else {
                StatementOperator::Or
            });
            self.skip_newlines();
            let next_pipeline = self.parse_pipeline()?;
            pipelines.push(next_pipeline);
        }

        // Check for background execution
        if self.check(&[TokenType::Amp]) {
            self.advance();
            background = true;
        }

        // Extract source text for verbose mode
        let end_offset = if self.pos > 0 && self.pos <= self.tokens.len() {
            self.tokens[self.pos - 1].end
        } else {
            start_offset
        };
        let source_text = self.input.get(start_offset..end_offset).map(|s| s.to_string());

        Ok(Some(AST::statement(
            pipelines,
            operators,
            background,
            None,
            source_text,
        )))
    }

    // ===========================================================================
    // PIPELINE PARSING
    // ===========================================================================

    fn parse_pipeline(&mut self) -> Result<PipelineNode, ParseException> {
        // Check for 'time' keyword
        let mut timed = false;
        let mut time_posix = false;
        if self.check(&[TokenType::Time]) {
            self.advance();
            timed = true;
            // Check for -p option
            if self.check(&[TokenType::Word, TokenType::Name]) && self.current().value == "-p" {
                self.advance();
                time_posix = true;
            }
        }

        // Check for ! (negation)
        let mut negation_count = 0;
        while self.check(&[TokenType::Bang]) {
            self.advance();
            negation_count += 1;
        }
        let negated = negation_count % 2 == 1;

        let mut commands = Vec::new();
        let mut pipe_stderr = Vec::new();

        // Parse first command
        let first_cmd = self.parse_command()?;
        commands.push(first_cmd);

        // Parse additional commands in pipeline
        while self.check(&[TokenType::Pipe, TokenType::PipeAmp]) {
            let pipe_token = self.advance();
            self.skip_newlines();
            pipe_stderr.push(pipe_token.token_type == TokenType::PipeAmp);
            let next_cmd = self.parse_command()?;
            commands.push(next_cmd);
        }

        Ok(AST::pipeline(
            commands,
            negated,
            timed,
            time_posix,
            if pipe_stderr.is_empty() { None } else { Some(pipe_stderr) },
        ))
    }

    // ===========================================================================
    // COMMAND PARSING
    // ===========================================================================

    fn parse_command(&mut self) -> Result<CommandNode, ParseException> {
        // Check for compound commands
        if self.check(&[TokenType::If]) {
            return Ok(self.parse_if()?);
        }
        if self.check(&[TokenType::For]) {
            return Ok(self.parse_for()?);
        }
        if self.check(&[TokenType::While]) {
            return Ok(self.parse_while()?);
        }
        if self.check(&[TokenType::Until]) {
            return Ok(self.parse_until()?);
        }
        if self.check(&[TokenType::Case]) {
            return Ok(self.parse_case()?);
        }
        if self.check(&[TokenType::LParen]) {
            return Ok(self.parse_subshell()?);
        }
        if self.check(&[TokenType::LBrace]) {
            return Ok(self.parse_group()?);
        }
        if self.check(&[TokenType::DParenStart]) {
            if self.dparen_closes_with_spaced_parens() {
                return Ok(self.parse_nested_subshells_from_dparen()?);
            }
            return Ok(self.parse_arithmetic_command()?);
        }
        if self.check(&[TokenType::DBrackStart]) {
            return Ok(self.parse_conditional_command()?);
        }
        if self.check(&[TokenType::Function]) {
            return Ok(self.parse_function_def()?);
        }

        // Check for function definition: name () { ... }
        if self.check(&[TokenType::Name, TokenType::Word])
            && self.peek(1).token_type == TokenType::LParen
            && self.peek(2).token_type == TokenType::RParen
        {
            return Ok(self.parse_function_def()?);
        }

        // Simple command
        Ok(self.parse_simple_command()?)
    }

    fn dparen_closes_with_spaced_parens(&self) -> bool {
        let mut depth = 1;
        let mut offset = 1;

        while self.pos + offset < self.tokens.len() {
            let tok = self.peek(offset);
            if tok.token_type == TokenType::Eof {
                return false;
            }

            if matches!(tok.token_type, TokenType::DParenStart | TokenType::LParen) {
                depth += 1;
            } else if tok.token_type == TokenType::DParenEnd {
                depth -= 2;
                if depth <= 0 {
                    return false;
                }
            } else if tok.token_type == TokenType::RParen {
                depth -= 1;
                if depth == 0 {
                    let next_tok = self.peek(offset + 1);
                    if next_tok.token_type == TokenType::RParen {
                        return true;
                    }
                }
            }
            offset += 1;
        }

        false
    }

    fn parse_nested_subshells_from_dparen(&mut self) -> Result<CommandNode, ParseException> {
        self.advance(); // Skip DPAREN_START

        let inner_body = self.parse_compound_list()?;

        self.expect(TokenType::RParen, None)?;
        self.expect(TokenType::RParen, None)?;

        let redirections = self.parse_optional_redirections()?;

        let inner_subshell = AST::subshell(inner_body, vec![]);

        Ok(CommandNode::Compound(CompoundCommandNode::Subshell(
            AST::subshell(
                vec![AST::statement(vec![AST::pipeline(vec![CommandNode::Compound(CompoundCommandNode::Subshell(inner_subshell))], false, false, false, None)], vec![], false, None, None)],
                redirections,
            ),
        )))
    }

    // ===========================================================================
    // WORD PARSING
    // ===========================================================================

    pub fn is_word(&self) -> bool {
        let t = self.current().token_type;
        matches!(
            t,
            TokenType::Word
                | TokenType::Name
                | TokenType::Number
                | TokenType::If
                | TokenType::For
                | TokenType::While
                | TokenType::Until
                | TokenType::Case
                | TokenType::Function
                | TokenType::Else
                | TokenType::Elif
                | TokenType::Fi
                | TokenType::Then
                | TokenType::Do
                | TokenType::Done
                | TokenType::Esac
                | TokenType::In
                | TokenType::Select
                | TokenType::Time
                | TokenType::Coproc
                | TokenType::Bang
        )
    }

    pub fn parse_word(&mut self) -> Result<WordNode, ParseException> {
        let token = self.advance();
        Ok(self.parse_word_from_string(&token.value, token.quoted, token.single_quoted, false, false))
    }

    pub fn parse_word_no_brace_expansion(&mut self) -> Result<WordNode, ParseException> {
        let token = self.advance();
        Ok(self.parse_word_from_string(&token.value, token.quoted, token.single_quoted, false, false))
    }

    pub fn parse_word_for_regex(&mut self) -> Result<WordNode, ParseException> {
        let token = self.advance();
        Ok(self.parse_word_from_string(&token.value, token.quoted, token.single_quoted, false, false))
    }

    pub fn parse_word_from_string(
        &mut self,
        value: &str,
        quoted: bool,
        single_quoted: bool,
        is_assignment: bool,
        here_doc: bool,
    ) -> WordNode {
        self.parse_word_from_string_full(value, quoted, single_quoted, is_assignment, here_doc, false, false)
    }

    /// Parse a word from string with all options
    pub fn parse_word_from_string_full(
        &mut self,
        value: &str,
        quoted: bool,
        single_quoted: bool,
        is_assignment: bool,
        here_doc: bool,
        no_brace_expansion: bool,
        regex_pattern: bool,
    ) -> WordNode {
        let ctx = create_expansion_context();
        let parts = parse_word_parts(
            &ctx,
            value,
            quoted,
            single_quoted,
            is_assignment,
            here_doc,
            false, // single_quotes_are_literal
            no_brace_expansion,
            regex_pattern,
            false, // in_parameter_expansion
        );
        AST::word(parts)
    }

    fn do_parse_command_substitution(&mut self, value: &str, start: usize) -> (Option<crate::ast::types::WordPart>, usize) {
        standalone_parse_command_substitution(value, start)
    }

    fn do_parse_backtick_substitution(&mut self, value: &str, start: usize, in_double_quotes: bool) -> (crate::ast::types::WordPart, usize) {
        standalone_parse_backtick_substitution(value, start, in_double_quotes)
    }

    fn do_parse_arithmetic_expansion(&mut self, value: &str, start: usize) -> (Option<crate::ast::types::WordPart>, usize) {
        standalone_parse_arithmetic_expansion(value, start)
    }

    pub fn parse_command_substitution(&mut self, value: &str, start: usize) -> Result<(crate::ast::types::CommandSubstitutionPart, usize), ParseException> {
        let create_parser: ParserFactory = |input| {
            let mut p = Parser::new();
            p.parse(input).unwrap_or_else(|_| AST::script(vec![]))
        };
        // Use a simple error fn that just panics - the real error handling is done elsewhere
        let error: ErrorFn = |msg| {
            panic!("Parse error: {}", msg);
        };
        Ok(parse_command_substitution_from_string(value, start, create_parser, error))
    }

    pub fn parse_backtick_substitution(&mut self, value: &str, start: usize, in_double_quotes: bool) -> Result<(crate::ast::types::CommandSubstitutionPart, usize), ParseException> {
        let create_parser: ParserFactory = |input| {
            let mut p = Parser::new();
            p.parse(input).unwrap_or_else(|_| AST::script(vec![]))
        };
        let error: ErrorFn = |msg| {
            panic!("Parse error: {}", msg);
        };
        Ok(parse_backtick_substitution_from_string(value, start, in_double_quotes, create_parser, error))
    }

    pub fn is_dollar_dparen_subshell(&self, value: &str, start: usize) -> bool {
        crate::parser::parser_substitution::is_dollar_dparen_subshell(value, start)
    }

    pub fn parse_arithmetic_expansion(&mut self, value: &str, start: usize) -> Result<(crate::ast::types::ArithmeticExpansionPart, usize), ParseException> {
        let chars: Vec<char> = value.chars().collect();
        let expr_start = start + 3;
        let mut arith_depth = 1;
        let mut paren_depth = 0;
        let mut i = expr_start;

        while i < value.len().saturating_sub(1) && arith_depth > 0 {
            if value[i..].starts_with("$((") {
                arith_depth += 1;
                i += 3;
            } else if value[i..].starts_with("$((") {
                paren_depth += 1;
                i += 2;
            } else if value[i..].starts_with("))") {
                if paren_depth > 0 {
                    paren_depth -= 1;
                    i += 1;
                } else {
                    arith_depth -= 1;
                    if arith_depth > 0 {
                        i += 2;
                    }
                }
            } else if value.chars().nth(i) == Some('(') {
                paren_depth += 1;
                i += 1;
            } else if value.chars().nth(i) == Some(')') {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                i += 1;
            } else {
                i += 1;
            }
        }

        let expr_str: String = value.chars().skip(expr_start).take(i - expr_start).collect();
        let expression = parse_arith_expr_from_string(&expr_str);

        Ok((
            crate::ast::types::ArithmeticExpansionPart { expression },
            i + 2,
        ))
    }

    fn parse_arithmetic_command(&mut self) -> Result<CommandNode, ParseException> {
        let start_token = self.expect(TokenType::DParenStart, None)?;

        let mut expr_str = String::new();
        let mut dparen_depth = 1;
        let mut paren_depth = 0;
        let mut pending_rparen = false;
        let mut found_closing = false;

        while dparen_depth > 0 && !self.check(&[TokenType::Eof]) {
            if pending_rparen {
                pending_rparen = false;
                if paren_depth > 0 {
                    paren_depth -= 1;
                    expr_str.push(')');
                    continue;
                }
                if self.check(&[TokenType::RParen]) {
                    dparen_depth -= 1;
                    found_closing = true;
                    self.advance();
                    continue;
                }
                if self.check(&[TokenType::DParenEnd]) {
                    dparen_depth -= 1;
                    found_closing = true;
                    continue;
                }
                expr_str.push(')');
                continue;
            }

            if self.check(&[TokenType::DParenStart]) {
                dparen_depth += 1;
                expr_str.push_str("((");
                self.advance();
            } else if self.check(&[TokenType::DParenEnd]) {
                if paren_depth >= 2 {
                    paren_depth -= 2;
                    expr_str.push_str("))");
                    self.advance();
                } else if paren_depth == 1 {
                    paren_depth -= 1;
                    expr_str.push(')');
                    pending_rparen = true;
                    self.advance();
                } else {
                    dparen_depth -= 1;
                    found_closing = true;
                    if dparen_depth > 0 {
                        expr_str.push_str("))");
                    }
                    self.advance();
                }
            } else if self.check(&[TokenType::LParen]) {
                paren_depth += 1;
                expr_str.push('(');
                self.advance();
            } else if self.check(&[TokenType::RParen]) {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                expr_str.push(')');
                self.advance();
            } else {
                let token_value = self.current().value.clone();
                let last_char = expr_str.chars().last().unwrap_or(' ');

                let needs_space = !expr_str.is_empty()
                    && !expr_str.ends_with(' ')
                    && !(token_value == "=" && expr_str.ends_with(|c: char| "|&^+*-*/%<>".contains(c)))
                    && !(token_value == "<" && last_char == '<')
                    && !(token_value == ">" && last_char == '>');

                if needs_space {
                    expr_str.push(' ');
                }
                expr_str.push_str(&token_value);
                self.advance();
            }
        }

        if !found_closing {
            self.expect(TokenType::DParenEnd, None)?;
        }

        let expression = parse_arith_expr_from_string(expr_str.trim());
        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::ArithmeticCommand(
            AST::arithmetic_command(expression, redirections, Some(start_token.line)),
        )))
    }

    fn parse_conditional_command(&mut self) -> Result<CommandNode, ParseException> {
        let start_token = self.expect(TokenType::DBrackStart, None)?;

        let expression = self.do_parse_conditional_expression()?;

        self.expect(TokenType::DBrackEnd, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::ConditionalCommand(
            AST::conditional_command(expression, redirections, Some(start_token.line)),
        )))
    }

    fn do_parse_conditional_expression(&mut self) -> Result<crate::ast::types::ConditionalExpressionNode, ParseException> {
        // Use RefCell to allow multiple borrows in closures
        let parser = RefCell::new(self);

        let is_word = || {
            parser.borrow().is_word()
        };

        let check = |tt: TokenType| {
            parser.borrow().check(&[tt])
        };

        let peek = |offset: isize| {
            let p = parser.borrow();
            let idx = if offset >= 0 {
                p.pos.saturating_add(offset as usize)
            } else {
                p.pos.saturating_sub((-offset) as usize)
            };
            if idx < p.tokens.len() {
                let t = &p.tokens[idx];
                CondToken {
                    token_type: t.token_type,
                    value: t.value.clone(),
                    quoted: t.quoted,
                    start: t.start,
                    end: t.end,
                }
            } else {
                CondToken {
                    token_type: TokenType::Eof,
                    value: String::new(),
                    quoted: false,
                    start: 0,
                    end: 0,
                }
            }
        };

        let current = || {
            let p = parser.borrow();
            let t = p.current();
            CondToken {
                token_type: t.token_type,
                value: t.value.clone(),
                quoted: t.quoted,
                start: t.start,
                end: t.end,
            }
        };

        let advance = || {
            let mut p = parser.borrow_mut();
            let t = p.advance();
            CondToken {
                token_type: t.token_type,
                value: t.value.clone(),
                quoted: t.quoted,
                start: t.start,
                end: t.end,
            }
        };

        let expect = |tt: TokenType| {
            let mut p = parser.borrow_mut();
            let _ = p.expect(tt, None);
        };

        let skip_newlines = || {
            parser.borrow_mut().skip_newlines();
        };

        let parse_word_no_brace_expansion = || {
            parser.borrow_mut().parse_word_no_brace_expansion().unwrap_or_else(|_| AST::word(vec![]))
        };

        let parse_word_for_regex = || {
            parser.borrow_mut().parse_word_for_regex().unwrap_or_else(|_| AST::word(vec![]))
        };

        let parse_word_from_string_fn = |value: &str, quoted: bool, single_quoted: bool, is_assignment: bool, here_doc: bool, no_brace_expansion: bool| {
            parser.borrow_mut().parse_word_from_string_full(value, quoted, single_quoted, is_assignment, here_doc, no_brace_expansion, false)
        };

        let get_input = || {
            parser.borrow().get_input().to_string()
        };

        let error = |msg: &str| {
            eprintln!("Conditional parse error: {}", msg);
        };

        let ctx = CondParserContext {
            is_word: &is_word,
            check: &check,
            peek: &peek,
            current: &current,
            advance: &advance,
            expect: &expect,
            skip_newlines: &skip_newlines,
            parse_word_no_brace_expansion: &parse_word_no_brace_expansion,
            parse_word_for_regex: &parse_word_for_regex,
            parse_word_from_string: &parse_word_from_string_fn,
            get_input: &get_input,
            error: &error,
        };

        Ok(parse_conditional_expression(&ctx))
    }

    fn parse_function_def(&mut self) -> Result<CommandNode, ParseException> {
        let name: String;

        if self.check(&[TokenType::Function]) {
            self.advance();
            if self.check(&[TokenType::Name, TokenType::Word]) {
                name = self.advance().value;
            } else {
                return Err(ParseException::new("Expected function name", self.current().line, self.current().column));
            }

            // Optional ()
            if self.check(&[TokenType::LParen]) {
                self.advance();
                self.expect(TokenType::RParen, None)?;
            }
        } else {
            name = self.advance().value;
            if name.contains('$') {
                return Err(ParseException::new(&format!("`{}': not a valid identifier", name), self.current().line, self.current().column));
            }
            self.expect(TokenType::LParen, None)?;
            self.expect(TokenType::RParen, None)?;
        }

        self.skip_newlines();

        let body = self.parse_compound_command_body(true)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::FunctionDef(
            AST::function_def(name, body, redirections, None),
        ))
    }

    fn parse_compound_command_body(&mut self, _for_function_body: bool) -> Result<CompoundCommandNode, ParseException> {
        let cmd = if self.check(&[TokenType::LBrace]) {
            self.parse_group()?
        } else if self.check(&[TokenType::LParen]) {
            self.parse_subshell()?
        } else if self.check(&[TokenType::If]) {
            self.parse_if()?
        } else if self.check(&[TokenType::For]) {
            self.parse_for()?
        } else if self.check(&[TokenType::While]) {
            self.parse_while()?
        } else if self.check(&[TokenType::Until]) {
            self.parse_until()?
        } else if self.check(&[TokenType::Case]) {
            self.parse_case()?
        } else {
            return Err(ParseException::new("Expected compound command for function body", self.current().line, self.current().column));
        };

        match cmd {
            CommandNode::Compound(compound) => Ok(compound),
            _ => Err(ParseException::new("Expected compound command for function body", self.current().line, self.current().column)),
        }
    }

    // ===========================================================================
    // COMPOUND COMMAND PARSING
    // ===========================================================================

    fn parse_if(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::If, None)?;

        let condition = self.parse_compound_list()?;

        self.expect(TokenType::Then, None)?;
        let mut then_body = Vec::new();
        while !self.check(&[TokenType::Eof, TokenType::Fi, TokenType::Elif, TokenType::Else]) {
            if let Some(stmt) = self.parse_statement()? {
                then_body.push(stmt);
            }
            self.skip_separators(true);
        }

        // Empty body is a syntax error in bash
        if then_body.is_empty() {
            let next_tok = if self.check(&[TokenType::Fi]) {
                "fi"
            } else if self.check(&[TokenType::Else]) {
                "else"
            } else if self.check(&[TokenType::Elif]) {
                "elif"
            } else {
                "fi"
            };
            return Err(ParseException::new(
                &format!("syntax error near unexpected token `{}'", next_tok),
                self.current().line,
                self.current().column,
            ));
        }

        let mut clauses = vec![crate::ast::types::IfClause {
            condition,
            body: then_body,
        }];

        // Parse elif clauses
        while self.check(&[TokenType::Elif]) {
            self.advance();
            let elif_condition = self.parse_compound_list()?;
            self.expect(TokenType::Then, None)?;
            let mut elif_body = Vec::new();
            while !self.check(&[TokenType::Eof, TokenType::Fi, TokenType::Elif, TokenType::Else]) {
                if let Some(stmt) = self.parse_statement()? {
                    elif_body.push(stmt);
                }
                self.skip_separators(true);
            }
            // Empty elif body is a syntax error
            if elif_body.is_empty() {
                let next_tok = if self.check(&[TokenType::Fi]) {
                    "fi"
                } else if self.check(&[TokenType::Else]) {
                    "else"
                } else if self.check(&[TokenType::Elif]) {
                    "elif"
                } else {
                    "fi"
                };
                return Err(ParseException::new(
                    &format!("syntax error near unexpected token `{}'", next_tok),
                    self.current().line,
                    self.current().column,
                ));
            }
            clauses.push(crate::ast::types::IfClause {
                condition: elif_condition,
                body: elif_body,
            });
        }

        // Parse else clause
        let mut else_body = None;
        if self.check(&[TokenType::Else]) {
            self.advance();
            let mut body = Vec::new();
            while !self.check(&[TokenType::Eof, TokenType::Fi]) {
                if let Some(stmt) = self.parse_statement()? {
                    body.push(stmt);
                }
                self.skip_separators(true);
            }
            // Empty else body is a syntax error
            if body.is_empty() {
                return Err(ParseException::new(
                    "syntax error near unexpected token `fi'",
                    self.current().line,
                    self.current().column,
                ));
            }
            else_body = Some(body);
        }

        self.expect(TokenType::Fi, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::If(
            AST::if_node(clauses, else_body, redirections),
        )))
    }

    fn parse_for(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::For, None)?;

        // Check for C-style for: for (( ... ))
        if self.check(&[TokenType::DParenStart]) {
            return self.parse_c_style_for(String::new());
        }

        // Regular for: for VAR in WORDS
        // The variable can be NAME, IN, or even invalid names like "i.j"
        // Invalid names are validated at runtime to match bash behavior
        let variable = if self.is_word() {
            self.advance().value
        } else {
            return Err(ParseException::new("Expected variable name in for loop", self.current().line, self.current().column));
        };

        self.skip_newlines();

        // Check for "in" keyword
        let mut words = None;
        if self.check(&[TokenType::In]) {
            self.advance();
            let mut word_list = Vec::new();
            while !self.check(&[TokenType::Eof, TokenType::Newline, TokenType::Semicolon, TokenType::Do]) {
                if self.is_word() {
                    word_list.push(self.parse_word()?);
                } else {
                    break;
                }
            }
            words = Some(word_list);
        }

        self.skip_separators(false);
        self.expect(TokenType::Do, None)?;

        let body = self.parse_compound_list()?;

        self.expect(TokenType::Done, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::For(
            AST::for_node(variable, words, body, redirections),
        )))
    }

    fn parse_c_style_for(&mut self, _variable: String) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::DParenStart, None)?;

        // Parse init; cond; step
        let mut init_str = String::new();
        let mut cond_str = String::new();
        let mut step_str = String::new();
        let mut current_phase = 0; // 0=init, 1=cond, 2=step
        let mut paren_depth = 0;
        let mut dparen_depth = 1;

        while dparen_depth > 0 && !self.check(&[TokenType::Eof]) {
            if self.check(&[TokenType::Semicolon]) {
                current_phase += 1;
                self.advance();
                continue;
            }

            let current_str = match current_phase {
                0 => &mut init_str,
                1 => &mut cond_str,
                _ => &mut step_str,
            };

            if self.check(&[TokenType::DParenStart]) {
                dparen_depth += 1;
                if !current_str.is_empty() {
                    current_str.push_str("((");
                }
                self.advance();
            } else if self.check(&[TokenType::DParenEnd]) {
                dparen_depth -= 1;
                if dparen_depth > 0 && !current_str.is_empty() {
                    current_str.push_str("))");
                }
                self.advance();
                if dparen_depth == 0 {
                    break;
                }
            } else if self.check(&[TokenType::LParen]) {
                paren_depth += 1;
                current_str.push('(');
                self.advance();
            } else if self.check(&[TokenType::RParen]) {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                current_str.push(')');
                self.advance();
            } else {
                let val = &self.current().value;
                if !current_str.is_empty() && !current_str.ends_with(' ') {
                    current_str.push(' ');
                }
                current_str.push_str(val);
                self.advance();
            }
        }

        self.expect(TokenType::DParenEnd, None)?;
        self.skip_newlines();
        if self.check(&[TokenType::Semicolon]) {
            self.advance();
        }
        self.skip_newlines();

        // Accept either do...done or { } for body (bash allows both)
        let body = if self.check(&[TokenType::LBrace]) {
            self.advance();
            let body = self.parse_compound_list()?;
            self.expect(TokenType::RBrace, None)?;
            body
        } else {
            self.expect(TokenType::Do, None)?;
            let body = self.parse_compound_list()?;
            self.expect(TokenType::Done, None)?;
            body
        };

        let redirections = self.parse_optional_redirections()?;

        let init = if init_str.trim().is_empty() {
            None
        } else {
            Some(parse_arith_expr_from_string(init_str.trim()))
        };
        let condition = if cond_str.trim().is_empty() {
            None
        } else {
            Some(parse_arith_expr_from_string(cond_str.trim()))
        };
        let update = if step_str.trim().is_empty() {
            None
        } else {
            Some(parse_arith_expr_from_string(step_str.trim()))
        };

        Ok(CommandNode::Compound(CompoundCommandNode::CStyleFor(
            crate::ast::types::CStyleForNode {
                init,
                condition,
                update,
                body,
                redirections,
                line: None,
            },
        )))
    }

    fn parse_while(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::While, None)?;

        let condition = self.parse_compound_list()?;

        self.skip_separators(false);
        self.expect(TokenType::Do, None)?;

        let body = self.parse_compound_list()?;

        // Empty body is a syntax error in bash
        if body.is_empty() {
            return Err(ParseException::new(
                "syntax error near unexpected token `done'",
                self.current().line,
                self.current().column,
            ));
        }

        self.expect(TokenType::Done, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::While(
            AST::while_node(condition, body, redirections),
        )))
    }

    fn parse_until(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::Until, None)?;

        let condition = self.parse_compound_list()?;

        self.skip_separators(false);
        self.expect(TokenType::Do, None)?;

        let body = self.parse_compound_list()?;

        // Empty body is a syntax error in bash
        if body.is_empty() {
            return Err(ParseException::new(
                "syntax error near unexpected token `done'",
                self.current().line,
                self.current().column,
            ));
        }

        self.expect(TokenType::Done, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::Until(
            AST::until_node(condition, body, redirections),
        )))
    }

    fn parse_case(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::Case, None)?;

        if !self.is_word() {
            return Err(ParseException::new(
                "Expected word after 'case'",
                self.current().line,
                self.current().column,
            ));
        }
        let word = self.parse_word()?;

        self.skip_newlines();
        self.expect(TokenType::In, None)?;
        self.skip_newlines();

        let mut items = Vec::new();

        while !self.check(&[TokenType::Eof, TokenType::Esac]) {
            self.check_iteration_limit()?;
            let pos_before = self.pos;

            if self.check(&[TokenType::Newline, TokenType::Semicolon]) {
                self.advance();
                continue;
            }
            if self.check(&[TokenType::Esac]) {
                break;
            }

            // Skip optional (
            if self.check(&[TokenType::LParen]) {
                self.advance();
            }

            // Parse patterns
            let mut patterns = Vec::new();
            while self.is_word() {
                patterns.push(self.parse_word()?);

                if self.check(&[TokenType::Pipe]) {
                    self.advance();
                } else {
                    break;
                }
            }

            if patterns.is_empty() {
                // Safety: if we didn't get any patterns and didn't advance, break
                if self.pos == pos_before {
                    break;
                }
                continue;
            }

            self.expect(TokenType::RParen, None)?;
            self.skip_newlines();

            // Parse body
            let mut body = Vec::new();
            while !self.check(&[TokenType::Eof, TokenType::DSemi, TokenType::SemiAnd, TokenType::SemiSemiAnd, TokenType::Esac]) {
                self.check_iteration_limit()?;

                // Check if we're looking at the start of another case pattern (word followed by ))
                // This handles the syntax error case of empty actions like: a) b) echo A ;;
                if self.is_word() && self.peek(1).token_type == TokenType::RParen {
                    // This looks like another case pattern starting without a terminator
                    // This is a syntax error in bash
                    return Err(ParseException::new(
                        "syntax error near unexpected token `)'",
                        self.current().line,
                        self.current().column,
                    ));
                }
                // Also check for optional ( before pattern
                if self.check(&[TokenType::LParen]) && self.peek(1).token_type == TokenType::Word {
                    let next_val = self.peek(1).value.clone();
                    return Err(ParseException::new(
                        &format!("syntax error near unexpected token `{}'", next_val),
                        self.current().line,
                        self.current().column,
                    ));
                }

                let inner_pos_before = self.pos;
                if let Some(stmt) = self.parse_statement()? {
                    body.push(stmt);
                }
                // Don't skip case terminators (;;, ;&, ;;&) - we need to see them
                self.skip_separators(false);

                // If we didn't advance and didn't get a statement, break to avoid infinite loop
                if self.pos == inner_pos_before {
                    break;
                }
            }

            // Parse terminator
            let terminator = if self.check(&[TokenType::DSemi]) {
                self.advance();
                crate::ast::types::CaseTerminator::DoubleSemi
            } else if self.check(&[TokenType::SemiAnd]) {
                self.advance();
                crate::ast::types::CaseTerminator::SemiAnd
            } else if self.check(&[TokenType::SemiSemiAnd]) {
                self.advance();
                crate::ast::types::CaseTerminator::SemiSemiAnd
            } else {
                crate::ast::types::CaseTerminator::DoubleSemi
            };

            items.push(crate::ast::types::CaseItemNode {
                patterns,
                body,
                terminator,
            });

            self.skip_newlines();

            // Safety: if we didn't advance and didn't get an item, break to prevent infinite loop
            if self.pos == pos_before {
                break;
            }
        }

        self.expect(TokenType::Esac, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::Case(
            AST::case_node(word, items, redirections),
        )))
    }

    fn parse_subshell(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::LParen, None)?;

        let body = self.parse_compound_list()?;

        self.expect(TokenType::RParen, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::Subshell(
            AST::subshell(body, redirections),
        )))
    }

    fn parse_group(&mut self) -> Result<CommandNode, ParseException> {
        self.expect(TokenType::LBrace, None)?;

        let body = self.parse_compound_list()?;

        self.expect(TokenType::RBrace, None)?;

        let redirections = self.parse_optional_redirections()?;

        Ok(CommandNode::Compound(CompoundCommandNode::Group(
            AST::group(body, redirections),
        )))
    }

    fn parse_simple_command(&mut self) -> Result<CommandNode, ParseException> {
        let mut assignments = Vec::new();
        let mut name = None;
        let mut args = Vec::new();
        let mut redirections = Vec::new();

        // Parse assignments before command name
        while self.check(&[TokenType::AssignmentWord]) {
            let token = self.advance();
            let value = token.value.clone();
            // Parse VAR=value or VAR+=value
            if let Some(eq_pos) = value.find('=') {
                let var_name = &value[..eq_pos];
                let var_value = &value[eq_pos + 1..];
                let append = var_value.starts_with('+');
                let actual_value = if append {
                    &var_value[1..]
                } else {
                    var_value
                };

                assignments.push(crate::ast::types::AssignmentNode {
                    name: var_name.to_string(),
                    value: Some(self.parse_word_from_string(actual_value, false, false, true, false)),
                    append,
                    array: None,
                });
            }
        }

        // Parse command name
        if self.is_word() {
            name = Some(self.parse_word()?);
        }

        // Parse arguments
        while self.is_word() {
            args.push(self.parse_word()?);
        }

        // Parse redirections
        while self.is_redirection() {
            if let Some(redir) = self.do_parse_redirection()? {
                redirections.push(redir);
            } else {
                break;
            }
        }

        Ok(CommandNode::Simple(
            AST::simple_command(name, args, assignments, redirections),
        ))
    }

    fn is_redirection(&self) -> bool {
        let t = self.current().token_type;
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

    fn do_parse_redirection(&mut self) -> Result<Option<RedirectionNode>, ParseException> {
        let mut fd = None;
        let mut fd_variable = None;

        // Check for fd or variable before redirection operator
        if self.check(&[TokenType::Number]) {
            fd = Some(self.advance().value.parse::<i32>().unwrap_or(0));
        }

        let operator = if self.check(&[TokenType::Less]) {
            self.advance();
            crate::ast::types::RedirectionOperator::Less
        } else if self.check(&[TokenType::Great]) {
            self.advance();
            crate::ast::types::RedirectionOperator::Great
        } else if self.check(&[TokenType::DGreat]) {
            self.advance();
            crate::ast::types::RedirectionOperator::DGreat
        } else if self.check(&[TokenType::LessAnd]) {
            self.advance();
            crate::ast::types::RedirectionOperator::LessAnd
        } else if self.check(&[TokenType::GreatAnd]) {
            self.advance();
            crate::ast::types::RedirectionOperator::GreatAnd
        } else if self.check(&[TokenType::LessGreat]) {
            self.advance();
            crate::ast::types::RedirectionOperator::LessGreat
        } else if self.check(&[TokenType::DLessDash]) {
            self.advance();
            crate::ast::types::RedirectionOperator::DLessDash
        } else if self.check(&[TokenType::Clobber]) {
            self.advance();
            crate::ast::types::RedirectionOperator::Clobber
        } else if self.check(&[TokenType::TLess]) {
            self.advance();
            crate::ast::types::RedirectionOperator::TLess
        } else if self.check(&[TokenType::AndGreat]) {
            self.advance();
            crate::ast::types::RedirectionOperator::AndGreat
        } else if self.check(&[TokenType::AndDGreat]) {
            self.advance();
            crate::ast::types::RedirectionOperator::AndDGreat
        } else if self.check(&[TokenType::DLess]) {
            self.advance();
            crate::ast::types::RedirectionOperator::DLess
        } else {
            return Ok(None);
        };

        // Parse target
        let target = if matches!(operator,
            crate::ast::types::RedirectionOperator::DLess
                | crate::ast::types::RedirectionOperator::DLessDash
        ) {
            // Here-document
            let delimiter = if self.is_word() {
                self.advance().value
            } else {
                return Err(ParseException::new("Expected here-document delimiter", self.current().line, self.current().column));
            };

            let quoted = delimiter.starts_with('\'') || delimiter.starts_with('"');
            let strip_tabs = operator == crate::ast::types::RedirectionOperator::DLessDash;

            // Add to pending heredocs
            let redirect_idx = self.pending_redirections.len();
            self.add_pending_heredoc(redirect_idx, delimiter.clone(), strip_tabs, quoted);

            let redirection = AST::redirection(
                operator,
                crate::ast::types::RedirectionTarget::HereDoc(
                    AST::here_doc(&delimiter, AST::word(vec![]), strip_tabs, quoted),
                ),
                fd,
                fd_variable.clone(),
            );
            self.pending_redirections.push(redirection.clone());

            crate::ast::types::RedirectionTarget::HereDoc(AST::here_doc(
                delimiter,
                AST::word(vec![]),
                strip_tabs,
                quoted,
            ))
        } else {
            // Word target
            let word = if self.is_word() {
                self.parse_word()?
            } else {
                return Err(ParseException::new("Expected redirection target", self.current().line, self.current().column));
            };
            crate::ast::types::RedirectionTarget::Word(word)
        };

        Ok(Some(AST::redirection(operator, target, fd, fd_variable.clone())))
    }

    // ===========================================================================
    // HELPER PARSING
    // ===========================================================================

    pub fn parse_compound_list(&mut self) -> Result<Vec<StatementNode>, ParseException> {
        let mut statements = Vec::new();

        self.skip_newlines();

        while !self.check(&[
            TokenType::Eof,
            TokenType::Fi,
            TokenType::Else,
            TokenType::Elif,
            TokenType::Then,
            TokenType::Do,
            TokenType::Done,
            TokenType::Esac,
            TokenType::RParen,
            TokenType::RBrace,
            TokenType::DSemi,
            TokenType::SemiAnd,
            TokenType::SemiSemiAnd,
        ]) && self.is_command_start()
        {
            self.check_iteration_limit()?;
            let pos_before = self.pos;

            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
            self.skip_separators(true);

            if self.pos == pos_before {
                break;
            }
        }

        Ok(statements)
    }

    pub fn parse_optional_redirections(&mut self) -> Result<Vec<RedirectionNode>, ParseException> {
        let mut redirections = Vec::new();

        while self.is_redirection() {
            self.check_iteration_limit()?;
            let pos_before = self.pos;

            if let Some(redir) = self.do_parse_redirection()? {
                redirections.push(redir);
            }

            if self.pos == pos_before {
                break;
            }
        }

        Ok(redirections)
    }

    // ===========================================================================
    // ARITHMETIC EXPRESSION PARSING
    // ===========================================================================

    pub fn parse_arithmetic_expression(&mut self, input: &str) -> ArithmeticExpressionNode {
        parse_arith_expr_from_string(input)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to parse a bash script
pub fn parse(input: &str) -> Result<ScriptNode, ParseException> {
    let mut parser = Parser::new();
    parser.parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let mut parser = Parser::new();
        let result = parser.parse("");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 0);
    }

    #[test]
    fn test_parse_simple_command() {
        let mut parser = Parser::new();
        let result = parser.parse("echo hello");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_pipeline() {
        let mut parser = Parser::new();
        let result = parser.parse("echo hello | cat");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_function() {
        let mut parser = Parser::new();
        let result = parser.parse("foo() { echo bar; }");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_if_statement() {
        let mut parser = Parser::new();
        let result = parser.parse("if true; then echo yes; fi");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_for_loop() {
        let mut parser = Parser::new();
        let result = parser.parse("for i in a b c; do echo $i; done");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_while_loop() {
        let mut parser = Parser::new();
        let result = parser.parse("while true; do echo yes; done");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_case_statement() {
        let mut parser = Parser::new();
        let result = parser.parse("case $x in a) echo a;; esac");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_subshell() {
        let mut parser = Parser::new();
        let result = parser.parse("(echo hello)");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_group() {
        let mut parser = Parser::new();
        let result = parser.parse("{ echo hello; }");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn test_parse_arithmetic_command() {
        let mut parser = Parser::new();
        let result = parser.parse("((x = 1 + 2))");
        assert!(result.is_ok());
        let script = result.unwrap();
        assert_eq!(script.statements.len(), 1);
    }
}
