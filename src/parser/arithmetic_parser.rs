//! Arithmetic Expression Parser
//!
//! Parses bash arithmetic expressions like:
//! - $((1 + 2))
//! - $((x++))
//! - $((a ? b : c))
//! - $((2#1010))

use crate::ast::types::*;
use super::arithmetic_primaries::{
    skip_arith_whitespace, parse_arith_number, ARITH_ASSIGN_OPS,
    parse_ansi_c_quoting, parse_localization_quoting, parse_nested_arithmetic,
};

/// Re-export for external use
pub use super::arithmetic_primaries::parse_arith_number as parse_number;

/// Preprocess arithmetic expression to handle double-quoted strings.
fn preprocess_arith_input(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        if chars[i] == '"' {
            // Skip opening quote
            i += 1;
            // Copy content until closing quote
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push(chars[i + 1]);
                    i += 2;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            // Skip closing quote
            if i < chars.len() {
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Parse an arithmetic expression string into an AST node
pub fn parse_arithmetic_expression(input: &str) -> ArithmeticExpressionNode {
    let preprocessed = preprocess_arith_input(input);
    let (expression, pos) = parse_arith_expr(&preprocessed, 0);
    
    // Validate that all input was consumed
    let final_pos = skip_arith_whitespace(&preprocessed, pos);
    if final_pos < preprocessed.len() {
        let remaining: String = preprocessed[final_pos..].trim().to_string();
        if !remaining.is_empty() {
            return ArithmeticExpressionNode {
                original_text: Some(input.to_string()),
                expression: ArithExpr::SyntaxError(ArithSyntaxErrorNode {
                    error_token: remaining.clone(),
                    message: format!("{}: syntax error: invalid arithmetic operator (error token is \"{}\")", remaining, remaining),
                }),
            };
        }
    }
    
    ArithmeticExpressionNode {
        expression,
        original_text: Some(input.to_string()),
    }
}

/// Helper to create a "missing operand" syntax error node.
fn make_missing_operand_error(op: &str, pos: usize) -> (ArithExpr, usize) {
    (
        ArithExpr::SyntaxError(ArithSyntaxErrorNode {
            error_token: op.to_string(),
            message: format!("syntax error: operand expected (error token is \"{}\")", op),
        }),
        pos,
    )
}

/// Check if we're at end of input (after skipping whitespace).
fn is_missing_operand(input: &str, pos: usize) -> bool {
    skip_arith_whitespace(input, pos) >= input.len()
}

pub fn parse_arith_expr(input: &str, pos: usize) -> (ArithExpr, usize) {
    parse_arith_comma(input, pos)
}

fn parse_arith_comma(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_ternary(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    current_pos = skip_arith_whitespace(input, current_pos);
    
    while current_pos < chars.len() && chars[current_pos] == ',' {
        let op = ",";
        current_pos += 1;
        if is_missing_operand(input, current_pos) {
            return make_missing_operand_error(op, current_pos);
        }
        let (right, p2) = parse_arith_ternary(input, current_pos);
        left = ArithExpr::Binary(Box::new(ArithBinaryNode {
            operator: ArithBinaryOperator::Comma,
            left,
            right,
        }));
        current_pos = skip_arith_whitespace(input, p2);
    }
    
    (left, current_pos)
}

fn parse_arith_ternary(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (condition, mut current_pos) = parse_arith_logical_or(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    current_pos = skip_arith_whitespace(input, current_pos);
    
    if current_pos < chars.len() && chars[current_pos] == '?' {
        current_pos += 1;
        let (consequent, p2) = parse_arith_expr(input, current_pos);
        current_pos = skip_arith_whitespace(input, p2);
        if current_pos < chars.len() && chars[current_pos] == ':' {
            current_pos += 1;
            let (alternate, p3) = parse_arith_expr(input, current_pos);
            return (
                ArithExpr::Ternary(Box::new(ArithTernaryNode {
                    condition,
                    consequent,
                    alternate,
                })),
                p3,
            );
        }
    }
    
    (condition, current_pos)
}

fn parse_arith_logical_or(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_logical_and(input, pos);
    
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if input[current_pos..].starts_with("||") {
            let op = "||";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_logical_and(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::LogOr,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_logical_and(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_bitwise_or(input, pos);
    
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if input[current_pos..].starts_with("&&") {
            let op = "&&";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_bitwise_or(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::LogAnd,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_bitwise_or(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_bitwise_xor(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if current_pos < chars.len() 
            && chars[current_pos] == '|' 
            && chars.get(current_pos + 1) != Some(&'|')
        {
            let op = "|";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_bitwise_xor(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::BitOr,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_bitwise_xor(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_bitwise_and(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if current_pos < chars.len() && chars[current_pos] == '^' {
            let op = "^";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_bitwise_and(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::BitXor,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_bitwise_and(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_equality(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if current_pos < chars.len() 
            && chars[current_pos] == '&' 
            && chars.get(current_pos + 1) != Some(&'&')
        {
            let op = "&";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_equality(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::BitAnd,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_equality(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_relational(input, pos);
    
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if input[current_pos..].starts_with("==") {
            let op = "==";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_relational(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Eq,
                left,
                right,
            }));
            current_pos = p2;
        } else if input[current_pos..].starts_with("!=") {
            let op = "!=";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_relational(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Ne,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_relational(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_shift(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if input[current_pos..].starts_with("<=") {
            let op = "<=";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_shift(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Le,
                left,
                right,
            }));
            current_pos = p2;
        } else if input[current_pos..].starts_with(">=") {
            let op = ">=";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_shift(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Ge,
                left,
                right,
            }));
            current_pos = p2;
        } else if current_pos < chars.len() && chars[current_pos] == '<' {
            let op = "<";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_shift(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Lt,
                left,
                right,
            }));
            current_pos = p2;
        } else if current_pos < chars.len() && chars[current_pos] == '>' {
            let op = ">";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_shift(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Gt,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_shift(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_additive(input, pos);
    
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if input[current_pos..].starts_with("<<") {
            let op = "<<";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_additive(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::LShift,
                left,
                right,
            }));
            current_pos = p2;
        } else if input[current_pos..].starts_with(">>") {
            let op = ">>";
            current_pos += 2;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_additive(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::RShift,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_additive(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_multiplicative(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if current_pos < chars.len() 
            && (chars[current_pos] == '+' || chars[current_pos] == '-')
            && chars.get(current_pos + 1) != Some(&chars[current_pos])
        {
            let op_char = chars[current_pos];
            let op = if op_char == '+' { "+" } else { "-" };
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_multiplicative(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: if op_char == '+' { ArithBinaryOperator::Add } else { ArithBinaryOperator::Sub },
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_multiplicative(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut left, mut current_pos) = parse_arith_power(input, pos);
    
    let chars: Vec<char> = input.chars().collect();
    loop {
        current_pos = skip_arith_whitespace(input, current_pos);
        if current_pos < chars.len() 
            && chars[current_pos] == '*' 
            && chars.get(current_pos + 1) != Some(&'*')
        {
            let op = "*";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_power(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Mul,
                left,
                right,
            }));
            current_pos = p2;
        } else if current_pos < chars.len() && chars[current_pos] == '/' {
            let op = "/";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_power(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Div,
                left,
                right,
            }));
            current_pos = p2;
        } else if current_pos < chars.len() && chars[current_pos] == '%' {
            let op = "%";
            current_pos += 1;
            if is_missing_operand(input, current_pos) {
                return make_missing_operand_error(op, current_pos);
            }
            let (right, p2) = parse_arith_power(input, current_pos);
            left = ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Mod,
                left,
                right,
            }));
            current_pos = p2;
        } else {
            break;
        }
    }
    
    (left, current_pos)
}

fn parse_arith_power(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (base, current_pos) = parse_arith_unary(input, pos);
    let mut p2 = skip_arith_whitespace(input, current_pos);
    
    if input[p2..].starts_with("**") {
        let op = "**";
        p2 += 2;
        if is_missing_operand(input, p2) {
            return make_missing_operand_error(op, p2);
        }
        let (exponent, p3) = parse_arith_power(input, p2); // Right associative
        return (
            ArithExpr::Binary(Box::new(ArithBinaryNode {
                operator: ArithBinaryOperator::Pow,
                left: base,
                right: exponent,
            })),
            p3,
        );
    }
    
    (base, current_pos)
}

fn parse_arith_unary(input: &str, pos: usize) -> (ArithExpr, usize) {
    let mut current_pos = skip_arith_whitespace(input, pos);
    let chars: Vec<char> = input.chars().collect();
    
    // Prefix operators: ++ -- + - ! ~
    if input[current_pos..].starts_with("++") {
        current_pos += 2;
        let (operand, p2) = parse_arith_unary(input, current_pos);
        return (
            ArithExpr::Unary(Box::new(ArithUnaryNode {
                operator: ArithUnaryOperator::Inc,
                operand,
                prefix: true,
            })),
            p2,
        );
    }
    
    if input[current_pos..].starts_with("--") {
        current_pos += 2;
        let (operand, p2) = parse_arith_unary(input, current_pos);
        return (
            ArithExpr::Unary(Box::new(ArithUnaryNode {
                operator: ArithUnaryOperator::Dec,
                operand,
                prefix: true,
            })),
            p2,
        );
    }
    
    if current_pos < chars.len() {
        let c = chars[current_pos];
        if c == '+' || c == '-' || c == '!' || c == '~' {
            let op = match c {
                '+' => ArithUnaryOperator::Pos,
                '-' => ArithUnaryOperator::Neg,
                '!' => ArithUnaryOperator::Not,
                '~' => ArithUnaryOperator::BitNot,
                _ => unreachable!(),
            };
            current_pos += 1;
            let (operand, p2) = parse_arith_unary(input, current_pos);
            return (
                ArithExpr::Unary(Box::new(ArithUnaryNode {
                    operator: op,
                    operand,
                    prefix: true,
                })),
                p2,
            );
        }
    }
    
    parse_arith_postfix(input, current_pos)
}

fn can_start_concat_primary(input: &str, pos: usize) -> bool {
    let chars: Vec<char> = input.chars().collect();
    if pos >= chars.len() {
        return false;
    }
    let c = chars[pos];
    c == '$' || c == '`'
}

fn parse_arith_postfix(input: &str, pos: usize) -> (ArithExpr, usize) {
    let (mut expr, mut current_pos) = parse_arith_primary(input, pos, false);
    
    // Check for adjacent primaries without whitespace (concatenation)
    let mut parts: Vec<ArithExpr> = vec![expr.clone()];
    while can_start_concat_primary(input, current_pos) {
        let (next_expr, next_pos) = parse_arith_primary(input, current_pos, true);
        parts.push(next_expr);
        current_pos = next_pos;
    }
    
    if parts.len() > 1 {
        expr = ArithExpr::Concat(ArithConcatNode { parts });
    }
    
    // Check for array subscript on concatenated expression
    let chars: Vec<char> = input.chars().collect();
    let mut subscript: Option<ArithExpr> = None;
    if current_pos < chars.len() && chars[current_pos] == '[' {
        if let ArithExpr::Concat(_) = &expr {
            current_pos += 1;
            let (index_expr, p2) = parse_arith_expr(input, current_pos);
            subscript = Some(index_expr);
            current_pos = p2;
            if current_pos < chars.len() && chars[current_pos] == ']' {
                current_pos += 1;
            }
        }
    }
    
    // Wrap subscript in ArithDynamicElement
    if let Some(sub) = subscript {
        if let ArithExpr::Concat(_) = &expr {
            expr = ArithExpr::DynamicElement(Box::new(ArithDynamicElementNode {
                name_expr: expr,
                subscript: Box::new(sub),
            }));
        }
    }
    
    current_pos = skip_arith_whitespace(input, current_pos);
    
    // Check for assignment operators
    if matches!(&expr, ArithExpr::Concat(_) | ArithExpr::Variable(_) | ArithExpr::DynamicElement(_)) {
        for op_str in ARITH_ASSIGN_OPS {
            if input[current_pos..].starts_with(op_str) 
                && !input[current_pos..].starts_with("==")
            {
                current_pos += op_str.len();
                let (value, p2) = parse_arith_ternary(input, current_pos);
                let op = match *op_str {
                    "=" => ArithAssignmentOperator::Assign,
                    "+=" => ArithAssignmentOperator::AddAssign,
                    "-=" => ArithAssignmentOperator::SubAssign,
                    "*=" => ArithAssignmentOperator::MulAssign,
                    "/=" => ArithAssignmentOperator::DivAssign,
                    "%=" => ArithAssignmentOperator::ModAssign,
                    "<<=" => ArithAssignmentOperator::LShiftAssign,
                    ">>=" => ArithAssignmentOperator::RShiftAssign,
                    "&=" => ArithAssignmentOperator::AndAssign,
                    "|=" => ArithAssignmentOperator::OrAssign,
                    "^=" => ArithAssignmentOperator::XorAssign,
                    _ => ArithAssignmentOperator::Assign,
                };
                
                if let ArithExpr::DynamicElement(de) = expr {
                    return (
                        ArithExpr::DynamicAssignment(Box::new(ArithDynamicAssignmentNode {
                            operator: op,
                            target: de.name_expr,
                            subscript: Some(de.subscript),
                            value,
                        })),
                        p2,
                    );
                }
                if let ArithExpr::Concat(_) = &expr {
                    return (
                        ArithExpr::DynamicAssignment(Box::new(ArithDynamicAssignmentNode {
                            operator: op,
                            target: expr,
                            subscript: None,
                            value,
                        })),
                        p2,
                    );
                }
                if let ArithExpr::Variable(v) = &expr {
                    return (
                        ArithExpr::Assignment(Box::new(ArithAssignmentNode {
                            operator: op,
                            variable: v.name.clone(),
                            subscript: None,
                            string_key: None,
                            value,
                        })),
                        p2,
                    );
                }
            }
        }
    }
    
    // Postfix operators: ++ --
    if input[current_pos..].starts_with("++") {
        current_pos += 2;
        return (
            ArithExpr::Unary(Box::new(ArithUnaryNode {
                operator: ArithUnaryOperator::Inc,
                operand: expr,
                prefix: false,
            })),
            current_pos,
        );
    }
    
    if input[current_pos..].starts_with("--") {
        current_pos += 2;
        return (
            ArithExpr::Unary(Box::new(ArithUnaryNode {
                operator: ArithUnaryOperator::Dec,
                operand: expr,
                prefix: false,
            })),
            current_pos,
        );
    }
    
    (expr, current_pos)
}

fn parse_arith_primary(input: &str, pos: usize, skip_assignment: bool) -> (ArithExpr, usize) {
    let mut current_pos = skip_arith_whitespace(input, pos);
    let chars: Vec<char> = input.chars().collect();

    // Nested arithmetic: $((expr))
    if let Some(result) = parse_nested_arithmetic(
        |s, p| Some(parse_arith_expr(s, p)),
        input,
        current_pos,
    ) {
        return (result.expr, result.pos);
    }

    // ANSI-C quoting: $'...'
    if let Some(result) = parse_ansi_c_quoting(input, current_pos) {
        return (result.expr, result.pos);
    }

    // Localization quoting: $"..."
    if let Some(result) = parse_localization_quoting(input, current_pos) {
        return (result.expr, result.pos);
    }
    
    // Command substitution: $(cmd)
    if input[current_pos..].starts_with("$(") && !input[current_pos..].starts_with("$((") {
        current_pos += 2;
        let mut depth = 1;
        let cmd_start = current_pos;
        while current_pos < chars.len() && depth > 0 {
            if chars[current_pos] == '(' {
                depth += 1;
            } else if chars[current_pos] == ')' {
                depth -= 1;
            }
            if depth > 0 {
                current_pos += 1;
            }
        }
        let cmd: String = chars[cmd_start..current_pos].iter().collect();
        current_pos += 1; // Skip )
        return (
            ArithExpr::CommandSubst(ArithCommandSubstNode { command: cmd }),
            current_pos,
        );
    }
    
    // Backtick command substitution: `cmd`
    if current_pos < chars.len() && chars[current_pos] == '`' {
        current_pos += 1;
        let cmd_start = current_pos;
        while current_pos < chars.len() && chars[current_pos] != '`' {
            current_pos += 1;
        }
        let cmd: String = chars[cmd_start..current_pos].iter().collect();
        if current_pos < chars.len() && chars[current_pos] == '`' {
            current_pos += 1;
        }
        return (
            ArithExpr::CommandSubst(ArithCommandSubstNode { command: cmd }),
            current_pos,
        );
    }
    
    // Grouped expression
    if current_pos < chars.len() && chars[current_pos] == '(' {
        current_pos += 1;
        let (expr, p2) = parse_arith_expr(input, current_pos);
        current_pos = skip_arith_whitespace(input, p2);
        if current_pos < chars.len() && chars[current_pos] == ')' {
            current_pos += 1;
        }
        return (
            ArithExpr::Group(Box::new(ArithGroupNode { expression: expr })),
            current_pos,
        );
    }
    
    // Single-quoted string
    if current_pos < chars.len() && chars[current_pos] == '\'' {
        current_pos += 1;
        let mut content = String::new();
        while current_pos < chars.len() && chars[current_pos] != '\'' {
            content.push(chars[current_pos]);
            current_pos += 1;
        }
        if current_pos < chars.len() && chars[current_pos] == '\'' {
            current_pos += 1;
        }
        let num_value = content.parse::<i64>().unwrap_or(0);
        return (
            ArithExpr::SingleQuote(ArithSingleQuoteNode {
                content,
                value: num_value,
            }),
            current_pos,
        );
    }
    
    // Double-quoted string
    if current_pos < chars.len() && chars[current_pos] == '"' {
        current_pos += 1;
        let mut content = String::new();
        while current_pos < chars.len() && chars[current_pos] != '"' {
            if chars[current_pos] == '\\' && current_pos + 1 < chars.len() {
                content.push(chars[current_pos + 1]);
                current_pos += 2;
            } else {
                content.push(chars[current_pos]);
                current_pos += 1;
            }
        }
        if current_pos < chars.len() && chars[current_pos] == '"' {
            current_pos += 1;
        }
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return (ArithExpr::Number(ArithNumberNode { value: 0 }), current_pos);
        }
        let (expr, _) = parse_arith_expr(trimmed, 0);
        return (expr, current_pos);
    }
    
    // Number
    if current_pos < chars.len() && chars[current_pos].is_ascii_digit() {
        let mut num_str = String::new();
        let mut seen_hash = false;
        let mut is_hex = false;
        
        while current_pos < chars.len() {
            let ch = chars[current_pos];
            if seen_hash {
                if ch.is_ascii_alphanumeric() || ch == '@' || ch == '_' {
                    num_str.push(ch);
                    current_pos += 1;
                } else {
                    break;
                }
            } else if ch == '#' {
                seen_hash = true;
                num_str.push(ch);
                current_pos += 1;
            } else if num_str == "0" 
                && (ch == 'x' || ch == 'X')
                && current_pos + 1 < chars.len()
                && chars[current_pos + 1].is_ascii_hexdigit()
            {
                is_hex = true;
                num_str.push(ch);
                current_pos += 1;
            } else if is_hex && ch.is_ascii_hexdigit() {
                num_str.push(ch);
                current_pos += 1;
            } else if !is_hex && ch.is_ascii_digit() {
                num_str.push(ch);
                current_pos += 1;
            } else {
                break;
            }
        }
        
        // Check for invalid constant
        if current_pos < chars.len() && (chars[current_pos].is_ascii_alphabetic() || chars[current_pos] == '_') {
            let mut invalid_token = num_str.clone();
            while current_pos < chars.len() && (chars[current_pos].is_ascii_alphanumeric() || chars[current_pos] == '_') {
                invalid_token.push(chars[current_pos]);
                current_pos += 1;
            }
            return (
                ArithExpr::SyntaxError(ArithSyntaxErrorNode {
                    error_token: invalid_token.clone(),
                    message: format!("{}: value too great for base (error token is \"{}\")", invalid_token, invalid_token),
                }),
                current_pos,
            );
        }

        // Check for floating point (not supported in bash arithmetic)
        if current_pos < chars.len() && chars[current_pos] == '.'
            && current_pos + 1 < chars.len() && chars[current_pos + 1].is_ascii_digit()
        {
            // Return syntax error for floating point
            let mut float_str = num_str.clone();
            float_str.push('.');
            float_str.push(chars[current_pos + 1]);
            return (
                ArithExpr::SyntaxError(ArithSyntaxErrorNode {
                    error_token: float_str.clone(),
                    message: format!("{}...: syntax error: invalid arithmetic operator", float_str),
                }),
                current_pos,
            );
        }

        // Check for array subscript on number
        if current_pos < chars.len() && chars[current_pos] == '[' {
            let error_token: String = chars[current_pos..].iter().collect::<String>().trim().to_string();
            return (
                ArithExpr::NumberSubscript(ArithNumberSubscriptNode {
                    number: num_str,
                    error_token,
                }),
                chars.len(),
            );
        }

        let value = parse_arith_number(&num_str).unwrap_or(0);
        return (ArithExpr::Number(ArithNumberNode { value }), current_pos);
    }
    
    // Variable (optionally with $ prefix)
    // Handle ${...} braced parameter expansion
    if input[current_pos..].starts_with("${") {
        let brace_start = current_pos + 2;
        let mut brace_depth = 1;
        let mut i = brace_start;
        while i < chars.len() && brace_depth > 0 {
            if chars[i] == '{' {
                brace_depth += 1;
            } else if chars[i] == '}' {
                brace_depth -= 1;
            }
            if brace_depth > 0 {
                i += 1;
            }
        }
        let content: String = chars[brace_start..i].iter().collect();
        let after_brace = i + 1;

        // Check for dynamic base constant: ${base}#value or ${base}xHEX or ${base}octal
        if after_brace < chars.len() && chars[after_brace] == '#' {
            // Dynamic base#value: ${base}#digits
            let mut value_end = after_brace + 1;
            while value_end < chars.len() && (chars[value_end].is_ascii_alphanumeric() || chars[value_end] == '@' || chars[value_end] == '_') {
                value_end += 1;
            }
            let value_str: String = chars[after_brace + 1..value_end].iter().collect();
            return (
                ArithExpr::DynamicBase(ArithDynamicBaseNode {
                    base_expr: content,
                    value: value_str,
                }),
                value_end,
            );
        }
        if after_brace < chars.len() && (chars[after_brace].is_ascii_digit() || chars[after_brace] == 'x' || chars[after_brace] == 'X') {
            // Dynamic octal (${zero}11) or hex (${zero}xAB)
            let mut num_end = after_brace;
            if chars[after_brace] == 'x' || chars[after_brace] == 'X' {
                num_end += 1; // Skip x/X
                while num_end < chars.len() && chars[num_end].is_ascii_hexdigit() {
                    num_end += 1;
                }
            } else {
                while num_end < chars.len() && chars[num_end].is_ascii_digit() {
                    num_end += 1;
                }
            }
            let suffix: String = chars[after_brace..num_end].iter().collect();
            return (
                ArithExpr::DynamicNumber(ArithDynamicNumberNode {
                    prefix: content,
                    suffix,
                }),
                num_end,
            );
        }

        current_pos = after_brace;
        return (
            ArithExpr::BracedExpansion(ArithBracedExpansionNode { content }),
            current_pos,
        );
    }
    
    // Handle $1, $2, etc. (positional parameters)
    if current_pos < chars.len() 
        && chars[current_pos] == '$'
        && current_pos + 1 < chars.len()
        && chars[current_pos + 1].is_ascii_digit()
    {
        current_pos += 1; // Skip $
        let mut name = String::new();
        while current_pos < chars.len() && chars[current_pos].is_ascii_digit() {
            name.push(chars[current_pos]);
            current_pos += 1;
        }
        return (
            ArithExpr::Variable(ArithVariableNode {
                name,
                has_dollar_prefix: true,
            }),
            current_pos,
        );
    }
    
    // Handle special variables: $*, $@, $#, $?, $-, $!, $$
    if current_pos < chars.len()
        && chars[current_pos] == '$'
        && current_pos + 1 < chars.len()
        && matches!(chars[current_pos + 1], '*' | '@' | '#' | '?' | '-' | '!' | '$')
    {
        let name = chars[current_pos + 1].to_string();
        current_pos += 2;
        return (
            ArithExpr::SpecialVar(ArithSpecialVarNode { name }),
            current_pos,
        );
    }
    
    // Handle $name (regular variables with $ prefix)
    let mut has_dollar_prefix = false;
    if current_pos < chars.len()
        && chars[current_pos] == '$'
        && current_pos + 1 < chars.len()
        && (chars[current_pos + 1].is_ascii_alphabetic() || chars[current_pos + 1] == '_')
    {
        has_dollar_prefix = true;
        current_pos += 1;
    }
    
    if current_pos < chars.len() && (chars[current_pos].is_ascii_alphabetic() || chars[current_pos] == '_') {
        let mut name = String::new();
        while current_pos < chars.len() && (chars[current_pos].is_ascii_alphanumeric() || chars[current_pos] == '_') {
            name.push(chars[current_pos]);
            current_pos += 1;
        }
        
        // Check for array indexing
        if current_pos < chars.len() && chars[current_pos] == '[' && !skip_assignment {
            current_pos += 1;
            
            // Check for quoted string key
            let mut string_key: Option<String> = None;
            if current_pos < chars.len() && (chars[current_pos] == '\'' || chars[current_pos] == '"') {
                let quote = chars[current_pos];
                current_pos += 1;
                let mut key = String::new();
                while current_pos < chars.len() && chars[current_pos] != quote {
                    key.push(chars[current_pos]);
                    current_pos += 1;
                }
                if current_pos < chars.len() && chars[current_pos] == quote {
                    current_pos += 1;
                }
                string_key = Some(key);
                current_pos = skip_arith_whitespace(input, current_pos);
                if current_pos < chars.len() && chars[current_pos] == ']' {
                    current_pos += 1;
                }
            }
            
            let mut index_expr: Option<ArithExpr> = None;
            if string_key.is_none() {
                let (expr, p2) = parse_arith_expr(input, current_pos);
                index_expr = Some(expr);
                current_pos = p2;
                if current_pos < chars.len() && chars[current_pos] == ']' {
                    current_pos += 1;
                }
            }
            
            current_pos = skip_arith_whitespace(input, current_pos);
            
            // Check for double subscript
            if current_pos < chars.len() && chars[current_pos] == '[' && index_expr.is_some() {
                return (
                    ArithExpr::DoubleSubscript(ArithDoubleSubscriptNode {
                        array: name,
                        index: Box::new(index_expr.unwrap()),
                    }),
                    current_pos,
                );
            }
            
            // Check for assignment operators
            if !skip_assignment {
                for op_str in ARITH_ASSIGN_OPS {
                    if input[current_pos..].starts_with(op_str)
                        && !input[current_pos..].starts_with("==")
                    {
                        current_pos += op_str.len();
                        let (value, p2) = parse_arith_ternary(input, current_pos);
                        let op = match *op_str {
                            "=" => ArithAssignmentOperator::Assign,
                            "+=" => ArithAssignmentOperator::AddAssign,
                            "-=" => ArithAssignmentOperator::SubAssign,
                            "*=" => ArithAssignmentOperator::MulAssign,
                            "/=" => ArithAssignmentOperator::DivAssign,
                            "%=" => ArithAssignmentOperator::ModAssign,
                            "<<=" => ArithAssignmentOperator::LShiftAssign,
                            ">>=" => ArithAssignmentOperator::RShiftAssign,
                            "&=" => ArithAssignmentOperator::AndAssign,
                            "|=" => ArithAssignmentOperator::OrAssign,
                            "^=" => ArithAssignmentOperator::XorAssign,
                            _ => ArithAssignmentOperator::Assign,
                        };
                        return (
                            ArithExpr::Assignment(Box::new(ArithAssignmentNode {
                                operator: op,
                                variable: name,
                                subscript: index_expr.map(Box::new),
                                string_key,
                                value,
                            })),
                            p2,
                        );
                    }
                }
            }
            
            return (
                ArithExpr::ArrayElement(ArithArrayElementNode {
                    array: name,
                    index: index_expr.map(Box::new),
                    string_key,
                }),
                current_pos,
            );
        }
        
        current_pos = skip_arith_whitespace(input, current_pos);
        
        // Check for assignment operators
        if !skip_assignment {
            for op_str in ARITH_ASSIGN_OPS {
                if input[current_pos..].starts_with(op_str)
                    && !input[current_pos..].starts_with("==")
                {
                    current_pos += op_str.len();
                    let (value, p2) = parse_arith_ternary(input, current_pos);
                    let op = match *op_str {
                        "=" => ArithAssignmentOperator::Assign,
                        "+=" => ArithAssignmentOperator::AddAssign,
                        "-=" => ArithAssignmentOperator::SubAssign,
                        "*=" => ArithAssignmentOperator::MulAssign,
                        "/=" => ArithAssignmentOperator::DivAssign,
                        "%=" => ArithAssignmentOperator::ModAssign,
                        "<<=" => ArithAssignmentOperator::LShiftAssign,
                        ">>=" => ArithAssignmentOperator::RShiftAssign,
                        "&=" => ArithAssignmentOperator::AndAssign,
                        "|=" => ArithAssignmentOperator::OrAssign,
                        "^=" => ArithAssignmentOperator::XorAssign,
                        _ => ArithAssignmentOperator::Assign,
                    };
                    return (
                        ArithExpr::Assignment(Box::new(ArithAssignmentNode {
                            operator: op,
                            variable: name,
                            subscript: None,
                            string_key: None,
                            value,
                        })),
                        p2,
                    );
                }
            }
        }
        
        return (
            ArithExpr::Variable(ArithVariableNode {
                name,
                has_dollar_prefix,
            }),
            current_pos,
        );
    }
    
    // Check for invalid characters like #
    if current_pos < chars.len() && chars[current_pos] == '#' {
        let mut error_end = current_pos + 1;
        while error_end < chars.len() && chars[error_end] != '\n' {
            error_end += 1;
        }
        let error_token: String = chars[current_pos..error_end].iter().collect::<String>().trim().to_string();
        let error_token = if error_token.is_empty() { "#".to_string() } else { error_token };
        return (
            ArithExpr::SyntaxError(ArithSyntaxErrorNode {
                error_token: error_token.clone(),
                message: format!("{}: syntax error: invalid arithmetic operator (error token is \"{}\")", error_token, error_token),
            }),
            chars.len(),
        );
    }
    
    // Default: 0
    (ArithExpr::Number(ArithNumberNode { value: 0 }), current_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_number() {
        let result = parse_arithmetic_expression("42");
        if let ArithExpr::Number(n) = result.expression {
            assert_eq!(n.value, 42);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_addition() {
        let result = parse_arithmetic_expression("1 + 2");
        if let ArithExpr::Binary(b) = result.expression {
            assert!(matches!(b.operator, ArithBinaryOperator::Add));
        } else {
            panic!("Expected binary");
        }
    }

    #[test]
    fn test_multiplication() {
        let result = parse_arithmetic_expression("3 * 4");
        if let ArithExpr::Binary(b) = result.expression {
            assert!(matches!(b.operator, ArithBinaryOperator::Mul));
        } else {
            panic!("Expected binary");
        }
    }

    #[test]
    fn test_variable() {
        let result = parse_arithmetic_expression("x");
        if let ArithExpr::Variable(v) = result.expression {
            assert_eq!(v.name, "x");
        } else {
            panic!("Expected variable");
        }
    }

    #[test]
    fn test_assignment() {
        let result = parse_arithmetic_expression("x = 5");
        if let ArithExpr::Assignment(a) = result.expression {
            assert_eq!(a.variable, "x");
            assert!(matches!(a.operator, ArithAssignmentOperator::Assign));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_ternary() {
        let result = parse_arithmetic_expression("1 ? 2 : 3");
        assert!(matches!(result.expression, ArithExpr::Ternary(_)));
    }

    #[test]
    fn test_increment() {
        let result = parse_arithmetic_expression("x++");
        if let ArithExpr::Unary(u) = result.expression {
            assert!(matches!(u.operator, ArithUnaryOperator::Inc));
            assert!(!u.prefix);
        } else {
            panic!("Expected unary");
        }
    }

    #[test]
    fn test_prefix_increment() {
        let result = parse_arithmetic_expression("++x");
        if let ArithExpr::Unary(u) = result.expression {
            assert!(matches!(u.operator, ArithUnaryOperator::Inc));
            assert!(u.prefix);
        } else {
            panic!("Expected unary");
        }
    }
}
