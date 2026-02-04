//! File test operators for bash conditionals.
//!
//! Implements file test operators like -e, -f, -d, -r, -w, -x, etc.
//! These are used in [[ ]] and test/[ ] commands.

/// File test operators supported by bash.
/// Unary operators that test file properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTestOperator {
    /// -e: file exists
    Exists,
    /// -a: file exists (deprecated synonym for -e)
    ExistsDeprecated,
    /// -f: regular file
    RegularFile,
    /// -d: directory
    Directory,
    /// -r: readable
    Readable,
    /// -w: writable
    Writable,
    /// -x: executable
    Executable,
    /// -s: file exists and has size > 0
    NonEmpty,
    /// -L: symbolic link
    SymbolicLink,
    /// -h: symbolic link (synonym for -L)
    SymbolicLinkH,
    /// -k: sticky bit set
    StickyBit,
    /// -g: setgid bit set
    SetGid,
    /// -u: setuid bit set
    SetUid,
    /// -G: owned by effective group ID
    OwnedByGroup,
    /// -O: owned by effective user ID
    OwnedByUser,
    /// -b: block special file
    BlockSpecial,
    /// -c: character special file
    CharSpecial,
    /// -p: named pipe (FIFO)
    NamedPipe,
    /// -S: socket
    Socket,
    /// -t: file descriptor is open and refers to a terminal
    Terminal,
    /// -N: file has been modified since last read
    ModifiedSinceRead,
}

impl FileTestOperator {
    /// Parse a file test operator from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "-e" => Some(FileTestOperator::Exists),
            "-a" => Some(FileTestOperator::ExistsDeprecated),
            "-f" => Some(FileTestOperator::RegularFile),
            "-d" => Some(FileTestOperator::Directory),
            "-r" => Some(FileTestOperator::Readable),
            "-w" => Some(FileTestOperator::Writable),
            "-x" => Some(FileTestOperator::Executable),
            "-s" => Some(FileTestOperator::NonEmpty),
            "-L" => Some(FileTestOperator::SymbolicLink),
            "-h" => Some(FileTestOperator::SymbolicLinkH),
            "-k" => Some(FileTestOperator::StickyBit),
            "-g" => Some(FileTestOperator::SetGid),
            "-u" => Some(FileTestOperator::SetUid),
            "-G" => Some(FileTestOperator::OwnedByGroup),
            "-O" => Some(FileTestOperator::OwnedByUser),
            "-b" => Some(FileTestOperator::BlockSpecial),
            "-c" => Some(FileTestOperator::CharSpecial),
            "-p" => Some(FileTestOperator::NamedPipe),
            "-S" => Some(FileTestOperator::Socket),
            "-t" => Some(FileTestOperator::Terminal),
            "-N" => Some(FileTestOperator::ModifiedSinceRead),
            _ => None,
        }
    }

    /// Get the operator string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            FileTestOperator::Exists => "-e",
            FileTestOperator::ExistsDeprecated => "-a",
            FileTestOperator::RegularFile => "-f",
            FileTestOperator::Directory => "-d",
            FileTestOperator::Readable => "-r",
            FileTestOperator::Writable => "-w",
            FileTestOperator::Executable => "-x",
            FileTestOperator::NonEmpty => "-s",
            FileTestOperator::SymbolicLink => "-L",
            FileTestOperator::SymbolicLinkH => "-h",
            FileTestOperator::StickyBit => "-k",
            FileTestOperator::SetGid => "-g",
            FileTestOperator::SetUid => "-u",
            FileTestOperator::OwnedByGroup => "-G",
            FileTestOperator::OwnedByUser => "-O",
            FileTestOperator::BlockSpecial => "-b",
            FileTestOperator::CharSpecial => "-c",
            FileTestOperator::NamedPipe => "-p",
            FileTestOperator::Socket => "-S",
            FileTestOperator::Terminal => "-t",
            FileTestOperator::ModifiedSinceRead => "-N",
        }
    }
}

/// Check if a string is a file test operator.
pub fn is_file_test_operator(op: &str) -> bool {
    FileTestOperator::from_str(op).is_some()
}

/// Binary file test operators for comparing two files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFileTestOperator {
    /// -nt: left is newer than right
    NewerThan,
    /// -ot: left is older than right
    OlderThan,
    /// -ef: same file (same device and inode)
    SameFile,
}

impl BinaryFileTestOperator {
    /// Parse a binary file test operator from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "-nt" => Some(BinaryFileTestOperator::NewerThan),
            "-ot" => Some(BinaryFileTestOperator::OlderThan),
            "-ef" => Some(BinaryFileTestOperator::SameFile),
            _ => None,
        }
    }

    /// Get the operator string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryFileTestOperator::NewerThan => "-nt",
            BinaryFileTestOperator::OlderThan => "-ot",
            BinaryFileTestOperator::SameFile => "-ef",
        }
    }
}

/// Check if a string is a binary file test operator.
pub fn is_binary_file_test_operator(op: &str) -> bool {
    BinaryFileTestOperator::from_str(op).is_some()
}

/// Common character device paths recognized in virtual filesystem.
pub const CHAR_DEVICES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
];

/// Check if a path is a known character device.
pub fn is_char_device(path: &str) -> bool {
    CHAR_DEVICES.iter().any(|dev| path == *dev || path.ends_with(dev))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_test_operator_from_str() {
        assert_eq!(FileTestOperator::from_str("-e"), Some(FileTestOperator::Exists));
        assert_eq!(FileTestOperator::from_str("-f"), Some(FileTestOperator::RegularFile));
        assert_eq!(FileTestOperator::from_str("-d"), Some(FileTestOperator::Directory));
        assert_eq!(FileTestOperator::from_str("-r"), Some(FileTestOperator::Readable));
        assert_eq!(FileTestOperator::from_str("-w"), Some(FileTestOperator::Writable));
        assert_eq!(FileTestOperator::from_str("-x"), Some(FileTestOperator::Executable));
        assert_eq!(FileTestOperator::from_str("-L"), Some(FileTestOperator::SymbolicLink));
        assert_eq!(FileTestOperator::from_str("-h"), Some(FileTestOperator::SymbolicLinkH));
        assert_eq!(FileTestOperator::from_str("-z"), None);
        assert_eq!(FileTestOperator::from_str("invalid"), None);
    }

    #[test]
    fn test_is_file_test_operator() {
        assert!(is_file_test_operator("-e"));
        assert!(is_file_test_operator("-f"));
        assert!(is_file_test_operator("-d"));
        assert!(is_file_test_operator("-s"));
        assert!(!is_file_test_operator("-z"));
        assert!(!is_file_test_operator("-eq"));
    }

    #[test]
    fn test_binary_file_test_operator_from_str() {
        assert_eq!(BinaryFileTestOperator::from_str("-nt"), Some(BinaryFileTestOperator::NewerThan));
        assert_eq!(BinaryFileTestOperator::from_str("-ot"), Some(BinaryFileTestOperator::OlderThan));
        assert_eq!(BinaryFileTestOperator::from_str("-ef"), Some(BinaryFileTestOperator::SameFile));
        assert_eq!(BinaryFileTestOperator::from_str("-e"), None);
    }

    #[test]
    fn test_is_binary_file_test_operator() {
        assert!(is_binary_file_test_operator("-nt"));
        assert!(is_binary_file_test_operator("-ot"));
        assert!(is_binary_file_test_operator("-ef"));
        assert!(!is_binary_file_test_operator("-e"));
        assert!(!is_binary_file_test_operator("-f"));
    }

    #[test]
    fn test_is_char_device() {
        assert!(is_char_device("/dev/null"));
        assert!(is_char_device("/dev/zero"));
        assert!(is_char_device("/dev/tty"));
        assert!(!is_char_device("/tmp/file"));
        assert!(!is_char_device("/dev/sda"));
    }
}
