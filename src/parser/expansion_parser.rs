//! Expansion Parser
//!
//! Handles parsing of parameter expansions, arithmetic expansions, etc.

use crate::ast::types::{
    ArithmeticExpressionNode, AssignDefaultOp, BadSubstitutionOp, CaseDirection,
    CaseModificationOp, DefaultValueOp, DoubleQuotedPart, ErrorIfUnsetOp,
    GlobPart, InnerParameterOperation, LengthOp, LengthSliceErrorOp,
    ParameterExpansionPart, ParameterOperation, PatternAnchor, PatternRemovalOp,
    PatternRemovalSide, PatternReplacementOp, SubstringOp, TildeExpansionPart,
    TransformOp, TransformOperator, UseAlternativeOp, WordNode, WordPart, AST,
};
use crate::parser::arithmetic_parser::parse_arithmetic_expression;
use crate::parser::types::ParseException;
use crate::parser::word_parser;

/// Find the closing parenthesis for an extglob pattern starting at openIdx.
/// Handles nested extglob patterns and escaped characters.
fn find_extglob_close(value: &str, open_idx: usize) -> isize {
    let chars: Vec<char> = value.chars().collect();
    let mut depth = 1;
    let mut i = open_idx + 1;

    while i < chars.len() && depth > 0 {
        let c = chars[i];
        if c == '\\' {
            i += 2; // Skip escaped char
            continue;
        }
        // Handle nested extglob patterns
        if "@*+?!".contains(c) && i + 1 < chars.len() && chars[i + 1] == '(' {
            i += 1; // Skip the extglob operator
            depth += 1;
            i += 1; // Skip the (
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return i as isize;
            }
        }
        i += 1;
    }
    -1
}

fn parse_simple_parameter(value: &str, start: usize) -> (ParameterExpansionPart, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start + 1;
    let char = chars.get(i).copied().unwrap_or('\0');

    // Special parameters: $@, $*, $#, $?, $$, $!, $-, $0-$9
    if "@*#?$!-0123456789".contains(char) {
        return (
            ParameterExpansionPart {
                parameter: char.to_string(),
                operation: None,
            },
            i + 1,
        );
    }

    // Variable name
    let mut name = String::new();
    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
        name.push(chars[i]);
        i += 1;
    }

    (
        ParameterExpansionPart {
            parameter: name,
            operation: None,
        },
        i,
    )
}

/// Context for expansion parsing that requires parser callbacks
pub struct ExpansionContext<'a> {
    pub parse_command_substitution: &'a dyn Fn(&str, usize) -> (Option<WordPart>, usize),
    pub parse_backtick_substitution: &'a dyn Fn(&str, usize, bool) -> (WordPart, usize),
    pub parse_arithmetic_expansion: &'a dyn Fn(&str, usize) -> (Option<WordPart>, usize),
    pub is_dollar_dparen_subshell: &'a dyn Fn(&str, usize) -> bool,
    pub report_error: &'a dyn Fn(&str),
}

/// A default/dummy context for cases where we don't have a parser
pub fn dummy_expansion_context() -> ExpansionContext<'static> {
    static DUMMY_CMD_SUB: fn(&str, usize) -> (Option<WordPart>, usize) = |_, i| (None, i);
    static DUMMY_BACKTICK: fn(&str, usize, bool) -> (WordPart, usize) =
        |_, i, _| (AST::literal("`"), i + 1);
    static DUMMY_ARITH: fn(&str, usize) -> (Option<WordPart>, usize) = |_, i| (None, i);
    static DUMMY_SUBSHELL: fn(&str, usize) -> bool = |_, _| false;
    static DUMMY_ERROR: fn(&str) = |_| {};

    ExpansionContext {
        parse_command_substitution: &DUMMY_CMD_SUB,
        parse_backtick_substitution: &DUMMY_BACKTICK,
        parse_arithmetic_expansion: &DUMMY_ARITH,
        is_dollar_dparen_subshell: &DUMMY_SUBSHELL,
        report_error: &DUMMY_ERROR,
    }
}

fn parse_parameter_expansion(
    ctx: &ExpansionContext,
    value: &str,
    start: usize,
    quoted: bool,
) -> Result<(ParameterExpansionPart, usize), ParseException> {
    let chars: Vec<char> = value.chars().collect();
    // Skip ${
    let mut i = start + 2;

    // Handle ${!var} indirection
    let mut indirection = false;
    if chars.get(i) == Some(&'!') {
        indirection = true;
        i += 1;
    }

    // Handle ${#var} length
    let mut length_op = false;
    let next_char = chars.get(i + 1).copied().unwrap_or('}');
    if chars.get(i) == Some(&'#') && !":#%/^,}".contains(next_char) {
        length_op = true;
        i += 1;
    }

    // Parse parameter name
    // For special single-char vars ($@, $*, $#, $?, $$, $!, $-), just take one char
    // For regular vars, stop at operators (#, %, /, :, etc.)
    let mut name = String::new();
    let first_char = chars.get(i).copied().unwrap_or('\0');
    let after_first = chars.get(i + 1).copied().unwrap_or('\0');

    if "@*#?$!-".contains(first_char) && !after_first.is_ascii_alphanumeric() && after_first != '_' {
        // Single special character variable
        name.push(first_char);
        i += 1;
    } else {
        // Regular variable name (alphanumeric + underscore only)
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            name.push(chars[i]);
            i += 1;
        }
    }

    // Handle array subscript
    if chars.get(i) == Some(&'[') {
        let close_idx = word_parser::find_matching_bracket(value, i, '[', ']');
        if close_idx >= 0 {
            let close_idx = close_idx as usize;
            let subscript: String = chars[i..=close_idx].iter().collect();
            name.push_str(&subscript);
            i = close_idx + 1;

            // Check for multiple subscripts like ${a[0][0]} - this is invalid syntax
            if chars.get(i) == Some(&'[') {
                // Find closing } to get full expansion text for error message
                let mut depth = 1;
                let mut j = i;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        j += 1;
                    }
                }
                let bad_text: String = chars[start + 2..j].iter().collect();
                return Ok((
                    ParameterExpansionPart {
                        parameter: String::new(),
                        operation: Some(ParameterOperation::Inner(
                            InnerParameterOperation::BadSubstitution(BadSubstitutionOp {
                                text: bad_text,
                            }),
                        )),
                    },
                    j + 1,
                ));
            }
        }
    }

    // Check for invalid parameter expansion with empty name and operator
    if name.is_empty() && !indirection && !length_op && chars.get(i) != Some(&'}') {
        // Find the closing } to get the full invalid text
        let mut depth = 1;
        let mut j = i;
        while j < chars.len() && depth > 0 {
            if chars[j] == '{' {
                depth += 1;
            } else if chars[j] == '}' {
                depth -= 1;
            }
            if depth > 0 {
                j += 1;
            }
        }
        // If we didn't find a closing }, this is an unterminated expansion - throw parse error
        if depth > 0 {
            return Err(ParseException::new(
                "unexpected EOF while looking for matching '}'",
                0,
                0,
            ));
        }
        let bad_text: String = chars[start + 2..j].iter().collect();
        return Ok((
            ParameterExpansionPart {
                parameter: String::new(),
                operation: Some(ParameterOperation::Inner(
                    InnerParameterOperation::BadSubstitution(BadSubstitutionOp { text: bad_text }),
                )),
            },
            j + 1,
        ));
    }

    let mut operation: Option<ParameterOperation> = None;

    if indirection {
        // Check for ${!arr[@]} or ${!arr[*]} - array keys/indices
        let array_keys_pattern =
            regex_lite::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\[([@*])\]$").unwrap();
        if let Some(caps) = array_keys_pattern.captures(&name) {
            let array_name = caps.get(1).unwrap().as_str().to_string();
            let star = caps.get(2).unwrap().as_str() == "*";

            // Check if there are additional operators
            let current_char = chars.get(i).copied().unwrap_or('}');
            if i < chars.len() && current_char != '}' && ":=-+?#%/^,@".contains(current_char) {
                // Parse as indirection with innerOp
                let (inner_op, end_idx) =
                    parse_parameter_operation(ctx, value, i, &name, quoted)?;
                if let Some(op) = inner_op {
                    operation = Some(ParameterOperation::Indirection(
                        crate::ast::types::IndirectionOp {
                            inner_op: Some(Box::new(op)),
                        },
                    ));
                    i = end_idx;
                } else {
                    operation = Some(ParameterOperation::ArrayKeys(
                        crate::ast::types::ArrayKeysOp {
                            array: array_name,
                            star,
                        },
                    ));
                    name = String::new();
                }
            } else {
                // No suffix operators - this is array keys
                operation = Some(ParameterOperation::ArrayKeys(
                    crate::ast::types::ArrayKeysOp {
                        array: array_name,
                        star,
                    },
                ));
                name = String::new();
            }
        } else {
            let current_char = chars.get(i).copied().unwrap_or('\0');
            let next_char = chars.get(i + 1).copied().unwrap_or('\0');

            if current_char == '*'
                || (current_char == '@' && !"QPaAEKkuUL".contains(next_char))
            {
                // Check for ${!prefix*} or ${!prefix@}
                let suffix = current_char;
                i += 1;
                operation = Some(ParameterOperation::VarNamePrefix(
                    crate::ast::types::VarNamePrefixOp {
                        prefix: name.clone(),
                        star: suffix == '*',
                    },
                ));
                name = String::new();
            } else {
                // Simple indirection ${!ref}
                let current_char = chars.get(i).copied().unwrap_or('}');
                if i < chars.len() && current_char != '}' && ":=-+?#%/^,@".contains(current_char) {
                    let (inner_op, end_idx) =
                        parse_parameter_operation(ctx, value, i, &name, quoted)?;
                    if let Some(op) = inner_op {
                        operation = Some(ParameterOperation::Indirection(
                            crate::ast::types::IndirectionOp {
                                inner_op: Some(Box::new(op)),
                            },
                        ));
                        i = end_idx;
                    } else {
                        operation = Some(ParameterOperation::Indirection(
                            crate::ast::types::IndirectionOp { inner_op: None },
                        ));
                    }
                } else {
                    operation = Some(ParameterOperation::Indirection(
                        crate::ast::types::IndirectionOp { inner_op: None },
                    ));
                }
            }
        }
    } else if length_op {
        let current_char = chars.get(i).copied().unwrap_or('}');
        if current_char == ':' {
            // ${#var:...} is invalid
            operation = Some(ParameterOperation::Inner(
                InnerParameterOperation::LengthSliceError(LengthSliceErrorOp),
            ));
            while i < chars.len() && chars[i] != '}' {
                i += 1;
            }
        } else if current_char != '}' && "-+=?".contains(current_char) {
            // ${#x-default} etc. are syntax errors
            let end_idx = chars[i..].iter().position(|&c| c == '}').unwrap_or(chars.len() - i) + i;
            let suffix: String = chars[i..end_idx].iter().collect();
            (ctx.report_error)(&format!("${{#{}{}}}): bad substitution", name, suffix));
        } else if current_char == '/' {
            // ${#x/pattern/repl} is a syntax error
            let end_idx = chars[i..].iter().position(|&c| c == '}').unwrap_or(chars.len() - i) + i;
            let suffix: String = chars[i..end_idx].iter().collect();
            (ctx.report_error)(&format!("${{#{}{}}}): bad substitution", name, suffix));
        } else {
            operation = Some(ParameterOperation::Inner(InnerParameterOperation::Length(
                LengthOp,
            )));
        }
    }

    // Parse operation
    if operation.is_none() && i < chars.len() && chars[i] != '}' {
        let (op, end_idx) = parse_parameter_operation(ctx, value, i, &name, quoted)?;
        if let Some(inner_op) = op {
            operation = Some(ParameterOperation::Inner(inner_op));
        }
        i = end_idx;
    }

    // Check for invalid characters
    if i < chars.len() && chars[i] != '}' {
        let c = chars[i];
        if !":-+=?#%/^,@[".contains(c) {
            let mut end_idx = i;
            while end_idx < chars.len() && chars[end_idx] != '}' {
                end_idx += 1;
            }
            let bad_exp: String = chars[start..end_idx + 1].iter().collect();
            (ctx.report_error)(&format!(
                "${{{}}}: bad substitution",
                &bad_exp[2..bad_exp.len() - 1]
            ));
        }
    }

    // Find closing }
    while i < chars.len() && chars[i] != '}' {
        i += 1;
    }

    // Check for unterminated expansion
    if i >= chars.len() {
        return Err(ParseException::new(
            "unexpected EOF while looking for matching '}'",
            0,
            0,
        ));
    }

    Ok((
        ParameterExpansionPart {
            parameter: name,
            operation,
        },
        i + 1,
    ))
}

fn parse_parameter_operation(
    ctx: &ExpansionContext,
    value: &str,
    start: usize,
    _param_name: &str,
    quoted: bool,
) -> Result<(Option<InnerParameterOperation>, usize), ParseException> {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start;
    let char = chars.get(i).copied().unwrap_or('\0');
    let next_char = chars.get(i + 1).copied().unwrap_or('\0');

    // :- := :? :+ or :offset:length (substring)
    if char == ':' {
        let op = next_char;

        // Check if this is a special operator :- := :? :+
        if "-=?+".contains(op) {
            let check_empty = true;
            i += 2; // Skip : and operator

            let word_end = word_parser::find_parameter_operation_end(value, i);
            let word_str: String = chars[i..word_end].iter().collect();
            // Parse the word for expansions
            let word_parts = parse_word_parts(
                ctx,
                &word_str,
                false,
                false,
                true, // isAssignment=true for tilde expansion
                false,
                quoted,
                false,
                false,
                true, // inParameterExpansion
            );
            let word = AST::word(if word_parts.is_empty() {
                vec![AST::literal("")]
            } else {
                word_parts
            });

            return match op {
                '-' => Ok((
                    Some(InnerParameterOperation::DefaultValue(DefaultValueOp {
                        word,
                        check_empty,
                    })),
                    word_end,
                )),
                '=' => Ok((
                    Some(InnerParameterOperation::AssignDefault(AssignDefaultOp {
                        word,
                        check_empty,
                    })),
                    word_end,
                )),
                '?' => Ok((
                    Some(InnerParameterOperation::ErrorIfUnset(ErrorIfUnsetOp {
                        word: Some(word),
                        check_empty,
                    })),
                    word_end,
                )),
                '+' => Ok((
                    Some(InnerParameterOperation::UseAlternative(UseAlternativeOp {
                        word,
                        check_empty,
                    })),
                    word_end,
                )),
                _ => Ok((None, i)),
            };
        }

        // Substring: ${var:offset} or ${var:offset:length}
        i += 1; // Skip only the first :
        let word_end = word_parser::find_parameter_operation_end(value, i);
        let word_str: String = chars[i..word_end].iter().collect();

        // Find the separator colon that's NOT part of a ternary expression
        let mut colon_idx: Option<usize> = None;
        let mut depth = 0;
        let mut ternary_depth = 0;
        let word_chars: Vec<char> = word_str.chars().collect();

        for (j, &c) in word_chars.iter().enumerate() {
            if c == '(' || c == '[' {
                depth += 1;
            } else if c == ')' || c == ']' {
                depth -= 1;
            } else if c == '?' && depth == 0 {
                ternary_depth += 1;
            } else if c == ':' && depth == 0 {
                if ternary_depth > 0 {
                    ternary_depth -= 1;
                } else {
                    colon_idx = Some(j);
                    break;
                }
            }
        }

        let offset_str = if let Some(idx) = colon_idx {
            word_str[..idx].to_string()
        } else {
            word_str.clone()
        };
        let length_str = colon_idx.map(|idx| word_str[idx + 1..].to_string());

        return Ok((
            Some(InnerParameterOperation::Substring(SubstringOp {
                offset: word_parser::parse_arith_expr_from_string(&offset_str),
                length: length_str.map(|s| word_parser::parse_arith_expr_from_string(&s)),
            })),
            word_end,
        ));
    }

    // - = ? + (without colon)
    if "-=?+".contains(char) {
        i += 1;
        let word_end = word_parser::find_parameter_operation_end(value, i);
        let word_str: String = chars[i..word_end].iter().collect();
        let word_parts = parse_word_parts(
            ctx,
            &word_str,
            false,
            false,
            true,
            false,
            quoted,
            false,
            false,
            true,
        );
        let word = AST::word(if word_parts.is_empty() {
            vec![AST::literal("")]
        } else {
            word_parts
        });

        return match char {
            '-' => Ok((
                Some(InnerParameterOperation::DefaultValue(DefaultValueOp {
                    word,
                    check_empty: false,
                })),
                word_end,
            )),
            '=' => Ok((
                Some(InnerParameterOperation::AssignDefault(AssignDefaultOp {
                    word,
                    check_empty: false,
                })),
                word_end,
            )),
            '?' => Ok((
                Some(InnerParameterOperation::ErrorIfUnset(ErrorIfUnsetOp {
                    word: if word_str.is_empty() { None } else { Some(word) },
                    check_empty: false,
                })),
                word_end,
            )),
            '+' => Ok((
                Some(InnerParameterOperation::UseAlternative(UseAlternativeOp {
                    word,
                    check_empty: false,
                })),
                word_end,
            )),
            _ => Ok((None, i)),
        };
    }

    // ## # %% % pattern removal
    if char == '#' || char == '%' {
        let greedy = next_char == char;
        let side = if char == '#' {
            PatternRemovalSide::Prefix
        } else {
            PatternRemovalSide::Suffix
        };
        i += if greedy { 2 } else { 1 };

        let pattern_end = word_parser::find_parameter_operation_end(value, i);
        let pattern_str: String = chars[i..pattern_end].iter().collect();
        let pattern_parts = parse_word_parts(ctx, &pattern_str, false, false, false, false, false, false, false, false);
        let pattern = AST::word(if pattern_parts.is_empty() {
            vec![AST::literal("")]
        } else {
            pattern_parts
        });

        return Ok((
            Some(InnerParameterOperation::PatternRemoval(PatternRemovalOp {
                pattern,
                side,
                greedy,
            })),
            pattern_end,
        ));
    }

    // / // pattern replacement
    if char == '/' {
        let all = next_char == '/';
        i += if all { 2 } else { 1 };

        // Check for anchor
        let mut anchor: Option<PatternAnchor> = None;
        if chars.get(i) == Some(&'#') {
            anchor = Some(PatternAnchor::Start);
            i += 1;
        } else if chars.get(i) == Some(&'%') {
            anchor = Some(PatternAnchor::End);
            i += 1;
        }

        // Find pattern/replacement separator
        let pattern_end = if anchor.is_some()
            && (chars.get(i) == Some(&'/') || chars.get(i) == Some(&'}'))
        {
            i // Pattern is empty
        } else {
            word_parser::find_pattern_end(value, i)
        };
        let pattern_str: String = chars[i..pattern_end].iter().collect();
        let pattern_parts = parse_word_parts(ctx, &pattern_str, false, false, false, false, false, false, false, false);
        let pattern = AST::word(if pattern_parts.is_empty() {
            vec![AST::literal("")]
        } else {
            pattern_parts
        });

        let mut replacement: Option<WordNode> = None;
        let mut end_idx = pattern_end;

        if chars.get(pattern_end) == Some(&'/') {
            let replace_start = pattern_end + 1;
            let replace_end = word_parser::find_parameter_operation_end(value, replace_start);
            let replace_str: String = chars[replace_start..replace_end].iter().collect();
            let replace_parts = parse_word_parts(ctx, &replace_str, false, false, false, false, false, false, false, false);
            replacement = Some(AST::word(if replace_parts.is_empty() {
                vec![AST::literal("")]
            } else {
                replace_parts
            }));
            end_idx = replace_end;
        }

        return Ok((
            Some(InnerParameterOperation::PatternReplacement(
                PatternReplacementOp {
                    pattern,
                    replacement,
                    all,
                    anchor,
                },
            )),
            end_idx,
        ));
    }

    // ^ ^^ , ,, case modification
    if char == '^' || char == ',' {
        let all = next_char == char;
        let direction = if char == '^' {
            CaseDirection::Upper
        } else {
            CaseDirection::Lower
        };
        i += if all { 2 } else { 1 };

        let pattern_end = word_parser::find_parameter_operation_end(value, i);
        let pattern_str: String = chars[i..pattern_end].iter().collect();
        let pattern = if pattern_str.is_empty() {
            None
        } else {
            Some(AST::word(vec![AST::literal(&pattern_str)]))
        };

        return Ok((
            Some(InnerParameterOperation::CaseModification(CaseModificationOp {
                direction,
                all,
                pattern,
            })),
            pattern_end,
        ));
    }

    // @Q @P @a @A @E @K @k @u @U @L transformations
    if char == '@' && "QPaAEKkuUL".contains(next_char) {
        let operator = match next_char {
            'Q' => TransformOperator::Q,
            'P' => TransformOperator::P,
            'a' => TransformOperator::LowerA,
            'A' => TransformOperator::A,
            'E' => TransformOperator::E,
            'K' => TransformOperator::K,
            'k' => TransformOperator::LowerK,
            'u' => TransformOperator::LowerU,
            'U' => TransformOperator::U,
            'L' => TransformOperator::L,
            _ => return Ok((None, i)),
        };
        return Ok((
            Some(InnerParameterOperation::Transform(TransformOp { operator })),
            i + 2,
        ));
    }

    Ok((None, i))
}

fn parse_expansion(
    ctx: &ExpansionContext,
    value: &str,
    start: usize,
    quoted: bool,
) -> Result<(Option<WordPart>, usize), ParseException> {
    let chars: Vec<char> = value.chars().collect();
    // $ at start
    let i = start + 1;

    if i >= chars.len() {
        return Ok((Some(AST::literal("$")), i));
    }

    let char = chars[i];

    // $((expr)) - arithmetic expansion OR $((cmd) ...) - command substitution
    if char == '(' && chars.get(i + 1) == Some(&'(') {
        // Check if this should be parsed as a subshell instead of arithmetic
        if (ctx.is_dollar_dparen_subshell)(value, start) {
            return Ok((ctx.parse_command_substitution)(value, start));
        }
        return Ok((ctx.parse_arithmetic_expansion)(value, start));
    }

    // $[expr] - old-style arithmetic expansion
    if char == '[' {
        // Find matching ]
        let mut depth = 1;
        let mut j = i + 1;
        while j < chars.len() && depth > 0 {
            if chars[j] == '[' {
                depth += 1;
            } else if chars[j] == ']' {
                depth -= 1;
            }
            if depth > 0 {
                j += 1;
            }
        }
        if depth == 0 {
            let expr: String = chars[i + 1..j].iter().collect();
            let arith_expr = parse_arithmetic_expression(&expr);
            return Ok((
                Some(WordPart::ArithmeticExpansion(
                    crate::ast::types::ArithmeticExpansionPart {
                        expression: arith_expr,
                    },
                )),
                j + 1,
            ));
        }
    }

    // $(cmd) - command substitution
    if char == '(' {
        return Ok((ctx.parse_command_substitution)(value, start));
    }

    // ${...} - parameter expansion with operators
    if char == '{' {
        let (part, end_idx) = parse_parameter_expansion(ctx, value, start, quoted)?;
        return Ok((Some(WordPart::ParameterExpansion(part)), end_idx));
    }

    // $VAR or $1 or $@ etc - simple parameter
    if char.is_ascii_alphanumeric() || "_@*#?$!-".contains(char) {
        let (part, end_idx) = parse_simple_parameter(value, start);
        return Ok((Some(WordPart::ParameterExpansion(part)), end_idx));
    }

    // Just a literal $
    Ok((Some(AST::literal("$")), i))
}

fn parse_double_quoted_content(ctx: &ExpansionContext, value: &str) -> Vec<WordPart> {
    let chars: Vec<char> = value.chars().collect();
    let mut parts: Vec<WordPart> = Vec::new();
    let mut i = 0;
    let mut literal = String::new();

    let flush_literal = |parts: &mut Vec<WordPart>, literal: &mut String| {
        if !literal.is_empty() {
            parts.push(AST::literal(literal.as_str()));
            literal.clear();
        }
    };

    while i < chars.len() {
        let char = chars[i];

        // Handle escape sequences in double quotes
        if char == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if "$`\"\\".contains(next) {
                literal.push(next);
                i += 2;
                continue;
            }
            literal.push(char);
            i += 1;
            continue;
        }

        // Handle $ expansions
        if char == '$' {
            flush_literal(&mut parts, &mut literal);
            if let Ok((Some(part), end_index)) = parse_expansion(ctx, value, i, true) {
                parts.push(part);
                i = end_index;
            } else {
                i += 1;
            }
            continue;
        }

        // Handle backtick command substitution
        if char == '`' {
            flush_literal(&mut parts, &mut literal);
            let (part, end_index) = (ctx.parse_backtick_substitution)(value, i, true);
            parts.push(part);
            i = end_index;
            continue;
        }

        literal.push(char);
        i += 1;
    }

    flush_literal(&mut parts, &mut literal);
    parts
}

fn parse_double_quoted(
    ctx: &ExpansionContext,
    value: &str,
    start: usize,
) -> (WordPart, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut inner_parts: Vec<WordPart> = Vec::new();
    let mut i = start;
    let mut literal = String::new();

    let flush_literal = |parts: &mut Vec<WordPart>, literal: &mut String| {
        if !literal.is_empty() {
            parts.push(AST::literal(literal.as_str()));
            literal.clear();
        }
    };

    while i < chars.len() && chars[i] != '"' {
        let char = chars[i];

        // Handle escapes in double quotes
        if char == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if "\"\\$`\n".contains(next) {
                literal.push(next);
                i += 2;
                continue;
            }
            literal.push(char);
            i += 1;
            continue;
        }

        // Handle $ expansions
        if char == '$' {
            flush_literal(&mut inner_parts, &mut literal);
            if let Ok((Some(part), end_index)) = parse_expansion(ctx, value, i, true) {
                inner_parts.push(part);
                i = end_index;
            } else {
                i += 1;
            }
            continue;
        }

        // Handle backtick
        if char == '`' {
            flush_literal(&mut inner_parts, &mut literal);
            let (part, end_index) = (ctx.parse_backtick_substitution)(value, i, true);
            inner_parts.push(part);
            i = end_index;
            continue;
        }

        literal.push(char);
        i += 1;
    }

    flush_literal(&mut inner_parts, &mut literal);

    (AST::double_quoted(inner_parts), i)
}

/// Parse word parts from a string
pub fn parse_word_parts(
    ctx: &ExpansionContext,
    value: &str,
    quoted: bool,
    single_quoted: bool,
    is_assignment: bool,
    here_doc: bool,
    single_quotes_are_literal: bool,
    no_brace_expansion: bool,
    regex_pattern: bool,
    in_parameter_expansion: bool,
) -> Vec<WordPart> {
    if single_quoted {
        return vec![AST::single_quoted(value)];
    }

    let chars: Vec<char> = value.chars().collect();

    // When quoted=true, wrap in DoubleQuoted
    if quoted {
        let inner_parts = parse_double_quoted_content(ctx, value);
        return vec![AST::double_quoted(inner_parts)];
    }

    // Check if value is a fully double-quoted string
    if value.len() >= 2 && chars.first() == Some(&'"') && chars.last() == Some(&'"') {
        let inner: String = chars[1..chars.len() - 1].iter().collect();
        // Check for unescaped double quotes inside
        let mut has_unescaped_quote = false;
        let inner_chars: Vec<char> = inner.chars().collect();
        let mut j = 0;
        while j < inner_chars.len() {
            if inner_chars[j] == '"' {
                has_unescaped_quote = true;
                break;
            }
            if inner_chars[j] == '\\' && j + 1 < inner_chars.len() {
                j += 1;
            }
            j += 1;
        }
        if !has_unescaped_quote {
            let inner_parts = parse_double_quoted_content(ctx, &inner);
            return vec![AST::double_quoted(inner_parts)];
        }
    }

    let mut parts: Vec<WordPart> = Vec::new();
    let mut i = 0;
    let mut literal = String::new();

    let flush_literal = |parts: &mut Vec<WordPart>, literal: &mut String| {
        if !literal.is_empty() {
            parts.push(AST::literal(literal.as_str()));
            literal.clear();
        }
    };

    while i < chars.len() {
        let char = chars[i];

        // Handle escape sequences
        if char == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];

            if regex_pattern {
                flush_literal(&mut parts, &mut literal);
                parts.push(AST::escaped(next.to_string()));
                i += 2;
                continue;
            }

            let is_escapable = if here_doc {
                "$`\n".contains(next)
            } else {
                "$`\"'\n".contains(next) || (in_parameter_expansion && next == '}')
            };

            let is_glob_meta_or_backslash = if single_quotes_are_literal {
                "*?[]\\".contains(next)
            } else {
                "*?[]\\(){}.^+".contains(next)
            };

            if is_escapable {
                literal.push(next);
            } else if is_glob_meta_or_backslash {
                flush_literal(&mut parts, &mut literal);
                parts.push(AST::escaped(next.to_string()));
            } else {
                literal.push('\\');
                literal.push(next);
            }
            i += 2;
            continue;
        }

        // Handle single quotes
        if char == '\'' && !single_quotes_are_literal && !here_doc {
            flush_literal(&mut parts, &mut literal);
            let close_quote = chars[i + 1..].iter().position(|&c| c == '\'');
            if let Some(pos) = close_quote {
                let quoted_content: String = chars[i + 1..i + 1 + pos].iter().collect();
                parts.push(AST::single_quoted(&quoted_content));
                i = i + 1 + pos + 1;
            } else {
                let remaining: String = chars[i..].iter().collect();
                literal.push_str(&remaining);
                break;
            }
            continue;
        }

        // Handle double quotes
        if char == '"' && !here_doc {
            flush_literal(&mut parts, &mut literal);
            let (part, end_index) = parse_double_quoted(ctx, value, i + 1);
            parts.push(part);
            i = end_index + 1;
            continue;
        }

        // Handle $'' ANSI-C quoting
        if char == '$' && chars.get(i + 1) == Some(&'\'') {
            flush_literal(&mut parts, &mut literal);
            let (part, end_index) = word_parser::parse_ansi_c_quoted(value, i + 2);
            parts.push(part);
            i = end_index;
            continue;
        }

        // Handle $ expansions
        if char == '$' {
            flush_literal(&mut parts, &mut literal);
            if let Ok((Some(part), end_index)) = parse_expansion(ctx, value, i, false) {
                parts.push(part);
                i = end_index;
            } else {
                literal.push('$');
                i += 1;
            }
            continue;
        }

        // Handle backtick command substitution
        if char == '`' {
            flush_literal(&mut parts, &mut literal);
            let (part, end_index) = (ctx.parse_backtick_substitution)(value, i, false);
            parts.push(part);
            i = end_index;
            continue;
        }

        // Handle tilde expansion
        if char == '~' {
            let prev_char = if i > 0 { Some(chars[i - 1]) } else { None };
            let can_expand_after_colon = is_assignment && prev_char == Some(':');
            if i == 0 || prev_char == Some('=') || can_expand_after_colon {
                let tilde_end = word_parser::find_tilde_end(value, i);
                let after_tilde = chars.get(tilde_end).copied();
                if after_tilde.is_none() || after_tilde == Some('/') || after_tilde == Some(':') {
                    flush_literal(&mut parts, &mut literal);
                    let user_str: String = chars[i + 1..tilde_end].iter().collect();
                    let user = if user_str.is_empty() {
                        None
                    } else {
                        Some(user_str)
                    };
                    parts.push(WordPart::TildeExpansion(TildeExpansionPart { user }));
                    i = tilde_end;
                    continue;
                }
            }
        }

        // Handle extglob patterns
        if "@*+?!".contains(char) && chars.get(i + 1) == Some(&'(') {
            let close_idx = find_extglob_close(value, i + 1);
            if close_idx != -1 {
                flush_literal(&mut parts, &mut literal);
                let close_idx = close_idx as usize;
                let pattern: String = chars[i..=close_idx].iter().collect();
                parts.push(WordPart::Glob(GlobPart { pattern }));
                i = close_idx + 1;
                continue;
            }
        }

        // Handle glob patterns
        if char == '*' || char == '?' || char == '[' {
            flush_literal(&mut parts, &mut literal);
            let (pattern, end_index) = word_parser::parse_glob_pattern(value, i);
            parts.push(WordPart::Glob(GlobPart { pattern }));
            i = end_index;
            continue;
        }

        // Handle brace expansion
        if char == '{' && !is_assignment && !no_brace_expansion {
            // For now, use a simplified brace expansion parser without recursion
            if let Some((part, end_index)) =
                word_parser::try_parse_brace_expansion(value, i, None)
            {
                flush_literal(&mut parts, &mut literal);
                parts.push(part);
                i = end_index;
                continue;
            }
        }

        // Regular character
        literal.push(char);
        i += 1;
    }

    flush_literal(&mut parts, &mut literal);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_parameter() {
        let (part, idx) = parse_simple_parameter("$var rest", 0);
        assert_eq!(part.parameter, "var");
        assert_eq!(idx, 4);
    }

    #[test]
    fn test_parse_special_parameter() {
        let (part, idx) = parse_simple_parameter("$@ rest", 0);
        assert_eq!(part.parameter, "@");
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_find_extglob_close() {
        assert_eq!(find_extglob_close("@(a|b)", 1), 5);
        assert_eq!(find_extglob_close("@(a|(b|c))", 1), 9);
    }
}
