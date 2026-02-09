use super::value::Value;

/// Token types for the jq lexer
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Operators
    Dot,
    Pipe,
    Comma,
    Colon,
    Semicolon,
    Question,
    DotDot,
    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    And,
    Or,
    Not,
    Alt, // //
    // Assignment
    Assign,
    UpdatePipe,
    UpdateAdd,
    UpdateSub,
    UpdateMul,
    UpdateDiv,
    UpdateMod,
    UpdateAlt,
    // Keywords
    If,
    Then,
    Elif,
    Else,
    End,
    As,
    Try,
    Catch,
    True,
    False,
    Null,
    Reduce,
    Foreach,
    Label,
    Break,
    Def,
    // Literals
    Ident(String),
    Number(f64),
    Str(String),
    // Special
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub pos: usize,
}

/// AST nodes for the jq query language
#[derive(Debug, Clone)]
pub enum AstNode {
    Identity,
    Field {
        name: String,
        base: Option<Box<AstNode>>,
    },
    Index {
        base: Option<Box<AstNode>>,
        index: Box<AstNode>,
    },
    Slice {
        base: Option<Box<AstNode>>,
        start: Option<Box<AstNode>>,
        end: Option<Box<AstNode>>,
    },
    Iterate {
        base: Option<Box<AstNode>>,
    },
    Recurse,
    Pipe {
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    Comma {
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    Literal {
        value: Value,
    },
    Array {
        elements: Option<Box<AstNode>>,
    },
    Object {
        entries: Vec<ObjectEntry>,
    },
    Paren {
        expr: Box<AstNode>,
    },
    BinaryOp {
        op: BinaryOp,
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<AstNode>,
    },
    Cond {
        cond: Box<AstNode>,
        then_branch: Box<AstNode>,
        elif_branches: Vec<(AstNode, AstNode)>,
        else_branch: Option<Box<AstNode>>,
    },
    Try {
        body: Box<AstNode>,
        catch: Option<Box<AstNode>>,
    },
    Optional {
        expr: Box<AstNode>,
    },
    Call {
        name: String,
        args: Vec<AstNode>,
    },
    VarRef {
        name: String,
    },
    VarBind {
        name: String,
        value: Box<AstNode>,
        body: Box<AstNode>,
        pattern: Option<DestructurePattern>,
        alternatives: Option<Vec<DestructurePattern>>,
    },
    Def {
        name: String,
        params: Vec<String>,
        func_body: Box<AstNode>,
        body: Box<AstNode>,
    },
    StringInterp {
        parts: Vec<StringPart>,
    },
    UpdateOp {
        op: UpdateOp,
        path: Box<AstNode>,
        value: Box<AstNode>,
    },
    Reduce {
        expr: Box<AstNode>,
        var_name: String,
        pattern: Option<DestructurePattern>,
        init: Box<AstNode>,
        update: Box<AstNode>,
    },
    Foreach {
        expr: Box<AstNode>,
        var_name: String,
        pattern: Option<DestructurePattern>,
        init: Box<AstNode>,
        update: Box<AstNode>,
        extract: Option<Box<AstNode>>,
    },
    Label {
        name: String,
        body: Box<AstNode>,
    },
    Break {
        name: String,
    },
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Expr(AstNode),
}

#[derive(Debug, Clone)]
pub enum ObjectEntry {
    KeyValue { key: ObjectKey, value: AstNode },
}

#[derive(Debug, Clone)]
pub enum ObjectKey {
    Ident(String),
    Expr(AstNode),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Alt,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateOp {
    Assign,     // =
    PipeUpdate, // |=
    AddUpdate,  // +=
    SubUpdate,  // -=
    MulUpdate,  // *=
    DivUpdate,  // /=
    ModUpdate,  // %=
    AltUpdate,  // //=
}

#[derive(Debug, Clone)]
pub enum DestructurePattern {
    Var { name: String },
    Array { elements: Vec<DestructurePattern> },
    Object { fields: Vec<PatternField> },
}

#[derive(Debug, Clone)]
pub struct PatternField {
    pub key: PatternKey,
    pub pattern: DestructurePattern,
    pub key_var: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PatternKey {
    Ident(String),
    Expr(AstNode),
}
