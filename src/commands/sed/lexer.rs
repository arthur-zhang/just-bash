//! SED Lexer
//!
//! Tokenizes sed scripts into a stream of tokens.
//! Sed has context-sensitive tokenization - the meaning of characters
//! depends heavily on what command is being parsed.

/// Token types for sed script lexer
#[derive(Debug, Clone, PartialEq)]
pub enum SedToken {
    // Addresses
    Number(usize),
    Dollar,                              // $ - last line
    Pattern(String),                     // /regex/
    Step { first: usize, step: usize },  // first~step
    RelativeOffset(usize),               // +N

    // Structure
    LBrace,
    RBrace,
    Semicolon,
    Newline,
    Comma,
    Negation,                            // !

    // Simple commands
    Command(char),                       // p, d, h, H, g, G, x, n, N, P, D, q, Q, z, =, l, F

    // Complex commands
    Substitute { pattern: String, replacement: String, flags: String },
    Transliterate { source: String, dest: String },
    LabelDef(String),                    // :name
    Branch { label: Option<String> },    // b [label]
    BranchOnSubst { label: Option<String> },    // t [label]
    BranchOnNoSubst { label: Option<String> },  // T [label]
    TextCmd { cmd: char, text: String }, // a\, i\, c\ with text
    FileRead(String),                    // r filename
    FileReadLine(String),                // R filename
    FileWrite(String),                   // w filename
    FileWriteLine(String),               // W filename
    Execute(Option<String>),             // e [command]
    Version(Option<String>),             // v [version]

    Eof,
    Error(String),
}

/// Lexer state for tokenizing sed scripts
struct SedLexer {
    input: Vec<char>,
    pos: usize,
}

impl SedLexer {
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Read an escaped string until the delimiter is reached.
    /// Handles escape sequences: \n -> newline, \t -> tab, \X -> X
    /// Returns None if newline is encountered before delimiter.
    fn read_escaped_string(&mut self, delimiter: char) -> Option<String> {
        let mut result = String::new();
        while !self.is_at_end() && self.peek(0) != Some(delimiter) {
            if self.peek(0) == Some('\\') {
                self.advance();
                if let Some(escaped) = self.advance() {
                    match escaped {
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        other => result.push(other),
                    }
                }
            } else if self.peek(0) == Some('\n') {
                return None; // Unterminated - newline before delimiter
            } else {
                if let Some(ch) = self.advance() {
                    result.push(ch);
                }
            }
        }
        Some(result)
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek(0) {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.advance();
                }
                Some('#') => {
                    // Comment - skip to end of line
                    while !self.is_at_end() && self.peek(0) != Some('\n') {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn is_digit(ch: char) -> bool {
        ch.is_ascii_digit()
    }

    fn next_token(&mut self) -> Option<SedToken> {
        self.skip_whitespace();

        if self.is_at_end() {
            return None;
        }

        let ch = self.peek(0)?;

        // Newline
        if ch == '\n' {
            self.advance();
            return Some(SedToken::Newline);
        }

        // Semicolon
        if ch == ';' {
            self.advance();
            return Some(SedToken::Semicolon);
        }

        // Braces
        if ch == '{' {
            self.advance();
            return Some(SedToken::LBrace);
        }
        if ch == '}' {
            self.advance();
            return Some(SedToken::RBrace);
        }

        // Comma (address range separator)
        if ch == ',' {
            self.advance();
            return Some(SedToken::Comma);
        }

        // Negation modifier (!)
        if ch == '!' {
            self.advance();
            return Some(SedToken::Negation);
        }

        // Dollar (last line address)
        if ch == '$' {
            self.advance();
            return Some(SedToken::Dollar);
        }

        // Number or step address (first~step)
        if Self::is_digit(ch) {
            return Some(self.read_number());
        }

        // Relative offset address +N (GNU extension for ,+N ranges)
        if ch == '+' && self.peek(1).map_or(false, Self::is_digit) {
            return Some(self.read_relative_offset());
        }

        // Pattern address /regex/
        if ch == '/' {
            return Some(self.read_pattern());
        }

        // Label definition :name
        if ch == ':' {
            return Some(self.read_label_def());
        }

        // Commands
        Some(self.read_command())
    }

    fn read_number(&mut self) -> SedToken {
        let mut num_str = String::new();

        while let Some(ch) = self.peek(0) {
            if Self::is_digit(ch) {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for step address: first~step
        if self.peek(0) == Some('~') {
            self.advance(); // skip ~
            let mut step_str = String::new();
            while let Some(ch) = self.peek(0) {
                if Self::is_digit(ch) {
                    step_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
            let first = num_str.parse::<usize>().unwrap_or(0);
            let step = step_str.parse::<usize>().unwrap_or(0);
            return SedToken::Step { first, step };
        }

        SedToken::Number(num_str.parse::<usize>().unwrap_or(0))
    }

    fn read_relative_offset(&mut self) -> SedToken {
        self.advance(); // skip +
        let mut num_str = String::new();

        while let Some(ch) = self.peek(0) {
            if Self::is_digit(ch) {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let offset = num_str.parse::<usize>().unwrap_or(0);
        SedToken::RelativeOffset(offset)
    }

    fn read_pattern(&mut self) -> SedToken {
        self.advance(); // skip opening /
        let mut pattern = String::new();
        let mut in_bracket = false;

        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();

            // Check for end of pattern (delimiter outside brackets)
            if ch == '/' && !in_bracket {
                break;
            }

            if ch == '\\' {
                pattern.push(self.advance().unwrap());
                if !self.is_at_end() && self.peek(0) != Some('\n') {
                    pattern.push(self.advance().unwrap());
                }
            } else if ch == '\n' {
                // Unterminated pattern
                break;
            } else if ch == '[' && !in_bracket {
                in_bracket = true;
                pattern.push(self.advance().unwrap());
                // Handle negation and literal ] at start of bracket
                if self.peek(0) == Some('^') {
                    pattern.push(self.advance().unwrap());
                }
                if self.peek(0) == Some(']') {
                    pattern.push(self.advance().unwrap()); // ] at start is literal
                }
            } else if ch == ']' && in_bracket {
                in_bracket = false;
                pattern.push(self.advance().unwrap());
            } else {
                pattern.push(self.advance().unwrap());
            }
        }

        if self.peek(0) == Some('/') {
            self.advance(); // skip closing /
        }

        SedToken::Pattern(pattern)
    }

    fn read_label_def(&mut self) -> SedToken {
        self.advance(); // skip :

        // Skip optional whitespace after colon (GNU sed allows ': label')
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Read label name (until whitespace, semicolon, newline, or brace)
        let mut label = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, ' ' | '\t' | '\n' | ';' | '}' | '{') {
                break;
            }
            label.push(self.advance().unwrap());
        }

        SedToken::LabelDef(label)
    }

    fn read_command(&mut self) -> SedToken {
        let ch = self.advance().unwrap();

        match ch {
            's' => self.read_substitute(),
            'y' => self.read_transliterate(),
            'a' | 'i' | 'c' => self.read_text_command(ch),
            'b' => self.read_branch_command(|label| SedToken::Branch { label }),
            't' => self.read_branch_command(|label| SedToken::BranchOnSubst { label }),
            'T' => self.read_branch_command(|label| SedToken::BranchOnNoSubst { label }),
            'r' => self.read_file_command(SedToken::FileRead),
            'R' => self.read_file_command(SedToken::FileReadLine),
            'w' => self.read_file_command(SedToken::FileWrite),
            'W' => self.read_file_command(SedToken::FileWriteLine),
            'e' => self.read_execute(),
            'v' => self.read_version(),
            'p' | 'P' | 'd' | 'D' | 'h' | 'H' | 'g' | 'G' | 'x' | 'n' | 'N' | 'q' | 'Q' | 'z'
            | '=' | 'l' | 'F' => SedToken::Command(ch),
            _ => SedToken::Error(format!("unknown command: {}", ch)),
        }
    }

    fn read_substitute(&mut self) -> SedToken {
        // Already consumed 's'
        // Read delimiter
        let delimiter = match self.advance() {
            Some(d) if d != '\n' => d,
            _ => return SedToken::Error("missing delimiter for s command".to_string()),
        };

        // Read pattern (handle bracket expressions where delimiter is literal)
        let mut pattern = String::new();
        let mut in_bracket = false;
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();

            // Check for end of pattern (delimiter outside brackets)
            if ch == delimiter && !in_bracket {
                break;
            }

            if ch == '\\' {
                self.advance(); // consume backslash
                if !self.is_at_end() && self.peek(0) != Some('\n') {
                    let escaped = self.peek(0).unwrap();
                    // Only convert escaped delimiter to literal outside of bracket expressions
                    // Inside brackets, keep the backslash for BRE escape sequences
                    if escaped == delimiter && !in_bracket {
                        // Escaped delimiter becomes literal delimiter in pattern
                        pattern.push(self.advance().unwrap());
                    } else {
                        // Keep backslash + escaped char for other escapes
                        pattern.push('\\');
                        pattern.push(self.advance().unwrap());
                    }
                } else {
                    pattern.push('\\');
                }
            } else if ch == '\n' {
                break;
            } else if ch == '[' && !in_bracket {
                in_bracket = true;
                pattern.push(self.advance().unwrap());
                // Handle negation and literal ] at start of bracket
                if self.peek(0) == Some('^') {
                    pattern.push(self.advance().unwrap());
                }
                if self.peek(0) == Some(']') {
                    pattern.push(self.advance().unwrap()); // ] at start is literal
                }
            } else if ch == ']' && in_bracket {
                in_bracket = false;
                pattern.push(self.advance().unwrap());
            } else {
                pattern.push(self.advance().unwrap());
            }
        }

        if self.peek(0) != Some(delimiter) {
            return SedToken::Error("unterminated substitution pattern".to_string());
        }
        self.advance(); // skip middle delimiter

        // Read replacement
        let mut replacement = String::new();
        while !self.is_at_end() && self.peek(0) != Some(delimiter) {
            if self.peek(0) == Some('\\') {
                self.advance(); // consume first backslash
                if !self.is_at_end() {
                    let next = self.peek(0).unwrap();
                    if next == '\\' {
                        // Double backslash - check what follows
                        self.advance(); // consume second backslash
                        if !self.is_at_end() && self.peek(0) == Some('\n') {
                            // \\<newline> = escaped newline (literal newline in output)
                            replacement.push('\n');
                            self.advance();
                        } else {
                            // \\\\ = literal backslash
                            replacement.push('\\');
                        }
                    } else if next == '\n' {
                        // \<newline> in replacement: include the newline as literal
                        replacement.push('\n');
                        self.advance();
                    } else {
                        // Keep the backslash and following character
                        replacement.push('\\');
                        replacement.push(self.advance().unwrap());
                    }
                } else {
                    replacement.push('\\');
                }
            } else if self.peek(0) == Some('\n') {
                break;
            } else {
                replacement.push(self.advance().unwrap());
            }
        }

        // Closing delimiter is optional for last part
        if self.peek(0) == Some(delimiter) {
            self.advance();
        }

        // Read flags
        let mut flags = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, 'g' | 'i' | 'p' | 'I') || Self::is_digit(ch) {
                flags.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        SedToken::Substitute {
            pattern,
            replacement,
            flags,
        }
    }

    fn read_transliterate(&mut self) -> SedToken {
        // Already consumed 'y'
        let delimiter = match self.advance() {
            Some(d) if d != '\n' => d,
            _ => return SedToken::Error("missing delimiter for y command".to_string()),
        };

        // Read source characters
        let source = match self.read_escaped_string(delimiter) {
            Some(s) => s,
            None => return SedToken::Error("unterminated transliteration source".to_string()),
        };
        if self.peek(0) != Some(delimiter) {
            return SedToken::Error("unterminated transliteration source".to_string());
        }
        self.advance(); // skip middle delimiter

        // Read dest characters
        let dest = match self.read_escaped_string(delimiter) {
            Some(d) => d,
            None => return SedToken::Error("unterminated transliteration dest".to_string()),
        };
        if self.peek(0) != Some(delimiter) {
            return SedToken::Error("unterminated transliteration dest".to_string());
        }
        self.advance(); // skip closing delimiter

        // Check for extra text after y command - only ; } newline or EOF allowed
        // Whitespace followed by more text is an error
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }
        // After y command, only command separators or EOF allowed
        if let Some(next_char) = self.peek(0) {
            if !matches!(next_char, ';' | '\n' | '}') {
                return SedToken::Error(
                    "extra text at the end of a transform command".to_string(),
                );
            }
        }

        SedToken::Transliterate { source, dest }
    }

    fn read_text_command(&mut self, cmd: char) -> SedToken {
        // a, i, c commands can be followed by:
        // 1. a\ followed by newline then text (traditional)
        // 2. a text (GNU extension one-liner, text after space)
        // 3. a\text (backslash followed by text on same line)

        let mut has_backslash = false;
        // Traditional a\ syntax: only consume backslash if followed by newline or space
        if self.peek(0) == Some('\\') {
            if let Some(next) = self.peek(1) {
                if matches!(next, '\n' | ' ' | '\t') {
                    has_backslash = true;
                    self.advance();
                }
            }
        }

        // Skip optional space after command or backslash
        if matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Check for \ at start of text to preserve leading spaces (GNU extension)
        // e.g., "a \   text" preserves "   text"
        // Only consume backslash if followed by space, otherwise it's an escape sequence
        if self.peek(0) == Some('\\') {
            if let Some(next) = self.peek(1) {
                if matches!(next, ' ' | '\t') {
                    self.advance();
                }
            }
        }

        // If we have backslash followed by newline, text is on next line(s)
        if has_backslash && self.peek(0) == Some('\n') {
            self.advance(); // consume newline
        }

        // Read text, handling multi-line continuation and escape sequences
        let mut text = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();

            if ch == '\n' {
                // Check if previous char was backslash for continuation
                if text.ends_with('\\') {
                    // Continuation: remove backslash and add newline
                    text.pop();
                    text.push('\n');
                    self.advance();
                    continue;
                }
                // End of text
                break;
            }

            // Handle escape sequences in text commands (\n, \t, \r)
            if ch == '\\' {
                if let Some(next) = self.peek(1) {
                    match next {
                        'n' => {
                            text.push('\n');
                            self.advance();
                            self.advance();
                            continue;
                        }
                        't' => {
                            text.push('\t');
                            self.advance();
                            self.advance();
                            continue;
                        }
                        'r' => {
                            text.push('\r');
                            self.advance();
                            self.advance();
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            text.push(self.advance().unwrap());
        }

        // Don't trim text - escape sequences like \t at the start are intentional
        SedToken::TextCmd { cmd, text }
    }

    fn read_branch_command<F>(&mut self, make_token: F) -> SedToken
    where
        F: FnOnce(Option<String>) -> SedToken,
    {
        // Skip whitespace
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Read optional label
        let mut label = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, ' ' | '\t' | '\n' | ';' | '}' | '{') {
                break;
            }
            label.push(self.advance().unwrap());
        }

        make_token(if label.is_empty() { None } else { Some(label) })
    }

    fn read_file_command<F>(&mut self, make_token: F) -> SedToken
    where
        F: FnOnce(String) -> SedToken,
    {
        // Skip whitespace (but not newline)
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Read filename until newline or semicolon
        let mut filename = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, '\n' | ';') {
                break;
            }
            filename.push(self.advance().unwrap());
        }

        make_token(filename.trim().to_string())
    }

    fn read_execute(&mut self) -> SedToken {
        // Skip whitespace
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Read optional command until newline or semicolon
        let mut command = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, '\n' | ';') {
                break;
            }
            command.push(self.advance().unwrap());
        }

        let trimmed = command.trim().to_string();
        SedToken::Execute(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        })
    }

    fn read_version(&mut self) -> SedToken {
        // Skip whitespace
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.advance();
        }

        // Read optional version string (e.g., "4.5.3")
        let mut version = String::new();
        while !self.is_at_end() {
            let ch = self.peek(0).unwrap();
            if matches!(ch, ' ' | '\t' | '\n' | ';' | '}' | '{') {
                break;
            }
            version.push(self.advance().unwrap());
        }

        SedToken::Version(if version.is_empty() {
            None
        } else {
            Some(version)
        })
    }
}

/// Tokenize a sed script into a vector of tokens
pub fn tokenize(input: &str) -> Vec<SedToken> {
    let mut lexer = SedLexer::new(input);
    let mut tokens = Vec::new();

    while let Some(token) = lexer.next_token() {
        tokens.push(token);
    }

    tokens.push(SedToken::Eof);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_substitute() {
        let tokens = tokenize("s/foo/bar/g");
        assert!(matches!(&tokens[0], SedToken::Substitute { pattern, replacement, flags }
            if pattern == "foo" && replacement == "bar" && flags == "g"));
    }

    #[test]
    fn test_tokenize_custom_delimiter() {
        let tokens = tokenize("s#foo#bar#");
        assert!(matches!(&tokens[0], SedToken::Substitute { pattern, .. } if pattern == "foo"));
    }

    #[test]
    fn test_tokenize_address_range() {
        let tokens = tokenize("1,3d");
        assert!(matches!(&tokens[0], SedToken::Number(1)));
        assert!(matches!(&tokens[1], SedToken::Comma));
        assert!(matches!(&tokens[2], SedToken::Number(3)));
        assert!(matches!(&tokens[3], SedToken::Command('d')));
    }

    #[test]
    fn test_tokenize_pattern_address() {
        let tokens = tokenize("/foo/d");
        assert!(matches!(&tokens[0], SedToken::Pattern(p) if p == "foo"));
        assert!(matches!(&tokens[1], SedToken::Command('d')));
    }

    #[test]
    fn test_tokenize_step_address() {
        let tokens = tokenize("0~2p");
        assert!(matches!(&tokens[0], SedToken::Step { first: 0, step: 2 }));
    }

    #[test]
    fn test_tokenize_relative_offset() {
        let tokens = tokenize("+3");
        assert!(matches!(&tokens[0], SedToken::RelativeOffset(3)));
    }

    #[test]
    fn test_tokenize_text_command() {
        let tokens = tokenize("a\\ text");
        assert!(matches!(&tokens[0], SedToken::TextCmd { cmd: 'a', text } if text == "text"));
    }

    #[test]
    fn test_tokenize_branch() {
        let tokens = tokenize("b loop");
        assert!(matches!(&tokens[0], SedToken::Branch { label: Some(l) } if l == "loop"));
    }

    #[test]
    fn test_tokenize_branch_no_label() {
        let tokens = tokenize("b");
        assert!(matches!(&tokens[0], SedToken::Branch { label: None }));
    }

    #[test]
    fn test_tokenize_label() {
        let tokens = tokenize(":loop");
        assert!(matches!(&tokens[0], SedToken::LabelDef(l) if l == "loop"));
    }

    #[test]
    fn test_tokenize_transliterate() {
        let tokens = tokenize("y/abc/xyz/");
        assert!(matches!(&tokens[0], SedToken::Transliterate { source, dest }
            if source == "abc" && dest == "xyz"));
    }

    #[test]
    fn test_tokenize_grouped() {
        let tokens = tokenize("{ p; d }");
        assert!(matches!(&tokens[0], SedToken::LBrace));
        assert!(matches!(&tokens[1], SedToken::Command('p')));
        assert!(matches!(&tokens[2], SedToken::Semicolon));
        assert!(matches!(&tokens[3], SedToken::Command('d')));
        assert!(matches!(&tokens[4], SedToken::RBrace));
    }

    #[test]
    fn test_tokenize_negation() {
        let tokens = tokenize("2!d");
        assert!(matches!(&tokens[0], SedToken::Number(2)));
        assert!(matches!(&tokens[1], SedToken::Negation));
        assert!(matches!(&tokens[2], SedToken::Command('d')));
    }

    #[test]
    fn test_tokenize_dollar_address() {
        let tokens = tokenize("$d");
        assert!(matches!(&tokens[0], SedToken::Dollar));
        assert!(matches!(&tokens[1], SedToken::Command('d')));
    }
}
