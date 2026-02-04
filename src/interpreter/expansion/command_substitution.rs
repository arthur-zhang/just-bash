//! Command Substitution Helpers
//!
//! Helper functions for handling command substitution patterns.

use crate::ast::types::{CommandNode, RedirectionOperator, RedirectionTarget, ScriptNode, WordNode};

/// Check if a command substitution body matches the $(<file) shorthand pattern.
/// This is a special case where $(< file) is equivalent to $(cat file) but reads
/// the file directly without spawning a subprocess.
///
/// For this to match, the body must consist of:
/// - One statement without operators (no && or ||)
/// - One pipeline with one command
/// - A SimpleCommand with no name, no args, no assignments
/// - Exactly one input redirection (<)
///
/// Note: The special $(<file) behavior only works when it's the ONLY element
/// in the command substitution. $(< file; cmd) or $(cmd; < file) are NOT special.
pub fn get_file_read_shorthand(body: &ScriptNode) -> Option<&WordNode> {
    // Must have exactly one statement
    if body.statements.len() != 1 {
        return None;
    }

    let statement = &body.statements[0];
    // Must not have any operators (no && or ||)
    if !statement.operators.is_empty() {
        return None;
    }
    // Must have exactly one pipeline
    if statement.pipelines.len() != 1 {
        return None;
    }

    let pipeline = &statement.pipelines[0];
    // Must not be negated
    if pipeline.negated {
        return None;
    }
    // Must have exactly one command
    if pipeline.commands.len() != 1 {
        return None;
    }

    let cmd = &pipeline.commands[0];
    // Must be a SimpleCommand
    let simple_cmd = match cmd {
        CommandNode::Simple(sc) => sc,
        _ => return None,
    };

    // Must have no command name
    if simple_cmd.name.is_some() {
        return None;
    }
    // Must have no arguments
    if !simple_cmd.args.is_empty() {
        return None;
    }
    // Must have no assignments
    if !simple_cmd.assignments.is_empty() {
        return None;
    }
    // Must have exactly one redirection
    if simple_cmd.redirections.len() != 1 {
        return None;
    }

    let redirect = &simple_cmd.redirections[0];
    // Must be an input redirection (<)
    if redirect.operator != RedirectionOperator::Less {
        return None;
    }
    // Target must be a WordNode (not heredoc)
    match &redirect.target {
        RedirectionTarget::Word(ref word) => Some(word),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn parse_script(input: &str) -> ScriptNode {
        let mut parser = Parser::new();
        parser.parse(input).unwrap()
    }

    #[test]
    fn test_file_read_shorthand() {
        // This would be the body of $(< file)
        let script = parse_script("< file");
        let result = get_file_read_shorthand(&script);
        assert!(result.is_some());
    }

    #[test]
    fn test_not_shorthand_with_command() {
        let script = parse_script("cat file");
        let result = get_file_read_shorthand(&script);
        assert!(result.is_none());
    }

    #[test]
    fn test_not_shorthand_with_multiple_statements() {
        let script = parse_script("< file; echo done");
        let result = get_file_read_shorthand(&script);
        assert!(result.is_none());
    }

    #[test]
    fn test_not_shorthand_with_output_redirect() {
        let script = parse_script("> file");
        let result = get_file_read_shorthand(&script);
        assert!(result.is_none());
    }
}
