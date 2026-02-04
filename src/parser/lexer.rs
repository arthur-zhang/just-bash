//! Lexer for Bash Scripts
//!
//! The lexer tokenizes input into a stream of tokens that the parser consumes.
//! It handles:
//! - Operators and delimiters
//! - Words (with quoting rules)
//! - Comments
//! - Here-documents
//! - Escape sequences

use std::collections::HashMap;

/// Token types for bash lexer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    // End of input
    Eof,

    // Newlines and separators
    Newline,
    Semicolon,
    Amp, // &

    // Operators
    Pipe,    // |
    PipeAmp, // |&
    AndAnd,  // &&
    OrOr,    // ||
    Bang,    // !

    // Redirections
    Less,      // <
    Great,     // >
    DLess,     // <<
    DGreat,    // >>
    LessAnd,   // <&
    GreatAnd,  // >&
    LessGreat, // <>
    DLessDash, // <<-
    Clobber,   // >|
    TLess,     // <<<
    AndGreat,  // &>
    AndDGreat, // &>>

    // Grouping
    LParen, // (
    RParen, // )
    LBrace, // {
    RBrace, // }

    // Special
    DSemi,       // ;;
    SemiAnd,     // ;&
    SemiSemiAnd, // ;;&

    // Compound commands
    DBrackStart, // [[
    DBrackEnd,   // ]]
    DParenStart, // ((
    DParenEnd,   // ))

    // Reserved words
    If,
    Then,
    Else,
    Elif,
    Fi,
    For,
    While,
    Until,
    Do,
    Done,
    Case,
    Esac,
    In,
    Function,
    Select,
    Time,
    Coproc,

    // Words and identifiers
    Word,
    Name,           // Valid variable name
    Number,         // For redirections like 2>&1
    AssignmentWord, // VAR=value
    FdVariable,     // {varname} before redirect operator

    // Comments
    Comment,

    // Here-document content
    HeredocContent,
}

impl TokenType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Eof => "EOF",
            Self::Newline => "NEWLINE",
            Self::Semicolon => ";",
            Self::Amp => "&",
            Self::Pipe => "|",
            Self::PipeAmp => "|&",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Bang => "!",
            Self::Less => "<",
            Self::Great => ">",
            Self::DLess => "<<",
            Self::DGreat => ">>",
            Self::LessAnd => "<&",
            Self::GreatAnd => ">&",
            Self::LessGreat => "<>",
            Self::DLessDash => "<<-",
            Self::Clobber => ">|",
            Self::TLess => "<<<",
            Self::AndGreat => "&>",
            Self::AndDGreat => "&>>",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBrace => "{",
            Self::RBrace => "}",
            Self::DSemi => ";;",
            Self::SemiAnd => ";&",
            Self::SemiSemiAnd => ";;&",
            Self::DBrackStart => "[[",
            Self::DBrackEnd => "]]",
            Self::DParenStart => "((",
            Self::DParenEnd => "))",
            Self::If => "if",
            Self::Then => "then",
            Self::Else => "else",
            Self::Elif => "elif",
            Self::Fi => "fi",
            Self::For => "for",
            Self::While => "while",
            Self::Until => "until",
            Self::Do => "do",
            Self::Done => "done",
            Self::Case => "case",
            Self::Esac => "esac",
            Self::In => "in",
            Self::Function => "function",
            Self::Select => "select",
            Self::Time => "time",
            Self::Coproc => "coproc",
            Self::Word => "WORD",
            Self::Name => "NAME",
            Self::Number => "NUMBER",
            Self::AssignmentWord => "ASSIGNMENT_WORD",
            Self::FdVariable => "FD_VARIABLE",
            Self::Comment => "COMMENT",
            Self::HeredocContent => "HEREDOC_CONTENT",
        }
    }
}

/// A token produced by the lexer
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    /// Original position in input
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    /// For WORD tokens: quote information
    pub quoted: bool,
    pub single_quoted: bool,
}

impl Token {
    pub fn new(
        token_type: TokenType,
        value: impl Into<String>,
        start: usize,
        end: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            token_type,
            value: value.into(),
            start,
            end,
            line,
            column,
            quoted: false,
            single_quoted: false,
        }
    }

    pub fn with_quotes(mut self, quoted: bool, single_quoted: bool) -> Self {
        self.quoted = quoted;
        self.single_quoted = single_quoted;
        self
    }
}

/// Error thrown when the lexer encounters invalid input
#[derive(Debug, Clone)]
pub struct LexerError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for LexerError {}

impl LexerError {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

/// Pending heredoc information
#[derive(Debug, Clone)]
struct PendingHeredoc {
    delimiter: String,
    strip_tabs: bool,
    quoted: bool,
}

lazy_static::lazy_static! {
    /// Reserved words in bash
    static ref RESERVED_WORDS: HashMap<&'static str, TokenType> = {
        let mut m = HashMap::new();
        m.insert("if", TokenType::If);
        m.insert("then", TokenType::Then);
        m.insert("else", TokenType::Else);
        m.insert("elif", TokenType::Elif);
        m.insert("fi", TokenType::Fi);
        m.insert("for", TokenType::For);
        m.insert("while", TokenType::While);
        m.insert("until", TokenType::Until);
        m.insert("do", TokenType::Do);
        m.insert("done", TokenType::Done);
        m.insert("case", TokenType::Case);
        m.insert("esac", TokenType::Esac);
        m.insert("in", TokenType::In);
        m.insert("function", TokenType::Function);
        m.insert("select", TokenType::Select);
        m.insert("time", TokenType::Time);
        m.insert("coproc", TokenType::Coproc);
        m
    };

    /// Single-character operators
    static ref SINGLE_CHAR_OPS: HashMap<char, TokenType> = {
        let mut m = HashMap::new();
        m.insert('|', TokenType::Pipe);
        m.insert('&', TokenType::Amp);
        m.insert(';', TokenType::Semicolon);
        m.insert('(', TokenType::LParen);
        m.insert(')', TokenType::RParen);
        m.insert('<', TokenType::Less);
        m.insert('>', TokenType::Great);
        m
    };
}

/// Three-character operators
const THREE_CHAR_OPS: &[(&str, TokenType)] = &[
    (";;&", TokenType::SemiSemiAnd),
    ("<<<", TokenType::TLess),
    ("&>>", TokenType::AndDGreat),
];

/// Two-character operators
const TWO_CHAR_OPS: &[(&str, TokenType)] = &[
    ("[[", TokenType::DBrackStart),
    ("]]", TokenType::DBrackEnd),
    ("((", TokenType::DParenStart),
    ("))", TokenType::DParenEnd),
    ("&&", TokenType::AndAnd),
    ("||", TokenType::OrOr),
    (";;", TokenType::DSemi),
    (";&", TokenType::SemiAnd),
    ("|&", TokenType::PipeAmp),
    (">>", TokenType::DGreat),
    ("<&", TokenType::LessAnd),
    (">&", TokenType::GreatAnd),
    ("<>", TokenType::LessGreat),
    (">|", TokenType::Clobber),
    ("&>", TokenType::AndGreat),
];

/// Check if a string is a valid variable name
fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Check if a character is a word boundary (ends a word token)
fn is_word_boundary(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>')
}

/// Check if a string is a valid assignment LHS with optional nested array subscript
fn is_valid_assignment_lhs(s: &str) -> bool {
    // Must start with valid variable name
    let name_end = s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    
    if name_end == 0 {
        return false;
    }
    
    // Check first char is letter or underscore
    let first = s.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    
    let after_name = &s[name_end..];
    
    // If nothing after name, it's valid (simple variable)
    if after_name.is_empty() || after_name == "+" {
        return true;
    }
    
    // If it's an array subscript, need to check for balanced brackets
    if after_name.starts_with('[') {
        let mut depth = 0;
        let mut i = 0;
        for c in after_name.chars() {
            if c == '[' {
                depth += 1;
            } else if c == ']' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += c.len_utf8();
        }
        // Must have found closing bracket
        if depth != 0 {
            return false;
        }
        // After closing bracket, only + is allowed (for +=)
        let after_bracket = &after_name[i + 1..];
        return after_bracket.is_empty() || after_bracket == "+";
    }
    
    false
}

/// Find the index of assignment '=' or '+=' outside of brackets.
fn find_assignment_eq(s: &str) -> Option<usize> {
    let mut depth = 0;
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '=' if depth == 0 => return Some(i),
            '+' if depth == 0 && chars.get(i + 1) == Some(&'=') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Lexer class
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
    pending_heredocs: Vec<PendingHeredoc>,
    /// Track depth inside (( )) for C-style for loops and arithmetic commands
    dparen_depth: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            pending_heredocs: Vec::new(),
            dparen_depth: 0,
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(mut self) -> Result<Vec<Token>, LexerError> {
        let len = self.input.len();

        while self.pos < len {
            // Check for pending here-documents after newline
            if !self.pending_heredocs.is_empty()
                && !self.tokens.is_empty()
                && self.tokens.last().map(|t| t.token_type) == Some(TokenType::Newline)
            {
                self.read_heredoc_content()?;
                continue;
            }

            self.skip_whitespace();

            if self.pos >= len {
                break;
            }

            if let Some(token) = self.next_token()? {
                self.tokens.push(token);
            }
        }

        // Add EOF token
        self.tokens.push(Token::new(
            TokenType::Eof,
            "",
            self.pos,
            self.pos,
            self.line,
            self.column,
        ));

        Ok(self.tokens)
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.current();
        if self.pos < self.input.len() {
            self.pos += 1;
            self.column += 1;
        }
        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            match c {
                ' ' | '\t' => {
                    self.pos += 1;
                    self.column += 1;
                }
                '\\' if self.peek(1) == Some('\n') => {
                    // Line continuation
                    self.pos += 2;
                    self.line += 1;
                    self.column = 1;
                }
                _ => break,
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexerError> {
        let start_line = self.line;
        let start_column = self.column;
        let start_pos = self.pos;

        let c0 = match self.current() {
            Some(c) => c,
            None => return Ok(None),
        };
        let c1 = self.peek(1);
        let c2 = self.peek(2);

        // Comments - but NOT inside (( )) arithmetic context where # is part of base notation
        if c0 == '#' && self.dparen_depth == 0 {
            return Ok(Some(self.read_comment(start_pos, start_line, start_column)));
        }

        // Newline
        if c0 == '\n' {
            self.pos += 1;
            self.line += 1;
            self.column = 1;
            return Ok(Some(Token::new(
                TokenType::Newline,
                "\n",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Three-character operators
        // Special case: <<- (heredoc with tab stripping)
        if c0 == '<' && c1 == Some('<') && c2 == Some('-') {
            self.pos += 3;
            self.column += 3;
            self.register_heredoc_from_lookahead(true);
            return Ok(Some(Token::new(
                TokenType::DLessDash,
                "<<-",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Table-driven three-char operators
        for (op_str, token_type) in THREE_CHAR_OPS {
            let chars: Vec<char> = op_str.chars().collect();
            if c0 == chars[0] && c1 == Some(chars[1]) && c2 == Some(chars[2]) {
                self.pos += 3;
                self.column += 3;
                return Ok(Some(Token::new(
                    *token_type,
                    *op_str,
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
        }

        // Two-character operators
        // Special case: << (heredoc)
        if c0 == '<' && c1 == Some('<') {
            self.pos += 2;
            self.column += 2;
            self.register_heredoc_from_lookahead(false);
            return Ok(Some(Token::new(
                TokenType::DLess,
                "<<",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Special handling for (( and )) to track nested parentheses
        if c0 == '(' && c1 == Some('(') {
            if self.dparen_depth > 0 {
                // Inside arithmetic context, (( is just two open parens
                self.pos += 1;
                self.column += 1;
                self.dparen_depth += 1;
                return Ok(Some(Token::new(
                    TokenType::LParen,
                    "(",
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
            // Check if this looks like nested subshells
            if self.looks_like_nested_subshells(self.pos + 2)
                || self.dparen_closes_with_spaced_parens(self.pos + 2)
            {
                self.pos += 1;
                self.column += 1;
                return Ok(Some(Token::new(
                    TokenType::LParen,
                    "(",
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
            self.pos += 2;
            self.column += 2;
            self.dparen_depth = 1;
            return Ok(Some(Token::new(
                TokenType::DParenStart,
                "((",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        if c0 == ')' && c1 == Some(')') {
            if self.dparen_depth == 1 {
                self.pos += 2;
                self.column += 2;
                self.dparen_depth = 0;
                return Ok(Some(Token::new(
                    TokenType::DParenEnd,
                    "))",
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            } else if self.dparen_depth > 1 {
                self.pos += 1;
                self.column += 1;
                self.dparen_depth -= 1;
                return Ok(Some(Token::new(
                    TokenType::RParen,
                    ")",
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
            // dparen_depth == 0: emit single RPAREN
            self.pos += 1;
            self.column += 1;
            return Ok(Some(Token::new(
                TokenType::RParen,
                ")",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Table-driven two-char operators
        for (op_str, token_type) in TWO_CHAR_OPS {
            let chars: Vec<char> = op_str.chars().collect();
            if chars.len() >= 2 && c0 == chars[0] && c1 == Some(chars[1]) {
                // Skip (( and )) handled above
                if *op_str == "((" || *op_str == "))" {
                    continue;
                }
                // Skip ;; and ;;& inside (( )) context
                if self.dparen_depth > 0
                    && chars[0] == ';'
                    && (*token_type == TokenType::DSemi
                        || *token_type == TokenType::SemiAnd
                        || *token_type == TokenType::SemiSemiAnd)
                {
                    continue;
                }
                // Special case: [[ and ]] should only be recognized at word boundary
                if *token_type == TokenType::DBrackStart || *token_type == TokenType::DBrackEnd {
                    if let Some(after) = self.peek(2) {
                        if !is_word_boundary(after) {
                            break;
                        }
                    }
                }
                self.pos += 2;
                self.column += 2;
                return Ok(Some(Token::new(
                    *token_type,
                    *op_str,
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
        }

        // Single-character operators
        if c0 == '(' && self.dparen_depth > 0 {
            self.pos += 1;
            self.column += 1;
            self.dparen_depth += 1;
            return Ok(Some(Token::new(
                TokenType::LParen,
                "(",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }
        if c0 == ')' && self.dparen_depth > 1 {
            self.pos += 1;
            self.column += 1;
            self.dparen_depth -= 1;
            return Ok(Some(Token::new(
                TokenType::RParen,
                ")",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        if let Some(&token_type) = SINGLE_CHAR_OPS.get(&c0) {
            self.pos += 1;
            self.column += 1;
            return Ok(Some(Token::new(
                token_type,
                c0.to_string(),
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Special cases: { } !
        if c0 == '{' {
            // Check for FD variable syntax
            if let Some(fd_var) = self.scan_fd_variable(start_pos) {
                self.pos = fd_var.end;
                self.column = start_column + (fd_var.end - start_pos);
                return Ok(Some(Token::new(
                    TokenType::FdVariable,
                    fd_var.varname,
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
            // Check for {} as a word
            if c1 == Some('}') {
                self.pos += 2;
                self.column += 2;
                return Ok(Some(
                    Token::new(TokenType::Word, "{}", start_pos, self.pos, start_line, start_column)
                        .with_quotes(false, false),
                ));
            }
            // Check for brace expansion
            if self.scan_brace_expansion(start_pos).is_some() {
                return self.read_word_with_brace_expansion(start_pos, start_line, start_column);
            }
            // Check for literal brace word
            if self.scan_literal_brace_word(start_pos).is_some() {
                return self.read_word_with_brace_expansion(start_pos, start_line, start_column);
            }
            // { must be followed by whitespace to be a group start
            if let Some(next) = c1 {
                if next != ' ' && next != '\t' && next != '\n' {
                    return self.read_word(start_pos, start_line, start_column);
                }
            }
            self.pos += 1;
            self.column += 1;
            return Ok(Some(Token::new(
                TokenType::LBrace,
                "{",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        if c0 == '}' {
            if self.is_word_char_following(self.pos + 1) {
                return self.read_word(start_pos, start_line, start_column);
            }
            self.pos += 1;
            self.column += 1;
            return Ok(Some(Token::new(
                TokenType::RBrace,
                "}",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        if c0 == '!' {
            if c1 == Some('=') {
                self.pos += 2;
                self.column += 2;
                return Ok(Some(Token::new(
                    TokenType::Word,
                    "!=",
                    start_pos,
                    self.pos,
                    start_line,
                    start_column,
                )));
            }
            self.pos += 1;
            self.column += 1;
            return Ok(Some(Token::new(
                TokenType::Bang,
                "!",
                start_pos,
                self.pos,
                start_line,
                start_column,
            )));
        }

        // Words
        self.read_word(start_pos, start_line, start_column)
    }

    fn read_comment(&mut self, start: usize, line: usize, column: usize) -> Token {
        while let Some(c) = self.current() {
            if c == '\n' {
                break;
            }
            self.pos += 1;
            self.column += 1;
        }
        let value: String = self.input[start..self.pos].iter().collect();
        Token::new(TokenType::Comment, value, start, self.pos, line, column)
    }

    fn looks_like_nested_subshells(&self, start_pos: usize) -> bool {
        let mut pos = start_pos;
        
        // Skip optional whitespace
        while pos < self.input.len() && matches!(self.input.get(pos), Some(' ' | '\t')) {
            pos += 1;
        }
        
        if pos >= self.input.len() {
            return false;
        }
        
        let c = self.input[pos];
        
        // If we see another ( immediately, recursively check
        if c == '(' {
            return self.looks_like_nested_subshells(pos + 1);
        }
        
        // Check if this looks like the start of a command name
        let is_letter = c.is_ascii_alphabetic() || c == '_';
        let is_special_command = c == '!' || c == '[';
        
        if !is_letter && !is_special_command {
            return false;
        }
        
        // Read the word-like content
        let mut word_end = pos;
        while word_end < self.input.len() {
            let ch = self.input[word_end];
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                word_end += 1;
            } else {
                break;
            }
        }
        
        if word_end == pos {
            return is_special_command;
        }
        
        // Skip whitespace after the word
        let mut after_word = word_end;
        while after_word < self.input.len() && matches!(self.input.get(after_word), Some(' ' | '\t')) {
            after_word += 1;
        }
        
        if after_word >= self.input.len() {
            return false;
        }
        
        let next_char = self.input[after_word];
        
        // If followed by =, it's likely arithmetic
        if next_char == '=' && self.input.get(after_word + 1) != Some(&'=') {
            return false;
        }
        
        // If followed by newline, not a proper subshell pattern
        if next_char == '\n' {
            return false;
        }
        
        // If followed by arithmetic operators without space, likely arithmetic
        if word_end == after_word && matches!(next_char, '+' | '*' | '/' | '%' | '<' | '>' | '&' | '|' | '^' | '!' | '~' | '?' | ':') && next_char != '-' {
            return false;
        }

        // If followed by )), it's arithmetic
        if next_char == ')' && self.input.get(after_word + 1) == Some(&')') {
            return false;
        }

        // If followed by command-like arguments after whitespace, it's likely a command
        if after_word > word_end
            && (next_char == '-'
                || next_char == '"'
                || next_char == '\''
                || next_char == '$'
                || next_char.is_ascii_alphabetic()
                || next_char == '_'
                || next_char == '/'
                || next_char == '.')
        {
            // Scan ahead to find ) on the same line
            let mut scan_pos = after_word;
            while scan_pos < self.input.len() && self.input[scan_pos] != '\n' {
                if self.input[scan_pos] == ')' {
                    return true;
                }
                scan_pos += 1;
            }
            // No ) found on this line - not a proper subshell
            return false;
        }

        // If followed by ) then || or &&, it's nested subshells
        if next_char == ')' {
            let mut after_paren = after_word + 1;
            while after_paren < self.input.len() && matches!(self.input.get(after_paren), Some(' ' | '\t')) {
                after_paren += 1;
            }
            if (self.input.get(after_paren) == Some(&'|') && self.input.get(after_paren + 1) == Some(&'|'))
                || (self.input.get(after_paren) == Some(&'&') && self.input.get(after_paren + 1) == Some(&'&'))
                || self.input.get(after_paren) == Some(&';')
                || (self.input.get(after_paren) == Some(&'|') && self.input.get(after_paren + 1) != Some(&'|'))
            {
                return true;
            }
        }
        
        false
    }

    fn dparen_closes_with_spaced_parens(&self, start_pos: usize) -> bool {
        let mut pos = start_pos;
        let mut depth = 2;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        
        while pos < self.input.len() && depth > 0 {
            let c = self.input[pos];
            
            if in_single_quote {
                if c == '\'' {
                    in_single_quote = false;
                }
                pos += 1;
                continue;
            }
            
            if in_double_quote {
                if c == '\\' && pos + 1 < self.input.len() {
                    pos += 2;
                    continue;
                }
                if c == '"' {
                    in_double_quote = false;
                }
                pos += 1;
                continue;
            }
            
            match c {
                '\'' => in_single_quote = true,
                '"' => in_double_quote = true,
                '\\' if pos + 1 < self.input.len() => {
                    pos += 2;
                    continue;
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 1 {
                        // Check if next char is ) with whitespace
                        let next_pos = pos + 1;
                        if self.input.get(next_pos) == Some(&')') {
                            return false;
                        }
                        let mut scan_pos = next_pos;
                        let mut has_whitespace = false;
                        while scan_pos < self.input.len() && matches!(self.input.get(scan_pos), Some(' ' | '\t' | '\n')) {
                            has_whitespace = true;
                            scan_pos += 1;
                        }
                        if has_whitespace && self.input.get(scan_pos) == Some(&')') {
                            return true;
                        }
                    }
                    if depth == 0 {
                        return false;
                    }
                }
                '|' if depth == 1 => {
                    if self.input.get(pos + 1) == Some(&'|') {
                        return true;
                    }
                    if self.input.get(pos + 1) != Some(&'|') {
                        return true;
                    }
                }
                '&' if depth == 1 && self.input.get(pos + 1) == Some(&'&') => {
                    return true;
                }
                _ => {}
            }
            pos += 1;
        }
        
        false
    }

    fn read_word(&mut self, start: usize, line: usize, column: usize) -> Result<Option<Token>, LexerError> {
        let mut value = String::new();
        let mut quoted = false;
        let mut single_quoted = false;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let starts_with_quote = matches!(self.current(), Some('"' | '\''));
        let mut has_content_after_quote = false;
        let mut bracket_depth = 0;
        let mut col = column;
        let mut ln = line;

        while let Some(c) = self.current() {
            // Check for word boundaries
            if !in_single_quote && !in_double_quote {
                // Handle extglob pattern
                if c == '(' && !value.is_empty() && "@*+?!".contains(value.chars().last().unwrap_or(' ')) {
                    if let Some(result) = self.scan_extglob_pattern(self.pos) {
                        value.push_str(&result.content);
                        self.pos = result.end;
                        col += result.content.len();
                        continue;
                    }
                }

                // Handle array subscript brackets
                if c == '[' && bracket_depth == 0 && is_valid_name(&value) {
                    if let Some(after) = self.peek(1) {
                        if after == '^' || after == '!' {
                            value.push(c);
                            self.pos += 1;
                            col += 1;
                            continue;
                        }
                    }
                    bracket_depth = 1;
                    value.push(c);
                    self.pos += 1;
                    col += 1;
                    continue;
                } else if c == '[' && bracket_depth > 0 {
                    if !value.is_empty() && value.chars().last() != Some('\\') {
                        bracket_depth += 1;
                    }
                    value.push(c);
                    self.pos += 1;
                    col += 1;
                    continue;
                } else if c == ']' && bracket_depth > 0 {
                    if !value.is_empty() && value.chars().last() != Some('\\') {
                        bracket_depth -= 1;
                    }
                    value.push(c);
                    self.pos += 1;
                    col += 1;
                    continue;
                }

                // Inside brackets, only break on newlines
                if bracket_depth > 0 {
                    if c == '\n' {
                        break;
                    }
                    value.push(c);
                    self.pos += 1;
                    col += 1;
                    continue;
                }

                if is_word_boundary(c) {
                    break;
                }
            }

            // Handle $'' ANSI-C quoting
            if c == '$' && self.peek(1) == Some('\'') && !in_single_quote && !in_double_quote {
                value.push_str("$'");
                self.pos += 2;
                col += 2;
                while let Some(ch) = self.current() {
                    if ch == '\'' {
                        break;
                    }
                    if ch == '\\' && self.peek(1).is_some() {
                        value.push(ch);
                        value.push(self.peek(1).unwrap());
                        self.pos += 2;
                        col += 2;
                    } else {
                        value.push(ch);
                        self.pos += 1;
                        col += 1;
                    }
                }
                if self.current() == Some('\'') {
                    value.push('\'');
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            // Handle $"..." locale quoting
            if c == '$' && self.peek(1) == Some('"') && !in_single_quote && !in_double_quote {
                self.pos += 1;
                col += 1;
                in_double_quote = true;
                quoted = true;
                if value.is_empty() {
                    // Treat as if word started with quote
                }
                self.pos += 1;
                col += 1;
                continue;
            }

            // Handle quotes
            if c == '\'' && !in_double_quote {
                if in_single_quote {
                    in_single_quote = false;
                    if !starts_with_quote || has_content_after_quote {
                        value.push(c);
                    } else if let Some(next) = self.peek(1) {
                        if !is_word_boundary(next) && next != '\'' {
                            if next == '"' {
                                has_content_after_quote = true;
                                value.push(c);
                                single_quoted = false;
                                quoted = false;
                            } else {
                                has_content_after_quote = true;
                                value.push(c);
                            }
                        }
                    }
                } else {
                    in_single_quote = true;
                    if starts_with_quote && !has_content_after_quote {
                        single_quoted = true;
                        quoted = true;
                    } else {
                        value.push(c);
                    }
                }
                self.pos += 1;
                col += 1;
                continue;
            }

            if c == '"' && !in_single_quote {
                if in_double_quote {
                    in_double_quote = false;
                    if !starts_with_quote || has_content_after_quote {
                        value.push(c);
                    } else if let Some(next) = self.peek(1) {
                        if !is_word_boundary(next) && next != '"' {
                            if next == '\'' {
                                has_content_after_quote = true;
                                value.push(c);
                                single_quoted = false;
                                quoted = false;
                            } else {
                                has_content_after_quote = true;
                                value.push(c);
                            }
                        }
                    }
                } else {
                    in_double_quote = true;
                    if starts_with_quote && !has_content_after_quote {
                        quoted = true;
                    } else {
                        value.push(c);
                    }
                }
                self.pos += 1;
                col += 1;
                continue;
            }

            // Handle escapes
            if c == '\\' && !in_single_quote {
                if let Some(next) = self.peek(1) {
                    if next == '\n' {
                        self.pos += 2;
                        ln += 1;
                        col = 1;
                        continue;
                    }
                    if in_double_quote {
                        if matches!(next, '"' | '\\' | '$' | '`' | '\n') {
                            if next == '\n' {
                                self.pos += 2;
                                col = 1;
                                ln += 1;
                                continue;
                            }
                            value.push(c);
                            value.push(next);
                            self.pos += 2;
                            col += 2;
                            continue;
                        }
                    } else {
                        if matches!(next, '\\' | '"' | '\'' | '`' | '*' | '?' | '[' | ']' | '(' | ')' | '$' | '-' | '.' | '^' | '+' | '{' | '}') {
                            value.push(c);
                            value.push(next);
                        } else {
                            value.push(next);
                        }
                        self.pos += 2;
                        col += 2;
                        continue;
                    }
                }
            }

            // Handle $(...) command substitution
            if c == '$' && self.peek(1) == Some('(') && !in_single_quote {
                value.push(c);
                self.pos += 1;
                col += 1;
                value.push(self.current().unwrap());
                self.pos += 1;
                col += 1;

                let mut depth = 1;
                let mut in_sq = false;
                let mut in_dq = false;
                let mut case_depth = 0;
                let mut in_case_pattern = false;
                let mut word_buffer = String::new();

                // Check if this is $((...)) arithmetic expansion
                let is_arithmetic = self.current() == Some('(') && !self.dollar_dparen_is_subshell(self.pos);

                while depth > 0 && self.pos < self.input.len() {
                    let ch = self.input[self.pos];
                    value.push(ch);

                    if in_sq {
                        if ch == '\'' {
                            in_sq = false;
                        }
                    } else if in_dq {
                        if ch == '\\' && self.pos + 1 < self.input.len() {
                            value.push(self.input[self.pos + 1]);
                            self.pos += 1;
                            col += 1;
                        } else if ch == '"' {
                            in_dq = false;
                        }
                    } else {
                        // Not in quotes
                        if ch == '\'' {
                            in_sq = true;
                            word_buffer.clear();
                        } else if ch == '"' {
                            in_dq = true;
                            word_buffer.clear();
                        } else if ch == '\\' && self.pos + 1 < self.input.len() {
                            value.push(self.input[self.pos + 1]);
                            self.pos += 1;
                            col += 1;
                            word_buffer.clear();
                        } else if ch == '$' && self.peek(1) == Some('{') {
                            // Handle ${...} parameter expansion - consume the entire construct
                            self.pos += 1;
                            col += 1;
                            value.push(self.input[self.pos]); // Add the {
                            self.pos += 1;
                            col += 1;
                            let mut brace_depth = 1;
                            let mut in_brace_sq = false;
                            let mut in_brace_dq = false;
                            while brace_depth > 0 && self.pos < self.input.len() {
                                let bc = self.input[self.pos];
                                if bc == '\\' && self.pos + 1 < self.input.len() && !in_brace_sq {
                                    value.push(bc);
                                    self.pos += 1;
                                    col += 1;
                                    value.push(self.input[self.pos]);
                                    self.pos += 1;
                                    col += 1;
                                    continue;
                                }
                                value.push(bc);
                                if in_brace_sq {
                                    if bc == '\'' {
                                        in_brace_sq = false;
                                    }
                                } else if in_brace_dq {
                                    if bc == '"' {
                                        in_brace_dq = false;
                                    }
                                } else {
                                    match bc {
                                        '\'' => in_brace_sq = true,
                                        '"' => in_brace_dq = true,
                                        '{' => brace_depth += 1,
                                        '}' => brace_depth -= 1,
                                        _ => {}
                                    }
                                }
                                if bc == '\n' {
                                    ln += 1;
                                    col = 0;
                                } else {
                                    col += 1;
                                }
                                self.pos += 1;
                            }
                            word_buffer.clear();
                            continue;
                        } else if ch == '#' && !is_arithmetic && (word_buffer.is_empty() || self.input.get(self.pos.wrapping_sub(1)).map_or(false, |c| c.is_whitespace())) {
                            // Comment - skip to end of line (only in command substitution, not arithmetic)
                            while self.pos + 1 < self.input.len() && self.input[self.pos + 1] != '\n' {
                                self.pos += 1;
                                col += 1;
                                value.push(self.input[self.pos]);
                            }
                            word_buffer.clear();
                        } else if ch.is_ascii_alphabetic() || ch == '_' {
                            word_buffer.push(ch);
                        } else {
                            // Check for keywords
                            if word_buffer == "case" {
                                case_depth += 1;
                                in_case_pattern = false;
                            } else if word_buffer == "in" && case_depth > 0 {
                                in_case_pattern = true;
                            } else if word_buffer == "esac" && case_depth > 0 {
                                case_depth -= 1;
                                in_case_pattern = false;
                            }
                            word_buffer.clear();

                            if ch == '(' {
                                // Check for $( which starts nested command substitution
                                if self.pos > 0 && self.input.get(self.pos.wrapping_sub(1)) == Some(&'$') {
                                    depth += 1;
                                } else if !in_case_pattern {
                                    depth += 1;
                                }
                            } else if ch == ')' {
                                if in_case_pattern {
                                    in_case_pattern = false;
                                } else {
                                    depth -= 1;
                                }
                            } else if ch == ';' {
                                // ;; in case body means next pattern
                                if case_depth > 0 && self.peek(1) == Some(';') {
                                    in_case_pattern = true;
                                }
                            }
                        }
                    }

                    if ch == '\n' {
                        ln += 1;
                        col = 0;
                        word_buffer.clear();
                    }
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            // Handle ${...} parameter expansion
            if c == '$' && self.peek(1) == Some('{') && !in_single_quote {
                value.push(c);
                self.pos += 1;
                col += 1;
                value.push(self.current().unwrap());
                self.pos += 1;
                col += 1;

                let mut depth = 1;
                let mut in_param_sq = false;
                let mut in_param_dq = false;
                let mut single_quote_start_line = ln;
                let mut single_quote_start_col = col;
                let mut double_quote_start_line = ln;
                let mut double_quote_start_col = col;

                while depth > 0 && self.pos < self.input.len() {
                    let ch = self.input[self.pos];

                    // Handle backslash-newline line continuation
                    if ch == '\\' && self.peek(1) == Some('\n') {
                        self.pos += 2;
                        ln += 1;
                        col = 1;
                        continue;
                    }

                    if ch == '\\' && self.pos + 1 < self.input.len() && !in_param_sq {
                        value.push(ch);
                        self.pos += 1;
                        col += 1;
                        value.push(self.input[self.pos]);
                        self.pos += 1;
                        col += 1;
                        continue;
                    }

                    value.push(ch);

                    if in_param_sq {
                        if ch == '\'' {
                            in_param_sq = false;
                        }
                    } else if in_param_dq {
                        if ch == '"' {
                            in_param_dq = false;
                        }
                    } else {
                        match ch {
                            '\'' => {
                                in_param_sq = true;
                                single_quote_start_line = ln;
                                single_quote_start_col = col;
                            }
                            '"' => {
                                in_param_dq = true;
                                double_quote_start_line = ln;
                                double_quote_start_col = col;
                            }
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                    }

                    if ch == '\n' {
                        ln += 1;
                        col = 0;
                    }
                    self.pos += 1;
                    col += 1;
                }

                // Check for unterminated quotes inside ${...}
                if in_param_sq {
                    return Err(LexerError::new(
                        "unexpected EOF while looking for matching `''",
                        single_quote_start_line,
                        single_quote_start_col,
                    ));
                }
                if in_param_dq {
                    return Err(LexerError::new(
                        "unexpected EOF while looking for matching `\"'",
                        double_quote_start_line,
                        double_quote_start_col,
                    ));
                }

                continue;
            }

            // Handle $[...] old-style arithmetic - consume the entire construct
            if c == '$' && self.peek(1) == Some('[') && !in_single_quote {
                value.push(c);
                self.pos += 1;
                col += 1;
                value.push(self.current().unwrap());
                self.pos += 1;
                col += 1;

                let mut depth = 1;
                while depth > 0 && self.pos < self.input.len() {
                    let ch = self.input[self.pos];
                    value.push(ch);
                    if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                    } else if ch == '\n' {
                        ln += 1;
                        col = 0;
                    }
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            // Handle special variables $#, $?, $$, etc.
            if c == '$' && !in_single_quote {
                if let Some(next) = self.peek(1) {
                    if matches!(next, '#' | '?' | '$' | '!' | '@' | '*' | '-') || next.is_ascii_digit() {
                        value.push(c);
                        value.push(next);
                        self.pos += 2;
                        col += 2;
                        continue;
                    }
                }
            }

            // Handle backtick command substitution
            if c == '`' && !in_single_quote {
                value.push(c);
                self.pos += 1;
                col += 1;
                while let Some(ch) = self.current() {
                    if ch == '`' {
                        break;
                    }
                    value.push(ch);
                    if ch == '\\' && self.peek(1).is_some() {
                        value.push(self.peek(1).unwrap());
                        self.pos += 1;
                        col += 1;
                    }
                    if ch == '\n' {
                        ln += 1;
                        col = 0;
                    }
                    self.pos += 1;
                    col += 1;
                }
                if self.current() == Some('`') {
                    value.push('`');
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            // Regular character
            value.push(c);
            self.pos += 1;
            if c == '\n' {
                ln += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        self.column = col;
        self.line = ln;

        // Handle content after quote
        if has_content_after_quote && starts_with_quote {
            let open_quote = self.input[start];
            value = format!("{}{}", open_quote, value);
            quoted = false;
            single_quoted = false;
        }

        // Check for unterminated quotes
        if in_single_quote || in_double_quote {
            let quote_type = if in_single_quote { "'" } else { "\"" };
            return Err(LexerError::new(
                format!("unexpected EOF while looking for matching `{}'", quote_type),
                line,
                column,
            ));
        }

        // Check if fully quoted
        if !starts_with_quote && value.len() >= 2 {
            let chars: Vec<char> = value.chars().collect();
            if chars[0] == '\'' && chars[chars.len() - 1] == '\'' {
                let inner: String = chars[1..chars.len() - 1].iter().collect();
                if !inner.contains('\'') && !inner.contains('"') {
                    value = inner;
                    quoted = true;
                    single_quoted = true;
                }
            } else if chars[0] == '"' && chars[chars.len() - 1] == '"' {
                let inner: String = chars[1..chars.len() - 1].iter().collect();
                let mut has_unescaped = false;
                let mut i = 0;
                let inner_chars: Vec<char> = inner.chars().collect();
                while i < inner_chars.len() {
                    if inner_chars[i] == '"' {
                        has_unescaped = true;
                        break;
                    }
                    if inner_chars[i] == '\\' && i + 1 < inner_chars.len() {
                        i += 1;
                    }
                    i += 1;
                }
                if !has_unescaped {
                    value = inner;
                    quoted = true;
                    single_quoted = false;
                }
            }
        }

        if value.is_empty() {
            return Ok(Some(
                Token::new(TokenType::Word, "", start, self.pos, line, column)
                    .with_quotes(quoted, single_quoted),
            ));
        }

        // Check for reserved words
        if !quoted {
            if let Some(&token_type) = RESERVED_WORDS.get(value.as_str()) {
                return Ok(Some(Token::new(
                    token_type,
                    value,
                    start,
                    self.pos,
                    line,
                    column,
                )));
            }
        }

        // Check for assignment
        if !starts_with_quote {
            if let Some(eq_idx) = find_assignment_eq(&value) {
                if eq_idx > 0 && is_valid_assignment_lhs(&value[..eq_idx]) {
                    return Ok(Some(
                        Token::new(TokenType::AssignmentWord, value, start, self.pos, line, column)
                            .with_quotes(quoted, single_quoted),
                    ));
                }
            }
        }

        // Check for number
        if value.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Some(Token::new(
                TokenType::Number,
                value,
                start,
                self.pos,
                line,
                column,
            )));
        }

        // Check for valid name
        if is_valid_name(&value) {
            return Ok(Some(
                Token::new(TokenType::Name, value, start, self.pos, line, column)
                    .with_quotes(quoted, single_quoted),
            ));
        }

        Ok(Some(
            Token::new(TokenType::Word, value, start, self.pos, line, column)
                .with_quotes(quoted, single_quoted),
        ))
    }

    fn read_heredoc_content(&mut self) -> Result<(), LexerError> {
        while let Some(heredoc) = self.pending_heredocs.pop() {
            let start = self.pos;
            let start_line = self.line;
            let start_column = self.column;
            let mut content = String::new();

            while self.pos < self.input.len() {
                let mut line_content = String::new();

                // Read one line
                while self.pos < self.input.len() && self.input[self.pos] != '\n' {
                    line_content.push(self.input[self.pos]);
                    self.pos += 1;
                    self.column += 1;
                }

                // Check for delimiter
                let line_to_check = if heredoc.strip_tabs {
                    line_content.trim_start_matches('\t').to_string()
                } else {
                    line_content.clone()
                };

                if line_to_check == heredoc.delimiter {
                    // Consume the newline
                    if self.pos < self.input.len() && self.input[self.pos] == '\n' {
                        self.pos += 1;
                        self.line += 1;
                        self.column = 1;
                    }
                    break;
                }

                content.push_str(&line_content);
                if self.pos < self.input.len() && self.input[self.pos] == '\n' {
                    content.push('\n');
                    self.pos += 1;
                    self.line += 1;
                    self.column = 1;
                }
            }

            self.tokens.push(Token::new(
                TokenType::HeredocContent,
                content,
                start,
                self.pos,
                start_line,
                start_column,
            ));
        }
        Ok(())
    }

    fn register_heredoc_from_lookahead(&mut self, strip_tabs: bool) {
        let saved_pos = self.pos;
        let saved_column = self.column;

        // Skip whitespace
        while self.pos < self.input.len() && matches!(self.input.get(self.pos), Some(' ' | '\t')) {
            self.pos += 1;
            self.column += 1;
        }

        let mut delimiter = String::new();
        let mut quoted = false;

        while self.pos < self.input.len() {
            let c = self.input[self.pos];

            if c.is_whitespace() || matches!(c, ';' | '<' | '>' | '&' | '|' | '(' | ')') {
                break;
            }

            if c == '\'' || c == '"' {
                quoted = true;
                let quote_char = c;
                self.pos += 1;
                self.column += 1;
                while self.pos < self.input.len() && self.input[self.pos] != quote_char {
                    delimiter.push(self.input[self.pos]);
                    self.pos += 1;
                    self.column += 1;
                }
                if self.pos < self.input.len() && self.input[self.pos] == quote_char {
                    self.pos += 1;
                    self.column += 1;
                }
            } else if c == '\\' {
                quoted = true;
                self.pos += 1;
                self.column += 1;
                if self.pos < self.input.len() {
                    delimiter.push(self.input[self.pos]);
                    self.pos += 1;
                    self.column += 1;
                }
            } else {
                delimiter.push(c);
                self.pos += 1;
                self.column += 1;
            }
        }

        self.pos = saved_pos;
        self.column = saved_column;

        if !delimiter.is_empty() {
            self.pending_heredocs.push(PendingHeredoc {
                delimiter,
                strip_tabs,
                quoted,
            });
        }
    }

    fn is_word_char_following(&self, pos: usize) -> bool {
        if pos >= self.input.len() {
            return false;
        }
        let c = self.input[pos];
        !is_word_boundary(c)
    }

    fn read_word_with_brace_expansion(
        &mut self,
        start: usize,
        line: usize,
        column: usize,
    ) -> Result<Option<Token>, LexerError> {
        let mut col = column;

        while self.pos < self.input.len() {
            let c = self.input[self.pos];

            if is_word_boundary(c) {
                break;
            }

            if c == '{' {
                if self.scan_brace_expansion(self.pos).is_some() {
                    let mut depth = 1;
                    self.pos += 1;
                    col += 1;
                    while self.pos < self.input.len() && depth > 0 {
                        match self.input[self.pos] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        self.pos += 1;
                        col += 1;
                    }
                    continue;
                }
                self.pos += 1;
                col += 1;
                continue;
            }

            if c == '}' {
                self.pos += 1;
                col += 1;
                continue;
            }

            if c == '$' && self.peek(1) == Some('(') {
                self.pos += 1;
                col += 1;
                self.pos += 1;
                col += 1;
                let mut depth = 1;
                while depth > 0 && self.pos < self.input.len() {
                    match self.input[self.pos] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            if c == '$' && self.peek(1) == Some('{') {
                self.pos += 1;
                col += 1;
                self.pos += 1;
                col += 1;
                let mut depth = 1;
                while depth > 0 && self.pos < self.input.len() {
                    match self.input[self.pos] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            if c == '`' {
                self.pos += 1;
                col += 1;
                while self.pos < self.input.len() && self.input[self.pos] != '`' {
                    if self.input[self.pos] == '\\' && self.pos + 1 < self.input.len() {
                        self.pos += 2;
                        col += 2;
                    } else {
                        self.pos += 1;
                        col += 1;
                    }
                }
                if self.pos < self.input.len() {
                    self.pos += 1;
                    col += 1;
                }
                continue;
            }

            self.pos += 1;
            col += 1;
        }

        let value: String = self.input[start..self.pos].iter().collect();
        self.column = col;

        Ok(Some(
            Token::new(TokenType::Word, value, start, self.pos, line, column)
                .with_quotes(false, false),
        ))
    }

    fn scan_brace_expansion(&self, start_pos: usize) -> Option<String> {
        let mut pos = start_pos + 1;
        let mut depth = 1;
        let mut has_comma = false;
        let mut has_range = false;

        while pos < self.input.len() && depth > 0 {
            let c = self.input[pos];

            match c {
                '{' => {
                    depth += 1;
                    pos += 1;
                }
                '}' => {
                    depth -= 1;
                    pos += 1;
                }
                ',' if depth == 1 => {
                    has_comma = true;
                    pos += 1;
                }
                '.' if pos + 1 < self.input.len() && self.input[pos + 1] == '.' => {
                    has_range = true;
                    pos += 2;
                }
                ' ' | '\t' | '\n' | ';' | '&' | '|' => return None,
                _ => pos += 1,
            }
        }

        if depth == 0 && (has_comma || has_range) {
            Some(self.input[start_pos..pos].iter().collect())
        } else {
            None
        }
    }

    fn scan_literal_brace_word(&self, start_pos: usize) -> Option<String> {
        let mut pos = start_pos + 1;
        let mut depth = 1;

        while pos < self.input.len() && depth > 0 {
            let c = self.input[pos];

            match c {
                '{' => {
                    depth += 1;
                    pos += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(self.input[start_pos..=pos].iter().collect());
                    }
                    pos += 1;
                }
                ' ' | '\t' | '\n' | ';' | '&' | '|' => return None,
                _ => pos += 1,
            }
        }

        None
    }

    fn scan_extglob_pattern(&self, start_pos: usize) -> Option<ExtglobResult> {
        let mut pos = start_pos + 1;
        let mut depth = 1;

        while pos < self.input.len() && depth > 0 {
            let c = self.input[pos];

            if c == '\\' && pos + 1 < self.input.len() {
                pos += 2;
                continue;
            }

            if "@*+?!".contains(c) && pos + 1 < self.input.len() && self.input[pos + 1] == '(' {
                pos += 1;
                depth += 1;
                pos += 1;
                continue;
            }

            match c {
                '(' => {
                    depth += 1;
                    pos += 1;
                }
                ')' => {
                    depth -= 1;
                    pos += 1;
                }
                '\n' => return None,
                _ => pos += 1,
            }
        }

        if depth == 0 {
            Some(ExtglobResult {
                content: self.input[start_pos..pos].iter().collect(),
                end: pos,
            })
        } else {
            None
        }
    }

    fn scan_fd_variable(&self, start_pos: usize) -> Option<FdVariableResult> {
        let mut pos = start_pos + 1;

        // Scan variable name
        let name_start = pos;
        while pos < self.input.len() {
            let c = self.input[pos];
            if pos == name_start {
                if !c.is_ascii_alphabetic() && c != '_' {
                    return None;
                }
            } else if !c.is_ascii_alphanumeric() && c != '_' {
                break;
            }
            pos += 1;
        }

        if pos == name_start {
            return None;
        }

        let varname: String = self.input[name_start..pos].iter().collect();

        // Must be followed by closing brace
        if pos >= self.input.len() || self.input[pos] != '}' {
            return None;
        }
        pos += 1;

        // Must be immediately followed by a redirect operator
        if pos >= self.input.len() {
            return None;
        }

        let c = self.input[pos];
        let c2 = self.input.get(pos + 1).copied();

        let is_redirect_op = c == '>' || c == '<' || (c == '&' && matches!(c2, Some('>' | '<')));

        if !is_redirect_op {
            return None;
        }

        Some(FdVariableResult { varname, end: pos })
    }

    /// Scan ahead from a $(( position to determine if it should be treated as
    /// $( ( subshell ) ) instead of $(( arithmetic )).
    /// This handles cases like:
    ///   echo $(( echo 1
    ///   echo 2
    ///   ) )
    /// which should be a command substitution containing a subshell, not arithmetic.
    ///
    /// @param start_pos - position at the second ( (i.e., at input[start_pos] === "(")
    /// @returns true if this is a subshell (closes with ) )), false if arithmetic (closes with )))
    fn dollar_dparen_is_subshell(&self, start_pos: usize) -> bool {
        let mut pos = start_pos + 1; // Skip the second (
        let mut depth = 2; // We've seen $((, so we start at depth 2
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut has_newline = false;

        while pos < self.input.len() && depth > 0 {
            let c = self.input[pos];

            if in_single_quote {
                if c == '\'' {
                    in_single_quote = false;
                }
                if c == '\n' {
                    has_newline = true;
                }
                pos += 1;
                continue;
            }

            if in_double_quote {
                if c == '\\' && pos + 1 < self.input.len() {
                    // Skip escaped char
                    pos += 2;
                    continue;
                }
                if c == '"' {
                    in_double_quote = false;
                }
                if c == '\n' {
                    has_newline = true;
                }
                pos += 1;
                continue;
            }

            // Not in quotes
            match c {
                '\'' => {
                    in_single_quote = true;
                    pos += 1;
                }
                '"' => {
                    in_double_quote = true;
                    pos += 1;
                }
                '\\' if pos + 1 < self.input.len() => {
                    // Skip escaped char
                    pos += 2;
                }
                '\n' => {
                    has_newline = true;
                    pos += 1;
                }
                '(' => {
                    depth += 1;
                    pos += 1;
                }
                ')' => {
                    depth -= 1;
                    if depth == 1 {
                        // We've closed the inner subshell. Check what follows.
                        let next_pos = pos + 1;
                        if next_pos < self.input.len() && self.input[next_pos] == ')' {
                            // )) - adjacent parens = arithmetic
                            return false;
                        }
                        // Check if there's whitespace followed by )
                        let mut scan_pos = next_pos;
                        let mut has_whitespace = false;
                        while scan_pos < self.input.len()
                            && matches!(self.input.get(scan_pos), Some(' ' | '\t' | '\n'))
                        {
                            has_whitespace = true;
                            scan_pos += 1;
                        }
                        if has_whitespace && scan_pos < self.input.len() && self.input[scan_pos] == ')' {
                            // This is ) ) with whitespace - subshell
                            return true;
                        }
                        // If it has newlines, treat as subshell
                        if has_newline {
                            return true;
                        }
                    }
                    if depth == 0 {
                        return false;
                    }
                    pos += 1;
                }
                _ => {
                    pos += 1;
                }
            }
        }

        // Didn't find a definitive answer - default to arithmetic behavior
        false
    }

    /// Add a pending heredoc (used by parser)
    pub fn add_pending_heredoc(&mut self, delimiter: String, strip_tabs: bool, quoted: bool) {
        self.pending_heredocs.push(PendingHeredoc {
            delimiter,
            strip_tabs,
            quoted,
        });
    }
}

struct ExtglobResult {
    content: String,
    end: usize,
}

struct FdVariableResult {
    varname: String,
    end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let lexer = Lexer::new("echo hello");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3); // echo, hello, EOF
        assert_eq!(tokens[0].token_type, TokenType::Name);
        assert_eq!(tokens[0].value, "echo");
        assert_eq!(tokens[1].token_type, TokenType::Name);
        assert_eq!(tokens[1].value, "hello");
    }

    #[test]
    fn test_pipeline() {
        let lexer = Lexer::new("cat file | grep pattern");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[2].token_type, TokenType::Pipe);
    }

    #[test]
    fn test_redirection() {
        let lexer = Lexer::new("echo hello > file.txt");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[2].token_type, TokenType::Great);
    }

    #[test]
    fn test_assignment() {
        let lexer = Lexer::new("VAR=value");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::AssignmentWord);
        assert_eq!(tokens[0].value, "VAR=value");
    }

    #[test]
    fn test_double_quotes() {
        let lexer = Lexer::new("echo \"hello world\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[1].value, "hello world");
        assert!(tokens[1].quoted);
    }

    #[test]
    fn test_single_quotes() {
        let lexer = Lexer::new("echo 'hello world'");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[1].value, "hello world");
        assert!(tokens[1].quoted);
        assert!(tokens[1].single_quoted);
    }

    #[test]
    fn test_reserved_words() {
        let lexer = Lexer::new("if then else fi");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::If);
        assert_eq!(tokens[1].token_type, TokenType::Then);
        assert_eq!(tokens[2].token_type, TokenType::Else);
        assert_eq!(tokens[3].token_type, TokenType::Fi);
    }

    #[test]
    fn test_heredoc() {
        let lexer = Lexer::new("cat <<EOF\nhello\nEOF\n");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[1].token_type, TokenType::DLess);
        // Find heredoc content token
        let heredoc_token = tokens.iter().find(|t| t.token_type == TokenType::HeredocContent);
        assert!(heredoc_token.is_some());
        assert_eq!(heredoc_token.unwrap().value, "hello\n");
    }

    #[test]
    fn test_comment() {
        let lexer = Lexer::new("echo hello # this is a comment");
        let tokens = lexer.tokenize().unwrap();
        let comment = tokens.iter().find(|t| t.token_type == TokenType::Comment);
        assert!(comment.is_some());
    }

    #[test]
    fn test_arithmetic() {
        let lexer = Lexer::new("(( x + 1 ))");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::DParenStart);
    }

    #[test]
    fn test_conditional() {
        let lexer = Lexer::new("[[ -f file ]]");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::DBrackStart);
    }
}
