// src/commands/tar/archive.rs

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

const BLOCK_SIZE: usize = 512;

#[derive(Debug, Clone)]
pub struct TarEntry {
    pub path: String,
    pub content: Vec<u8>,
    pub mode: u32,
    pub size: u64,
    pub mtime: u64,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub link_target: String,
}

impl Default for TarEntry {
    fn default() -> Self {
        Self {
            path: String::new(),
            content: Vec::new(),
            mode: 0o644,
            size: 0,
            mtime: 0,
            is_directory: false,
            is_symlink: false,
            link_target: String::new(),
        }
    }
}

/// Write a null-terminated string into a fixed-size field.
fn write_string(header: &mut [u8], offset: usize, len: usize, s: &str) {
    let bytes = s.as_bytes();
    let copy_len = bytes.len().min(len);
    header[offset..offset + copy_len].copy_from_slice(&bytes[..copy_len]);
    // Rest is already zeroed
}

/// Write an octal ASCII value into a fixed-size field, null-terminated.
fn write_octal(header: &mut [u8], offset: usize, len: usize, value: u64) {
    let s = format!("{:0>width$o}", value, width = len - 1);
    let bytes = s.as_bytes();
    // Take the last (len-1) characters to fit the field
    let start = if bytes.len() > len - 1 {
        bytes.len() - (len - 1)
    } else {
        0
    };
    let slice = &bytes[start..];
    header[offset..offset + slice.len()].copy_from_slice(slice);
    header[offset + slice.len()] = 0; // null terminator
}

/// Calculate the checksum for a tar header.
/// Sum of all bytes, treating checksum field (148..156) as spaces.
fn calculate_checksum(header: &[u8; BLOCK_SIZE]) -> u32 {
    let mut sum: u32 = 0;
    for (i, &byte) in header.iter().enumerate() {
        if (148..156).contains(&i) {
            sum += 0x20u32; // treat as space
        } else {
            sum += byte as u32;
        }
    }
    sum
}

/// Split a long path into (prefix, name) for ustar format.
/// prefix max 155 chars, name max 100 chars.
fn split_path(path: &str) -> (String, String) {
    if path.len() <= 100 {
        return (String::new(), path.to_string());
    }
    // Find a '/' to split on such that name <= 100 and prefix <= 155
    // Try to split at the last '/' that keeps name <= 100
    if let Some(pos) = path.rfind('/') {
        let prefix = &path[..pos];
        let name = &path[pos + 1..];
        if prefix.len() <= 155 && name.len() <= 100 {
            return (prefix.to_string(), name.to_string());
        }
    }
    // Try all '/' positions
    for (i, c) in path.char_indices() {
        if c == '/' && i <= 155 {
            let name = &path[i + 1..];
            if name.len() <= 100 {
                return (path[..i].to_string(), name.to_string());
            }
        }
    }
    // Fallback: truncate
    (String::new(), path[..100.min(path.len())].to_string())
}

/// Build a 512-byte ustar header for a TarEntry.
fn build_header(entry: &TarEntry) -> [u8; BLOCK_SIZE] {
    let mut header = [0u8; BLOCK_SIZE];

    let mut path = entry.path.clone();
    if entry.is_directory && !path.ends_with('/') {
        path.push('/');
    }

    let (prefix, name) = split_path(&path);

    // filename (0, 100)
    write_string(&mut header, 0, 100, &name);
    // mode (100, 8)
    write_octal(&mut header, 100, 8, entry.mode as u64);
    // uid (108, 8)
    write_octal(&mut header, 108, 8, 0);
    // gid (116, 8)
    write_octal(&mut header, 116, 8, 0);
    // size (124, 12)
    let size = if entry.is_directory || entry.is_symlink {
        0
    } else {
        entry.size
    };
    write_octal(&mut header, 124, 12, size);
    // mtime (136, 12)
    write_octal(&mut header, 136, 12, entry.mtime);
    // checksum placeholder (148, 8) - filled later
    header[148..156].copy_from_slice(b"        "); // 8 spaces
    // type flag (156, 1)
    header[156] = if entry.is_directory {
        b'5'
    } else if entry.is_symlink {
        b'2'
    } else {
        b'0'
    };
    // link name (157, 100)
    if entry.is_symlink {
        write_string(&mut header, 157, 100, &entry.link_target);
    }
    // magic (257, 6)
    header[257..263].copy_from_slice(b"ustar\0");
    // version (263, 2)
    header[263..265].copy_from_slice(b"00");
    // uname (265, 32)
    write_string(&mut header, 265, 32, "root");
    // gname (297, 32)
    write_string(&mut header, 297, 32, "root");
    // devmajor (329, 8)
    write_octal(&mut header, 329, 8, 0);
    // devminor (337, 8)
    write_octal(&mut header, 337, 8, 0);
    // prefix (345, 155)
    write_string(&mut header, 345, 155, &prefix);

    // Calculate and write checksum
    let checksum = calculate_checksum(&header);
    let cksum_str = format!("{:06o}\0 ", checksum);
    header[148..156]
        .copy_from_slice(&cksum_str.as_bytes()[..8]);

    header
}

/// Create a tar archive from entries.
pub fn create_archive(entries: &[TarEntry]) -> Vec<u8> {
    let mut archive = Vec::new();

    for entry in entries {
        let header = build_header(entry);
        archive.extend_from_slice(&header);

        if !entry.is_directory && !entry.is_symlink {
            archive.extend_from_slice(&entry.content);
            // Pad to 512-byte boundary
            let remainder = entry.content.len() % BLOCK_SIZE;
            if remainder != 0 {
                let padding = BLOCK_SIZE - remainder;
                archive.extend(std::iter::repeat(0u8).take(padding));
            }
        }
    }

    // End-of-archive: two 512-byte zero blocks
    archive.extend(std::iter::repeat(0u8).take(BLOCK_SIZE * 2));

    archive
}

/// Read a null-terminated string from a fixed-size field.
fn read_string(header: &[u8], offset: usize, len: usize) -> String {
    let slice = &header[offset..offset + len];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(len);
    String::from_utf8_lossy(&slice[..end]).to_string()
}

/// Read an octal ASCII value from a fixed-size field.
fn read_octal(header: &[u8], offset: usize, len: usize) -> u64 {
    let s = read_string(header, offset, len);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0;
    }
    u64::from_str_radix(trimmed, 8).unwrap_or(0)
}

/// Check if a 512-byte block is all zeros (end-of-archive marker).
fn is_zero_block(block: &[u8]) -> bool {
    block.iter().all(|&b| b == 0)
}

/// Verify the checksum of a tar header block.
fn verify_checksum(header: &[u8; BLOCK_SIZE]) -> bool {
    let stored = read_octal(header, 148, 8) as u32;
    let computed = calculate_checksum(header);
    stored == computed
}

/// Parse a tar archive into entries.
pub fn parse_archive(data: &[u8]) -> Result<Vec<TarEntry>, String> {
    let mut entries = Vec::new();
    let mut offset = 0;
    let mut zero_blocks = 0;

    while offset + BLOCK_SIZE <= data.len() {
        let block = &data[offset..offset + BLOCK_SIZE];

        if is_zero_block(block) {
            zero_blocks += 1;
            offset += BLOCK_SIZE;
            if zero_blocks >= 2 {
                break;
            }
            continue;
        }
        zero_blocks = 0;

        // Verify it's a valid header
        let header: [u8; BLOCK_SIZE] = block.try_into().map_err(|_| {
            "tar: invalid header block".to_string()
        })?;

        if !verify_checksum(&header) {
            return Err("tar: invalid header checksum".to_string());
        }

        let name = read_string(&header, 0, 100);
        let prefix = read_string(&header, 345, 155);
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{}/{}", prefix, name)
        };

        let mode = read_octal(&header, 100, 8) as u32;
        let size = read_octal(&header, 124, 12);
        let mtime = read_octal(&header, 136, 12);
        let type_flag = header[156];
        let link_target = read_string(&header, 157, 100);

        let is_directory = type_flag == b'5';
        let is_symlink = type_flag == b'2';

        offset += BLOCK_SIZE;

        // Read content
        let content = if !is_directory && !is_symlink && size > 0 {
            let end = offset + size as usize;
            if end > data.len() {
                return Err("tar: unexpected end of archive".to_string());
            }
            let content = data[offset..end].to_vec();
            // Advance past content + padding
            let blocks =
                (size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
            offset += blocks * BLOCK_SIZE;
            content
        } else {
            Vec::new()
        };

        entries.push(TarEntry {
            path,
            content,
            mode,
            size,
            mtime,
            is_directory,
            is_symlink,
            link_target,
        });
    }

    Ok(entries)
}

/// Compress data with gzip.
pub fn compress_gzip(data: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let mut encoder =
        GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// Decompress gzip data.
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| e.to_string())?;
    Ok(decompressed)
}

/// Check if data is gzip compressed (magic bytes 0x1f 0x8b).
pub fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file_entry(path: &str, content: &[u8]) -> TarEntry {
        TarEntry {
            path: path.to_string(),
            content: content.to_vec(),
            mode: 0o644,
            size: content.len() as u64,
            mtime: 1700000000,
            is_directory: false,
            is_symlink: false,
            link_target: String::new(),
        }
    }

    fn make_dir_entry(path: &str) -> TarEntry {
        TarEntry {
            path: path.to_string(),
            content: Vec::new(),
            mode: 0o755,
            size: 0,
            mtime: 1700000000,
            is_directory: true,
            is_symlink: false,
            link_target: String::new(),
        }
    }

    #[test]
    fn test_create_archive_single_file() {
        let entry = make_file_entry("hello.txt", b"Hello, World!");
        let archive = create_archive(&[entry]);
        // Header (512) + content padded to 512 + 2 end blocks
        assert!(archive.len() >= BLOCK_SIZE * 4);
        // Check magic
        assert_eq!(&archive[257..263], b"ustar\0");
    }

    #[test]
    fn test_create_archive_directory() {
        let entry = make_dir_entry("mydir");
        let archive = create_archive(&[entry]);
        // Header (512) + 2 end blocks (no content for dir)
        assert_eq!(archive.len(), BLOCK_SIZE * 3);
        // Type flag should be '5'
        assert_eq!(archive[156], b'5');
    }

    #[test]
    fn test_parse_archive_single_file() {
        let entry = make_file_entry("test.txt", b"test content");
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");
        assert_eq!(entries[0].content, b"test content");
        assert_eq!(entries[0].mode, 0o644);
        assert!(!entries[0].is_directory);
    }

    #[test]
    fn test_round_trip() {
        let entries = vec![
            make_dir_entry("project"),
            make_file_entry("project/main.rs", b"fn main() {}"),
            make_file_entry(
                "project/lib.rs",
                b"pub fn hello() { println!(\"hi\"); }",
            ),
        ];
        let archive = create_archive(&entries);
        let parsed = parse_archive(&archive).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].path, "project/");
        assert!(parsed[0].is_directory);
        assert_eq!(parsed[1].path, "project/main.rs");
        assert_eq!(parsed[1].content, b"fn main() {}");
        assert_eq!(parsed[2].path, "project/lib.rs");
        assert_eq!(
            parsed[2].content,
            b"pub fn hello() { println!(\"hi\"); }"
        );
    }

    #[test]
    fn test_gzip_round_trip() {
        let data = b"Hello, this is test data for gzip compression!";
        let compressed = compress_gzip(data, 6).unwrap();
        assert!(is_gzip(&compressed));
        let decompressed = decompress_gzip(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_create_gzipped_archive() {
        let entry = make_file_entry("file.txt", b"gzip test data");
        let archive = create_archive(&[entry]);
        let compressed = compress_gzip(&archive, 6).unwrap();
        assert!(is_gzip(&compressed));
        assert!(compressed.len() < archive.len());
    }

    #[test]
    fn test_parse_gzipped_archive() {
        let entry =
            make_file_entry("file.txt", b"gzip archive test");
        let archive = create_archive(&[entry]);
        let compressed = compress_gzip(&archive, 6).unwrap();
        let decompressed = decompress_gzip(&compressed).unwrap();
        let entries = parse_archive(&decompressed).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "file.txt");
        assert_eq!(entries[0].content, b"gzip archive test");
    }

    #[test]
    fn test_header_checksum_validation() {
        let entry = make_file_entry("check.txt", b"checksum test");
        let archive = create_archive(&[entry]);
        let header: [u8; BLOCK_SIZE] =
            archive[..BLOCK_SIZE].try_into().unwrap();
        assert!(verify_checksum(&header));

        // Corrupt a byte and verify checksum fails
        let mut corrupted = header;
        corrupted[0] ^= 0xFF;
        assert!(!verify_checksum(&corrupted));
    }

    #[test]
    fn test_long_filename() {
        let long_prefix =
            "a/very/deeply/nested/directory/structure/that/goes/on/and/on";
        let long_name = format!("{}/file.txt", long_prefix);
        let entry = make_file_entry(&long_name, b"long path");
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, long_name);
        assert_eq!(entries[0].content, b"long path");
    }

    #[test]
    fn test_empty_archive() {
        let archive = create_archive(&[]);
        // Just two 512-byte zero blocks
        assert_eq!(archive.len(), BLOCK_SIZE * 2);
        assert!(archive.iter().all(|&b| b == 0));
        let entries = parse_archive(&archive).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_symlink_entry() {
        let entry = TarEntry {
            path: "link.txt".to_string(),
            content: Vec::new(),
            mode: 0o777,
            size: 0,
            mtime: 1700000000,
            is_directory: false,
            is_symlink: true,
            link_target: "target.txt".to_string(),
        };
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_symlink);
        assert_eq!(entries[0].link_target, "target.txt");
        assert_eq!(entries[0].path, "link.txt");
    }

    #[test]
    fn test_file_permissions_preserved() {
        let entry = TarEntry {
            path: "script.sh".to_string(),
            content: b"#!/bin/bash\necho hi".to_vec(),
            mode: 0o755,
            size: 19,
            mtime: 1700000000,
            is_directory: false,
            is_symlink: false,
            link_target: String::new(),
        };
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries[0].mode, 0o755);
    }

    #[test]
    fn test_is_gzip_detection() {
        assert!(is_gzip(&[0x1f, 0x8b, 0x08]));
        assert!(!is_gzip(&[0x00, 0x00]));
        assert!(!is_gzip(&[0x1f]));
        assert!(!is_gzip(&[]));
    }

    #[test]
    fn test_large_file_content() {
        let content: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
        let entry = TarEntry {
            path: "large.bin".to_string(),
            content: content.clone(),
            mode: 0o644,
            size: content.len() as u64,
            mtime: 0,
            is_directory: false,
            is_symlink: false,
            link_target: String::new(),
        };
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries[0].content, content);
    }

    #[test]
    fn test_multiple_files_round_trip() {
        let entries = vec![
            make_file_entry("a.txt", b"aaa"),
            make_file_entry("b.txt", b"bbb"),
            make_file_entry("c.txt", b"ccc"),
        ];
        let archive = create_archive(&entries);
        let parsed = parse_archive(&archive).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].content, b"aaa");
        assert_eq!(parsed[1].content, b"bbb");
        assert_eq!(parsed[2].content, b"ccc");
    }

    #[test]
    fn test_gzip_empty_data() {
        let compressed = compress_gzip(b"", 6).unwrap();
        assert!(is_gzip(&compressed));
        let decompressed = decompress_gzip(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_decompress_invalid_data() {
        let result = decompress_gzip(&[0x1f, 0x8b, 0xFF, 0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_path_short() {
        let (prefix, name) = split_path("hello.txt");
        assert_eq!(prefix, "");
        assert_eq!(name, "hello.txt");
    }

    #[test]
    fn test_split_path_long() {
        let long = format!(
            "{}/file.txt",
            "a".repeat(60) + "/" + &"b".repeat(60)
        );
        let (prefix, name) = split_path(&long);
        assert!(!prefix.is_empty());
        assert!(name.len() <= 100);
        assert!(prefix.len() <= 155);
    }

    #[test]
    fn test_directory_path_gets_trailing_slash() {
        let entry = make_dir_entry("mydir");
        let archive = create_archive(&[entry]);
        let entries = parse_archive(&archive).unwrap();
        assert_eq!(entries[0].path, "mydir/");
    }
}
