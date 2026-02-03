//! Conditional Expression Parser
//!
//! Handles parsing of [[ ... ]] conditional commands.

use crate::ast::types::{
    CondAndNode, CondBinaryNode, CondBinaryOperator, CondGroupNode, CondNotNode,
    CondOrNode, CondUnaryNode, CondUnaryOperator, CondWordNode, ConditionalExpressionNode,
    LiteralPart, WordNode, WordPart,
};
use crate::parser::lexer::TokenType;

/// Unary operators for conditional expressions
pub const UNARY_OPS: &[&str] = &[
    "-a", "-b", "-c", "-d", "-e", "-f", "-g", "-h", "-k", "-p", "-r", "-s", "-t", "-u", "-w", "-x",
    "-G", "-L", "-N", "-O", "-S", "-z", "-n", "-o", "-v", "-R",
];

/// Binary operators for conditional expressions
pub const BINARY_OPS: &[&str] = &[
    "==", "!=", "=~", "<", ">", "-eq", "-ne", "-lt", "-le", "-gt", "-ge", "-nt", "-ot", "-ef",
];

/// Token information for conditional parsing
#[derive(Debug, Clone)]
pub struct CondToken {
    pub token_type: TokenType,
    pub value: String,
    pub quoted: bool,
    pub start: usize,
    pub end: usize,
}

/// Context for conditional expression parsing
pub struct CondParserContext<'a> {
    pub is_word: &'a dyn Fn() -> bool,
    pub check: &'a dyn Fn(TokenType) -> bool,
    pub peek: &'a dyn Fn(isize) -> CondToken,
    pub current: &'a dyn Fn() -> CondToken,
    pub advance: &'a dyn Fn() -> CondToken,
    pub expect: &'a dyn Fn(TokenType),
    pub skip_newlines: &'a dyn Fn(),
    pub parse_word_no_brace_expansion: &'a dyn Fn() -> WordNode,
    pub parse_word_for_regex: &'a dyn Fn() -> WordNode,
    pub parse_word_from_string:
        &'a dyn Fn(&str, bool, bool, bool, bool, bool) -> WordNode,
    pub get_input: &'a dyn Fn() -> String,
    pub error: &'a dyn Fn(&str),
}

/// Check if the current token is a valid conditional operand.
/// In [[ ]], { and } can be used as plain string operands.
/// Also, ASSIGNMENT_WORD tokens (like a=x) are treated as plain words in [[ ]].
fn is_cond_operand(ctx: &CondParserContext) -> bool {
    (ctx.is_word)()
        || (ctx.check)(TokenType::LBrace)
        || (ctx.check)(TokenType::RBrace)
        || (ctx.check)(TokenType::AssignmentWord)
}

/// Parse a pattern word for the RHS of == or != in [[ ]].
/// Handles the special case of !(...) extglob patterns where the lexer
/// tokenizes `!` as BANG and `(` as LPAREN separately.
fn parse_pattern_word(ctx: &CondParserContext) -> WordNode {
    // Check for !(...) extglob pattern: BANG followed by LPAREN
    if (ctx.check)(TokenType::Bang) && (ctx.peek)(1).token_type == TokenType::LParen {
        // Consume the BANG token
        (ctx.advance)();
        // Consume the LPAREN token
        (ctx.advance)();

        // Now we need to find the matching ) and collect everything as an extglob pattern
        // Track parenthesis depth
        let mut depth = 1;
        let mut pattern = String::from("!(");

        while depth > 0 && !(ctx.check)(TokenType::Eof) {
            if (ctx.check)(TokenType::LParen) {
                depth += 1;
                pattern.push('(');
                (ctx.advance)();
            } else if (ctx.check)(TokenType::RParen) {
                depth -= 1;
                if depth > 0 {
                    pattern.push(')');
                }
                (ctx.advance)();
            } else if (ctx.is_word)() {
                pattern.push_str(&(ctx.advance)().value);
            } else if (ctx.check)(TokenType::Pipe) {
                pattern.push('|');
                (ctx.advance)();
            } else {
                // Unexpected token
                break;
            }
        }

        pattern.push(')');

        // Parse the pattern string to create proper word parts
        return (ctx.parse_word_from_string)(&pattern, false, false, false, false, true);
    }

    // Normal case - just parse a word
    (ctx.parse_word_no_brace_expansion)()
}

/// Parse a conditional expression
pub fn parse_conditional_expression(ctx: &CondParserContext) -> ConditionalExpressionNode {
    // Skip leading newlines inside [[ ]]
    (ctx.skip_newlines)();
    parse_cond_or(ctx)
}

fn parse_cond_or(ctx: &CondParserContext) -> ConditionalExpressionNode {
    let mut left = parse_cond_and(ctx);

    // Skip newlines before ||
    (ctx.skip_newlines)();
    while (ctx.check)(TokenType::OrOr) {
        (ctx.advance)();
        // Skip newlines after ||
        (ctx.skip_newlines)();
        let right = parse_cond_and(ctx);
        left = ConditionalExpressionNode::Or(Box::new(CondOrNode { left, right }));
        (ctx.skip_newlines)();
    }

    left
}

fn parse_cond_and(ctx: &CondParserContext) -> ConditionalExpressionNode {
    let mut left = parse_cond_not(ctx);

    // Skip newlines before &&
    (ctx.skip_newlines)();
    while (ctx.check)(TokenType::AndAnd) {
        (ctx.advance)();
        // Skip newlines after &&
        (ctx.skip_newlines)();
        let right = parse_cond_not(ctx);
        left = ConditionalExpressionNode::And(Box::new(CondAndNode { left, right }));
        (ctx.skip_newlines)();
    }

    left
}

fn parse_cond_not(ctx: &CondParserContext) -> ConditionalExpressionNode {
    (ctx.skip_newlines)();
    if (ctx.check)(TokenType::Bang) {
        (ctx.advance)();
        (ctx.skip_newlines)();
        let operand = parse_cond_not(ctx);
        return ConditionalExpressionNode::Not(Box::new(CondNotNode { operand }));
    }

    parse_cond_primary(ctx)
}

fn parse_cond_primary(ctx: &CondParserContext) -> ConditionalExpressionNode {
    // Handle grouping: ( expr )
    if (ctx.check)(TokenType::LParen) {
        (ctx.advance)();
        let expression = parse_conditional_expression(ctx);
        (ctx.expect)(TokenType::RParen);
        return ConditionalExpressionNode::Group(Box::new(CondGroupNode { expression }));
    }

    // Handle unary operators: -f file, -z string, etc.
    // In [[ ]], { and } can be used as plain string operands
    if is_cond_operand(ctx) {
        let first_token = (ctx.current)();
        let first = first_token.value.clone();

        // Check for unary operators - only if NOT quoted
        // Quoted '-f' etc. are string operands, not test operators
        if UNARY_OPS.contains(&first.as_str()) && !first_token.quoted {
            (ctx.advance)();
            // Unary operators require an operand - syntax error if at end
            if (ctx.check)(TokenType::DBrackEnd) {
                (ctx.error)(&format!("Expected operand after {}", first));
                unreachable!();
            }
            if is_cond_operand(ctx) {
                let operand = (ctx.parse_word_no_brace_expansion)();
                let operator = match first.as_str() {
                    "-a" => CondUnaryOperator::A,
                    "-b" => CondUnaryOperator::B,
                    "-c" => CondUnaryOperator::C,
                    "-d" => CondUnaryOperator::D,
                    "-e" => CondUnaryOperator::E,
                    "-f" => CondUnaryOperator::F,
                    "-g" => CondUnaryOperator::G,
                    "-h" => CondUnaryOperator::H,
                    "-k" => CondUnaryOperator::K,
                    "-p" => CondUnaryOperator::P,
                    "-r" => CondUnaryOperator::R,
                    "-s" => CondUnaryOperator::S,
                    "-t" => CondUnaryOperator::T,
                    "-u" => CondUnaryOperator::U,
                    "-w" => CondUnaryOperator::W,
                    "-x" => CondUnaryOperator::X,
                    "-G" => CondUnaryOperator::UpperG,
                    "-L" => CondUnaryOperator::L,
                    "-N" => CondUnaryOperator::N,
                    "-O" => CondUnaryOperator::UpperO,
                    "-S" => CondUnaryOperator::UpperS,
                    "-z" => CondUnaryOperator::Z,
                    "-n" => CondUnaryOperator::LowerN,
                    "-o" => CondUnaryOperator::LowerO,
                    "-v" => CondUnaryOperator::V,
                    "-R" => CondUnaryOperator::UpperR,
                    _ => {
                        (ctx.error)(&format!("Unknown unary operator: {}", first));
                        unreachable!();
                    }
                };
                return ConditionalExpressionNode::Unary(CondUnaryNode { operator, operand });
            }
            // Unary operator followed by non-word token (like < > && ||) is a syntax error
            let bad_token = (ctx.current)();
            (ctx.error)(&format!(
                "unexpected argument `{}' to conditional unary operator",
                bad_token.value
            ));
            unreachable!();
        }

        // Parse as word, then check for binary operator
        let left = (ctx.parse_word_no_brace_expansion)();

        // Check for binary operators
        if (ctx.is_word)() && BINARY_OPS.contains(&(ctx.current)().value.as_str()) {
            let operator_str = (ctx.advance)().value;
            // For =~ operator, the RHS can include unquoted ( and ) for regex grouping
            // Parse until we hit ]], &&, ||, or newline
            // For == and != operators, the RHS is a pattern (may include !(...) extglob)
            let right: WordNode;
            if operator_str == "=~" {
                right = parse_regex_pattern(ctx);
            } else if operator_str == "==" || operator_str == "!=" {
                right = parse_pattern_word(ctx);
            } else {
                right = (ctx.parse_word_no_brace_expansion)();
            }
            let operator = match operator_str.as_str() {
                "==" => CondBinaryOperator::EqEq,
                "!=" => CondBinaryOperator::Ne,
                "=~" => CondBinaryOperator::Match,
                "<" => CondBinaryOperator::Lt,
                ">" => CondBinaryOperator::Gt,
                "-eq" => CondBinaryOperator::NumEq,
                "-ne" => CondBinaryOperator::NumNe,
                "-lt" => CondBinaryOperator::NumLt,
                "-le" => CondBinaryOperator::NumLe,
                "-gt" => CondBinaryOperator::NumGt,
                "-ge" => CondBinaryOperator::NumGe,
                "-nt" => CondBinaryOperator::Nt,
                "-ot" => CondBinaryOperator::Ot,
                "-ef" => CondBinaryOperator::Ef,
                _ => {
                    (ctx.error)(&format!("Unknown binary operator: {}", operator_str));
                    unreachable!();
                }
            };
            return ConditionalExpressionNode::Binary(CondBinaryNode {
                operator,
                left,
                right,
            });
        }

        // Check for < and > which are tokenized as LESS and GREAT
        if (ctx.check)(TokenType::Less) {
            (ctx.advance)();
            let right = (ctx.parse_word_no_brace_expansion)();
            return ConditionalExpressionNode::Binary(CondBinaryNode {
                operator: CondBinaryOperator::Lt,
                left,
                right,
            });
        }
        if (ctx.check)(TokenType::Great) {
            (ctx.advance)();
            let right = (ctx.parse_word_no_brace_expansion)();
            return ConditionalExpressionNode::Binary(CondBinaryNode {
                operator: CondBinaryOperator::Gt,
                left,
                right,
            });
        }

        // Check for = (assignment/equality in test)
        if (ctx.is_word)() && (ctx.current)().value == "=" {
            (ctx.advance)();
            let right = parse_pattern_word(ctx);
            return ConditionalExpressionNode::Binary(CondBinaryNode {
                operator: CondBinaryOperator::EqEq,
                left,
                right,
            });
        }

        // Just a word (non-empty string test)
        return ConditionalExpressionNode::Word(CondWordNode { word: left });
    }

    (ctx.error)("Expected conditional expression");
    unreachable!();
}

/// Parse a regex pattern for the =~ operator.
/// In bash, the RHS of =~ can include unquoted ( and ) for regex grouping.
/// We collect tokens until we hit ]], &&, ||, or newline.
///
/// Important rules:
/// - Track parenthesis depth to distinguish between regex grouping and conditional grouping
/// - At the top level (parenDepth === 0), tokens must be adjacent (no spaces)
/// - Inside parentheses (parenDepth > 0), spaces are allowed and operators lose special meaning
/// - This matches bash behavior: "[[ a =~ c a ]]" is a syntax error,
///   but "[[ a =~ (c a) ]]" is valid
fn parse_regex_pattern(ctx: &CondParserContext) -> WordNode {
    let mut parts: Vec<WordPart> = Vec::new();
    let mut paren_depth = 0; // Track nested parens in the regex pattern
    let mut last_token_end: isize = -1; // Track end position of last consumed token
    let input = (ctx.get_input)(); // Get raw input for extracting exact whitespace

    // Helper to check if we're at a pattern terminator
    let is_terminator = || {
        (ctx.check)(TokenType::DBrackEnd)
            || (ctx.check)(TokenType::AndAnd)
            || (ctx.check)(TokenType::OrOr)
            || (ctx.check)(TokenType::Newline)
            || (ctx.check)(TokenType::Eof)
    };

    while !is_terminator() {
        let current_token = (ctx.current)();
        let has_gap =
            last_token_end >= 0 && (current_token.start as isize) > last_token_end;

        // At top level (outside parens), tokens must be adjacent (no space gap)
        // Inside parens, spaces are allowed (regex groups can contain spaces)
        if paren_depth == 0 && has_gap {
            // There's a gap (whitespace) between the last token and this one
            // Stop parsing - remaining tokens will cause a syntax error
            break;
        }

        // Inside parens, preserve the exact whitespace from the input
        if paren_depth > 0 && has_gap {
            // Extract the exact whitespace characters from the raw input
            let whitespace = &input[last_token_end as usize..current_token.start];
            parts.push(WordPart::Literal(LiteralPart {
                value: whitespace.to_string(),
            }));
        }

        if (ctx.is_word)() || (ctx.check)(TokenType::AssignmentWord) {
            // Parse word parts for regex (this preserves backslash escapes as Escaped nodes)
            // ASSIGNMENT_WORD tokens (like a=) are treated as plain words in regex patterns
            let word = (ctx.parse_word_for_regex)();
            parts.extend(word.parts);
            // After parseWord, position has advanced - get the consumed token's end
            last_token_end = (ctx.peek)(-1).end as isize;
        } else if (ctx.check)(TokenType::LParen) {
            // Unquoted ( in regex pattern - part of regex grouping
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "(".to_string(),
            }));
            paren_depth += 1;
            last_token_end = token.end as isize;
        } else if (ctx.check)(TokenType::DParenStart) {
            // (( is tokenized as DPAREN_START, but inside regex it's two ( chars
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "((".to_string(),
            }));
            paren_depth += 2;
            last_token_end = token.end as isize;
        } else if (ctx.check)(TokenType::DParenEnd) {
            // )) is tokenized as DPAREN_END, but inside regex it's two ) chars
            if paren_depth >= 2 {
                let token = (ctx.advance)();
                parts.push(WordPart::Literal(LiteralPart {
                    value: "))".to_string(),
                }));
                paren_depth -= 2;
                last_token_end = token.end as isize;
            } else if paren_depth == 1 {
                // Only one ( is open, this )) closes it and the extra ) is conditional grouping
                // Don't consume, let the RPAREN handler deal with it
                break;
            } else {
                // No open regex parens - this )) is part of the conditional expression
                break;
            }
        } else if (ctx.check)(TokenType::RParen) {
            // Unquoted ) - could be regex grouping or conditional expression grouping
            if paren_depth > 0 {
                // We have an open paren from the regex, this ) closes it
                let token = (ctx.advance)();
                parts.push(WordPart::Literal(LiteralPart {
                    value: ")".to_string(),
                }));
                paren_depth -= 1;
                last_token_end = token.end as isize;
            } else {
                // No open regex parens - this ) is part of the conditional expression
                // Stop parsing the regex pattern here
                break;
            }
        } else if (ctx.check)(TokenType::Pipe) {
            // Unquoted | in regex pattern - regex alternation (foo|bar)
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "|".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if (ctx.check)(TokenType::Semicolon) {
            // Unquoted ; in regex pattern - only valid inside parentheses
            if paren_depth > 0 {
                let token = (ctx.advance)();
                parts.push(WordPart::Literal(LiteralPart {
                    value: ";".to_string(),
                }));
                last_token_end = token.end as isize;
            } else {
                // At top level, semicolon is a command terminator, stop parsing
                break;
            }
        } else if paren_depth > 0 && (ctx.check)(TokenType::Less) {
            // Unquoted < inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "<".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::Great) {
            // Unquoted > inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: ">".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::DGreat) {
            // Unquoted >> inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: ">>".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::DLess) {
            // Unquoted << inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "<<".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::LessAnd) {
            // Unquoted <& inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "<&".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::GreatAnd) {
            // Unquoted >& inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: ">&".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::LessGreat) {
            // Unquoted <> inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "<>".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::Clobber) {
            // Unquoted >| inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: ">|".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::TLess) {
            // Unquoted <<< inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "<<<".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::Amp) {
            // Unquoted & inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "&".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::LBrace) {
            // Unquoted { inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "{".to_string(),
            }));
            last_token_end = token.end as isize;
        } else if paren_depth > 0 && (ctx.check)(TokenType::RBrace) {
            // Unquoted } inside parentheses - treated as literal in regex
            let token = (ctx.advance)();
            parts.push(WordPart::Literal(LiteralPart {
                value: "}".to_string(),
            }));
            last_token_end = token.end as isize;
        } else {
            // Unknown token, stop parsing
            break;
        }
    }

    if parts.is_empty() {
        (ctx.error)("Expected regex pattern after =~");
        unreachable!();
    }

    WordNode { parts }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unary_ops_list() {
        assert!(UNARY_OPS.contains(&"-f"));
        assert!(UNARY_OPS.contains(&"-z"));
        assert!(UNARY_OPS.contains(&"-n"));
        assert!(!UNARY_OPS.contains(&"-eq"));
    }

    #[test]
    fn test_binary_ops_list() {
        assert!(BINARY_OPS.contains(&"=="));
        assert!(BINARY_OPS.contains(&"!="));
        assert!(BINARY_OPS.contains(&"-eq"));
        assert!(BINARY_OPS.contains(&"=~"));
    }
}
