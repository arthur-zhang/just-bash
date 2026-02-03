//! Abstract Syntax Tree (AST) Types for Bash
//!
//! This module defines the complete AST structure for bash scripts.
//! The design follows the actual bash grammar while being Rust-idiomatic.

use std::fmt;

// =============================================================================
// BASE TYPES
// =============================================================================

/// Position information for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

/// Span in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

// =============================================================================
// SCRIPT & STATEMENTS
// =============================================================================

/// Root node: a complete script
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptNode {
    pub statements: Vec<StatementNode>,
}

/// A statement is a list of pipelines connected by && or ||
#[derive(Debug, Clone, PartialEq)]
pub struct StatementNode {
    pub pipelines: Vec<PipelineNode>,
    /// Operators between pipelines: "&&" | "||" | ";"
    pub operators: Vec<StatementOperator>,
    /// Run in background?
    pub background: bool,
    /// Deferred syntax error
    pub deferred_error: Option<DeferredError>,
    /// Original source text for verbose mode (set -v)
    pub source_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementOperator {
    And,    // &&
    Or,     // ||
    Semi,   // ;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredError {
    pub message: String,
    pub token: String,
}

// =============================================================================
// PIPELINES & COMMANDS
// =============================================================================

/// A pipeline: cmd1 | cmd2 | cmd3
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineNode {
    pub commands: Vec<CommandNode>,
    /// Negate exit status with !
    pub negated: bool,
    /// Time the pipeline with 'time' keyword
    pub timed: bool,
    /// Use POSIX format for time output (-p flag)
    pub time_posix: bool,
    /// For each pipe, whether it's |& (pipe stderr too)
    pub pipe_stderr: Option<Vec<bool>>,
}

/// Union of all command types
#[derive(Debug, Clone, PartialEq)]
pub enum CommandNode {
    Simple(SimpleCommandNode),
    Compound(CompoundCommandNode),
    FunctionDef(FunctionDefNode),
}

/// Simple command: name args... with optional redirections
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleCommandNode {
    /// Variable assignments before command: VAR=value cmd
    pub assignments: Vec<AssignmentNode>,
    /// Command name (may be None for assignment-only)
    pub name: Option<WordNode>,
    /// Command arguments
    pub args: Vec<WordNode>,
    /// I/O redirections
    pub redirections: Vec<RedirectionNode>,
    /// Source line number for $LINENO
    pub line: Option<usize>,
}

/// Compound commands: control structures
#[derive(Debug, Clone, PartialEq)]
pub enum CompoundCommandNode {
    If(IfNode),
    For(ForNode),
    CStyleFor(CStyleForNode),
    While(WhileNode),
    Until(UntilNode),
    Case(CaseNode),
    Subshell(SubshellNode),
    Group(GroupNode),
    ArithmeticCommand(ArithmeticCommandNode),
    ConditionalCommand(ConditionalCommandNode),
}

// =============================================================================
// CONTROL FLOW
// =============================================================================

/// if statement
#[derive(Debug, Clone, PartialEq)]
pub struct IfNode {
    pub clauses: Vec<IfClause>,
    pub else_body: Option<Vec<StatementNode>>,
    pub redirections: Vec<RedirectionNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfClause {
    pub condition: Vec<StatementNode>,
    pub body: Vec<StatementNode>,
}

/// for loop: for VAR in WORDS; do ...; done
#[derive(Debug, Clone, PartialEq)]
pub struct ForNode {
    pub variable: String,
    /// Words to iterate over (None = "$@")
    pub words: Option<Vec<WordNode>>,
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
}

/// C-style for loop: for ((init; cond; step)); do ...; done
#[derive(Debug, Clone, PartialEq)]
pub struct CStyleForNode {
    pub init: Option<ArithmeticExpressionNode>,
    pub condition: Option<ArithmeticExpressionNode>,
    pub update: Option<ArithmeticExpressionNode>,
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
    pub line: Option<usize>,
}

/// while loop
#[derive(Debug, Clone, PartialEq)]
pub struct WhileNode {
    pub condition: Vec<StatementNode>,
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
}

/// until loop
#[derive(Debug, Clone, PartialEq)]
pub struct UntilNode {
    pub condition: Vec<StatementNode>,
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
}

/// case statement
#[derive(Debug, Clone, PartialEq)]
pub struct CaseNode {
    pub word: WordNode,
    pub items: Vec<CaseItemNode>,
    pub redirections: Vec<RedirectionNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseItemNode {
    pub patterns: Vec<WordNode>,
    pub body: Vec<StatementNode>,
    /// Terminator: ";;" | ";&" | ";;&"
    pub terminator: CaseTerminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseTerminator {
    DoubleSemi,     // ;;
    SemiAnd,        // ;&
    SemiSemiAnd,    // ;;&
}

/// Subshell: ( ... )
#[derive(Debug, Clone, PartialEq)]
pub struct SubshellNode {
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
}

/// Command group: { ...; }
#[derive(Debug, Clone, PartialEq)]
pub struct GroupNode {
    pub body: Vec<StatementNode>,
    pub redirections: Vec<RedirectionNode>,
}

/// Arithmetic command: (( expr ))
#[derive(Debug, Clone, PartialEq)]
pub struct ArithmeticCommandNode {
    pub expression: ArithmeticExpressionNode,
    pub redirections: Vec<RedirectionNode>,
    pub line: Option<usize>,
}

/// Conditional command: [[ expr ]]
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalCommandNode {
    pub expression: ConditionalExpressionNode,
    pub redirections: Vec<RedirectionNode>,
    pub line: Option<usize>,
}

// =============================================================================
// FUNCTIONS
// =============================================================================

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDefNode {
    pub name: String,
    pub body: Box<CompoundCommandNode>,
    pub redirections: Vec<RedirectionNode>,
    /// Source file where the function was defined
    pub source_file: Option<String>,
}

// =============================================================================
// ASSIGNMENTS
// =============================================================================

/// Variable assignment: VAR=value or VAR+=value
#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentNode {
    pub name: String,
    pub value: Option<WordNode>,
    /// Append mode: VAR+=value
    pub append: bool,
    /// Array assignment: VAR=(a b c)
    pub array: Option<Vec<WordNode>>,
}

// =============================================================================
// REDIRECTIONS
// =============================================================================

/// I/O redirection
#[derive(Debug, Clone, PartialEq)]
pub struct RedirectionNode {
    /// File descriptor (default depends on operator)
    pub fd: Option<i32>,
    /// Variable name for automatic FD allocation
    pub fd_variable: Option<String>,
    pub operator: RedirectionOperator,
    pub target: RedirectionTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirectionTarget {
    Word(WordNode),
    HereDoc(HereDocNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectionOperator {
    Less,           // <
    Great,          // >
    DGreat,         // >>
    GreatAnd,       // >&
    LessAnd,        // <&
    LessGreat,      // <>
    Clobber,        // >|
    AndGreat,       // &>
    AndDGreat,      // &>>
    TLess,          // <<<
    DLess,          // <<
    DLessDash,      // <<-
}

impl fmt::Display for RedirectionOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Less => write!(f, "<"),
            Self::Great => write!(f, ">"),
            Self::DGreat => write!(f, ">>"),
            Self::GreatAnd => write!(f, ">&"),
            Self::LessAnd => write!(f, "<&"),
            Self::LessGreat => write!(f, "<>"),
            Self::Clobber => write!(f, ">|"),
            Self::AndGreat => write!(f, "&>"),
            Self::AndDGreat => write!(f, "&>>"),
            Self::TLess => write!(f, "<<<"),
            Self::DLess => write!(f, "<<"),
            Self::DLessDash => write!(f, "<<-"),
        }
    }
}

/// Here document
#[derive(Debug, Clone, PartialEq)]
pub struct HereDocNode {
    pub delimiter: String,
    pub content: WordNode,
    /// Strip leading tabs (<<- vs <<)
    pub strip_tabs: bool,
    /// Quoted delimiter means no expansion
    pub quoted: bool,
}

// =============================================================================
// WORDS (the heart of shell parsing)
// =============================================================================

/// A Word is a sequence of parts that form a single shell word.
#[derive(Debug, Clone, PartialEq)]
pub struct WordNode {
    pub parts: Vec<WordPart>,
}

/// Parts that can make up a word
#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    Literal(LiteralPart),
    SingleQuoted(SingleQuotedPart),
    DoubleQuoted(DoubleQuotedPart),
    Escaped(EscapedPart),
    ParameterExpansion(ParameterExpansionPart),
    CommandSubstitution(CommandSubstitutionPart),
    ArithmeticExpansion(ArithmeticExpansionPart),
    ProcessSubstitution(ProcessSubstitutionPart),
    BraceExpansion(BraceExpansionPart),
    TildeExpansion(TildeExpansionPart),
    Glob(GlobPart),
}

/// Literal text (no special meaning)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralPart {
    pub value: String,
}

/// Single-quoted string: 'literal'
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleQuotedPart {
    pub value: String,
}

/// Double-quoted string: "with $expansion"
#[derive(Debug, Clone, PartialEq)]
pub struct DoubleQuotedPart {
    pub parts: Vec<WordPart>,
}

/// Escaped character: \x
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapedPart {
    pub value: String,
}

// =============================================================================
// PARAMETER EXPANSION
// =============================================================================

/// Parameter/variable expansion: $VAR or ${VAR...}
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterExpansionPart {
    pub parameter: String,
    /// Expansion operation
    pub operation: Option<ParameterOperation>,
}

/// Operations that can be used as inner operations for indirection
#[derive(Debug, Clone, PartialEq)]
pub enum InnerParameterOperation {
    DefaultValue(DefaultValueOp),
    AssignDefault(AssignDefaultOp),
    ErrorIfUnset(ErrorIfUnsetOp),
    UseAlternative(UseAlternativeOp),
    Length(LengthOp),
    LengthSliceError(LengthSliceErrorOp),
    BadSubstitution(BadSubstitutionOp),
    Substring(SubstringOp),
    PatternRemoval(PatternRemovalOp),
    PatternReplacement(PatternReplacementOp),
    CaseModification(CaseModificationOp),
    Transform(TransformOp),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterOperation {
    Inner(InnerParameterOperation),
    Indirection(IndirectionOp),
    ArrayKeys(ArrayKeysOp),
    VarNamePrefix(VarNamePrefixOp),
}

/// ${#VAR:...} - invalid syntax
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthSliceErrorOp;

/// Bad substitution - parsed but errors at runtime
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadSubstitutionOp {
    pub text: String,
}

/// ${VAR:-default} or ${VAR-default}
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultValueOp {
    pub word: WordNode,
    pub check_empty: bool,
}

/// ${VAR:=default} or ${VAR=default}
#[derive(Debug, Clone, PartialEq)]
pub struct AssignDefaultOp {
    pub word: WordNode,
    pub check_empty: bool,
}

/// ${VAR:?error} or ${VAR?error}
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorIfUnsetOp {
    pub word: Option<WordNode>,
    pub check_empty: bool,
}

/// ${VAR:+alternative} or ${VAR+alternative}
#[derive(Debug, Clone, PartialEq)]
pub struct UseAlternativeOp {
    pub word: WordNode,
    pub check_empty: bool,
}

/// ${#VAR}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthOp;

/// ${VAR:offset} or ${VAR:offset:length}
#[derive(Debug, Clone, PartialEq)]
pub struct SubstringOp {
    pub offset: ArithmeticExpressionNode,
    pub length: Option<ArithmeticExpressionNode>,
}

/// ${VAR#pattern}, ${VAR##pattern}, ${VAR%pattern}, ${VAR%%pattern}
#[derive(Debug, Clone, PartialEq)]
pub struct PatternRemovalOp {
    pub pattern: WordNode,
    pub side: PatternRemovalSide,
    pub greedy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternRemovalSide {
    Prefix,
    Suffix,
}

/// ${VAR/pattern/replacement} or ${VAR//pattern/replacement}
#[derive(Debug, Clone, PartialEq)]
pub struct PatternReplacementOp {
    pub pattern: WordNode,
    pub replacement: Option<WordNode>,
    pub all: bool,
    pub anchor: Option<PatternAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternAnchor {
    Start,
    End,
}

/// ${VAR^}, ${VAR^^}, ${VAR,}, ${VAR,,}
#[derive(Debug, Clone, PartialEq)]
pub struct CaseModificationOp {
    pub direction: CaseDirection,
    pub all: bool,
    pub pattern: Option<WordNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseDirection {
    Upper,
    Lower,
}

/// ${var@Q}, ${var@P}, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformOp {
    pub operator: TransformOperator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformOperator {
    Q, P, A, LowerA, E, K, LowerK, LowerU, U, L,
}

/// ${!VAR} - indirect expansion
#[derive(Debug, Clone, PartialEq)]
pub struct IndirectionOp {
    pub inner_op: Option<Box<InnerParameterOperation>>,
}

/// ${!arr[@]} or ${!arr[*]} - array keys/indices
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayKeysOp {
    pub array: String,
    pub star: bool,
}

/// ${!prefix*} or ${!prefix@} - list variable names with prefix
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarNamePrefixOp {
    pub prefix: String,
    pub star: bool,
}

// =============================================================================
// COMMAND SUBSTITUTION
// =============================================================================

/// Command substitution: $(cmd) or `cmd`
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSubstitutionPart {
    pub body: ScriptNode,
    /// Legacy backtick syntax
    pub legacy: bool,
}

// =============================================================================
// ARITHMETIC
// =============================================================================

/// Arithmetic expansion: $((expr))
#[derive(Debug, Clone, PartialEq)]
pub struct ArithmeticExpansionPart {
    pub expression: ArithmeticExpressionNode,
}

/// Arithmetic expression (for $((...)) and ((...)))
#[derive(Debug, Clone, PartialEq)]
pub struct ArithmeticExpressionNode {
    pub expression: ArithExpr,
    /// Original expression text before parsing
    pub original_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArithExpr {
    Number(ArithNumberNode),
    Variable(ArithVariableNode),
    SpecialVar(ArithSpecialVarNode),
    Binary(Box<ArithBinaryNode>),
    Unary(Box<ArithUnaryNode>),
    Ternary(Box<ArithTernaryNode>),
    Assignment(Box<ArithAssignmentNode>),
    DynamicAssignment(Box<ArithDynamicAssignmentNode>),
    DynamicElement(Box<ArithDynamicElementNode>),
    Group(Box<ArithGroupNode>),
    Nested(Box<ArithNestedNode>),
    CommandSubst(ArithCommandSubstNode),
    BracedExpansion(ArithBracedExpansionNode),
    ArrayElement(ArithArrayElementNode),
    DynamicBase(ArithDynamicBaseNode),
    DynamicNumber(ArithDynamicNumberNode),
    Concat(ArithConcatNode),
    DoubleSubscript(ArithDoubleSubscriptNode),
    NumberSubscript(ArithNumberSubscriptNode),
    SyntaxError(ArithSyntaxErrorNode),
    SingleQuote(ArithSingleQuoteNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithBracedExpansionNode {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithDynamicBaseNode {
    pub base_expr: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithDynamicNumberNode {
    pub prefix: String,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithConcatNode {
    pub parts: Vec<ArithExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithArrayElementNode {
    pub array: String,
    pub index: Option<Box<ArithExpr>>,
    pub string_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithDoubleSubscriptNode {
    pub array: String,
    pub index: Box<ArithExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithNumberSubscriptNode {
    pub number: String,
    pub error_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithSyntaxErrorNode {
    pub error_token: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithSingleQuoteNode {
    pub content: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithNumberNode {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithVariableNode {
    pub name: String,
    pub has_dollar_prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithSpecialVarNode {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithBinaryNode {
    pub operator: ArithBinaryOperator,
    pub left: ArithExpr,
    pub right: ArithExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithBinaryOperator {
    Add, Sub, Mul, Div, Mod, Pow,
    LShift, RShift,
    Lt, Le, Gt, Ge, Eq, Ne,
    BitAnd, BitOr, BitXor,
    LogAnd, LogOr,
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithUnaryNode {
    pub operator: ArithUnaryOperator,
    pub operand: ArithExpr,
    pub prefix: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithUnaryOperator {
    Neg, Pos, Not, BitNot, Inc, Dec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithTernaryNode {
    pub condition: ArithExpr,
    pub consequent: ArithExpr,
    pub alternate: ArithExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithAssignmentOperator {
    Assign, AddAssign, SubAssign, MulAssign, DivAssign, ModAssign,
    LShiftAssign, RShiftAssign, AndAssign, OrAssign, XorAssign,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithAssignmentNode {
    pub operator: ArithAssignmentOperator,
    pub variable: String,
    pub subscript: Option<Box<ArithExpr>>,
    pub string_key: Option<String>,
    pub value: ArithExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithDynamicAssignmentNode {
    pub operator: ArithAssignmentOperator,
    pub target: ArithExpr,
    pub subscript: Option<Box<ArithExpr>>,
    pub value: ArithExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithDynamicElementNode {
    pub name_expr: ArithExpr,
    pub subscript: Box<ArithExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithGroupNode {
    pub expression: ArithExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArithNestedNode {
    pub expression: ArithExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithCommandSubstNode {
    pub command: String,
}

// =============================================================================
// PROCESS SUBSTITUTION
// =============================================================================

/// Process substitution: <(cmd) or >(cmd)
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSubstitutionPart {
    pub body: ScriptNode,
    pub direction: ProcessDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessDirection {
    Input,  // <(...)
    Output, // >(...)
}

// =============================================================================
// BRACE & TILDE EXPANSION
// =============================================================================

/// Brace expansion: {a,b,c} or {1..10}
#[derive(Debug, Clone, PartialEq)]
pub struct BraceExpansionPart {
    pub items: Vec<BraceItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BraceItem {
    Word { word: WordNode },
    Range {
        start: BraceRangeValue,
        end: BraceRangeValue,
        step: Option<i64>,
        start_str: Option<String>,
        end_str: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BraceRangeValue {
    Number(i64),
    Char(char),
}

impl fmt::Display for BraceRangeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{}", n),
            Self::Char(c) => write!(f, "{}", c),
        }
    }
}

/// Tilde expansion: ~ or ~user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TildeExpansionPart {
    pub user: Option<String>,
}

// =============================================================================
// GLOB PATTERNS
// =============================================================================

/// Glob pattern part
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobPart {
    pub pattern: String,
}

// =============================================================================
// CONDITIONAL EXPRESSIONS (for [[ ]])
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum ConditionalExpressionNode {
    Binary(CondBinaryNode),
    Unary(CondUnaryNode),
    Not(Box<CondNotNode>),
    And(Box<CondAndNode>),
    Or(Box<CondOrNode>),
    Group(Box<CondGroupNode>),
    Word(CondWordNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondBinaryOperator {
    Eq,      // =
    EqEq,    // ==
    Ne,      // !=
    Match,   // =~
    Lt,      // <
    Gt,      // >
    NumEq,   // -eq
    NumNe,   // -ne
    NumLt,   // -lt
    NumLe,   // -le
    NumGt,   // -gt
    NumGe,   // -ge
    Nt,      // -nt
    Ot,      // -ot
    Ef,      // -ef
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondBinaryNode {
    pub operator: CondBinaryOperator,
    pub left: WordNode,
    pub right: WordNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondUnaryOperator {
    A, B, C, D, E, F, G, H, K, P, R, S, T, U, W, X,
    UpperG, L, N, UpperO, UpperS, Z, LowerN, LowerO, V, UpperR,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondUnaryNode {
    pub operator: CondUnaryOperator,
    pub operand: WordNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondNotNode {
    pub operand: ConditionalExpressionNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondAndNode {
    pub left: ConditionalExpressionNode,
    pub right: ConditionalExpressionNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondOrNode {
    pub left: ConditionalExpressionNode,
    pub right: ConditionalExpressionNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondGroupNode {
    pub expression: ConditionalExpressionNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CondWordNode {
    pub word: WordNode,
}

// =============================================================================
// FACTORY FUNCTIONS (AST builders)
// =============================================================================

/// AST factory for building nodes
pub struct AST;

impl AST {
    pub fn script(statements: Vec<StatementNode>) -> ScriptNode {
        ScriptNode { statements }
    }

    pub fn statement(
        pipelines: Vec<PipelineNode>,
        operators: Vec<StatementOperator>,
        background: bool,
        deferred_error: Option<DeferredError>,
        source_text: Option<String>,
    ) -> StatementNode {
        StatementNode {
            pipelines,
            operators,
            background,
            deferred_error,
            source_text,
        }
    }

    pub fn pipeline(
        commands: Vec<CommandNode>,
        negated: bool,
        timed: bool,
        time_posix: bool,
        pipe_stderr: Option<Vec<bool>>,
    ) -> PipelineNode {
        PipelineNode {
            commands,
            negated,
            timed,
            time_posix,
            pipe_stderr,
        }
    }

    pub fn simple_command(
        name: Option<WordNode>,
        args: Vec<WordNode>,
        assignments: Vec<AssignmentNode>,
        redirections: Vec<RedirectionNode>,
    ) -> SimpleCommandNode {
        SimpleCommandNode {
            name,
            args,
            assignments,
            redirections,
            line: None,
        }
    }

    pub fn word(parts: Vec<WordPart>) -> WordNode {
        WordNode { parts }
    }

    pub fn literal(value: impl Into<String>) -> WordPart {
        WordPart::Literal(LiteralPart { value: value.into() })
    }

    pub fn single_quoted(value: impl Into<String>) -> WordPart {
        WordPart::SingleQuoted(SingleQuotedPart { value: value.into() })
    }

    pub fn double_quoted(parts: Vec<WordPart>) -> WordPart {
        WordPart::DoubleQuoted(DoubleQuotedPart { parts })
    }

    pub fn escaped(value: impl Into<String>) -> WordPart {
        WordPart::Escaped(EscapedPart { value: value.into() })
    }

    pub fn parameter_expansion(
        parameter: impl Into<String>,
        operation: Option<ParameterOperation>,
    ) -> WordPart {
        WordPart::ParameterExpansion(ParameterExpansionPart {
            parameter: parameter.into(),
            operation,
        })
    }

    pub fn command_substitution(body: ScriptNode, legacy: bool) -> WordPart {
        WordPart::CommandSubstitution(CommandSubstitutionPart { body, legacy })
    }

    pub fn arithmetic_expansion(expression: ArithmeticExpressionNode) -> WordPart {
        WordPart::ArithmeticExpansion(ArithmeticExpansionPart { expression })
    }

    pub fn assignment(
        name: impl Into<String>,
        value: Option<WordNode>,
        append: bool,
        array: Option<Vec<WordNode>>,
    ) -> AssignmentNode {
        AssignmentNode {
            name: name.into(),
            value,
            append,
            array,
        }
    }

    pub fn redirection(
        operator: RedirectionOperator,
        target: RedirectionTarget,
        fd: Option<i32>,
        fd_variable: Option<String>,
    ) -> RedirectionNode {
        RedirectionNode {
            fd,
            fd_variable,
            operator,
            target,
        }
    }

    pub fn here_doc(
        delimiter: impl Into<String>,
        content: WordNode,
        strip_tabs: bool,
        quoted: bool,
    ) -> HereDocNode {
        HereDocNode {
            delimiter: delimiter.into(),
            content,
            strip_tabs,
            quoted,
        }
    }

    pub fn if_node(
        clauses: Vec<IfClause>,
        else_body: Option<Vec<StatementNode>>,
        redirections: Vec<RedirectionNode>,
    ) -> IfNode {
        IfNode {
            clauses,
            else_body,
            redirections,
        }
    }

    pub fn for_node(
        variable: impl Into<String>,
        words: Option<Vec<WordNode>>,
        body: Vec<StatementNode>,
        redirections: Vec<RedirectionNode>,
    ) -> ForNode {
        ForNode {
            variable: variable.into(),
            words,
            body,
            redirections,
        }
    }

    pub fn while_node(
        condition: Vec<StatementNode>,
        body: Vec<StatementNode>,
        redirections: Vec<RedirectionNode>,
    ) -> WhileNode {
        WhileNode {
            condition,
            body,
            redirections,
        }
    }

    pub fn until_node(
        condition: Vec<StatementNode>,
        body: Vec<StatementNode>,
        redirections: Vec<RedirectionNode>,
    ) -> UntilNode {
        UntilNode {
            condition,
            body,
            redirections,
        }
    }

    pub fn case_node(
        word: WordNode,
        items: Vec<CaseItemNode>,
        redirections: Vec<RedirectionNode>,
    ) -> CaseNode {
        CaseNode {
            word,
            items,
            redirections,
        }
    }

    pub fn case_item(
        patterns: Vec<WordNode>,
        body: Vec<StatementNode>,
        terminator: CaseTerminator,
    ) -> CaseItemNode {
        CaseItemNode {
            patterns,
            body,
            terminator,
        }
    }

    pub fn subshell(
        body: Vec<StatementNode>,
        redirections: Vec<RedirectionNode>,
    ) -> SubshellNode {
        SubshellNode { body, redirections }
    }

    pub fn group(
        body: Vec<StatementNode>,
        redirections: Vec<RedirectionNode>,
    ) -> GroupNode {
        GroupNode { body, redirections }
    }

    pub fn function_def(
        name: impl Into<String>,
        body: CompoundCommandNode,
        redirections: Vec<RedirectionNode>,
        source_file: Option<String>,
    ) -> FunctionDefNode {
        FunctionDefNode {
            name: name.into(),
            body: Box::new(body),
            redirections,
            source_file,
        }
    }

    pub fn conditional_command(
        expression: ConditionalExpressionNode,
        redirections: Vec<RedirectionNode>,
        line: Option<usize>,
    ) -> ConditionalCommandNode {
        ConditionalCommandNode {
            expression,
            redirections,
            line,
        }
    }

    pub fn arithmetic_command(
        expression: ArithmeticExpressionNode,
        redirections: Vec<RedirectionNode>,
        line: Option<usize>,
    ) -> ArithmeticCommandNode {
        ArithmeticCommandNode {
            expression,
            redirections,
            line,
        }
    }
}
