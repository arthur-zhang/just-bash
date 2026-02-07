//! SED Parser
//!
//! Parses tokenized sed scripts into executable commands.
//! Handles address ranges, command parsing, grouped commands,
//! and label validation.

use super::lexer::{tokenize, SedToken};
use super::types::{AddressRange, SedAddress, SedCmd};

/// Result of parsing sed scripts
pub struct ParseResult {
    pub commands: Vec<SedCmd>,
    pub error: Option<String>,
    pub silent_mode: bool,
    pub extended_regex_mode: bool,
}

/// Parser state for sed scripts
struct SedParser {
    tokens: Vec<SedToken>,
    pos: usize,
    extended_regex: bool,
}

impl SedParser {
    fn new(tokens: Vec<SedToken>, extended_regex: bool) -> Self {
        Self {
            tokens,
            pos: 0,
            extended_regex,
        }
    }

    fn peek(&self) -> &SedToken {
        self.tokens.get(self.pos).unwrap_or(&SedToken::Eof)
    }

    fn advance(&mut self) -> &SedToken {
        let token = self.tokens.get(self.pos).unwrap_or(&SedToken::Eof);
        if !matches!(token, SedToken::Eof) {
            self.pos += 1;
        }
        self.tokens.get(self.pos - 1).unwrap_or(&SedToken::Eof)
    }

    fn check(&self, expected: &SedToken) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(expected)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek(), SedToken::Eof)
    }

    fn parse(&mut self) -> Result<Vec<SedCmd>, String> {
        let mut commands = Vec::new();

        while !self.is_at_end() {
            // Skip empty tokens (newlines, semicolons)
            if matches!(self.peek(), SedToken::Newline | SedToken::Semicolon) {
                self.advance();
                continue;
            }

            let cmd = self.parse_command()?;
            if let Some(c) = cmd {
                commands.push(c);
            }
        }

        Ok(commands)
    }

    fn parse_command(&mut self) -> Result<Option<SedCmd>, String> {
        // Parse optional address range
        let address_result = self.parse_address_range()?;
        let mut address = address_result;

        // Check for negation modifier (!)
        if matches!(self.peek(), SedToken::Negation) {
            self.advance();
            if let Some(ref mut addr) = address {
                addr.negated = true;
            }
        }

        // Skip whitespace tokens
        while matches!(self.peek(), SedToken::Newline | SedToken::Semicolon) {
            self.advance();
        }

        if self.is_at_end() {
            // Address with no command is an error
            if address.is_some() {
                return Err("command expected".to_string());
            }
            return Ok(None);
        }

        let token = self.peek().clone();

        match token {
            SedToken::Command(c) => {
                self.advance();
                self.parse_simple_command(c, address)
            }
            SedToken::Substitute {
                pattern,
                replacement,
                flags,
            } => {
                self.advance();
                self.parse_substitute(pattern, replacement, flags, address)
            }
            SedToken::Transliterate { source, dest } => {
                self.advance();
                self.parse_transliterate(source, dest, address)
            }
            SedToken::LabelDef(name) => {
                self.advance();
                Ok(Some(SedCmd::Label { name }))
            }
            SedToken::Branch { label } => {
                self.advance();
                Ok(Some(SedCmd::Branch { address, label }))
            }
            SedToken::BranchOnSubst { label } => {
                self.advance();
                Ok(Some(SedCmd::BranchOnSubst { address, label }))
            }
            SedToken::BranchOnNoSubst { label } => {
                self.advance();
                Ok(Some(SedCmd::BranchOnNoSubst { address, label }))
            }
            SedToken::TextCmd { cmd, text } => {
                self.advance();
                self.parse_text_command(cmd, text, address)
            }
            SedToken::FileRead(filename) => {
                self.advance();
                Ok(Some(SedCmd::ReadFile { address, filename }))
            }
            SedToken::FileReadLine(filename) => {
                self.advance();
                Ok(Some(SedCmd::ReadFileLine { address, filename }))
            }
            SedToken::FileWrite(filename) => {
                self.advance();
                Ok(Some(SedCmd::WriteFile { address, filename }))
            }
            SedToken::FileWriteLine(filename) => {
                self.advance();
                Ok(Some(SedCmd::WriteFirstLine { address, filename }))
            }
            SedToken::Execute(command) => {
                self.advance();
                // Execute command not yet implemented in types, skip for now
                // Return a placeholder or error
                Err(format!("execute command not implemented: {:?}", command))
            }
            SedToken::Version(min_version) => {
                self.advance();
                Ok(Some(SedCmd::Version {
                    address,
                    min_version,
                }))
            }
            SedToken::LBrace => self.parse_group(address),
            SedToken::RBrace => {
                // End of group - handled by parse_group
                Ok(None)
            }
            SedToken::Error(msg) => Err(format!("invalid command: {}", msg)),
            _ => {
                // Address with no recognized command is an error
                if address.is_some() {
                    return Err("command expected".to_string());
                }
                Ok(None)
            }
        }
    }

    fn parse_simple_command(
        &mut self,
        cmd: char,
        address: Option<AddressRange>,
    ) -> Result<Option<SedCmd>, String> {
        match cmd {
            'p' => Ok(Some(SedCmd::Print { address })),
            'P' => Ok(Some(SedCmd::PrintFirstLine { address })),
            'd' => Ok(Some(SedCmd::Delete { address })),
            'D' => Ok(Some(SedCmd::DeleteFirstLine { address })),
            'h' => Ok(Some(SedCmd::Hold { address })),
            'H' => Ok(Some(SedCmd::HoldAppend { address })),
            'g' => Ok(Some(SedCmd::Get { address })),
            'G' => Ok(Some(SedCmd::GetAppend { address })),
            'x' => Ok(Some(SedCmd::Exchange { address })),
            'n' => Ok(Some(SedCmd::Next { address })),
            'N' => Ok(Some(SedCmd::NextAppend { address })),
            'q' => Ok(Some(SedCmd::Quit { address })),
            'Q' => Ok(Some(SedCmd::QuitSilent { address })),
            'z' => Ok(Some(SedCmd::Zap { address })),
            '=' => Ok(Some(SedCmd::LineNumber { address })),
            'l' => Ok(Some(SedCmd::List { address })),
            'F' => Ok(Some(SedCmd::PrintFilename { address })),
            _ => Err(format!("unknown command: {}", cmd)),
        }
    }

    fn parse_substitute(
        &mut self,
        pattern: String,
        replacement: String,
        flags: String,
        address: Option<AddressRange>,
    ) -> Result<Option<SedCmd>, String> {
        let global = flags.contains('g');
        let ignore_case = flags.contains('i') || flags.contains('I');
        let print_on_match = flags.contains('p');

        // Parse numeric flag for nth occurrence
        let nth_occurrence = flags
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<usize>()
            .ok();

        Ok(Some(SedCmd::Substitute {
            address,
            pattern,
            replacement,
            global,
            ignore_case,
            print_on_match,
            nth_occurrence,
            extended_regex: self.extended_regex,
        }))
    }

    fn parse_transliterate(
        &mut self,
        source: String,
        dest: String,
        address: Option<AddressRange>,
    ) -> Result<Option<SedCmd>, String> {
        if source.len() != dest.len() {
            return Err("transliteration sets must have same length".to_string());
        }

        Ok(Some(SedCmd::Transliterate {
            address,
            source,
            dest,
        }))
    }

    fn parse_text_command(
        &mut self,
        cmd: char,
        text: String,
        address: Option<AddressRange>,
    ) -> Result<Option<SedCmd>, String> {
        match cmd {
            'a' => Ok(Some(SedCmd::Append { address, text })),
            'i' => Ok(Some(SedCmd::Insert { address, text })),
            'c' => Ok(Some(SedCmd::Change { address, text })),
            _ => Err(format!("unknown text command: {}", cmd)),
        }
    }

    fn parse_group(&mut self, address: Option<AddressRange>) -> Result<Option<SedCmd>, String> {
        self.advance(); // consume {

        let mut commands = Vec::new();

        while !self.is_at_end() && !matches!(self.peek(), SedToken::RBrace) {
            // Skip empty tokens
            if matches!(self.peek(), SedToken::Newline | SedToken::Semicolon) {
                self.advance();
                continue;
            }

            let cmd = self.parse_command()?;
            if let Some(c) = cmd {
                commands.push(c);
            }
        }

        if !matches!(self.peek(), SedToken::RBrace) {
            return Err("unmatched brace in grouped commands".to_string());
        }
        self.advance(); // consume }

        Ok(Some(SedCmd::Group { address, commands }))
    }

    fn parse_address_range(&mut self) -> Result<Option<AddressRange>, String> {
        // Try to parse first address
        let start = self.parse_address();
        if start.is_none() {
            return Ok(None);
        }

        // Check for range separator or relative offset (GNU extension: ,+N)
        let end = if matches!(self.peek(), SedToken::RelativeOffset(_)) {
            // GNU extension: /pattern/,+N means "match N more lines after pattern"
            if let SedToken::RelativeOffset(n) = self.peek().clone() {
                self.advance();
                Some(SedAddress::RelativeOffset(n))
            } else {
                None
            }
        } else if matches!(self.peek(), SedToken::Comma) {
            self.advance();
            let end_addr = self.parse_address();
            // If we consumed a comma but have no end address, that's an error
            if end_addr.is_none() {
                return Err("expected context address".to_string());
            }
            end_addr
        } else {
            None
        };

        Ok(Some(AddressRange {
            start,
            end,
            negated: false,
        }))
    }

    fn parse_address(&mut self) -> Option<SedAddress> {
        match self.peek().clone() {
            SedToken::Number(n) => {
                self.advance();
                Some(SedAddress::Line(n))
            }
            SedToken::Dollar => {
                self.advance();
                Some(SedAddress::Last)
            }
            SedToken::Pattern(p) => {
                self.advance();
                Some(SedAddress::Pattern(p))
            }
            SedToken::Step { first, step } => {
                self.advance();
                Some(SedAddress::Step { first, step })
            }
            SedToken::RelativeOffset(n) => {
                self.advance();
                Some(SedAddress::RelativeOffset(n))
            }
            _ => None,
        }
    }
}

/// Collect all defined labels from commands (including nested groups)
fn collect_labels(commands: &[SedCmd], labels: &mut std::collections::HashSet<String>) {
    for cmd in commands {
        if let SedCmd::Label { name } = cmd {
            labels.insert(name.clone());
        } else if let SedCmd::Group { commands, .. } = cmd {
            collect_labels(commands, labels);
        }
    }
}

/// Find first undefined label in branch commands
fn find_undefined_label(
    commands: &[SedCmd],
    defined_labels: &std::collections::HashSet<String>,
) -> Option<String> {
    for cmd in commands {
        match cmd {
            SedCmd::Branch {
                label: Some(l), ..
            }
            | SedCmd::BranchOnSubst {
                label: Some(l), ..
            }
            | SedCmd::BranchOnNoSubst {
                label: Some(l), ..
            } => {
                if !defined_labels.contains(l) {
                    return Some(l.clone());
                }
            }
            SedCmd::Group { commands, .. } => {
                if let Some(label) = find_undefined_label(commands, defined_labels) {
                    return Some(label);
                }
            }
            _ => {}
        }
    }
    None
}

/// Validate that all branch targets reference existing labels
fn validate_labels(commands: &[SedCmd]) -> Option<String> {
    let mut defined_labels = std::collections::HashSet::new();
    collect_labels(commands, &mut defined_labels);

    if let Some(undefined) = find_undefined_label(commands, &defined_labels) {
        return Some(format!("undefined label '{}'", undefined));
    }

    None
}

/// Parse multiple sed scripts into a list of commands.
///
/// This is the main entry point for parsing sed scripts.
///
/// Also detects #n or #r special comments at the start of the first script:
/// - #n enables silent mode (equivalent to -n flag)
/// - #r enables extended regex mode (equivalent to -r/-E flag)
///
/// Handles backslash continuation across -e arguments:
/// - If a script ends with \, the next script is treated as continuation
pub fn parse_scripts(scripts: &[&str], extended_regex: bool) -> ParseResult {
    // Check for #n or #r special comments at the start of the first script
    let mut silent_mode = false;
    let mut extended_regex_from_comment = false;

    // First, join scripts that have backslash continuation
    // e.g., -e 'a\' -e 'text' becomes 'a\ntext'
    let mut joined_scripts: Vec<String> = Vec::new();

    for (i, script) in scripts.iter().enumerate() {
        let mut script_str = script.to_string();

        // Handle #n/#r comments in first script
        if joined_scripts.is_empty() && i == 0 {
            if let Some(rest) = script_str.strip_prefix("#n") {
                silent_mode = true;
                // Check if it's #nr or just #n
                if let Some(after_r) = rest.strip_prefix('r') {
                    extended_regex_from_comment = true;
                    script_str = after_r.trim_start().to_string();
                    // Skip leading newline if present
                    if script_str.starts_with('\n') {
                        script_str = script_str[1..].to_string();
                    }
                } else {
                    script_str = rest.trim_start().to_string();
                    if script_str.starts_with('\n') {
                        script_str = script_str[1..].to_string();
                    }
                }
            } else if let Some(rest) = script_str.strip_prefix("#r") {
                extended_regex_from_comment = true;
                // Check if it's #rn
                if let Some(after_n) = rest.strip_prefix('n') {
                    silent_mode = true;
                    script_str = after_n.trim_start().to_string();
                    if script_str.starts_with('\n') {
                        script_str = script_str[1..].to_string();
                    }
                } else {
                    script_str = rest.trim_start().to_string();
                    if script_str.starts_with('\n') {
                        script_str = script_str[1..].to_string();
                    }
                }
            }
        }

        // Check if last script ends with backslash (continuation)
        if !joined_scripts.is_empty() && joined_scripts.last().unwrap().ends_with('\\') {
            // Keep trailing backslash and join with newline
            let last_script = joined_scripts.pop().unwrap();
            joined_scripts.push(format!("{}\n{}", last_script, script_str));
        } else {
            joined_scripts.push(script_str);
        }
    }

    // Join all scripts with newlines to form a single script
    let combined_script = joined_scripts.join("\n");

    // Tokenize the combined script
    let tokens = tokenize(&combined_script);

    // Parse tokens into commands
    let mut parser = SedParser::new(tokens, extended_regex || extended_regex_from_comment);
    let result = parser.parse();

    match result {
        Ok(commands) => {
            // Validate that all branch targets exist
            if let Some(label_error) = validate_labels(&commands) {
                return ParseResult {
                    commands: Vec::new(),
                    error: Some(label_error),
                    silent_mode,
                    extended_regex_mode: extended_regex_from_comment,
                };
            }

            ParseResult {
                commands,
                error: None,
                silent_mode,
                extended_regex_mode: extended_regex_from_comment,
            }
        }
        Err(e) => ParseResult {
            commands: Vec::new(),
            error: Some(e),
            silent_mode,
            extended_regex_mode: extended_regex_from_comment,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_substitute() {
        let result = parse_scripts(&["s/foo/bar/"], false);
        assert!(result.error.is_none());
        assert_eq!(result.commands.len(), 1);
        match &result.commands[0] {
            SedCmd::Substitute {
                pattern,
                replacement,
                global,
                ..
            } => {
                assert_eq!(pattern, "foo");
                assert_eq!(replacement, "bar");
                assert!(!global);
            }
            _ => panic!("Expected Substitute"),
        }
    }

    #[test]
    fn test_parse_substitute_global() {
        let result = parse_scripts(&["s/foo/bar/g"], false);
        match &result.commands[0] {
            SedCmd::Substitute { global, .. } => assert!(*global),
            _ => panic!("Expected Substitute"),
        }
    }

    #[test]
    fn test_parse_address_range() {
        let result = parse_scripts(&["1,3d"], false);
        assert!(result.error.is_none());
        match &result.commands[0] {
            SedCmd::Delete {
                address: Some(addr),
            } => {
                assert!(matches!(addr.start, Some(SedAddress::Line(1))));
                assert!(matches!(addr.end, Some(SedAddress::Line(3))));
            }
            _ => panic!("Expected Delete with address range"),
        }
    }

    #[test]
    fn test_parse_pattern_address() {
        let result = parse_scripts(&["/foo/d"], false);
        match &result.commands[0] {
            SedCmd::Delete {
                address: Some(addr),
            } => {
                assert!(matches!(&addr.start, Some(SedAddress::Pattern(p)) if p == "foo"));
            }
            _ => panic!("Expected Delete with pattern address"),
        }
    }

    #[test]
    fn test_parse_negated_address() {
        let result = parse_scripts(&["2!d"], false);
        match &result.commands[0] {
            SedCmd::Delete {
                address: Some(addr),
            } => {
                assert!(addr.negated);
            }
            _ => panic!("Expected Delete with negated address"),
        }
    }

    #[test]
    fn test_parse_group() {
        let result = parse_scripts(&["{ p; d }"], false);
        match &result.commands[0] {
            SedCmd::Group { commands, .. } => {
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("Expected Group"),
        }
    }

    #[test]
    fn test_parse_branch_label() {
        let result = parse_scripts(&[":loop\nb loop"], false);
        assert!(result.error.is_none());
        assert_eq!(result.commands.len(), 2);
    }

    #[test]
    fn test_parse_undefined_label_error() {
        let result = parse_scripts(&["b nonexistent"], false);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("undefined label"));
    }

    #[test]
    fn test_parse_silent_comment() {
        let result = parse_scripts(&["#n\np"], false);
        assert!(result.silent_mode);
    }

    #[test]
    fn test_parse_multiple_scripts() {
        let result = parse_scripts(&["s/a/b/", "s/c/d/"], false);
        assert_eq!(result.commands.len(), 2);
    }

    #[test]
    fn test_parse_text_commands() {
        let result = parse_scripts(&["a\\ hello"], false);
        match &result.commands[0] {
            SedCmd::Append { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("Expected Append"),
        }
    }

    #[test]
    fn test_parse_transliterate() {
        let result = parse_scripts(&["y/abc/xyz/"], false);
        match &result.commands[0] {
            SedCmd::Transliterate { source, dest, .. } => {
                assert_eq!(source, "abc");
                assert_eq!(dest, "xyz");
            }
            _ => panic!("Expected Transliterate"),
        }
    }

    #[test]
    fn test_parse_transliterate_length_error() {
        let result = parse_scripts(&["y/abc/xy/"], false);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("same length"));
    }
}
