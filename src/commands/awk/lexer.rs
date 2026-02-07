/// AWK Lexer
///
/// Tokenizes AWK source code into a stream of tokens.
/// Ported from the TypeScript implementation.

use super::types::{Token, TokenType};

// ─── POSIX Character Class Expansion ─────────────────────────

/// Expand POSIX character classes in regex patterns to their equivalent
/// character class expressions.
fn expand_posix_classes(pattern: &str) -> String {
    pattern
        .replace("[[:space:]]", r"[ \t\n\r\f\v]")
        .replace("[[:blank:]]", r"[ \t]")
        .replace("[[:alpha:]]", "[a-zA-Z]")
        .replace("[[:digit:]]", "[0-9]")
        .replace("[[:alnum:]]", "[a-zA-Z0-9]")
        .replace("[[:upper:]]", "[A-Z]")
        .replace("[[:lower:]]", "[a-z]")
        .replace(
            "[[:punct:]]",
            r##"[!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~]"##,
        )
        .replace("[[:xdigit:]]", "[0-9A-Fa-f]")
        .replace("[[:graph:]]", "[!-~]")
        .replace("[[:print:]]", "[ -~]")
        .replace("[[:cntrl:]]", r"[\x00-\x1f\x7f]")
}

// ─── Keywords Map ────────────────────────────────────────────

/// Look up a keyword by name. Returns the corresponding TokenType if the
/// identifier is a reserved keyword, or None if it is a regular identifier.
fn lookup_keyword(name: &str) -> Option<TokenType> {
    match name {
        "BEGIN" => Some(TokenType::Begin),
        "END" => Some(TokenType::End),
        "if" => Some(TokenType::If),
        "else" => Some(TokenType::Else),
        "while" => Some(TokenType::While),
        "do" => Some(TokenType::Do),
        "for" => Some(TokenType::For),
        "in" => Some(TokenType::In),
        "break" => Some(TokenType::Break),
        "continue" => Some(TokenType::Continue),
        "next" => Some(TokenType::Next),
        "nextfile" => Some(TokenType::NextFile),
        "exit" => Some(TokenType::Exit),
        "return" => Some(TokenType::Return),
        "delete" => Some(TokenType::Delete),
        "function" => Some(TokenType::Function),
        "print" => Some(TokenType::Print),
        "printf" => Some(TokenType::Printf),
        "getline" => Some(TokenType::Getline),
        _ => None,
    }
}

// ─── Context-Sensitive Regex Detection ───────────────────────

/// Determines whether a `/` at the current position should be interpreted
/// as the start of a regex literal (true) or as a division operator (false).
///
/// After certain token types (Number, String, Ident, RParen, RBracket,
/// Increment, Decrement), `/` means division. After everything else
/// (or at the start of input), `/` starts a regex.
fn can_be_regex(last_token: Option<&TokenType>) -> bool {
    match last_token {
        None => true,
        Some(tt) => !matches!(
            tt,
            TokenType::Number
                | TokenType::String
                | TokenType::Ident
                | TokenType::RParen
                | TokenType::RBracket
                | TokenType::Increment
                | TokenType::Decrement
        ),
    }
}

// ─── Lexer Struct ────────────────────────────────────────────

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    last_token_type: Option<TokenType>,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            last_token_type: None,
        }
    }

    // ── Helpers ──────────────────────────────────────────────

    fn peek(&self, offset: usize) -> char {
        if self.pos + offset < self.chars.len() {
            self.chars[self.pos + offset]
        } else {
            '\0'
        }
    }

    fn advance(&mut self) -> char {
        if self.pos >= self.chars.len() {
            return '\0';
        }
        let ch = self.chars[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn make_token(&self, token_type: TokenType, value: String, line: usize, column: usize) -> Token {
        Token {
            token_type,
            value,
            line,
            column,
        }
    }

    // ── Whitespace / Comments / Line Continuation ────────────

    fn skip_whitespace(&mut self) {
        while !self.at_end() {
            let ch = self.peek(0);
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else if ch == '\\' && self.peek(1) == '\n' {
                // Line continuation
                self.advance(); // skip backslash
                self.advance(); // skip newline
            } else if ch == '#' {
                // Comment - skip to end of line
                while !self.at_end() && self.peek(0) != '\n' {
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    // ── String Reading ───────────────────────────────────────

    fn read_string(&mut self) -> Token {
        let start_line = self.line;
        let start_column = self.column;
        self.advance(); // skip opening quote
        let mut value = String::new();

        while !self.at_end() && self.peek(0) != '"' {
            if self.peek(0) == '\\' {
                self.advance(); // skip backslash
                let escaped = self.advance();
                match escaped {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    'f' => value.push('\x0C'),
                    'b' => value.push('\x08'),
                    'v' => value.push('\x0B'),
                    'a' => value.push('\x07'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '/' => value.push('/'),
                    'x' => {
                        // Hex escape: \xHH (exactly 2 hex digits)
                        let mut hex = String::new();
                        while hex.len() < 2 && !self.at_end() && self.peek(0).is_ascii_hexdigit()
                        {
                            hex.push(self.advance());
                        }
                        if !hex.is_empty() {
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(code) {
                                    value.push(c);
                                }
                            }
                        } else {
                            value.push('x');
                        }
                    }
                    c if c.is_ascii_digit() && c < '8' => {
                        // Octal escape: \0 to \377 (1-3 octal digits)
                        let mut octal = String::new();
                        octal.push(c);
                        while octal.len() < 3
                            && !self.at_end()
                            && self.peek(0) >= '0'
                            && self.peek(0) <= '7'
                        {
                            octal.push(self.advance());
                        }
                        if let Ok(code) = u32::from_str_radix(&octal, 8) {
                            if let Some(ch) = char::from_u32(code) {
                                value.push(ch);
                            }
                        }
                    }
                    other => {
                        value.push(other);
                    }
                }
            } else {
                value.push(self.advance());
            }
        }

        if !self.at_end() && self.peek(0) == '"' {
            self.advance(); // skip closing quote
        }

        self.make_token(TokenType::String, value, start_line, start_column)
    }

    // ── Regex Reading ────────────────────────────────────────

    fn read_regex(&mut self) -> Token {
        let start_line = self.line;
        let start_column = self.column;
        self.advance(); // skip opening /
        let mut pattern = String::new();

        while !self.at_end() && self.peek(0) != '/' {
            if self.peek(0) == '\\' {
                pattern.push(self.advance()); // push backslash
                if !self.at_end() {
                    pattern.push(self.advance()); // push escaped char
                }
            } else if self.peek(0) == '\n' {
                // Unterminated regex
                break;
            } else {
                pattern.push(self.advance());
            }
        }

        if !self.at_end() && self.peek(0) == '/' {
            self.advance(); // skip closing /
        }

        // Expand POSIX character classes
        let pattern = expand_posix_classes(&pattern);

        self.make_token(TokenType::Regex, pattern, start_line, start_column)
    }

    // ── Number Reading ───────────────────────────────────────

    fn read_number(&mut self) -> Token {
        let start_line = self.line;
        let start_column = self.column;
        let mut num_str = String::new();

        // Integer part
        while !self.at_end() && self.peek(0).is_ascii_digit() {
            num_str.push(self.advance());
        }

        // Decimal part
        if !self.at_end() && self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
            num_str.push(self.advance()); // the dot
            while !self.at_end() && self.peek(0).is_ascii_digit() {
                num_str.push(self.advance());
            }
        }

        // Exponent part
        if !self.at_end() && (self.peek(0) == 'e' || self.peek(0) == 'E') {
            num_str.push(self.advance());
            if !self.at_end() && (self.peek(0) == '+' || self.peek(0) == '-') {
                num_str.push(self.advance());
            }
            while !self.at_end() && self.peek(0).is_ascii_digit() {
                num_str.push(self.advance());
            }
        }

        self.make_token(TokenType::Number, num_str, start_line, start_column)
    }

    // ── Identifier / Keyword Reading ─────────────────────────

    fn read_identifier(&mut self) -> Token {
        let start_line = self.line;
        let start_column = self.column;
        let mut name = String::new();

        while !self.at_end()
            && (self.peek(0).is_ascii_alphanumeric() || self.peek(0) == '_')
        {
            name.push(self.advance());
        }

        if let Some(keyword_type) = lookup_keyword(&name) {
            self.make_token(keyword_type, name, start_line, start_column)
        } else {
            self.make_token(TokenType::Ident, name, start_line, start_column)
        }
    }

    // ── Operator Reading ─────────────────────────────────────

    fn read_operator(&mut self) -> Token {
        let start_line = self.line;
        let start_column = self.column;
        let ch = self.advance();
        let next = self.peek(0);

        match ch {
            '+' => {
                if next == '+' {
                    self.advance();
                    self.make_token(TokenType::Increment, "++".into(), start_line, start_column)
                } else if next == '=' {
                    self.advance();
                    self.make_token(TokenType::PlusAssign, "+=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Plus, "+".into(), start_line, start_column)
                }
            }
            '-' => {
                if next == '-' {
                    self.advance();
                    self.make_token(TokenType::Decrement, "--".into(), start_line, start_column)
                } else if next == '=' {
                    self.advance();
                    self.make_token(TokenType::MinusAssign, "-=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Minus, "-".into(), start_line, start_column)
                }
            }
            '*' => {
                if next == '*' {
                    self.advance();
                    // ** is an alias for ^ (power operator)
                    self.make_token(TokenType::Caret, "**".into(), start_line, start_column)
                } else if next == '=' {
                    self.advance();
                    self.make_token(TokenType::StarAssign, "*=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Star, "*".into(), start_line, start_column)
                }
            }
            '/' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::SlashAssign, "/=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Slash, "/".into(), start_line, start_column)
                }
            }
            '%' => {
                if next == '=' {
                    self.advance();
                    self.make_token(
                        TokenType::PercentAssign,
                        "%=".into(),
                        start_line,
                        start_column,
                    )
                } else {
                    self.make_token(TokenType::Percent, "%".into(), start_line, start_column)
                }
            }
            '^' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::CaretAssign, "^=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Caret, "^".into(), start_line, start_column)
                }
            }
            '=' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::Eq, "==".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Assign, "=".into(), start_line, start_column)
                }
            }
            '!' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::Ne, "!=".into(), start_line, start_column)
                } else if next == '~' {
                    self.advance();
                    self.make_token(TokenType::NotMatch, "!~".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Not, "!".into(), start_line, start_column)
                }
            }
            '<' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::Le, "<=".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Lt, "<".into(), start_line, start_column)
                }
            }
            '>' => {
                if next == '=' {
                    self.advance();
                    self.make_token(TokenType::Ge, ">=".into(), start_line, start_column)
                } else if next == '>' {
                    self.advance();
                    self.make_token(TokenType::Append, ">>".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Gt, ">".into(), start_line, start_column)
                }
            }
            '&' => {
                if next == '&' {
                    self.advance();
                    self.make_token(TokenType::And, "&&".into(), start_line, start_column)
                } else {
                    // Single & is not valid in AWK, treat as unknown
                    self.make_token(
                        TokenType::Ident,
                        "&".into(),
                        start_line,
                        start_column,
                    )
                }
            }
            '|' => {
                if next == '|' {
                    self.advance();
                    self.make_token(TokenType::Or, "||".into(), start_line, start_column)
                } else {
                    self.make_token(TokenType::Pipe, "|".into(), start_line, start_column)
                }
            }
            '~' => self.make_token(TokenType::Match, "~".into(), start_line, start_column),
            '?' => self.make_token(TokenType::Question, "?".into(), start_line, start_column),
            ':' => self.make_token(TokenType::Colon, ":".into(), start_line, start_column),
            ',' => self.make_token(TokenType::Comma, ",".into(), start_line, start_column),
            ';' => self.make_token(TokenType::Semicolon, ";".into(), start_line, start_column),
            '(' => self.make_token(TokenType::LParen, "(".into(), start_line, start_column),
            ')' => self.make_token(TokenType::RParen, ")".into(), start_line, start_column),
            '{' => self.make_token(TokenType::LBrace, "{".into(), start_line, start_column),
            '}' => self.make_token(TokenType::RBrace, "}".into(), start_line, start_column),
            '[' => self.make_token(TokenType::LBracket, "[".into(), start_line, start_column),
            ']' => self.make_token(TokenType::RBracket, "]".into(), start_line, start_column),
            '$' => self.make_token(TokenType::Dollar, "$".into(), start_line, start_column),
            _ => {
                // Unknown character - return as identifier to allow graceful handling
                self.make_token(
                    TokenType::Ident,
                    ch.to_string(),
                    start_line,
                    start_column,
                )
            }
        }
    }

    // ── Main Token Dispatch ──────────────────────────────────

    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();

        if self.at_end() {
            return None;
        }

        let ch = self.peek(0);

        // Newline
        if ch == '\n' {
            let start_line = self.line;
            let start_column = self.column;
            self.advance();
            return Some(self.make_token(
                TokenType::Newline,
                "\n".into(),
                start_line,
                start_column,
            ));
        }

        // String literal
        if ch == '"' {
            return Some(self.read_string());
        }

        // Regex literal - context-sensitive
        if ch == '/' && can_be_regex(self.last_token_type.as_ref()) {
            return Some(self.read_regex());
        }

        // Number
        if ch.is_ascii_digit() || (ch == '.' && self.peek(1).is_ascii_digit()) {
            return Some(self.read_number());
        }

        // Identifier or keyword
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Some(self.read_identifier());
        }

        // Operators and punctuation
        Some(self.read_operator())
    }
}

// ─── Public API ──────────────────────────────────────────────

/// Tokenize AWK source code into a vector of tokens.
///
/// This is the main entry point for the lexer. It processes the entire
/// input string and returns a complete token stream ending with an Eof token.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    while !lexer.at_end() {
        if let Some(token) = lexer.next_token() {
            lexer.last_token_type = Some(token.token_type.clone());
            tokens.push(token);
        }
    }

    tokens.push(Token {
        token_type: TokenType::Eof,
        value: String::new(),
        line: lexer.line,
        column: lexer.column,
    });

    tokens
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: collect just the token types from a token stream (excluding Eof).
    fn types(input: &str) -> Vec<TokenType> {
        tokenize(input)
            .into_iter()
            .map(|t| t.token_type)
            .filter(|t| *t != TokenType::Eof)
            .collect()
    }

    /// Helper: collect (type, value) pairs excluding Eof.
    fn type_vals(input: &str) -> Vec<(TokenType, String)> {
        tokenize(input)
            .into_iter()
            .filter(|t| t.token_type != TokenType::Eof)
            .map(|t| (t.token_type, t.value))
            .collect()
    }

    #[test]
    fn test_print_field() {
        // { print $1 }
        let toks = types("{ print $1 }");
        assert_eq!(
            toks,
            vec![
                TokenType::LBrace,
                TokenType::Print,
                TokenType::Dollar,
                TokenType::Number,
                TokenType::RBrace,
            ]
        );
    }

    #[test]
    fn test_print_string() {
        // { print "hello" }
        let toks = type_vals(r#"{ print "hello" }"#);
        assert!(toks.contains(&(TokenType::String, "hello".into())));
    }

    #[test]
    fn test_regex_pattern() {
        // /pattern/ { print }
        let toks = types("/pattern/ { print }");
        assert_eq!(toks[0], TokenType::Regex);
        assert_eq!(
            tokenize("/pattern/ { print }")[0].value,
            "pattern"
        );
    }

    #[test]
    fn test_arithmetic_operators() {
        // a + b * c
        let toks = types("a + b * c");
        assert_eq!(
            toks,
            vec![
                TokenType::Ident,
                TokenType::Plus,
                TokenType::Ident,
                TokenType::Star,
                TokenType::Ident,
            ]
        );
    }

    #[test]
    fn test_field_comparison() {
        // $1 == "foo"
        let toks = types(r#"$1 == "foo""#);
        assert_eq!(
            toks,
            vec![
                TokenType::Dollar,
                TokenType::Number,
                TokenType::Eq,
                TokenType::String,
            ]
        );
    }

    #[test]
    fn test_plus_assign() {
        // x += 1
        let toks = types("x += 1");
        assert_eq!(
            toks,
            vec![TokenType::Ident, TokenType::PlusAssign, TokenType::Number]
        );
    }

    #[test]
    fn test_increment() {
        // i++
        let toks = types("i++");
        assert_eq!(toks, vec![TokenType::Ident, TokenType::Increment]);
    }

    #[test]
    fn test_begin_keyword() {
        // BEGIN { x=0 }
        let toks = types("BEGIN { x=0 }");
        assert_eq!(
            toks,
            vec![
                TokenType::Begin,
                TokenType::LBrace,
                TokenType::Ident,
                TokenType::Assign,
                TokenType::Number,
                TokenType::RBrace,
            ]
        );
    }

    #[test]
    fn test_all_keywords() {
        let keywords = vec![
            ("BEGIN", TokenType::Begin),
            ("END", TokenType::End),
            ("if", TokenType::If),
            ("else", TokenType::Else),
            ("while", TokenType::While),
            ("do", TokenType::Do),
            ("for", TokenType::For),
            ("in", TokenType::In),
            ("break", TokenType::Break),
            ("continue", TokenType::Continue),
            ("next", TokenType::Next),
            ("nextfile", TokenType::NextFile),
            ("exit", TokenType::Exit),
            ("return", TokenType::Return),
            ("delete", TokenType::Delete),
            ("function", TokenType::Function),
            ("print", TokenType::Print),
            ("printf", TokenType::Printf),
            ("getline", TokenType::Getline),
        ];
        for (kw, expected) in keywords {
            let toks = types(kw);
            assert_eq!(toks, vec![expected], "keyword '{}' mismatch", kw);
        }
    }

    #[test]
    fn test_regex_vs_division() {
        // After identifier, / is division
        let toks = types("a / b");
        assert_eq!(
            toks,
            vec![TokenType::Ident, TokenType::Slash, TokenType::Ident]
        );

        // At start, / is regex
        let toks = types("/pattern/");
        assert_eq!(toks, vec![TokenType::Regex]);
    }

    #[test]
    fn test_string_escapes() {
        let toks = tokenize(r#""hello\tworld\n""#);
        let string_tok = &toks[0];
        assert_eq!(string_tok.token_type, TokenType::String);
        assert_eq!(string_tok.value, "hello\tworld\n");
    }

    #[test]
    fn test_hex_octal_escapes() {
        // \x41 = 'A', \101 = 'A' (octal 101 = 65 decimal)
        let toks = tokenize(r#""\x41\101""#);
        let string_tok = &toks[0];
        assert_eq!(string_tok.token_type, TokenType::String);
        assert_eq!(string_tok.value, "AA");
    }

    #[test]
    fn test_comments_skipped() {
        let toks = types("a # this is a comment\nb");
        assert_eq!(
            toks,
            vec![TokenType::Ident, TokenType::Newline, TokenType::Ident]
        );
    }

    #[test]
    fn test_line_continuation() {
        // backslash-newline should be skipped
        let toks = types("a +\\\nb");
        assert_eq!(
            toks,
            vec![TokenType::Ident, TokenType::Plus, TokenType::Ident]
        );
    }

    #[test]
    fn test_scientific_number() {
        let toks = tokenize("1.5e-10");
        assert_eq!(toks[0].token_type, TokenType::Number);
        assert_eq!(toks[0].value, "1.5e-10");
    }

    #[test]
    fn test_append_operator() {
        let toks = types("print >> file");
        assert!(toks.contains(&TokenType::Append));
    }

    #[test]
    fn test_pipe_operator() {
        let toks = types(r#"print | "cmd""#);
        assert!(toks.contains(&TokenType::Pipe));
    }

    #[test]
    fn test_ternary_operators() {
        let toks = types("a ? b : c");
        assert!(toks.contains(&TokenType::Question));
        assert!(toks.contains(&TokenType::Colon));
    }

    #[test]
    fn test_regex_match_operators() {
        let toks = types(r#"$0 ~ /foo/"#);
        assert!(toks.contains(&TokenType::Match));

        let toks2 = types(r#"$0 !~ /bar/"#);
        assert!(toks2.contains(&TokenType::NotMatch));
    }

    #[test]
    fn test_double_star_becomes_caret() {
        let toks = type_vals("2 ** 3");
        // ** should produce Caret token type with value "**"
        assert_eq!(toks[1].0, TokenType::Caret);
        assert_eq!(toks[1].1, "**");
    }
}
