//! File test operators for bash conditionals.
//!
//! Implements file test operators like -e, -f, -d, -r, -w, -x, etc.
//! These are used in [[ ]] and test/[ ] commands.

use std::path::Path;

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

/// File stat information needed for file tests.
pub struct FileStat {
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mode: u32,
    pub mtime: i64,
}

/// Trait for filesystem operations needed by file tests.
/// This allows for both real filesystem and virtual filesystem implementations.
pub trait FileSystem {
    /// Check if a path exists.
    fn exists(&self, path: &Path) -> bool;
    /// Get file stat information (follows symlinks).
    fn stat(&self, path: &Path) -> Option<FileStat>;
    /// Get file stat information (does not follow symlinks).
    fn lstat(&self, path: &Path) -> Option<FileStat>;
    /// Resolve a path relative to cwd.
    fn resolve_path(&self, cwd: &str, path: &str) -> String;
}

/// Evaluate a file test operator against a path.
///
/// # Arguments
/// * `fs` - Filesystem implementation
/// * `cwd` - Current working directory
/// * `operator` - The file test operator
/// * `operand` - The path to test
pub fn evaluate_file_test<F: FileSystem>(
    fs: &F,
    cwd: &str,
    operator: FileTestOperator,
    operand: &str,
) -> bool {
    let path_str = fs.resolve_path(cwd, operand);
    let path = Path::new(&path_str);

    match operator {
        FileTestOperator::Exists | FileTestOperator::ExistsDeprecated => {
            fs.exists(path)
        }

        FileTestOperator::RegularFile => {
            fs.stat(path).map_or(false, |s| s.is_file)
        }

        FileTestOperator::Directory => {
            fs.stat(path).map_or(false, |s| s.is_directory)
        }

        FileTestOperator::Readable => {
            // Check user read bit (0o400)
            fs.stat(path).map_or(false, |s| (s.mode & 0o400) != 0)
        }

        FileTestOperator::Writable => {
            // Check user write bit (0o200)
            fs.stat(path).map_or(false, |s| (s.mode & 0o200) != 0)
        }

        FileTestOperator::Executable => {
            // Check user execute bit (0o100)
            fs.stat(path).map_or(false, |s| (s.mode & 0o100) != 0)
        }

        FileTestOperator::NonEmpty => {
            // File exists and has size > 0
            fs.stat(path).map_or(false, |s| s.size > 0)
        }

        FileTestOperator::SymbolicLink | FileTestOperator::SymbolicLinkH => {
            // Use lstat to check without following
            fs.lstat(path).map_or(false, |s| s.is_symlink)
        }

        FileTestOperator::StickyBit => {
            // Sticky bit (mode & 0o1000)
            fs.stat(path).map_or(false, |s| (s.mode & 0o1000) != 0)
        }

        FileTestOperator::SetGid => {
            // Setgid bit (mode & 0o2000)
            fs.stat(path).map_or(false, |s| (s.mode & 0o2000) != 0)
        }

        FileTestOperator::SetUid => {
            // Setuid bit (mode & 0o4000)
            fs.stat(path).map_or(false, |s| (s.mode & 0o4000) != 0)
        }

        FileTestOperator::OwnedByGroup | FileTestOperator::OwnedByUser => {
            // In virtual fs, assume user owns everything that exists
            fs.exists(path)
        }

        FileTestOperator::BlockSpecial => {
            // Block special file - virtual fs doesn't have these
            false
        }

        FileTestOperator::CharSpecial => {
            // Character special file - check known devices
            is_char_device(&path_str)
        }

        FileTestOperator::NamedPipe => {
            // Named pipe (FIFO) - virtual fs doesn't have these
            false
        }

        FileTestOperator::Socket => {
            // Socket - virtual fs doesn't have these
            false
        }

        FileTestOperator::Terminal => {
            // File descriptor refers to terminal
            // We don't support terminal detection
            false
        }

        FileTestOperator::ModifiedSinceRead => {
            // We don't track read times, so just check if file exists
            fs.exists(path)
        }
    }
}

/// Evaluate a binary file test operator comparing two files.
///
/// # Arguments
/// * `fs` - Filesystem implementation
/// * `cwd` - Current working directory
/// * `operator` - The binary file test operator
/// * `left` - Left operand (file path)
/// * `right` - Right operand (file path)
pub fn evaluate_binary_file_test<F: FileSystem>(
    fs: &F,
    cwd: &str,
    operator: BinaryFileTestOperator,
    left: &str,
    right: &str,
) -> bool {
    let left_path_str = fs.resolve_path(cwd, left);
    let right_path_str = fs.resolve_path(cwd, right);
    let left_path = Path::new(&left_path_str);
    let right_path = Path::new(&right_path_str);

    match operator {
        BinaryFileTestOperator::NewerThan => {
            // left is newer than right
            match (fs.stat(left_path), fs.stat(right_path)) {
                (Some(left_stat), Some(right_stat)) => left_stat.mtime > right_stat.mtime,
                _ => false,
            }
        }

        BinaryFileTestOperator::OlderThan => {
            // left is older than right
            match (fs.stat(left_path), fs.stat(right_path)) {
                (Some(left_stat), Some(right_stat)) => left_stat.mtime < right_stat.mtime,
                _ => false,
            }
        }

        BinaryFileTestOperator::SameFile => {
            // Same file (same device and inode)
            // In virtual fs, compare resolved canonical paths
            if !fs.exists(left_path) || !fs.exists(right_path) {
                return false;
            }
            // Compare canonical paths
            left_path_str == right_path_str
        }
    }
}

/// Evaluate a file test operator from string.
pub fn evaluate_file_test_str<F: FileSystem>(
    fs: &F,
    cwd: &str,
    operator: &str,
    operand: &str,
) -> Option<bool> {
    FileTestOperator::from_str(operator)
        .map(|op| evaluate_file_test(fs, cwd, op, operand))
}

/// Evaluate a binary file test operator from string.
pub fn evaluate_binary_file_test_str<F: FileSystem>(
    fs: &F,
    cwd: &str,
    operator: &str,
    left: &str,
    right: &str,
) -> Option<bool> {
    BinaryFileTestOperator::from_str(operator)
        .map(|op| evaluate_binary_file_test(fs, cwd, op, left, right))
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
