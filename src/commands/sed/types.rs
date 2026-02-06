use std::collections::HashMap;

/// Address types for sed commands
#[derive(Debug, Clone, PartialEq)]
pub enum SedAddress {
    Line(usize),                        // Line number (1-indexed)
    Last,                               // $ - last line
    Pattern(String),                    // /regex/
    Step { first: usize, step: usize }, // first~step (e.g., 0~2 for even lines)
    RelativeOffset(usize),              // +N (GNU extension)
}

/// Address range with optional negation
#[derive(Debug, Clone, Default)]
pub struct AddressRange {
    pub start: Option<SedAddress>,
    pub end: Option<SedAddress>,
    pub negated: bool,
}

/// All sed commands as an enum
#[derive(Debug, Clone)]
pub enum SedCmd {
    Substitute {
        address: Option<AddressRange>,
        pattern: String,
        replacement: String,
        global: bool,
        ignore_case: bool,
        print_on_match: bool,
        nth_occurrence: Option<usize>,
        extended_regex: bool,
    },
    Print { address: Option<AddressRange> },
    PrintFirstLine { address: Option<AddressRange> },  // P
    Delete { address: Option<AddressRange> },
    DeleteFirstLine { address: Option<AddressRange> }, // D
    Append { address: Option<AddressRange>, text: String },
    Insert { address: Option<AddressRange>, text: String },
    Change { address: Option<AddressRange>, text: String },
    Hold { address: Option<AddressRange> },            // h
    HoldAppend { address: Option<AddressRange> },      // H
    Get { address: Option<AddressRange> },             // g
    GetAppend { address: Option<AddressRange> },       // G
    Exchange { address: Option<AddressRange> },        // x
    Next { address: Option<AddressRange> },            // n
    NextAppend { address: Option<AddressRange> },      // N
    Quit { address: Option<AddressRange> },            // q
    QuitSilent { address: Option<AddressRange> },      // Q
    Transliterate {
        address: Option<AddressRange>,
        source: String,
        dest: String,
    },
    LineNumber { address: Option<AddressRange> },      // =
    Branch { address: Option<AddressRange>, label: Option<String> },      // b
    BranchOnSubst { address: Option<AddressRange>, label: Option<String> }, // t
    BranchOnNoSubst { address: Option<AddressRange>, label: Option<String> }, // T
    Label { name: String },                            // :label
    Zap { address: Option<AddressRange> },             // z
    Group { address: Option<AddressRange>, commands: Vec<SedCmd> },
    List { address: Option<AddressRange> },            // l
    PrintFilename { address: Option<AddressRange> },   // F
    Version { address: Option<AddressRange>, min_version: Option<String> }, // v
    ReadFile { address: Option<AddressRange>, filename: String },    // r
    ReadFileLine { address: Option<AddressRange>, filename: String }, // R
    WriteFile { address: Option<AddressRange>, filename: String },   // w
    WriteFirstLine { address: Option<AddressRange>, filename: String }, // W
}

/// Range state for pattern ranges like /start/,/end/
#[derive(Debug, Clone, Default)]
pub struct RangeState {
    pub active: bool,
    pub start_line: Option<usize>,
    pub completed: bool,
}

/// Pending file read operation
#[derive(Debug, Clone)]
pub struct PendingFileRead {
    pub filename: String,
    pub whole_file: bool,
}

/// Pending file write operation
#[derive(Debug, Clone)]
pub struct PendingFileWrite {
    pub filename: String,
    pub content: String,
}

/// Execution state for sed
#[derive(Debug)]
pub struct SedState {
    pub pattern_space: String,
    pub hold_space: String,
    pub line_number: usize,
    pub total_lines: usize,
    pub deleted: bool,
    pub printed: bool,
    pub quit: bool,
    pub quit_silent: bool,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub append_buffer: Vec<String>,
    pub changed_text: Option<String>,
    pub substitution_made: bool,
    pub line_number_output: Vec<String>,
    pub n_command_output: Vec<String>,
    pub restart_cycle: bool,
    pub in_d_restarted_cycle: bool,
    pub current_filename: Option<String>,
    pub pending_file_reads: Vec<PendingFileRead>,
    pub pending_file_writes: Vec<PendingFileWrite>,
    pub range_states: HashMap<String, RangeState>,
    pub last_pattern: Option<String>,
    pub branch_request: Option<String>,
    pub lines_consumed_in_cycle: usize,
}

impl SedState {
    pub fn new(total_lines: usize) -> Self {
        Self {
            pattern_space: String::new(),
            hold_space: String::new(),
            line_number: 0,
            total_lines,
            deleted: false,
            printed: false,
            quit: false,
            quit_silent: false,
            exit_code: None,
            error_message: None,
            append_buffer: Vec::new(),
            changed_text: None,
            substitution_made: false,
            line_number_output: Vec::new(),
            n_command_output: Vec::new(),
            restart_cycle: false,
            in_d_restarted_cycle: false,
            current_filename: None,
            pending_file_reads: Vec::new(),
            pending_file_writes: Vec::new(),
            range_states: HashMap::new(),
            last_pattern: None,
            branch_request: None,
            lines_consumed_in_cycle: 0,
        }
    }
}

/// Execution context for n/N commands that need to read more lines
#[derive(Debug)]
pub struct ExecuteContext {
    pub lines: Vec<String>,
    pub current_line_index: usize,
}
