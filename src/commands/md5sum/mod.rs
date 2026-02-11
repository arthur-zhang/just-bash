// src/commands/md5sum/mod.rs
// md5sum, sha1sum, sha256sum — checksum commands
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

/// Pure Rust MD5 implementation (matches TypeScript pure-JS version)
fn md5(data: &[u8]) -> String {
    fn rotate_left(x: u32, n: u32) -> u32 { (x << n) | (x >> (32 - n)) }

    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a,
        0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
        0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
        0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
        0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    const S: [u32; 64] = [
        7,12,17,22, 7,12,17,22, 7,12,17,22, 7,12,17,22,
        5, 9,14,20, 5, 9,14,20, 5, 9,14,20, 5, 9,14,20,
        4,11,16,23, 4,11,16,23, 4,11,16,23, 4,11,16,23,
        6,10,15,21, 6,10,15,21, 6,10,15,21, 6,10,15,21,
    ];

    let bit_len = (data.len() as u64) * 8;
    let padding_len = if data.len() % 64 < 56 { 56 - data.len() % 64 } else { 120 - data.len() % 64 };
    let mut padded = Vec::with_capacity(data.len() + padding_len + 8);
    padded.extend_from_slice(data);
    padded.push(0x80);
    padded.resize(data.len() + 1 + padding_len - 1, 0);
    // Ensure exact padding
    while padded.len() < data.len() + padding_len { padded.push(0); }
    padded.extend_from_slice(&(bit_len as u32).to_le_bytes());
    padded.extend_from_slice(&((bit_len >> 32) as u32).to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in padded.chunks(64) {
        let mut m = [0u32; 16];
        for j in 0..16 {
            m[j] = u32::from_le_bytes([chunk[j*4], chunk[j*4+1], chunk[j*4+2], chunk[j*4+3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for j in 0..64usize {
            let (f, g) = if j < 16 {
                ((b & c) | (!b & d), j)
            } else if j < 32 {
                ((d & b) | (!d & c), (5 * j + 1) % 16)
            } else if j < 48 {
                (b ^ c ^ d, (3 * j + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * j) % 16)
            };
            let f = f.wrapping_add(a).wrapping_add(K[j]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotate_left(f, S[j]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let result = [a0.to_le_bytes(), b0.to_le_bytes(), c0.to_le_bytes(), d0.to_le_bytes()];
    result.iter().flat_map(|b| b.iter()).map(|b| format!("{:02x}", b)).collect()
}

/// Pure Rust SHA-1 implementation
fn sha1(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 { padded.push(0); }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = if i < 20 {
                ((b & c) | (!b & d), 0x5A827999u32)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9EBA1u32)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32)
            } else {
                (b ^ c ^ d, 0xCA62C1D6u32)
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

/// Pure Rust SHA-256 implementation
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 { padded.push(0); }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let temp1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let temp2 = s0.wrapping_add(maj);
            v[7] = v[6]; v[6] = v[5]; v[5] = v[4];
            v[4] = v[3].wrapping_add(temp1);
            v[3] = v[2]; v[2] = v[1]; v[1] = v[0];
            v[0] = temp1.wrapping_add(temp2);
        }
        for i in 0..8 { h[i] = h[i].wrapping_add(v[i]); }
    }
    h.iter().map(|x| format!("{:08x}", x)).collect()
}

#[derive(Clone, Copy)]
pub enum HashAlgorithm { Md5, Sha1, Sha256 }

fn compute_hash(algorithm: HashAlgorithm, data: &[u8]) -> String {
    match algorithm {
        HashAlgorithm::Md5 => md5(data),
        HashAlgorithm::Sha1 => sha1(data),
        HashAlgorithm::Sha256 => sha256(data),
    }
}

fn algo_name(algo: HashAlgorithm) -> &'static str {
    match algo { HashAlgorithm::Md5 => "MD5", HashAlgorithm::Sha1 => "SHA1", HashAlgorithm::Sha256 => "SHA256" }
}

fn make_help(name: &str, summary: &str) -> String {
    format!("Usage: {} [OPTION]... [FILE]...\n\n{}\n\nOptions:\n  -c, --check    read checksums from FILEs and check them\n      --help     display this help and exit\n", name, summary)
}

async fn checksum_execute(name: &str, algorithm: HashAlgorithm, summary: &str, ctx: CommandContext) -> CommandResult {
    let args = &ctx.args;
    if args.iter().any(|a| a == "--help") {
        return CommandResult::success(make_help(name, summary));
    }

    let mut check = false;
    let mut files: Vec<String> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-c" | "--check" => check = true,
            "-b" | "-t" | "--binary" | "--text" => { /* ignored */ }
            a if a.starts_with('-') && a != "-" => {
                return CommandResult::with_exit_code("".into(), format!("{}: invalid option -- '{}'\n", name, &a[1..2]), 1);
            }
            _ => files.push(arg.clone()),
        }
    }
    if files.is_empty() { files.push("-".into()); }

    if check {
        let mut failed = 0;
        let mut output = String::new();

        for file in &files {
            let content = if file == "-" {
                Some(ctx.stdin.clone())
            } else {
                let path = resolve_path(&ctx.cwd, file);
                ctx.fs.read_file(&path).await.ok()
            };
            let content = match content {
                Some(c) => c,
                None => return CommandResult::with_exit_code("".into(), format!("{}: {}: No such file or directory\n", name, file), 1),
            };

            for line in content.lines() {
                // Parse: hash  filename  or  hash *filename
                let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace()).collect();
                if parts.len() < 2 { continue; }
                let expected_hash = parts[0];
                let target_file = parts[1].trim_start_matches(|c: char| c == ' ' || c == '*');
                if target_file.is_empty() { continue; }

                let file_data = if target_file == "-" {
                    Some(ctx.stdin.as_bytes().to_vec())
                } else {
                    let path = resolve_path(&ctx.cwd, target_file);
                    ctx.fs.read_file_buffer(&path).await.ok()
                };

                match file_data {
                    None => { output.push_str(&format!("{}: FAILED open or read\n", target_file)); failed += 1; }
                    Some(data) => {
                        let hash = compute_hash(algorithm, &data);
                        let ok = hash == expected_hash.to_lowercase();
                        output.push_str(&format!("{}: {}\n", target_file, if ok { "OK" } else { "FAILED" }));
                        if !ok { failed += 1; }
                    }
                }
            }
        }

        if failed > 0 {
            let s = if failed > 1 { "s" } else { "" };
            output.push_str(&format!("{}: WARNING: {} computed checksum{} did NOT match\n", name, failed, s));
        }
        return CommandResult::with_exit_code(output, "".into(), if failed > 0 { 1 } else { 0 });
    }

    // Normal hash mode
    let mut output = String::new();
    let mut exit_code = 0;

    for file in &files {
        let data = if file == "-" {
            Some(ctx.stdin.as_bytes().to_vec())
        } else {
            let path = resolve_path(&ctx.cwd, file);
            ctx.fs.read_file_buffer(&path).await.ok()
        };
        match data {
            None => { output.push_str(&format!("{}: {}: No such file or directory\n", name, file)); exit_code = 1; }
            Some(data) => { output.push_str(&format!("{}  {}\n", compute_hash(algorithm, &data), file)); }
        }
    }

    CommandResult::with_exit_code(output, "".into(), exit_code)
}

fn resolve_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') { path.to_string() }
    else {
        let cwd = cwd.trim_end_matches('/');
        format!("{}/{}", cwd, path)
    }
}

// --- md5sum ---
pub struct Md5sumCommand;
#[async_trait]
impl Command for Md5sumCommand {
    fn name(&self) -> &'static str { "md5sum" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        checksum_execute("md5sum", HashAlgorithm::Md5, "compute MD5 message digest", ctx).await
    }
}

// --- sha1sum ---
pub struct Sha1sumCommand;
#[async_trait]
impl Command for Sha1sumCommand {
    fn name(&self) -> &'static str { "sha1sum" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        checksum_execute("sha1sum", HashAlgorithm::Sha1, "compute SHA1 message digest", ctx).await
    }
}

// --- sha256sum ---
pub struct Sha256sumCommand;
#[async_trait]
impl Command for Sha256sumCommand {
    fn name(&self) -> &'static str { "sha256sum" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        checksum_execute("sha256sum", HashAlgorithm::Sha256, "compute SHA256 message digest", ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: stdin.into(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    fn make_ctx_with_fs(args: Vec<&str>, stdin: &str, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: stdin.into(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    #[tokio::test]
    async fn test_md5_hello() {
        let r = Md5sumCommand.execute(make_ctx(vec![], "hello")).await;
        assert_eq!(r.stdout, "5d41402abc4b2a76b9719d911017c592  -\n");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_md5_empty() {
        let r = Md5sumCommand.execute(make_ctx(vec![], "")).await;
        assert_eq!(r.stdout, "d41d8cd98f00b204e9800998ecf8427e  -\n");
    }

    #[tokio::test]
    async fn test_md5_file() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/tmp/test.txt", "test".as_bytes()).await.unwrap();
        let r = Md5sumCommand.execute(make_ctx_with_fs(vec!["/tmp/test.txt"], "", fs)).await;
        assert_eq!(r.stdout, "098f6bcd4621d373cade4e832627b4f6  /tmp/test.txt\n");
    }

    #[tokio::test]
    async fn test_md5_missing_file() {
        let r = Md5sumCommand.execute(make_ctx(vec!["/tmp/nonexistent"], "")).await;
        assert!(r.stdout.contains("No such file or directory"));
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn test_md5_check_ok() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/tmp/hello.txt", "hello".as_bytes()).await.unwrap();
        fs.write_file("/tmp/sums.txt", "5d41402abc4b2a76b9719d911017c592  /tmp/hello.txt\n".as_bytes()).await.unwrap();
        let r = Md5sumCommand.execute(make_ctx_with_fs(vec!["-c", "/tmp/sums.txt"], "", fs)).await;
        assert!(r.stdout.contains("/tmp/hello.txt: OK"));
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_md5_check_fail() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/tmp/wrong.txt", "wrong".as_bytes()).await.unwrap();
        fs.write_file("/tmp/sums.txt", "5d41402abc4b2a76b9719d911017c592  /tmp/wrong.txt\n".as_bytes()).await.unwrap();
        let r = Md5sumCommand.execute(make_ctx_with_fs(vec!["-c", "/tmp/sums.txt"], "", fs)).await;
        assert!(r.stdout.contains("/tmp/wrong.txt: FAILED"));
        assert!(r.stdout.contains("WARNING"));
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn test_md5_help() {
        let r = Md5sumCommand.execute(make_ctx(vec!["--help"], "")).await;
        assert!(r.stdout.contains("md5sum"));
        assert!(r.stdout.contains("MD5"));
    }

    #[tokio::test]
    async fn test_sha1_hello() {
        let r = Sha1sumCommand.execute(make_ctx(vec![], "hello")).await;
        assert_eq!(r.stdout, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d  -\n");
    }

    #[tokio::test]
    async fn test_sha1_empty() {
        let r = Sha1sumCommand.execute(make_ctx(vec![], "")).await;
        assert_eq!(r.stdout, "da39a3ee5e6b4b0d3255bfef95601890afd80709  -\n");
    }

    #[tokio::test]
    async fn test_sha256_hello() {
        let r = Sha256sumCommand.execute(make_ctx(vec![], "hello")).await;
        assert_eq!(r.stdout, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  -\n");
    }

    #[tokio::test]
    async fn test_sha256_empty() {
        let r = Sha256sumCommand.execute(make_ctx(vec![], "")).await;
        assert_eq!(r.stdout, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  -\n");
    }

    #[tokio::test]
    async fn test_sha256_help() {
        let r = Sha256sumCommand.execute(make_ctx(vec!["--help"], "")).await;
        assert!(r.stdout.contains("sha256sum"));
        assert!(r.stdout.contains("SHA256"));
    }
}
