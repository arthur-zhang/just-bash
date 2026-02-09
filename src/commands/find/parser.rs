use super::types::*;

/// Internal token type for the recursive descent parser.
#[derive(Debug, Clone)]
enum Token {
    Expr(Expression),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

/// Parse find command-line arguments into an Expression tree and FindOptions.
///
/// Extracts global options (-maxdepth, -mindepth, -depth) first, then parses
/// the remaining arguments into an expression tree with proper operator precedence.
/// If no action (print/print0/printf/delete/exec) is present, adds implicit -print.
pub fn parse_expressions(args: &[String]) -> Result<(Expression, FindOptions), String> {
    let mut options = FindOptions {
        max_depth: None,
        min_depth: None,
        depth_first: false,
    };

    // First pass: extract global options and collect remaining args
    let mut remaining: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-maxdepth" => {
                i += 1;
                if i >= args.len() {
                    return Err("find: missing argument to `-maxdepth'".to_string());
                }
                options.max_depth = Some(args[i].parse::<usize>().map_err(|_| {
                    format!("find: invalid argument `{}' to `-maxdepth'", args[i])
                })?);
            }
            "-mindepth" => {
                i += 1;
                if i >= args.len() {
                    return Err("find: missing argument to `-mindepth'".to_string());
                }
                options.min_depth = Some(args[i].parse::<usize>().map_err(|_| {
                    format!("find: invalid argument `{}' to `-mindepth'", args[i])
                })?);
            }
            "-depth" => {
                options.depth_first = true;
            }
            _ => {
                remaining.push(args[i].clone());
            }
        }
        i += 1;
    }

    // Second pass: tokenize the remaining arguments
    let mut tokens: Vec<Token> = Vec::new();
    let mut _has_action = false;
    let mut i = 0;

    while i < remaining.len() {
        let arg = remaining[i].clone();
        match arg.as_str() {
            "(" | "\\(" => tokens.push(Token::LParen),
            ")" | "\\)" => tokens.push(Token::RParen),
            "-not" | "!" => tokens.push(Token::Not),
            "-a" | "-and" => tokens.push(Token::And),
            "-o" | "-or" => tokens.push(Token::Or),
            "-name" | "-iname" => {
                i += 1;
                if i >= remaining.len() {
                    return Err(format!("find: missing argument to `{}'", arg));
                }
                let case_insensitive = arg == "-iname";
                tokens.push(Token::Expr(Expression::Name {
                    pattern: remaining[i].clone(),
                    case_insensitive,
                }));
            }
            "-path" | "-ipath" | "-wholename" | "-iwholename" => {
                i += 1;
                if i >= remaining.len() {
                    return Err(format!("find: missing argument to `{}'", arg));
                }
                let case_insensitive = arg == "-ipath" || arg == "-iwholename";
                tokens.push(Token::Expr(Expression::Path {
                    pattern: remaining[i].clone(),
                    case_insensitive,
                }));
            }
            "-regex" | "-iregex" => {
                i += 1;
                if i >= remaining.len() {
                    return Err(format!("find: missing argument to `{}'", arg));
                }
                let case_insensitive = arg == "-iregex";
                tokens.push(Token::Expr(Expression::Regex {
                    pattern: remaining[i].clone(),
                    case_insensitive,
                }));
            }
            "-type" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-type'".to_string());
                }
                let file_type = match remaining[i].as_str() {
                    "f" => FileType::File,
                    "d" => FileType::Directory,
                    "l" => FileType::Symlink,
                    other => {
                        return Err(format!("find: Unknown argument to -type: {}", other));
                    }
                };
                tokens.push(Token::Expr(Expression::Type(file_type)));
            }
            "-empty" => tokens.push(Token::Expr(Expression::Empty)),
            "-mtime" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-mtime'".to_string());
                }
                let (comparison, num_str) = parse_comparison_prefix(&remaining[i]);
                let days: i64 = num_str.parse().map_err(|_| {
                    format!("find: invalid argument `{}' to `-mtime'", remaining[i])
                })?;
                tokens.push(Token::Expr(Expression::Mtime { days, comparison }));
            }
            "-newer" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-newer'".to_string());
                }
                tokens.push(Token::Expr(Expression::Newer {
                    reference_path: remaining[i].clone(),
                }));
            }
            "-size" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-size'".to_string());
                }
                let (comparison, num_str) = parse_comparison_prefix(&remaining[i]);
                let (value, unit) = parse_size_value(num_str)?;
                tokens.push(Token::Expr(Expression::Size {
                    value,
                    unit,
                    comparison,
                }));
            }
            "-perm" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-perm'".to_string());
                }
                let perm_arg = &remaining[i];
                let (match_type, mode_str) = if let Some(rest) = perm_arg.strip_prefix('-') {
                    (PermMatch::AllBits, rest)
                } else if let Some(rest) = perm_arg.strip_prefix('/') {
                    (PermMatch::AnyBits, rest)
                } else {
                    (PermMatch::Exact, perm_arg.as_str())
                };
                let mode = u32::from_str_radix(mode_str, 8).map_err(|_| {
                    format!("find: invalid mode `{}'", perm_arg)
                })?;
                tokens.push(Token::Expr(Expression::Perm { mode, match_type }));
            }
            "-prune" => tokens.push(Token::Expr(Expression::Prune)),
            "-print" => {
                _has_action = true;
                tokens.push(Token::Expr(Expression::Print));
            }
            "-print0" => {
                _has_action = true;
                tokens.push(Token::Expr(Expression::Print0));
            }
            "-printf" => {
                i += 1;
                if i >= remaining.len() {
                    return Err("find: missing argument to `-printf'".to_string());
                }
                _has_action = true;
                tokens.push(Token::Expr(Expression::Printf {
                    format: remaining[i].clone(),
                }));
            }
            "-delete" => {
                _has_action = true;
                tokens.push(Token::Expr(Expression::Delete));
            }
            "-exec" => {
                _has_action = true;
                i += 1;
                let mut command_parts: Vec<String> = Vec::new();
                while i < remaining.len()
                    && remaining[i] != ";"
                    && remaining[i] != "+"
                {
                    command_parts.push(remaining[i].clone());
                    i += 1;
                }
                if i >= remaining.len() {
                    return Err("find: missing argument to `-exec'".to_string());
                }
                let batch = remaining[i] == "+";
                tokens.push(Token::Expr(Expression::Exec {
                    command: command_parts,
                    batch,
                }));
            }
            other => {
                if other.starts_with('-') {
                    return Err(format!("find: unknown predicate `{}'", other));
                }
                // Non-option argument (path) - skip if at start, otherwise stop
                if tokens.is_empty() {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        i += 1;
    }

    // If no tokens, return implicit print
    if tokens.is_empty() {
        return Ok((Expression::Print, options));
    }

    // Build expression tree using recursive descent
    let expr = build_expression_tree(&tokens)?;

    // If no action was specified, the expression is used as-is
    // (the caller should add implicit -print behavior at evaluation time)
    Ok((expr, options))
}
/// Parse a +/- prefix from a numeric argument to determine comparison type.
fn parse_comparison_prefix(s: &str) -> (Comparison, &str) {
    if let Some(rest) = s.strip_prefix('+') {
        (Comparison::GreaterThan, rest)
    } else if let Some(rest) = s.strip_prefix('-') {
        (Comparison::LessThan, rest)
    } else {
        (Comparison::Exact, s)
    }
}

/// Parse a size value with optional unit suffix.
fn parse_size_value(s: &str) -> Result<(i64, SizeUnit), String> {
    if s.is_empty() {
        return Err("find: invalid argument to `-size'".to_string());
    }
    let last = s.as_bytes()[s.len() - 1];
    let (num_str, unit) = match last {
        b'c' => (&s[..s.len() - 1], SizeUnit::Bytes),
        b'k' => (&s[..s.len() - 1], SizeUnit::Kilobytes),
        b'M' => (&s[..s.len() - 1], SizeUnit::Megabytes),
        b'G' => (&s[..s.len() - 1], SizeUnit::Gigabytes),
        b'b' => (&s[..s.len() - 1], SizeUnit::Blocks),
        _ => (s, SizeUnit::Blocks), // default is 512-byte blocks
    };
    let value: i64 = num_str
        .parse()
        .map_err(|_| format!("find: invalid argument `{}' to `-size'", s))?;
    Ok((value, unit))
}

/// Build an expression tree from tokens using recursive descent parsing.
///
/// Operator precedence (highest to lowest):
/// 1. Parentheses `\(` ... `\)`
/// 2. NOT: `-not` or `!`
/// 3. AND: `-and` or `-a` (implicit between adjacent expressions)
/// 4. OR: `-or` or `-o`
fn build_expression_tree(tokens: &[Token]) -> Result<Expression, String> {
    let mut pos = 0;

    let result = parse_or(tokens, &mut pos)?;

    Ok(result)
}

/// Parse OR expressions (lowest precedence).
fn parse_or(tokens: &[Token], pos: &mut usize) -> Result<Expression, String> {
    let mut left = parse_and(tokens, pos)?;

    while *pos < tokens.len() {
        if matches!(&tokens[*pos], Token::Or) {
            *pos += 1;
            let right = parse_and(tokens, pos)?;
            left = Expression::Or(Box::new(left), Box::new(right));
        } else {
            break;
        }
    }

    Ok(left)
}
/// Parse AND expressions (implicit or explicit `-a`).
fn parse_and(tokens: &[Token], pos: &mut usize) -> Result<Expression, String> {
    let mut left = parse_unary(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            // Explicit AND
            Token::And => {
                *pos += 1;
                let right = parse_unary(tokens, pos)?;
                left = Expression::And(Box::new(left), Box::new(right));
            }
            // Implicit AND: next token is an expression, NOT, or LParen
            Token::Expr(_) | Token::Not | Token::LParen => {
                let right = parse_unary(tokens, pos)?;
                left = Expression::And(Box::new(left), Box::new(right));
            }
            _ => break,
        }
    }

    Ok(left)
}

/// Parse NOT (unary) expressions.
fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<Expression, String> {
    if *pos < tokens.len() && matches!(&tokens[*pos], Token::Not) {
        *pos += 1;
        let expr = parse_unary(tokens, pos)?; // NOT can chain: ! ! expr
        return Ok(Expression::Not(Box::new(expr)));
    }
    parse_primary(tokens, pos)
}

/// Parse primary expressions (atoms and parenthesized groups).
fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<Expression, String> {
    if *pos >= tokens.len() {
        return Err("find: expression expected".to_string());
    }

    match &tokens[*pos] {
        Token::LParen => {
            *pos += 1; // consume '('
            let expr = parse_or(tokens, pos)?;
            // Consume closing paren if present
            if *pos < tokens.len() && matches!(&tokens[*pos], Token::RParen) {
                *pos += 1;
            }
            Ok(expr)
        }
        Token::Expr(e) => {
            let expr = e.clone();
            *pos += 1;
            Ok(expr)
        }
        Token::RParen => {
            Err("find: unexpected ')'".to_string())
        }
        _ => {
            Err("find: expression expected".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse_name() {
        let (expr, _) = parse_expressions(&args(&["-name", "*.txt"])).unwrap();
        match expr {
            Expression::Name { pattern, case_insensitive } => {
                assert_eq!(pattern, "*.txt");
                assert!(!case_insensitive);
            }
            _ => panic!("Expected Name expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_type_file() {
        let (expr, _) = parse_expressions(&args(&["-type", "f"])).unwrap();
        match expr {
            Expression::Type(FileType::File) => {}
            _ => panic!("Expected Type(File), got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_implicit_and() {
        let (expr, _) =
            parse_expressions(&args(&["-name", "*.rs", "-type", "f"])).unwrap();
        match expr {
            Expression::And(left, right) => {
                assert!(matches!(*left, Expression::Name { .. }));
                assert!(matches!(*right, Expression::Type(FileType::File)));
            }
            _ => panic!("Expected And expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_or() {
        let (expr, _) = parse_expressions(&args(&[
            "-name", "*.rs", "-o", "-name", "*.toml",
        ]))
        .unwrap();
        match expr {
            Expression::Or(left, right) => {
                assert!(matches!(*left, Expression::Name { .. }));
                assert!(matches!(*right, Expression::Name { .. }));
            }
            _ => panic!("Expected Or expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_not() {
        let (expr, _) =
            parse_expressions(&args(&["!", "-name", "*.tmp"])).unwrap();
        match expr {
            Expression::Not(inner) => {
                assert!(matches!(*inner, Expression::Name { .. }));
            }
            _ => panic!("Expected Not expression, got {:?}", expr),
        }
    }
    #[test]
    fn test_parse_parenthesized_group() {
        let (expr, _) = parse_expressions(&args(&[
            "\\(", "-name", "*.rs", "-o", "-name", "*.toml", "\\)",
        ]))
        .unwrap();
        // The parenthesized group should produce an Or
        match expr {
            Expression::Or(left, right) => {
                assert!(matches!(*left, Expression::Name { .. }));
                assert!(matches!(*right, Expression::Name { .. }));
            }
            _ => panic!("Expected Or expression from parens, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_size_greater_than_megabytes() {
        let (expr, _) = parse_expressions(&args(&["-size", "+1M"])).unwrap();
        match expr {
            Expression::Size {
                value,
                unit,
                comparison,
            } => {
                assert_eq!(value, 1);
                assert_eq!(unit, SizeUnit::Megabytes);
                assert_eq!(comparison, Comparison::GreaterThan);
            }
            _ => panic!("Expected Size expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_mtime_less_than() {
        let (expr, _) = parse_expressions(&args(&["-mtime", "-7"])).unwrap();
        match expr {
            Expression::Mtime { days, comparison } => {
                assert_eq!(days, 7);
                assert_eq!(comparison, Comparison::LessThan);
            }
            _ => panic!("Expected Mtime expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_perm_exact() {
        let (expr, _) = parse_expressions(&args(&["-perm", "755"])).unwrap();
        match expr {
            Expression::Perm { mode, match_type } => {
                assert_eq!(mode, 0o755);
                assert_eq!(match_type, PermMatch::Exact);
            }
            _ => panic!("Expected Perm expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_perm_all_bits() {
        let (expr, _) = parse_expressions(&args(&["-perm", "-755"])).unwrap();
        match expr {
            Expression::Perm { mode, match_type } => {
                assert_eq!(mode, 0o755);
                assert_eq!(match_type, PermMatch::AllBits);
            }
            _ => panic!("Expected Perm expression, got {:?}", expr),
        }
    }
    #[test]
    fn test_parse_exec_semicolon() {
        let (expr, _) = parse_expressions(&args(&[
            "-exec", "grep", "-l", "TODO", "{}", ";",
        ]))
        .unwrap();
        match expr {
            Expression::Exec { command, batch } => {
                assert_eq!(command, vec!["grep", "-l", "TODO", "{}"]);
                assert!(!batch);
            }
            _ => panic!("Expected Exec expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_exec_batch() {
        let (expr, _) = parse_expressions(&args(&[
            "-exec", "grep", "-l", "TODO", "{}", "+",
        ]))
        .unwrap();
        match expr {
            Expression::Exec { command, batch } => {
                assert_eq!(command, vec!["grep", "-l", "TODO", "{}"]);
                assert!(batch);
            }
            _ => panic!("Expected Exec expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_maxdepth_mindepth() {
        let (_, options) = parse_expressions(&args(&[
            "-maxdepth", "3", "-mindepth", "1",
        ]))
        .unwrap();
        assert_eq!(options.max_depth, Some(3));
        assert_eq!(options.min_depth, Some(1));
    }

    #[test]
    fn test_parse_printf() {
        let (expr, _) =
            parse_expressions(&args(&["-printf", "%f\\n"])).unwrap();
        match expr {
            Expression::Printf { format } => {
                assert_eq!(format, "%f\\n");
            }
            _ => panic!("Expected Printf expression, got {:?}", expr),
        }
    }

    #[test]
    fn test_implicit_print_when_no_action() {
        // When no action is specified, parse should still succeed
        // (implicit print is handled at evaluation time)
        let (expr, _) =
            parse_expressions(&args(&["-name", "*.txt"])).unwrap();
        assert!(matches!(expr, Expression::Name { .. }));
    }

    #[test]
    fn test_default_print_when_empty() {
        // No arguments at all should return Print
        let (expr, _) = parse_expressions(&args(&[])).unwrap();
        assert!(matches!(expr, Expression::Print));
    }

    #[test]
    fn test_complex_expression() {
        // \( -name "*.rs" -o -name "*.toml" \) -type f
        let (expr, _) = parse_expressions(&args(&[
            "\\(", "-name", "*.rs", "-o", "-name", "*.toml", "\\)", "-type", "f",
        ]))
        .unwrap();
        match expr {
            Expression::And(left, right) => {
                assert!(matches!(*left, Expression::Or(_, _)));
                assert!(matches!(*right, Expression::Type(FileType::File)));
            }
            _ => panic!("Expected And(Or(...), Type), got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_depth_flag() {
        let (_, options) =
            parse_expressions(&args(&["-depth", "-name", "*.txt"])).unwrap();
        assert!(options.depth_first);
    }

    #[test]
    fn test_parse_iname() {
        let (expr, _) =
            parse_expressions(&args(&["-iname", "*.TXT"])).unwrap();
        match expr {
            Expression::Name { pattern, case_insensitive } => {
                assert_eq!(pattern, "*.TXT");
                assert!(case_insensitive);
            }
            _ => panic!("Expected Name expression, got {:?}", expr),
        }
    }
}
