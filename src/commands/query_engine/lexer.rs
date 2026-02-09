use super::ast::{Token, TokenType};

/// Keywords mapping for the jq language
fn keyword_lookup(ident: &str) -> Option<TokenType> {
    match ident {
        "and" => Some(TokenType::And),
        "or" => Some(TokenType::Or),
        "not" => Some(TokenType::Not),
        "if" => Some(TokenType::If),
        "then" => Some(TokenType::Then),
        "elif" => Some(TokenType::Elif),
        "else" => Some(TokenType::Else),
        "end" => Some(TokenType::End),
        "as" => Some(TokenType::As),
        "try" => Some(TokenType::Try),
        "catch" => Some(TokenType::Catch),
        "true" => Some(TokenType::True),
        "false" => Some(TokenType::False),
        "null" => Some(TokenType::Null),
        "reduce" => Some(TokenType::Reduce),
        "foreach" => Some(TokenType::Foreach),
        "label" => Some(TokenType::Label),
        "break" => Some(TokenType::Break),
        "def" => Some(TokenType::Def),
        _ => None,
    }
}

fn is_alpha(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_alnum(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenize a jq query string into a vector of tokens.
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut tokens: Vec<Token> = Vec::new();
    let mut pos: usize = 0;

    let peek = |pos: usize, offset: usize| -> Option<char> {
        let idx = pos + offset;
        if idx < len { Some(chars[idx]) } else { None }
    };

    while pos < len {
        let start = pos;
        let c = chars[pos];
        pos += 1;

        // Whitespace
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            continue;
        }

        // Comments
        if c == '#' {
            while pos < len && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        // Two-character operators: ..
        if c == '.' && peek(pos, 0) == Some('.') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::DotDot, pos: start });
            continue;
        }

        // ==
        if c == '=' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::Eq, pos: start });
            continue;
        }

        // !=
        if c == '!' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::Ne, pos: start });
            continue;
        }

        // <=
        if c == '<' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::Le, pos: start });
            continue;
        }

        // >=
        if c == '>' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::Ge, pos: start });
            continue;
        }

        // // and //=
        if c == '/' && peek(pos, 0) == Some('/') {
            pos += 1;
            if peek(pos, 0) == Some('=') {
                pos += 1;
                tokens.push(Token { token_type: TokenType::UpdateAlt, pos: start });
            } else {
                tokens.push(Token { token_type: TokenType::Alt, pos: start });
            }
            continue;
        }

        // +=
        if c == '+' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::UpdateAdd, pos: start });
            continue;
        }

        // -=
        if c == '-' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::UpdateSub, pos: start });
            continue;
        }

        // *=
        if c == '*' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::UpdateMul, pos: start });
            continue;
        }

        // /= (note: // and //= already handled above)
        if c == '/' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::UpdateDiv, pos: start });
            continue;
        }

        // %=
        if c == '%' && peek(pos, 0) == Some('=') {
            pos += 1;
            tokens.push(Token { token_type: TokenType::UpdateMod, pos: start });
            continue;
        }

        // = (assign, but not ==)
        if c == '=' {
            tokens.push(Token { token_type: TokenType::Assign, pos: start });
            continue;
        }

        // Single-character: .
        if c == '.' {
            tokens.push(Token { token_type: TokenType::Dot, pos: start });
            continue;
        }

        // | and |=
        if c == '|' {
            if peek(pos, 0) == Some('=') {
                pos += 1;
                tokens.push(Token { token_type: TokenType::UpdatePipe, pos: start });
            } else {
                tokens.push(Token { token_type: TokenType::Pipe, pos: start });
            }
            continue;
        }

        // Remaining single-character tokens
        match c {
            ',' => { tokens.push(Token { token_type: TokenType::Comma, pos: start }); continue; }
            ':' => { tokens.push(Token { token_type: TokenType::Colon, pos: start }); continue; }
            ';' => { tokens.push(Token { token_type: TokenType::Semicolon, pos: start }); continue; }
            '(' => { tokens.push(Token { token_type: TokenType::LParen, pos: start }); continue; }
            ')' => { tokens.push(Token { token_type: TokenType::RParen, pos: start }); continue; }
            '[' => { tokens.push(Token { token_type: TokenType::LBracket, pos: start }); continue; }
            ']' => { tokens.push(Token { token_type: TokenType::RBracket, pos: start }); continue; }
            '{' => { tokens.push(Token { token_type: TokenType::LBrace, pos: start }); continue; }
            '}' => { tokens.push(Token { token_type: TokenType::RBrace, pos: start }); continue; }
            '?' => { tokens.push(Token { token_type: TokenType::Question, pos: start }); continue; }
            '+' => { tokens.push(Token { token_type: TokenType::Plus, pos: start }); continue; }
            '-' => { tokens.push(Token { token_type: TokenType::Minus, pos: start }); continue; }
            '*' => { tokens.push(Token { token_type: TokenType::Star, pos: start }); continue; }
            '/' => { tokens.push(Token { token_type: TokenType::Slash, pos: start }); continue; }
            '%' => { tokens.push(Token { token_type: TokenType::Percent, pos: start }); continue; }
            '<' => { tokens.push(Token { token_type: TokenType::Lt, pos: start }); continue; }
            '>' => { tokens.push(Token { token_type: TokenType::Gt, pos: start }); continue; }
            _ => {}
        }

        // Numbers
        if c.is_ascii_digit() {
            let mut num = String::new();
            num.push(c);
            while pos < len
                && (chars[pos].is_ascii_digit()
                    || chars[pos] == '.'
                    || chars[pos] == 'e'
                    || chars[pos] == 'E')
            {
                if (chars[pos] == 'e' || chars[pos] == 'E')
                    && pos + 1 < len
                    && (chars[pos + 1] == '+' || chars[pos + 1] == '-')
                {
                    num.push(chars[pos]);
                    pos += 1;
                    num.push(chars[pos]);
                    pos += 1;
                } else {
                    num.push(chars[pos]);
                    pos += 1;
                }
            }
            let value: f64 = num
                .parse()
                .map_err(|_| format!("Invalid number '{}' at position {}", num, start))?;
            tokens.push(Token {
                token_type: TokenType::Number(value),
                pos: start,
            });
            continue;
        }

        // Strings
        if c == '"' {
            let mut s = String::new();
            while pos < len && chars[pos] != '"' {
                if chars[pos] == '\\' {
                    pos += 1;
                    if pos >= len {
                        break;
                    }
                    let escaped = chars[pos];
                    pos += 1;
                    match escaped {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '(' => {
                            s.push('\\');
                            s.push('(');
                        }
                        other => s.push(other),
                    }
                } else {
                    s.push(chars[pos]);
                    pos += 1;
                }
            }
            if pos < len {
                pos += 1; // closing quote
            }
            tokens.push(Token {
                token_type: TokenType::Str(s),
                pos: start,
            });
            continue;
        }

        // Identifiers, keywords, $variables, @format-strings
        if is_alpha(c) || c == '$' || c == '@' {
            let mut ident = String::new();
            ident.push(c);
            while pos < len && is_alnum(chars[pos]) {
                ident.push(chars[pos]);
                pos += 1;
            }
            if let Some(kw) = keyword_lookup(&ident) {
                tokens.push(Token {
                    token_type: kw,
                    pos: start,
                });
            } else {
                tokens.push(Token {
                    token_type: TokenType::Ident(ident),
                    pos: start,
                });
            }
            continue;
        }

        return Err(format!("Unexpected character '{}' at position {}", c, start));
    }

    tokens.push(Token {
        token_type: TokenType::Eof,
        pos,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to extract just the token types from a tokenize result
    fn types(input: &str) -> Vec<TokenType> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.token_type)
            .collect()
    }

    #[test]
    fn test_dot() {
        assert_eq!(types("."), vec![TokenType::Dot, TokenType::Eof]);
    }

    #[test]
    fn test_dotdot() {
        assert_eq!(types(".."), vec![TokenType::DotDot, TokenType::Eof]);
    }

    #[test]
    fn test_dot_field() {
        assert_eq!(
            types(".foo"),
            vec![TokenType::Dot, TokenType::Ident("foo".into()), TokenType::Eof]
        );
    }

    #[test]
    fn test_dot_index() {
        assert_eq!(
            types(".[0]"),
            vec![
                TokenType::Dot,
                TokenType::LBracket,
                TokenType::Number(0.0),
                TokenType::RBracket,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_string_hello() {
        assert_eq!(
            types("\"hello\""),
            vec![TokenType::Str("hello".into()), TokenType::Eof]
        );
    }

    #[test]
    fn test_number_integer() {
        assert_eq!(types("123"), vec![TokenType::Number(123.0), TokenType::Eof]);
    }

    #[test]
    fn test_number_decimal() {
        assert_eq!(types("3.14"), vec![TokenType::Number(3.14), TokenType::Eof]);
    }

    #[test]
    fn test_number_scientific() {
        assert_eq!(types("1e-5"), vec![TokenType::Number(1e-5), TokenType::Eof]);
    }

    #[test]
    fn test_keywords_true_false_null() {
        assert_eq!(
            types("true false null"),
            vec![TokenType::True, TokenType::False, TokenType::Null, TokenType::Eof]
        );
    }

    #[test]
    fn test_keywords_if_then_else_elif_end() {
        assert_eq!(
            types("if then else elif end"),
            vec![
                TokenType::If,
                TokenType::Then,
                TokenType::Else,
                TokenType::Elif,
                TokenType::End,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_keywords_and_or_not() {
        assert_eq!(
            types("and or not"),
            vec![TokenType::And, TokenType::Or, TokenType::Not, TokenType::Eof]
        );
    }

    #[test]
    fn test_comparison_ops() {
        assert_eq!(
            types("== != < <= > >="),
            vec![
                TokenType::Eq,
                TokenType::Ne,
                TokenType::Lt,
                TokenType::Le,
                TokenType::Gt,
                TokenType::Ge,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_arithmetic_ops() {
        assert_eq!(
            types("+ - * / %"),
            vec![
                TokenType::Plus,
                TokenType::Minus,
                TokenType::Star,
                TokenType::Slash,
                TokenType::Percent,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_alt_and_update_alt() {
        assert_eq!(
            types("// //="),
            vec![TokenType::Alt, TokenType::UpdateAlt, TokenType::Eof]
        );
    }

    #[test]
    fn test_update_ops() {
        assert_eq!(
            types("|= += -= *= /= %="),
            vec![
                TokenType::UpdatePipe,
                TokenType::UpdateAdd,
                TokenType::UpdateSub,
                TokenType::UpdateMul,
                TokenType::UpdateDiv,
                TokenType::UpdateMod,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_dollar_var() {
        assert_eq!(
            types("$var"),
            vec![TokenType::Ident("$var".into()), TokenType::Eof]
        );
    }

    #[test]
    fn test_at_format() {
        assert_eq!(
            types("@base64"),
            vec![TokenType::Ident("@base64".into()), TokenType::Eof]
        );
    }

    #[test]
    fn test_comment() {
        assert_eq!(
            types("# comment\n."),
            vec![TokenType::Dot, TokenType::Eof]
        );
    }

    #[test]
    fn test_string_escapes() {
        let tokens = tokenize("\"a\\nb\\tc\"").unwrap();
        match &tokens[0].token_type {
            TokenType::Str(s) => assert_eq!(s, "a\nb\tc"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_string_interpolation_escape() {
        let tokens = tokenize("\"hello \\(name)\"").unwrap();
        match &tokens[0].token_type {
            TokenType::Str(s) => assert_eq!(s, "hello \\(name)"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_assign_and_update_pipe() {
        assert_eq!(
            types("= |="),
            vec![TokenType::Assign, TokenType::UpdatePipe, TokenType::Eof]
        );
    }

    #[test]
    fn test_complex_expression() {
        assert_eq!(
            types(".foo | .bar[0]"),
            vec![
                TokenType::Dot,
                TokenType::Ident("foo".into()),
                TokenType::Pipe,
                TokenType::Dot,
                TokenType::Ident("bar".into()),
                TokenType::LBracket,
                TokenType::Number(0.0),
                TokenType::RBracket,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(types(""), vec![TokenType::Eof]);
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(types("   \t\n  "), vec![TokenType::Eof]);
    }

    #[test]
    fn test_delimiters() {
        assert_eq!(
            types("()[]{}"),
            vec![
                TokenType::LParen,
                TokenType::RParen,
                TokenType::LBracket,
                TokenType::RBracket,
                TokenType::LBrace,
                TokenType::RBrace,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_question_colon_semicolon_comma() {
        assert_eq!(
            types("? : ; ,"),
            vec![
                TokenType::Question,
                TokenType::Colon,
                TokenType::Semicolon,
                TokenType::Comma,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_keywords_reduce_foreach_def() {
        assert_eq!(
            types("reduce foreach def label break"),
            vec![
                TokenType::Reduce,
                TokenType::Foreach,
                TokenType::Def,
                TokenType::Label,
                TokenType::Break,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_keywords_as_try_catch() {
        assert_eq!(
            types("as try catch"),
            vec![TokenType::As, TokenType::Try, TokenType::Catch, TokenType::Eof]
        );
    }

    #[test]
    fn test_unexpected_character() {
        let result = tokenize("~");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unexpected character"));
    }

    #[test]
    fn test_string_with_escaped_backslash_and_quote() {
        let tokens = tokenize(r#""a\\b\"c""#).unwrap();
        match &tokens[0].token_type {
            TokenType::Str(s) => assert_eq!(s, "a\\b\"c"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_pipe_expression() {
        assert_eq!(
            types(". | length"),
            vec![
                TokenType::Dot,
                TokenType::Pipe,
                TokenType::Ident("length".into()),
                TokenType::Eof,
            ]
        );
    }
}