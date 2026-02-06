//! File System Types
//!
//! Core types and traits for the virtual file system.

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::SystemTime;
use thiserror::Error;

/// File system errors
#[derive(Error, Debug, Clone)]
pub enum FsError {
    #[error("ENOENT: no such file or directory, {operation} '{path}'")]
    NotFound { path: String, operation: String },

    #[error("EEXIST: file already exists, {operation} '{path}'")]
    AlreadyExists { path: String, operation: String },

    #[error("EISDIR: illegal operation on a directory, {operation} '{path}'")]
    IsDirectory { path: String, operation: String },

    #[error("ENOTDIR: not a directory, {operation} '{path}'")]
    NotDirectory { path: String, operation: String },

    #[error("ENOTEMPTY: directory not empty, {operation} '{path}'")]
    NotEmpty { path: String, operation: String },

    #[error("EINVAL: invalid argument, {operation} '{path}'")]
    InvalidArgument { path: String, operation: String },

    #[error("ELOOP: too many levels of symbolic links, {operation} '{path}'")]
    SymlinkLoop { path: String, operation: String },

    #[error("EPERM: operation not permitted, {operation} '{path}'")]
    PermissionDenied { path: String, operation: String },

    #[error("EROFS: read-only file system, {operation}")]
    ReadOnly { operation: String },

    #[error("{message}")]
    Other { message: String },
}

/// Supported buffer encodings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferEncoding {
    #[default]
    Utf8,
    Ascii,
    Binary,
    Base64,
    Hex,
    Latin1,
}

impl BufferEncoding {
    /// Parse encoding from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "utf8" | "utf-8" => Some(Self::Utf8),
            "ascii" => Some(Self::Ascii),
            "binary" => Some(Self::Binary),
            "base64" => Some(Self::Base64),
            "hex" => Some(Self::Hex),
            "latin1" => Some(Self::Latin1),
            _ => None,
        }
    }
}

/// File content type
#[derive(Debug, Clone)]
pub enum FileContent {
    Text(String),
    Binary(Vec<u8>),
}

impl From<String> for FileContent {
    fn from(s: String) -> Self {
        FileContent::Text(s)
    }
}

impl From<&str> for FileContent {
    fn from(s: &str) -> Self {
        FileContent::Text(s.to_string())
    }
}

impl From<Vec<u8>> for FileContent {
    fn from(v: Vec<u8>) -> Self {
        FileContent::Binary(v)
    }
}

/// File system entry types
#[derive(Debug, Clone)]
pub enum FsEntry {
    File {
        content: Vec<u8>,
        mode: u32,
        mtime: SystemTime,
    },
    Directory {
        mode: u32,
        mtime: SystemTime,
    },
    Symlink {
        target: String,
        mode: u32,
        mtime: SystemTime,
    },
}

impl FsEntry {
    /// Check if entry is a file
    pub fn is_file(&self) -> bool {
        matches!(self, FsEntry::File { .. })
    }

    /// Check if entry is a directory
    pub fn is_directory(&self) -> bool {
        matches!(self, FsEntry::Directory { .. })
    }

    /// Check if entry is a symlink
    pub fn is_symlink(&self) -> bool {
        matches!(self, FsEntry::Symlink { .. })
    }

    /// Get the mode of the entry
    pub fn mode(&self) -> u32 {
        match self {
            FsEntry::File { mode, .. } => *mode,
            FsEntry::Directory { mode, .. } => *mode,
            FsEntry::Symlink { mode, .. } => *mode,
        }
    }

    /// Get the mtime of the entry
    pub fn mtime(&self) -> SystemTime {
        match self {
            FsEntry::File { mtime, .. } => *mtime,
            FsEntry::Directory { mtime, .. } => *mtime,
            FsEntry::Symlink { mtime, .. } => *mtime,
        }
    }
}

/// File status information
#[derive(Debug, Clone)]
pub struct FsStat {
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub mode: u32,
    pub size: u64,
    pub mtime: SystemTime,
}

/// Directory entry with type information (similar to Node's Dirent)
#[derive(Debug, Clone)]
pub struct DirentEntry {
    pub name: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// Options for mkdir operation
#[derive(Debug, Clone, Default)]
pub struct MkdirOptions {
    pub recursive: bool,
}

/// Options for rm operation
#[derive(Debug, Clone, Default)]
pub struct RmOptions {
    pub recursive: bool,
    pub force: bool,
}

/// Options for cp operation
#[derive(Debug, Clone, Default)]
pub struct CpOptions {
    pub recursive: bool,
}

/// Initial file specification with optional metadata
#[derive(Debug, Clone)]
pub struct FileInit {
    pub content: FileContent,
    pub mode: Option<u32>,
    pub mtime: Option<SystemTime>,
}

impl From<String> for FileInit {
    fn from(s: String) -> Self {
        FileInit {
            content: FileContent::Text(s),
            mode: None,
            mtime: None,
        }
    }
}

impl From<&str> for FileInit {
    fn from(s: &str) -> Self {
        FileInit {
            content: FileContent::Text(s.to_string()),
            mode: None,
            mtime: None,
        }
    }
}

/// Initial files map type
pub type InitialFiles = HashMap<String, FileInit>;

/// Abstract filesystem interface that can be implemented by different backends.
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Read the contents of a file as a string (default: utf8)
    async fn read_file(&self, path: &str) -> Result<String, FsError>;

    /// Read the contents of a file as bytes (binary)
    async fn read_file_buffer(&self, path: &str) -> Result<Vec<u8>, FsError>;

    /// Write content to a file, creating it if it doesn't exist
    async fn write_file(&self, path: &str, content: &[u8]) -> Result<(), FsError>;

    /// Append content to a file, creating it if it doesn't exist
    async fn append_file(&self, path: &str, content: &[u8]) -> Result<(), FsError>;

    /// Check if a path exists
    async fn exists(&self, path: &str) -> bool;

    /// Get file/directory information (follows symlinks)
    async fn stat(&self, path: &str) -> Result<FsStat, FsError>;

    /// Get file/directory information (does not follow symlinks)
    async fn lstat(&self, path: &str) -> Result<FsStat, FsError>;

    /// Create a directory
    async fn mkdir(&self, path: &str, options: &MkdirOptions) -> Result<(), FsError>;

    /// Read directory contents (returns entry names)
    async fn readdir(&self, path: &str) -> Result<Vec<String>, FsError>;

    /// Read directory contents with file type information
    async fn readdir_with_file_types(&self, path: &str) -> Result<Vec<DirentEntry>, FsError>;

    /// Remove a file or directory
    async fn rm(&self, path: &str, options: &RmOptions) -> Result<(), FsError>;

    /// Copy a file or directory
    async fn cp(&self, src: &str, dest: &str, options: &CpOptions) -> Result<(), FsError>;

    /// Move/rename a file or directory
    async fn mv(&self, src: &str, dest: &str) -> Result<(), FsError>;

    /// Change file/directory permissions
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError>;

    /// Create a symbolic link
    async fn symlink(&self, target: &str, link_path: &str) -> Result<(), FsError>;

    /// Create a hard link
    async fn link(&self, existing_path: &str, new_path: &str) -> Result<(), FsError>;

    /// Read the target of a symbolic link
    async fn readlink(&self, path: &str) -> Result<String, FsError>;

    /// Resolve all symlinks in a path to get the canonical physical path
    async fn realpath(&self, path: &str) -> Result<String, FsError>;

    /// Set modification time of a file
    async fn utimes(&self, path: &str, mtime: SystemTime) -> Result<(), FsError>;

    /// Resolve a relative path against a base path
    fn resolve_path(&self, base: &str, path: &str) -> String;

    /// Get all paths in the filesystem (useful for glob matching)
    fn get_all_paths(&self) -> Vec<String>;
}

// ============================================================================
// Encoding utilities
// ============================================================================

/// Convert content to bytes with encoding
pub fn to_buffer(content: &FileContent, encoding: BufferEncoding) -> Vec<u8> {
    match content {
        FileContent::Binary(bytes) => bytes.clone(),
        FileContent::Text(text) => match encoding {
            BufferEncoding::Base64 => {
                // Decode base64 string to bytes
                base64_decode(text)
            }
            BufferEncoding::Hex => {
                // Decode hex string to bytes
                hex_decode(text)
            }
            BufferEncoding::Binary | BufferEncoding::Latin1 => {
                // Each char becomes a byte (truncated to 8 bits)
                text.chars().map(|c| c as u8).collect()
            }
            BufferEncoding::Utf8 | BufferEncoding::Ascii => {
                text.as_bytes().to_vec()
            }
        },
    }
}

/// Convert bytes to string with encoding
pub fn from_buffer(buffer: &[u8], encoding: BufferEncoding) -> String {
    match encoding {
        BufferEncoding::Base64 => base64_encode(buffer),
        BufferEncoding::Hex => hex_encode(buffer),
        BufferEncoding::Binary | BufferEncoding::Latin1 => {
            buffer.iter().map(|&b| b as char).collect()
        }
        BufferEncoding::Utf8 | BufferEncoding::Ascii => {
            String::from_utf8_lossy(buffer).to_string()
        }
    }
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Simple base64 decoding
fn base64_decode(s: &str) -> Vec<u8> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];

    let mut result = Vec::new();
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && b < 128 && DECODE[b as usize] >= 0).collect();

    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let b0 = DECODE[chunk[0] as usize] as u8;
        let b1 = DECODE[chunk[1] as usize] as u8;
        result.push((b0 << 2) | (b1 >> 4));

        if chunk.len() > 2 {
            let b2 = DECODE[chunk[2] as usize] as u8;
            result.push((b1 << 4) | (b2 >> 2));

            if chunk.len() > 3 {
                let b3 = DECODE[chunk[3] as usize] as u8;
                result.push((b2 << 6) | b3);
            }
        }
    }

    result
}

/// Simple hex encoding
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Simple hex decoding
fn hex_decode(s: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let chars: Vec<char> = s.chars().collect();

    for chunk in chars.chunks(2) {
        if chunk.len() == 2 {
            if let Ok(byte) = u8::from_str_radix(&format!("{}{}", chunk[0], chunk[1]), 16) {
                result.push(byte);
            }
        }
    }

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_encoding_from_str() {
        assert_eq!(BufferEncoding::from_str("utf8"), Some(BufferEncoding::Utf8));
        assert_eq!(BufferEncoding::from_str("UTF-8"), Some(BufferEncoding::Utf8));
        assert_eq!(BufferEncoding::from_str("base64"), Some(BufferEncoding::Base64));
        assert_eq!(BufferEncoding::from_str("hex"), Some(BufferEncoding::Hex));
        assert_eq!(BufferEncoding::from_str("invalid"), None);
    }

    #[test]
    fn test_base64_encode_decode() {
        let data = b"Hello, World!";
        let encoded = base64_encode(data);
        assert_eq!(encoded, "SGVsbG8sIFdvcmxkIQ==");

        let decoded = base64_decode(&encoded);
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_encode_decode() {
        let data = b"Hello";
        let encoded = hex_encode(data);
        assert_eq!(encoded, "48656c6c6f");

        let decoded = hex_decode(&encoded);
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_fs_entry_methods() {
        let file = FsEntry::File {
            content: vec![],
            mode: 0o644,
            mtime: SystemTime::now(),
        };
        assert!(file.is_file());
        assert!(!file.is_directory());
        assert!(!file.is_symlink());
        assert_eq!(file.mode(), 0o644);

        let dir = FsEntry::Directory {
            mode: 0o755,
            mtime: SystemTime::now(),
        };
        assert!(!dir.is_file());
        assert!(dir.is_directory());
        assert!(!dir.is_symlink());

        let symlink = FsEntry::Symlink {
            target: "/foo".to_string(),
            mode: 0o777,
            mtime: SystemTime::now(),
        };
        assert!(!symlink.is_file());
        assert!(!symlink.is_directory());
        assert!(symlink.is_symlink());
    }
}
