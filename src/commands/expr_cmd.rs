use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use regex_lite::Regex;

pub struct ExprCommand;

#[async_trait]
impl Command for ExprCommand {
    fn name(&self) -> &'static str {
        "expr"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "expr: missing operand\n".to_string(),
                2,
            );
        }

        match evaluate_expr(&ctx.args) {
            Ok(result) => {
                let exit_code = if result == "0" || result.is_empty() { 1 } else { 0 };
                CommandResult::with_exit_code(format!("{}\n", result), String::new(), exit_code)
            }
            Err(e) => CommandResult::with_exit_code(
                String::new(),
                format!("expr: {}\n", e),
                2,
            ),
        }
    }
}

fn evaluate_expr(args: &[String]) -> Result<String, String> {
    if args.len() == 1 {
        return Ok(args[0].clone());
    }

    let mut parser = ExprParser::new(args);
    parser.parse_or()
}

struct ExprParser<'a> {
    args: &'a [String],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    fn new(args: &'a [String]) -> Self {
        Self { args, pos: 0 }
    }

    fn current(&self) -> Option<&str> {
        self.args.get(self.pos).map(|s| s.as_str())
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_or(&mut self) -> Result<String, String> {
        let mut left = self.parse_and()?;
        while self.current() == Some("|") {
            self.advance();
            let right = self.parse_and()?;
            if left != "0" && !left.is_empty() {
                continue;
            }
            left = right;
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<String, String> {
        let mut left = self.parse_comparison()?;
        while self.current() == Some("&") {
            self.advance();
            let right = self.parse_comparison()?;
            if left == "0" || left.is_empty() || right == "0" || right.is_empty() {
                left = "0".to_string();
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<String, String> {
        let mut left = self.parse_add_sub()?;
        while let Some(op) = self.current() {
            if !["=", "!=", "<", ">", "<=", ">="].contains(&op) {
                break;
            }
            let op = op.to_string();
            self.advance();
            let right = self.parse_add_sub()?;

            let left_num: Result<i64, _> = left.parse();
            let right_num: Result<i64, _> = right.parse();
            let is_numeric = left_num.is_ok() && right_num.is_ok();

            let result = if is_numeric {
                let l = left_num.unwrap();
                let r = right_num.unwrap();
                match op.as_str() {
                    "=" => l == r,
                    "!=" => l != r,
                    "<" => l < r,
                    ">" => l > r,
                    "<=" => l <= r,
                    ">=" => l >= r,
                    _ => false,
                }
            } else {
                match op.as_str() {
                    "=" => left == right,
                    "!=" => left != right,
                    "<" => left < right,
                    ">" => left > right,
                    "<=" => left <= right,
                    ">=" => left >= right,
                    _ => false,
                }
            };
            left = if result { "1".to_string() } else { "0".to_string() };
        }
        Ok(left)
    }

    fn parse_add_sub(&mut self) -> Result<String, String> {
        let mut left = self.parse_mul_div()?;
        while let Some(op) = self.current() {
            if op != "+" && op != "-" {
                break;
            }
            let op = op.to_string();
            self.advance();
            let right = self.parse_mul_div()?;
            let l: i64 = left.parse().map_err(|_| "non-integer argument")?;
            let r: i64 = right.parse().map_err(|_| "non-integer argument")?;
            left = if op == "+" {
                (l + r).to_string()
            } else {
                (l - r).to_string()
            };
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<String, String> {
        let mut left = self.parse_match()?;
        while let Some(op) = self.current() {
            if op != "*" && op != "/" && op != "%" {
                break;
            }
            let op = op.to_string();
            self.advance();
            let right = self.parse_match()?;
            let l: i64 = left.parse().map_err(|_| "non-integer argument")?;
            let r: i64 = right.parse().map_err(|_| "non-integer argument")?;
            if (op == "/" || op == "%") && r == 0 {
                return Err("division by zero".to_string());
            }
            left = match op.as_str() {
                "*" => (l * r).to_string(),
                "/" => (l / r).to_string(),
                "%" => (l % r).to_string(),
                _ => left,
            };
        }
        Ok(left)
    }

    fn parse_match(&mut self) -> Result<String, String> {
        let mut left = self.parse_primary()?;
        while self.current() == Some(":") {
            self.advance();
            let pattern = self.parse_primary()?;
            let regex_pattern = format!("^{}", pattern);
            match Regex::new(&regex_pattern) {
                Ok(re) => {
                    if let Some(caps) = re.captures(&left) {
                        if caps.len() > 1 {
                            left = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                        } else {
                            left = caps.get(0).map(|m| m.as_str().len().to_string()).unwrap_or("0".to_string());
                        }
                    } else {
                        left = "0".to_string();
                    }
                }
                Err(_) => left = "0".to_string(),
            }
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<String, String> {
        let token = self.current().ok_or("syntax error")?;

        match token {
            "match" => {
                self.advance();
                let s = self.parse_primary()?;
                let pattern = self.parse_primary()?;
                match Regex::new(&pattern) {
                    Ok(re) => {
                        if let Some(caps) = re.captures(&s) {
                            if caps.len() > 1 {
                                Ok(caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default())
                            } else {
                                Ok(caps.get(0).map(|m| m.as_str().len().to_string()).unwrap_or("0".to_string()))
                            }
                        } else {
                            Ok("0".to_string())
                        }
                    }
                    Err(_) => Ok("0".to_string()),
                }
            }
            "substr" => {
                self.advance();
                let s = self.parse_primary()?;
                let pos: usize = self.parse_primary()?.parse().map_err(|_| "non-integer argument")?;
                let len: usize = self.parse_primary()?.parse().map_err(|_| "non-integer argument")?;
                let start = pos.saturating_sub(1);
                Ok(s.chars().skip(start).take(len).collect())
            }
            "index" => {
                self.advance();
                let s = self.parse_primary()?;
                let chars = self.parse_primary()?;
                for (i, c) in s.chars().enumerate() {
                    if chars.contains(c) {
                        return Ok((i + 1).to_string());
                    }
                }
                Ok("0".to_string())
            }
            "length" => {
                self.advance();
                let s = self.parse_primary()?;
                Ok(s.len().to_string())
            }
            "(" => {
                self.advance();
                let result = self.parse_or()?;
                if self.current() != Some(")") {
                    return Err("syntax error".to_string());
                }
                self.advance();
                Ok(result)
            }
            _ => {
                let val = token.to_string();
                self.advance();
                Ok(val)
            }
        }
    }
}
