//! Alias Expansion
//!
//! Handles bash alias expansion for SimpleCommandNodes.
//!
//! Alias expansion rules:
//! 1. Only expands if command name is a literal unquoted word
//! 2. Alias value is substituted for the command name
//! 3. If alias value ends with a space, the next word is also checked for alias expansion
//! 4. Recursive expansion is allowed but limited to prevent infinite loops

use std::collections::{HashMap, HashSet};
use crate::{
    WordNode, WordPart, LiteralPart, SimpleCommandNode, AssignmentNode,
    RedirectionNode, ScriptNode,
};

/// Alias prefix used in environment variables
pub const ALIAS_PREFIX: &str = "BASH_ALIAS_";

/// Context needed for alias expansion operations
pub struct AliasExpansionContext<'a> {
    pub env: &'a HashMap<String, String>,
}

/// Check if a word is a literal unquoted word (eligible for alias expansion).
/// Aliases only expand for literal words, not for quoted strings or expansions.
pub fn is_literal_unquoted_word(word: &WordNode) -> bool {
    // Must have exactly one part that is a literal
    if word.parts.len() != 1 {
        return false;
    }
    matches!(&word.parts[0], WordPart::Literal(_))
}

/// Get the literal value of a word if it's a simple literal
pub fn get_literal_value(word: &WordNode) -> Option<&str> {
    if word.parts.len() != 1 {
        return None;
    }
    match &word.parts[0] {
        WordPart::Literal(LiteralPart { value }) => Some(value),
        _ => None,
    }
}

/// Get the alias value for a name, if defined
pub fn get_alias<'a>(ctx: &'a AliasExpansionContext<'a>, name: &str) -> Option<&'a str> {
    let key = format!("{}{}", ALIAS_PREFIX, name);
    ctx.env.get(&key).map(|s| s.as_str())
}

/// Check if an alias is defined for a name
pub fn has_alias(ctx: &AliasExpansionContext, name: &str) -> bool {
    let key = format!("{}{}", ALIAS_PREFIX, name);
    ctx.env.contains_key(&key)
}

/// Set an alias in the environment
pub fn set_alias(env: &mut HashMap<String, String>, name: &str, value: &str) {
    let key = format!("{}{}", ALIAS_PREFIX, name);
    env.insert(key, value.to_string());
}

/// Remove an alias from the environment
pub fn unset_alias(env: &mut HashMap<String, String>, name: &str) -> bool {
    let key = format!("{}{}", ALIAS_PREFIX, name);
    env.remove(&key).is_some()
}

/// Get all defined aliases as (name, value) pairs
pub fn get_all_aliases(env: &HashMap<String, String>) -> Vec<(String, String)> {
    env.iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(ALIAS_PREFIX)
                .map(|name| (name.to_string(), v.clone()))
        })
        .collect()
}

/// Convert a WordNode back to a string representation for re-parsing.
/// This is a simplified conversion that handles common cases.
pub fn word_node_to_string(word: &WordNode) -> String {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(LiteralPart { value }) => {
                // Escape special characters
                for c in value.chars() {
                    if matches!(c, ' ' | '\t' | '"' | '\'' | '$' | '`' | '\\' | '*' | '?' |
                                   '[' | ']' | '{' | '}' | '(' | ')' | '<' | '>' | '|' |
                                   '&' | ';' | '#' | '!' | '\n') {
                        result.push('\\');
                    }
                    result.push(c);
                }
            }
            WordPart::SingleQuoted(sq) => {
                result.push('\'');
                result.push_str(&sq.value);
                result.push('\'');
            }
            WordPart::DoubleQuoted(dq) => {
                result.push('"');
                for inner in &dq.parts {
                    if let WordPart::Literal(LiteralPart { value }) = inner {
                        result.push_str(value);
                    }
                }
                result.push('"');
            }
            WordPart::ParameterExpansion(pe) => {
                result.push_str("${");
                result.push_str(&pe.parameter);
                result.push('}');
            }
            WordPart::CommandSubstitution(_) => {
                result.push_str("$(...)");
            }
            WordPart::ArithmeticExpansion(_) => {
                // ArithmeticExpansion contains an AST node, not a string
                // For alias expansion purposes, we use a placeholder
                result.push_str("$((...))");
            }
            WordPart::Glob(g) => {
                result.push_str(&g.pattern);
            }
            _ => {}
        }
    }
    result
}

/// Result of alias expansion
#[derive(Debug, Clone)]
pub enum AliasExpansionResult {
    /// No expansion occurred, return original node
    NoExpansion,
    /// Expansion succeeded, return new node
    Expanded(SimpleCommandNode),
    /// Expansion resulted in a complex command (multiple statements/pipelines)
    /// that needs to be executed as a script
    ComplexAlias(String),
    /// Parse error during expansion
    ParseError(String),
}

/// Expand alias in a SimpleCommandNode if applicable.
/// Returns the expansion result.
pub fn expand_alias(
    ctx: &AliasExpansionContext,
    node: &SimpleCommandNode,
    alias_expansion_stack: &mut HashSet<String>,
) -> AliasExpansionResult {
    // Need a command name to expand
    let name = match &node.name {
        Some(n) => n,
        None => return AliasExpansionResult::NoExpansion,
    };

    // Check if the command name is a literal unquoted word
    if !is_literal_unquoted_word(name) {
        return AliasExpansionResult::NoExpansion;
    }

    let cmd_name = match get_literal_value(name) {
        Some(n) => n,
        None => return AliasExpansionResult::NoExpansion,
    };

    // Check for alias
    let alias_value = match get_alias(ctx, cmd_name) {
        Some(v) => v.to_string(),
        None => return AliasExpansionResult::NoExpansion,
    };

    // Prevent infinite recursion
    if alias_expansion_stack.contains(cmd_name) {
        return AliasExpansionResult::NoExpansion;
    }

    alias_expansion_stack.insert(cmd_name.to_string());

    // Build the full command line: alias value + original args
    let mut full_command = alias_value.clone();

    // Check if alias value ends with a space (triggers expansion of next word)
    let expand_next = alias_value.ends_with(' ');

    // If not expanding next, append args directly
    if !expand_next {
        for arg in &node.args {
            let arg_literal = word_node_to_string(arg);
            full_command.push(' ');
            full_command.push_str(&arg_literal);
        }
    }

    // Parse the expanded command
    let mut parser = crate::parser::Parser::new();
    let expanded_ast = match parser.parse(&full_command) {
        Ok(ast) => ast,
        Err(e) => {
            alias_expansion_stack.remove(cmd_name);
            return AliasExpansionResult::ParseError(e.to_string());
        }
    };

    // Check if we got a simple command
    if expanded_ast.statements.len() != 1
        || expanded_ast.statements[0].pipelines.len() != 1
        || expanded_ast.statements[0].pipelines[0].commands.len() != 1
    {
        // Complex alias - multiple commands, pipelines, etc.
        alias_expansion_stack.remove(cmd_name);
        return AliasExpansionResult::ComplexAlias(full_command);
    }

    let expanded_cmd = &expanded_ast.statements[0].pipelines[0].commands[0];
    match expanded_cmd {
        crate::CommandNode::Simple(simple_cmd) => {
            // Merge the expanded command with original node's context
            let mut new_node = SimpleCommandNode {
                name: simple_cmd.name.clone(),
                args: simple_cmd.args.clone(),
                // Preserve original assignments (prefix assignments like FOO=bar alias_cmd)
                assignments: {
                    let mut assignments = node.assignments.clone();
                    assignments.extend(simple_cmd.assignments.clone());
                    assignments
                },
                // Preserve original redirections
                redirections: {
                    let mut redirections = simple_cmd.redirections.clone();
                    redirections.extend(node.redirections.clone());
                    redirections
                },
                // Preserve line number
                line: node.line,
            };

            // If alias ends with space, expand next word too (recursive alias on first arg)
            if expand_next && !node.args.is_empty() {
                // Add the original args to the expanded command's args
                new_node.args.extend(node.args.clone());
            }

            AliasExpansionResult::Expanded(new_node)
        }
        _ => {
            // Alias expanded to a compound command
            alias_expansion_stack.remove(cmd_name);
            AliasExpansionResult::ComplexAlias(full_command)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn make_literal_word(value: &str) -> WordNode {
        WordNode {
            parts: vec![WordPart::Literal(LiteralPart { value: value.to_string() })],
        }
    }

    #[test]
    fn test_is_literal_unquoted_word() {
        let word = make_literal_word("echo");
        assert!(is_literal_unquoted_word(&word));

        // Multiple parts - not literal
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "a".to_string() }),
                WordPart::Literal(LiteralPart { value: "b".to_string() }),
            ],
        };
        assert!(!is_literal_unquoted_word(&word));
    }

    #[test]
    fn test_get_literal_value() {
        let word = make_literal_word("hello");
        assert_eq!(get_literal_value(&word), Some("hello"));

        let word = WordNode { parts: vec![] };
        assert_eq!(get_literal_value(&word), None);
    }

    #[test]
    fn test_set_get_alias() {
        let mut env = make_env();
        set_alias(&mut env, "ll", "ls -la");

        let ctx = AliasExpansionContext { env: &env };
        assert_eq!(get_alias(&ctx, "ll"), Some("ls -la"));
        assert_eq!(get_alias(&ctx, "nonexistent"), None);
    }

    #[test]
    fn test_unset_alias() {
        let mut env = make_env();
        set_alias(&mut env, "ll", "ls -la");

        assert!(unset_alias(&mut env, "ll"));
        assert!(!unset_alias(&mut env, "ll")); // Already removed

        let ctx = AliasExpansionContext { env: &env };
        assert_eq!(get_alias(&ctx, "ll"), None);
    }

    #[test]
    fn test_get_all_aliases() {
        let mut env = make_env();
        set_alias(&mut env, "ll", "ls -la");
        set_alias(&mut env, "la", "ls -a");

        let aliases = get_all_aliases(&env);
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn test_word_node_to_string_literal() {
        let word = make_literal_word("hello");
        assert_eq!(word_node_to_string(&word), "hello");
    }

    #[test]
    fn test_word_node_to_string_with_spaces() {
        let word = make_literal_word("hello world");
        assert_eq!(word_node_to_string(&word), "hello\\ world");
    }

    #[test]
    fn test_expand_alias_no_alias() {
        let env = make_env();
        let ctx = AliasExpansionContext { env: &env };

        let node = SimpleCommandNode {
            name: Some(make_literal_word("echo")),
            args: vec![make_literal_word("hello")],
            assignments: vec![],
            redirections: vec![],
            line: None,
        };

        let mut stack = HashSet::new();
        let result = expand_alias(&ctx, &node, &mut stack);
        assert!(matches!(result, AliasExpansionResult::NoExpansion));
    }

    #[test]
    fn test_expand_alias_simple() {
        let mut env = make_env();
        set_alias(&mut env, "ll", "ls -la");
        let ctx = AliasExpansionContext { env: &env };

        let node = SimpleCommandNode {
            name: Some(make_literal_word("ll")),
            args: vec![],
            assignments: vec![],
            redirections: vec![],
            line: None,
        };

        let mut stack = HashSet::new();
        let result = expand_alias(&ctx, &node, &mut stack);

        match result {
            AliasExpansionResult::Expanded(expanded) => {
                let cmd_name = expanded.name.as_ref().and_then(|w| get_literal_value(w));
                assert_eq!(cmd_name, Some("ls"));
            }
            _ => panic!("Expected Expanded result"),
        }
    }

    #[test]
    fn test_expand_alias_prevents_recursion() {
        let mut env = make_env();
        // Create a self-referencing alias
        set_alias(&mut env, "foo", "foo bar");
        let ctx = AliasExpansionContext { env: &env };

        let node = SimpleCommandNode {
            name: Some(make_literal_word("foo")),
            args: vec![],
            assignments: vec![],
            redirections: vec![],
            line: None,
        };

        let mut stack = HashSet::new();
        stack.insert("foo".to_string()); // Simulate already expanding foo

        let result = expand_alias(&ctx, &node, &mut stack);
        assert!(matches!(result, AliasExpansionResult::NoExpansion));
    }
}
