use std::collections::{HashMap, HashSet};
use super::value::Value;
use super::ast::AstNode;

pub const DEFAULT_MAX_ITERATIONS: usize = 10_000;
pub const DEFAULT_MAX_DEPTH: usize = 2_000;

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub params: Vec<String>,
    pub body: AstNode,
    pub closure: Option<HashMap<String, FunctionDef>>,
}

#[derive(Clone)]
pub struct EvalContext {
    pub vars: HashMap<String, Value>,
    pub funcs: HashMap<String, FunctionDef>,
    pub env: HashMap<String, String>,
    pub root: Option<Value>,
    pub current_path: Vec<PathElement>,
    pub labels: HashSet<String>,
    pub max_iterations: usize,
    pub max_depth: usize,
    pub iteration_count: usize,
}

#[derive(Debug, Clone)]
pub enum PathElement {
    Key(String),
    Index(i64),
}

impl EvalContext {
    pub fn new() -> Self {
        EvalContext {
            vars: HashMap::new(),
            funcs: HashMap::new(),
            env: HashMap::new(),
            root: None,
            current_path: Vec::new(),
            labels: HashSet::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_depth: DEFAULT_MAX_DEPTH,
            iteration_count: 0,
        }
    }

    pub fn with_env(env: HashMap<String, String>) -> Self {
        let mut ctx = Self::new();
        ctx.env = env;
        ctx
    }

    pub fn with_var(&self, name: &str, value: Value) -> Self {
        let mut ctx = self.clone();
        ctx.vars.insert(name.to_string(), value);
        ctx
    }

    pub fn with_func(&self, key: &str, def: FunctionDef) -> Self {
        let mut ctx = self.clone();
        ctx.funcs.insert(key.to_string(), def);
        ctx
    }
}

impl Default for EvalContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Error types for the jq query engine
#[derive(Debug)]
pub enum JqError {
    Type(String),
    Runtime(String),
    Parse(String),
    Break { name: String, results: Vec<Value> },
    ExecutionLimit(String),
    Value(Value),
}

impl std::fmt::Display for JqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JqError::Type(msg) => write!(f, "Type error: {}", msg),
            JqError::Runtime(msg) => write!(f, "Runtime error: {}", msg),
            JqError::Parse(msg) => write!(f, "Parse error: {}", msg),
            JqError::Break { name, .. } => write!(f, "Break: {}", name),
            JqError::ExecutionLimit(msg) => {
                write!(f, "Execution limit: {}", msg)
            }
            JqError::Value(v) => write!(f, "{}", v),
        }
    }
}

impl std::error::Error for JqError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_context_new() {
        let ctx = EvalContext::new();
        assert!(ctx.vars.is_empty());
        assert!(ctx.funcs.is_empty());
        assert!(ctx.root.is_none());
        assert_eq!(ctx.max_iterations, DEFAULT_MAX_ITERATIONS);
        assert_eq!(ctx.max_depth, DEFAULT_MAX_DEPTH);
    }

    #[test]
    fn test_eval_context_with_env() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/user".to_string());
        let ctx = EvalContext::with_env(env);
        assert_eq!(ctx.env.get("HOME"), Some(&"/home/user".to_string()));
    }

    #[test]
    fn test_eval_context_with_var() {
        let ctx = EvalContext::new();
        let ctx2 = ctx.with_var("x", Value::Number(42.0));
        assert!(ctx.vars.is_empty());
        assert_eq!(ctx2.vars.get("x"), Some(&Value::Number(42.0)));
    }

    #[test]
    fn test_eval_context_default() {
        let ctx: EvalContext = Default::default();
        assert!(ctx.vars.is_empty());
    }

    #[test]
    fn test_jq_error_display() {
        let e1 = JqError::Type("invalid type".to_string());
        assert!(format!("{}", e1).contains("Type error"));

        let e2 = JqError::Runtime("runtime issue".to_string());
        assert!(format!("{}", e2).contains("Runtime error"));

        let e3 = JqError::Parse("parse failed".to_string());
        assert!(format!("{}", e3).contains("Parse error"));

        let e4 = JqError::ExecutionLimit("too many iterations".to_string());
        assert!(format!("{}", e4).contains("Execution limit"));
    }

    #[test]
    fn test_path_element() {
        let key = PathElement::Key("name".to_string());
        let idx = PathElement::Index(0);
        assert!(matches!(key, PathElement::Key(_)));
        assert!(matches!(idx, PathElement::Index(0)));
    }
}
