use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum Expression {
    Name { pattern: String, case_insensitive: bool },
    Path { pattern: String, case_insensitive: bool },
    Regex { pattern: String, case_insensitive: bool },
    Type(FileType),
    Empty,
    Mtime { days: i64, comparison: Comparison },
    Newer { reference_path: String },
    Size { value: i64, unit: SizeUnit, comparison: Comparison },
    Perm { mode: u32, match_type: PermMatch },
    Prune,
    Print,
    Print0,
    Printf { format: String },
    Delete,
    Exec { command: Vec<String>, batch: bool }, // batch = true for {} +, false for {} ;
    Not(Box<Expression>),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comparison {
    Exact,
    GreaterThan,
    LessThan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeUnit {
    Bytes,      // c
    Kilobytes,  // k
    Megabytes,  // M
    Gigabytes,  // G
    Blocks,     // b (512-byte blocks, default)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermMatch {
    Exact,    // 755
    AllBits,  // -755
    AnyBits,  // /755
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileType {
    File,      // f
    Directory, // d
    Symlink,   // l
}

#[derive(Debug, Clone)]
pub struct EvalContext {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mode: u32,
    pub mtime: SystemTime,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub matches: bool,
    pub pruned: bool,
    pub printed: bool,
    pub output: String,
}

pub struct FindOptions {
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
    pub depth_first: bool,  // -depth flag
}
