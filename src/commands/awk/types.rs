/// AWK Abstract Syntax Tree Types
///
/// Defines all token types, AST nodes, and program structures
/// for the AWK interpreter.

// ─── Token Types ──────────────────────────────────────────

/// All token types recognized by the AWK lexer
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Literals
    Number,
    String,
    Regex,
    Ident,

    // Keywords
    Begin,
    End,
    If,
    Else,
    While,
    Do,
    For,
    In,
    Break,
    Continue,
    Next,
    NextFile,
    Exit,
    Return,
    Delete,
    Function,
    Print,
    Printf,
    Getline,

    // Arithmetic operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,

    // Comparison operators
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,

    // Regex match operators
    Match,      // ~
    NotMatch,   // !~

    // Logical operators
    And,        // &&
    Or,         // ||
    Not,        // !

    // Assignment operators
    Assign,         // =
    PlusAssign,     // +=
    MinusAssign,    // -=
    StarAssign,     // *=
    SlashAssign,    // /=
    PercentAssign,  // %=
    CaretAssign,    // ^=

    // Increment / Decrement
    Increment,  // ++
    Decrement,  // --

    // Ternary
    Question,   // ?
    Colon,      // :

    // Punctuation
    Comma,
    Semicolon,
    Newline,

    // Brackets
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    // Special
    Dollar,     // $
    Append,     // >>
    Pipe,       // |
    Eof,
}

/// A single token produced by the lexer
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    pub line: usize,
    pub column: usize,
}

// ─── Operator Types ───────────────────────────────────────

/// Binary operators for AWK expressions
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    MatchOp,
    NotMatchOp,
    And,
    Or,
}

/// Unary operators for AWK expressions
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
}

/// Assignment operators for AWK expressions
#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    PowAssign,
}

// ─── Expressions ──────────────────────────────────────────

/// AWK expression AST nodes
#[derive(Debug, Clone)]
pub enum AwkExpr {
    NumberLiteral(f64),
    StringLiteral(String),
    RegexLiteral(String),
    FieldRef(Box<AwkExpr>),
    Variable(String),
    ArrayAccess {
        array: String,
        key: Box<AwkExpr>,
    },
    BinaryOp {
        operator: BinaryOp,
        left: Box<AwkExpr>,
        right: Box<AwkExpr>,
    },
    UnaryOp {
        operator: UnaryOp,
        operand: Box<AwkExpr>,
    },
    PreIncrement(Box<AwkExpr>),
    PreDecrement(Box<AwkExpr>),
    PostIncrement(Box<AwkExpr>),
    PostDecrement(Box<AwkExpr>),
    Ternary {
        condition: Box<AwkExpr>,
        consequent: Box<AwkExpr>,
        alternate: Box<AwkExpr>,
    },
    FunctionCall {
        name: String,
        args: Vec<AwkExpr>,
    },
    Assignment {
        operator: AssignOp,
        target: Box<AwkExpr>,
        value: Box<AwkExpr>,
    },
    InExpr {
        key: Box<AwkExpr>,
        array: String,
    },
    Getline {
        variable: Option<String>,
        file: Option<Box<AwkExpr>>,
        command: Option<Box<AwkExpr>>,
    },
    Tuple(Vec<AwkExpr>),
    Concatenation {
        left: Box<AwkExpr>,
        right: Box<AwkExpr>,
    },
}

// ─── Redirect Types ───────────────────────────────────────

/// Output redirection type for print/printf statements
#[derive(Debug, Clone, PartialEq)]
pub enum RedirectType {
    Write,   // >
    Append,  // >>
    Pipe,    // |
}

/// Output redirection info for print/printf statements
#[derive(Debug, Clone)]
pub struct RedirectInfo {
    pub redirect_type: RedirectType,
    pub target: AwkExpr,
}

// ─── Statements ───────────────────────────────────────────

/// AWK statement AST nodes
#[derive(Debug, Clone)]
pub enum AwkStmt {
    ExprStmt(AwkExpr),
    Print {
        args: Vec<AwkExpr>,
        output: Option<RedirectInfo>,
    },
    Printf {
        format: AwkExpr,
        args: Vec<AwkExpr>,
        output: Option<RedirectInfo>,
    },
    If {
        condition: AwkExpr,
        consequent: Box<AwkStmt>,
        alternate: Option<Box<AwkStmt>>,
    },
    While {
        condition: AwkExpr,
        body: Box<AwkStmt>,
    },
    DoWhile {
        body: Box<AwkStmt>,
        condition: AwkExpr,
    },
    For {
        init: Option<Box<AwkStmt>>,
        condition: Option<AwkExpr>,
        update: Option<Box<AwkStmt>>,
        body: Box<AwkStmt>,
    },
    ForIn {
        variable: String,
        array: String,
        body: Box<AwkStmt>,
    },
    Block(Vec<AwkStmt>),
    Break,
    Continue,
    Next,
    NextFile,
    Exit(Option<AwkExpr>),
    Return(Option<AwkExpr>),
    Delete {
        target: AwkExpr,
    },
}

// ─── Program Structure ────────────────────────────────────

/// AWK pattern types for rule matching
#[derive(Debug, Clone)]
pub enum AwkPattern {
    Begin,
    End,
    Expression(AwkExpr),
    Regex(String),
    Range {
        start: Box<AwkPattern>,
        end: Box<AwkPattern>,
    },
}

/// A single AWK rule (pattern-action pair)
#[derive(Debug, Clone)]
pub struct AwkRule {
    pub pattern: Option<AwkPattern>,
    pub action: Vec<AwkStmt>,
}

/// A user-defined AWK function
#[derive(Debug, Clone)]
pub struct AwkFunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<AwkStmt>,
}

/// The top-level AWK program structure
#[derive(Debug, Clone)]
pub struct AwkProgram {
    pub functions: Vec<AwkFunctionDef>,
    pub rules: Vec<AwkRule>,
}
