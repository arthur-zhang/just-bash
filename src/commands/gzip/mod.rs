// src/commands/gzip/mod.rs
use async_trait::async_trait;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::{Read, Write};
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::types::RmOptions;

pub struct GzipCommand;
pub struct GunzipCommand;
pub struct ZcatCommand;

#[async_trait]
impl Command for GzipCommand {
    fn name(&self) -> &'static str { "gzip" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        execute_gzip(ctx, "gzip").await
    }
}

#[async_trait]
impl Command for GunzipCommand {
    fn name(&self) -> &'static str { "gunzip" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        execute_gzip(ctx, "gunzip").await
    }
}

#[async_trait]
impl Command for ZcatCommand {
    fn name(&self) -> &'static str { "zcat" }
    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        execute_gzip(ctx, "zcat").await
    }
}

struct GzipFlags {
    stdout: bool,
    decompress: bool,
    force: bool,
    keep: bool,
    list: bool,
    #[allow(dead_code)]
    no_name: bool,
    name: bool,
    quiet: bool,
    recursive: bool,
    suffix: String,
    test: bool,
    verbose: bool,
    level: u32,
}

impl Default for GzipFlags {
    fn default() -> Self {
        Self {
            stdout: false,
            decompress: false,
            force: false,
            keep: false,
            list: false,
            no_name: false,
            name: false,
            quiet: false,
            recursive: false,
            suffix: ".gz".to_string(),
            test: false,
            verbose: false,
            level: 6,
        }
    }
}

fn parse_flags(args: &[String]) -> Result<(GzipFlags, Vec<String>), String> {
    let mut flags = GzipFlags::default();
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            files.extend(args[i + 1..].iter().cloned());
            break;
        }
        match arg.as_str() {
            "-c" | "--stdout" | "--to-stdout" => flags.stdout = true,
            "-d" | "--decompress" | "--uncompress" => flags.decompress = true,
            "-f" | "--force" => flags.force = true,
            "-k" | "--keep" => flags.keep = true,
            "-l" | "--list" => flags.list = true,
            "-n" | "--no-name" => flags.no_name = true,
            "-N" | "--name" => flags.name = true,
            "-q" | "--quiet" => flags.quiet = true,
            "-r" | "--recursive" => flags.recursive = true,
            "-t" | "--test" => flags.test = true,
            "-v" | "--verbose" => flags.verbose = true,
            "-1" | "--fast" => flags.level = 1,
            "-2" => flags.level = 2,
            "-3" => flags.level = 3,
            "-4" => flags.level = 4,
            "-5" => flags.level = 5,
            "-6" => flags.level = 6,
            "-7" => flags.level = 7,
            "-8" => flags.level = 8,
            "-9" | "--best" => flags.level = 9,
            "-S" => {
                i += 1;
                if i >= args.len() {
                    return Err("gzip: option requires an argument -- 'S'\n".to_string());
                }
                flags.suffix = args[i].clone();
            }
            s if s.starts_with("--suffix=") => {
                flags.suffix = s["--suffix=".len()..].to_string();
            }
            s if s.starts_with('-') && s.len() > 1 => {
                // Handle combined short flags like -cd, -kv, etc.
                let chars: Vec<char> = s[1..].chars().collect();
                let mut j = 0;
                while j < chars.len() {
                    match chars[j] {
                        'c' => flags.stdout = true,
                        'd' => flags.decompress = true,
                        'f' => flags.force = true,
                        'k' => flags.keep = true,
                        'l' => flags.list = true,
                        'n' => flags.no_name = true,
                        'N' => flags.name = true,
                        'q' => flags.quiet = true,
                        'r' => flags.recursive = true,
                        't' => flags.test = true,
                        'v' => flags.verbose = true,
                        '1' => flags.level = 1,
                        '2' => flags.level = 2,
                        '3' => flags.level = 3,
                        '4' => flags.level = 4,
                        '5' => flags.level = 5,
                        '6' => flags.level = 6,
                        '7' => flags.level = 7,
                        '8' => flags.level = 8,
                        '9' => flags.level = 9,
                        'S' => {
                            if j + 1 < chars.len() {
                                flags.suffix = chars[j + 1..].iter().collect();
                            } else {
                                i += 1;
                                if i >= args.len() {
                                    return Err("gzip: option requires an argument -- 'S'\n".to_string());
                                }
                                flags.suffix = args[i].clone();
                            }
                            j = chars.len();
                            continue;
                        }
                        _ => {
                            return Err(format!("gzip: unrecognized option '{}'\n", arg));
                        }
                    }
                    j += 1;
                }
            }
            _ => files.push(arg.clone()),
        }
        i += 1;
    }
    Ok((flags, files))
}

/// Check if data starts with gzip magic bytes
fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

/// Parse gzip header to extract original filename (RFC 1952)
fn parse_gzip_header(data: &[u8]) -> Option<String> {
    if data.len() < 10 || data[0] != 0x1f || data[1] != 0x8b {
        return None;
    }
    let flags = data[3];
    let mut offset = 10usize;

    // FEXTRA
    if flags & 0x04 != 0 {
        if offset + 2 > data.len() { return None; }
        let xlen = data[offset] as usize | ((data[offset + 1] as usize) << 8);
        offset += 2 + xlen;
    }

    // FNAME
    if flags & 0x08 != 0 {
        let name_start = offset;
        while offset < data.len() && data[offset] != 0 {
            offset += 1;
        }
        if offset < data.len() {
            return Some(String::from_utf8_lossy(&data[name_start..offset]).to_string());
        }
    }

    None
}

/// Get uncompressed size from gzip trailer (last 4 bytes, little-endian u32)
fn get_uncompressed_size(data: &[u8]) -> u32 {
    if data.len() < 4 { return 0; }
    let len = data.len();
    u32::from_le_bytes([data[len - 4], data[len - 3], data[len - 2], data[len - 1]])
}

/// Compress data using gzip
fn gzip_compress(data: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// Decompress gzip data
fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).map_err(|e| e.to_string())?;
    Ok(decompressed)
}

struct GzipResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

impl GzipResult {
    fn ok() -> Self {
        Self { stdout: String::new(), stderr: String::new(), exit_code: 0 }
    }
    fn ok_stdout(stdout: String) -> Self {
        Self { stdout, stderr: String::new(), exit_code: 0 }
    }
    fn err(stderr: String) -> Self {
        Self { stdout: String::new(), stderr, exit_code: 1 }
    }
    fn silent_err() -> Self {
        Self { stdout: String::new(), stderr: String::new(), exit_code: 1 }
    }
}

fn process_file<'a>(
    ctx: &'a CommandContext,
    file: &'a str,
    flags: &'a GzipFlags,
    cmd_name: &'a str,
    decompress: bool,
    to_stdout: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = GzipResult> + Send + 'a>> {
    Box::pin(async move {
    let suffix = &flags.suffix;

    // Handle stdin
    if file == "-" || file.is_empty() {
        let input_data: Vec<u8> = ctx.stdin.chars().map(|c| c as u8).collect();
        if decompress {
            if !is_gzip(&input_data) {
                if !flags.quiet {
                    return GzipResult::err(format!("{}: stdin: not in gzip format\n", cmd_name));
                }
                return GzipResult::silent_err();
            }
            match gzip_decompress(&input_data) {
                Ok(decompressed) => {
                    return GzipResult::ok_stdout(
                        decompressed.iter().map(|&b| b as char).collect(),
                    );
                }
                Err(msg) => {
                    return GzipResult::err(format!("{}: stdin: {}\n", cmd_name, msg));
                }
            }
        } else {
            match gzip_compress(&input_data, flags.level) {
                Ok(compressed) => {
                    return GzipResult::ok_stdout(
                        compressed.iter().map(|&b| b as char).collect(),
                    );
                }
                Err(msg) => {
                    return GzipResult::err(format!("{}: stdin: {}\n", cmd_name, msg));
                }
            }
        }
    }

    // Resolve file path
    let input_path = ctx.fs.resolve_path(&ctx.cwd, file);

    // Check if file exists and handle directories
    match ctx.fs.stat(&input_path).await {
        Ok(stat) => {
            if stat.is_directory {
                if flags.recursive {
                    return process_directory(ctx, &input_path, flags, cmd_name, decompress, to_stdout).await;
                }
                if !flags.quiet {
                    return GzipResult::err(format!("{}: {}: is a directory -- ignored\n", cmd_name, file));
                }
                return GzipResult::silent_err();
            }
        }
        Err(_) => {
            return GzipResult::err(format!("{}: {}: No such file or directory\n", cmd_name, file));
        }
    }

    // Read input file
    let input_data = match ctx.fs.read_file_buffer(&input_path).await {
        Ok(data) => data,
        Err(_) => {
            return GzipResult::err(format!("{}: {}: No such file or directory\n", cmd_name, file));
        }
    };

    if decompress {
        // Check suffix
        if !file.ends_with(suffix.as_str()) {
            if !flags.quiet {
                return GzipResult::err(format!("{}: {}: unknown suffix -- ignored\n", cmd_name, file));
            }
            return GzipResult::silent_err();
        }

        if !is_gzip(&input_data) {
            if !flags.quiet {
                return GzipResult::err(format!("{}: {}: not in gzip format\n", cmd_name, file));
            }
            return GzipResult::silent_err();
        }

        let decompressed = match gzip_decompress(&input_data) {
            Ok(d) => d,
            Err(msg) => {
                return GzipResult::err(format!("{}: {}: {}\n", cmd_name, file, msg));
            }
        };

        if to_stdout {
            return GzipResult::ok_stdout(
                decompressed.iter().map(|&b| b as char).collect(),
            );
        }

        // Determine output filename
        let output_path = if flags.name {
            if let Some(orig_name) = parse_gzip_header(&input_data) {
                ctx.fs.resolve_path(&ctx.cwd, &orig_name)
            } else {
                input_path[..input_path.len() - suffix.len()].to_string()
            }
        } else {
            input_path[..input_path.len() - suffix.len()].to_string()
        };

        // Check if output exists
        if !flags.force && ctx.fs.exists(&output_path).await {
            return GzipResult::err(format!("{}: {} already exists; not overwritten\n", cmd_name, output_path));
        }

        // Write decompressed file
        if ctx.fs.write_file(&output_path, &decompressed).await.is_err() {
            return GzipResult::err(format!("{}: {}: write error\n", cmd_name, file));
        }

        // Remove original unless -k
        if !flags.keep && !to_stdout {
            let _ = ctx.fs.rm(&input_path, &RmOptions { recursive: false, force: true }).await;
        }

        if flags.verbose {
            let ratio = if !input_data.is_empty() {
                (1.0 - input_data.len() as f64 / decompressed.len() as f64) * 100.0
            } else {
                0.0
            };
            let out_name = output_path.rsplit('/').next().unwrap_or(&output_path);
            return GzipResult {
                stdout: String::new(),
                stderr: format!("{}:\t{:.1}% -- replaced with {}\n", file, ratio, out_name),
                exit_code: 0,
            };
        }

        GzipResult::ok()
    } else {
        // Compression
        if file.ends_with(suffix.as_str()) {
            if !flags.quiet {
                return GzipResult::err(format!("{}: {} already has {} suffix -- unchanged\n", cmd_name, file, suffix));
            }
            return GzipResult::silent_err();
        }

        let compressed = match gzip_compress(&input_data, flags.level) {
            Ok(c) => c,
            Err(msg) => {
                return GzipResult::err(format!("{}: {}: {}\n", cmd_name, file, msg));
            }
        };

        if to_stdout {
            return GzipResult::ok_stdout(
                compressed.iter().map(|&b| b as char).collect(),
            );
        }

        let output_path = format!("{}{}", input_path, suffix);

        // Check if output exists
        if !flags.force && ctx.fs.exists(&output_path).await {
            return GzipResult::err(format!("{}: {} already exists; not overwritten\n", cmd_name, output_path));
        }

        // Write compressed file
        if ctx.fs.write_file(&output_path, &compressed).await.is_err() {
            return GzipResult::err(format!("{}: {}: write error\n", cmd_name, file));
        }

        // Remove original unless -k
        if !flags.keep && !to_stdout {
            let _ = ctx.fs.rm(&input_path, &RmOptions { recursive: false, force: true }).await;
        }

        if flags.verbose {
            let ratio = if !input_data.is_empty() {
                (1.0 - compressed.len() as f64 / input_data.len() as f64) * 100.0
            } else {
                0.0
            };
            let out_name = output_path.rsplit('/').next().unwrap_or(&output_path);
            return GzipResult {
                stdout: String::new(),
                stderr: format!("{}:\t{:.1}% -- replaced with {}\n", file, ratio, out_name),
                exit_code: 0,
            };
        }

        GzipResult::ok()
    }
    })
}

async fn process_directory(
    ctx: &CommandContext,
    dir_path: &str,
    flags: &GzipFlags,
    cmd_name: &str,
    decompress: bool,
    to_stdout: bool,
) -> GzipResult {
    let entries = match ctx.fs.readdir_with_file_types(dir_path).await {
        Ok(e) => e,
        Err(_) => return GzipResult::err(format!("{}: {}: No such file or directory\n", cmd_name, dir_path)),
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    for entry in entries {
        let entry_path = ctx.fs.resolve_path(dir_path, &entry.name);
        if entry.is_directory {
            let result = Box::pin(process_directory(ctx, &entry_path, flags, cmd_name, decompress, to_stdout)).await;
            stdout.push_str(&result.stdout);
            stderr.push_str(&result.stderr);
            if result.exit_code != 0 { exit_code = result.exit_code; }
        } else if entry.is_file {
            let suffix = &flags.suffix;
            if decompress && !entry.name.ends_with(suffix.as_str()) { continue; }
            if !decompress && entry.name.ends_with(suffix.as_str()) { continue; }

            let relative_path = if entry_path.starts_with(&format!("{}/", ctx.cwd)) {
                entry_path[ctx.cwd.len() + 1..].to_string()
            } else {
                entry_path.clone()
            };
            let result = process_file(ctx, &relative_path, flags, cmd_name, decompress, to_stdout).await;
            stdout.push_str(&result.stdout);
            stderr.push_str(&result.stderr);
            if result.exit_code != 0 { exit_code = result.exit_code; }
        }
    }

    GzipResult { stdout, stderr, exit_code }
}

async fn list_file(
    ctx: &CommandContext,
    file: &str,
    flags: &GzipFlags,
    cmd_name: &str,
) -> GzipResult {
    let input_data = if file == "-" || file.is_empty() {
        ctx.stdin.chars().map(|c| c as u8).collect()
    } else {
        let input_path = ctx.fs.resolve_path(&ctx.cwd, file);
        match ctx.fs.read_file_buffer(&input_path).await {
            Ok(data) => data,
            Err(_) => {
                return GzipResult::err(format!("{}: {}: No such file or directory\n", cmd_name, file));
            }
        }
    };

    if !is_gzip(&input_data) {
        if !flags.quiet {
            return GzipResult::err(format!("{}: {}: not in gzip format\n", cmd_name, file));
        }
        return GzipResult::silent_err();
    }

    let compressed = input_data.len();
    let uncompressed = get_uncompressed_size(&input_data) as usize;
    let ratio = if uncompressed > 0 {
        (1.0 - compressed as f64 / uncompressed as f64) * 100.0
    } else {
        0.0
    };

    let header_name = parse_gzip_header(&input_data);
    let name = if let Some(ref orig) = header_name {
        orig.clone()
    } else if file == "-" {
        String::new()
    } else {
        file.replace(".gz", "")
    };

    let line = format!("{:>10} {:>10} {:>5.1}% {}\n", compressed, uncompressed, ratio, name);
    GzipResult::ok_stdout(line)
}

async fn test_file(
    ctx: &CommandContext,
    file: &str,
    flags: &GzipFlags,
    cmd_name: &str,
) -> GzipResult {
    let input_data = if file == "-" || file.is_empty() {
        ctx.stdin.chars().map(|c| c as u8).collect()
    } else {
        let input_path = ctx.fs.resolve_path(&ctx.cwd, file);
        match ctx.fs.read_file_buffer(&input_path).await {
            Ok(data) => data,
            Err(_) => {
                return GzipResult::err(format!("{}: {}: No such file or directory\n", cmd_name, file));
            }
        }
    };

    if !is_gzip(&input_data) {
        if !flags.quiet {
            return GzipResult::err(format!("{}: {}: not in gzip format\n", cmd_name, file));
        }
        return GzipResult::silent_err();
    }

    match gzip_decompress(&input_data) {
        Ok(_) => {
            if flags.verbose {
                GzipResult {
                    stdout: String::new(),
                    stderr: format!("{}:\tOK\n", file),
                    exit_code: 0,
                }
            } else {
                GzipResult::ok()
            }
        }
        Err(msg) => {
            GzipResult::err(format!("{}: {}: {}\n", cmd_name, file, msg))
        }
    }
}

async fn execute_gzip(ctx: CommandContext, cmd_name: &str) -> CommandResult {
    // Check --help
    if ctx.args.iter().any(|a| a == "--help") {
        let help = match cmd_name {
            "gunzip" => "Usage: gunzip [OPTION]... [FILE]...\n\
                Decompress FILEs.\n\n\
                Options:\n\
                  -c, --stdout      write to standard output, keep original files\n\
                  -f, --force       force overwrite of output file\n\
                  -k, --keep        keep input files\n\
                  -l, --list        list compressed file contents\n\
                  -n, --no-name     do not restore the original name and timestamp\n\
                  -N, --name        restore the original file name and timestamp\n\
                  -q, --quiet       suppress all warnings\n\
                  -r, --recursive   operate recursively on directories\n\
                  -S, --suffix=SUF  use suffix SUF (default: .gz)\n\
                  -t, --test        test compressed file integrity\n\
                  -v, --verbose     verbose mode\n\
                      --help        display this help and exit\n",
            "zcat" => "Usage: zcat [OPTION]... [FILE]...\n\
                Decompress FILEs to standard output.\n\n\
                Options:\n\
                  -f, --force       force\n\
                  -l, --list        list compressed file contents\n\
                  -q, --quiet       suppress all warnings\n\
                  -S, --suffix=SUF  use suffix SUF (default: .gz)\n\
                  -t, --test        test compressed file integrity\n\
                  -v, --verbose     verbose mode\n\
                      --help        display this help and exit\n",
            _ => "Usage: gzip [OPTION]... [FILE]...\n\
                Compress FILEs (by default, in-place).\n\n\
                Options:\n\
                  -c, --stdout      write to standard output, keep original files\n\
                  -d, --decompress  decompress\n\
                  -f, --force       force overwrite of output file\n\
                  -k, --keep        keep input files\n\
                  -l, --list        list compressed file contents\n\
                  -n, --no-name     do not save or restore the original name and timestamp\n\
                  -N, --name        save or restore the original file name and timestamp\n\
                  -q, --quiet       suppress all warnings\n\
                  -r, --recursive   operate recursively on directories\n\
                  -S, --suffix=SUF  use suffix SUF (default: .gz)\n\
                  -t, --test        test compressed file integrity\n\
                  -v, --verbose     verbose mode\n\
                  -1, --fast        compress faster\n\
                  -9, --best        compress better\n\
                      --help        display this help and exit\n",
        };
        return CommandResult::success(help.to_string());
    }

    let (flags, mut files) = match parse_flags(&ctx.args) {
        Ok(r) => r,
        Err(e) => return CommandResult::error(e),
    };

    // Determine mode based on command name and flags
    let decompress = cmd_name == "gunzip" || cmd_name == "zcat" || flags.decompress;
    let to_stdout = cmd_name == "zcat" || flags.stdout;

    // Handle -l (list)
    if flags.list {
        if files.is_empty() { files.push("-".to_string()); }

        let mut stdout = "  compressed uncompressed  ratio uncompressed_name\n".to_string();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let result = list_file(&ctx, file, &flags, cmd_name).await;
            stdout.push_str(&result.stdout);
            stderr.push_str(&result.stderr);
            if result.exit_code != 0 { exit_code = result.exit_code; }
        }

        return CommandResult::with_exit_code(stdout, stderr, exit_code);
    }

    // Handle -t (test)
    if flags.test {
        if files.is_empty() { files.push("-".to_string()); }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        for file in &files {
            let result = test_file(&ctx, file, &flags, cmd_name).await;
            stdout.push_str(&result.stdout);
            stderr.push_str(&result.stderr);
            if result.exit_code != 0 { exit_code = result.exit_code; }
        }

        return CommandResult::with_exit_code(stdout, stderr, exit_code);
    }

    // No files specified - use stdin
    if files.is_empty() {
        files.push("-".to_string());
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;

    for file in &files {
        let result = process_file(&ctx, file, &flags, cmd_name, decompress, to_stdout).await;
        stdout.push_str(&result.stdout);
        stderr.push_str(&result.stderr);
        if result.exit_code != 0 { exit_code = result.exit_code; }
    }

    CommandResult::with_exit_code(stdout, stderr, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use crate::fs::types::FileSystem;
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn make_ctx(
        args: Vec<&str>,
        stdin: &str,
        files: Vec<(&str, &[u8])>,
    ) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            fs.write_file(path, content).await.unwrap();
        }
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: stdin.to_string(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn: None,
        }
    }

    async fn make_ctx_str(
        args: Vec<&str>,
        stdin: &str,
        files: Vec<(&str, &str)>,
    ) -> CommandContext {
        let byte_files: Vec<(&str, &[u8])> = files.iter().map(|(p, c)| (*p, c.as_bytes())).collect();
        make_ctx(args, stdin, byte_files).await
    }

    fn compress_bytes(data: &[u8]) -> Vec<u8> {
        gzip_compress(data, 6).unwrap()
    }

    /// Decode latin1-encoded binary string back to bytes
    fn decode_binary_stdout(s: &str) -> Vec<u8> {
        s.chars().map(|c| c as u8).collect()
    }

    #[tokio::test]
    async fn test_compress_produces_valid_gzip() {
        let ctx = make_ctx_str(vec!["-c", "/test.txt"], "", vec![("/test.txt", "hello world")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let bytes = decode_binary_stdout(&result.stdout);
        assert!(bytes.len() >= 2);
        assert_eq!(bytes[0], 0x1f);
        assert_eq!(bytes[1], 0x8b);
    }

    #[tokio::test]
    async fn test_decompress_gzip_data() {
        let original = b"hello world";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["-d", "-c", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let out_bytes = decode_binary_stdout(&result.stdout);
        assert_eq!(out_bytes, original);
    }

    #[tokio::test]
    async fn test_round_trip() {
        let original = "The quick brown fox jumps over the lazy dog";
        let ctx = make_ctx_str(vec!["-k", "/test.txt"], "", vec![("/test.txt", original)]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);

        // Now decompress
        let compressed = fs.read_file_buffer("/test.txt.gz").await.unwrap();
        let ctx2 = make_ctx(vec!["-d", "-c", "/test.txt.gz"], "", vec![("/test.txt.gz", &compressed)]).await;
        let result2 = GzipCommand.execute(ctx2).await;
        assert_eq!(result2.exit_code, 0);
        let out_bytes = decode_binary_stdout(&result2.stdout);
        assert_eq!(String::from_utf8(out_bytes).unwrap(), original);
    }

    #[tokio::test]
    async fn test_stdout_flag_keeps_original() {
        let ctx = make_ctx_str(vec!["-c", "/test.txt"], "", vec![("/test.txt", "hello")]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Original file should still exist
        assert!(fs.exists("/test.txt").await);
        // stdout should have gzip data
        let bytes = decode_binary_stdout(&result.stdout);
        assert!(is_gzip(&bytes));
    }

    #[tokio::test]
    async fn test_keep_flag() {
        let ctx = make_ctx_str(vec!["-k", "/test.txt"], "", vec![("/test.txt", "hello")]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Original file should still exist
        assert!(fs.exists("/test.txt").await);
        // Compressed file should exist
        assert!(fs.exists("/test.txt.gz").await);
    }

    #[tokio::test]
    async fn test_default_removes_input() {
        let ctx = make_ctx_str(vec!["/test.txt"], "", vec![("/test.txt", "hello")]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Original file should be removed
        assert!(!fs.exists("/test.txt").await);
        // Compressed file should exist
        assert!(fs.exists("/test.txt.gz").await);
    }

    #[tokio::test]
    async fn test_decompress_flag() {
        let original = b"hello world";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["-d", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Decompressed file should exist
        let decompressed = fs.read_file_buffer("/test").await.unwrap();
        assert_eq!(decompressed, original);
        // Original .gz should be removed
        assert!(!fs.exists("/test.gz").await);
    }

    #[tokio::test]
    async fn test_list_flag() {
        let original = b"hello world test data";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["-l", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("compressed"));
        assert!(result.stdout.contains("uncompressed"));
        assert!(result.stdout.contains("ratio"));
        assert!(result.stdout.contains("test"));
    }

    #[tokio::test]
    async fn test_test_flag_valid() {
        let compressed = compress_bytes(b"hello world");
        let ctx = make_ctx(vec!["-t", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_test_flag_corrupt() {
        let corrupt = vec![0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xFF];
        let ctx = make_ctx(vec!["-t", "/test.gz"], "", vec![("/test.gz", &corrupt)]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_custom_suffix() {
        let ctx = make_ctx_str(vec!["-S", ".z", "/test.txt"], "", vec![("/test.txt", "hello")]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/test.txt.z").await);
        assert!(!fs.exists("/test.txt").await);
    }

    #[tokio::test]
    async fn test_fast_compression() {
        let ctx = make_ctx_str(vec!["-1", "-c", "/test.txt"], "", vec![("/test.txt", "hello world")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let bytes = decode_binary_stdout(&result.stdout);
        assert!(is_gzip(&bytes));
    }

    #[tokio::test]
    async fn test_best_compression() {
        let ctx = make_ctx_str(vec!["-9", "-c", "/test.txt"], "", vec![("/test.txt", "hello world")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let bytes = decode_binary_stdout(&result.stdout);
        assert!(is_gzip(&bytes));
    }

    #[tokio::test]
    async fn test_force_overwrite() {
        let compressed = compress_bytes(b"old data");
        let ctx = make_ctx(
            vec!["-f", "/test.txt"],
            "",
            vec![("/test.txt", b"new data"), ("/test.txt.gz", &compressed)],
        ).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // The .gz file should be overwritten with new compressed data
        let new_compressed = fs.read_file_buffer("/test.txt.gz").await.unwrap();
        let decompressed = gzip_decompress(&new_compressed).unwrap();
        assert_eq!(decompressed, b"new data");
    }

    #[tokio::test]
    async fn test_refuse_overwrite_without_force() {
        let compressed = compress_bytes(b"old data");
        let ctx = make_ctx(
            vec!["/test.txt"],
            "",
            vec![("/test.txt", b"new data"), ("/test.txt.gz", &compressed)],
        ).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("already exists"));
    }

    #[tokio::test]
    async fn test_gunzip_command() {
        let original = b"hello from gunzip";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let fs = ctx.fs.clone();
        let result = GunzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let decompressed = fs.read_file_buffer("/test").await.unwrap();
        assert_eq!(decompressed, original);
    }

    #[tokio::test]
    async fn test_zcat_command() {
        let original = b"hello from zcat";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let fs = ctx.fs.clone();
        let result = ZcatCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let out_bytes = decode_binary_stdout(&result.stdout);
        assert_eq!(out_bytes, original);
        // zcat should keep the original file
        assert!(fs.exists("/test.gz").await);
    }

    #[tokio::test]
    async fn test_stdin_stdout_piping() {
        // Compress from stdin
        let ctx = make_ctx(vec![], "hello stdin", vec![]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let compressed_bytes = decode_binary_stdout(&result.stdout);
        assert!(is_gzip(&compressed_bytes));

        // Decompress from stdin
        let stdin_str: String = compressed_bytes.iter().map(|&b| b as char).collect();
        let ctx2 = make_ctx(vec!["-d"], &stdin_str, vec![]).await;
        let result2 = GzipCommand.execute(ctx2).await;
        assert_eq!(result2.exit_code, 0);
        let out_bytes = decode_binary_stdout(&result2.stdout);
        assert_eq!(String::from_utf8(out_bytes).unwrap(), "hello stdin");
    }

    #[tokio::test]
    async fn test_verbose_output() {
        let ctx = make_ctx_str(vec!["-v", "/test.txt"], "", vec![("/test.txt", "hello world verbose test data")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Verbose output goes to stderr
        assert!(result.stderr.contains("%"));
        assert!(result.stderr.contains("replaced with"));
    }

    #[tokio::test]
    async fn test_recursive_directory() {
        let fs = Arc::new(InMemoryFs::new());
        use crate::fs::types::MkdirOptions;
        fs.mkdir("/dir", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/dir/a.txt", b"file a").await.unwrap();
        fs.write_file("/dir/b.txt", b"file b").await.unwrap();

        let ctx = CommandContext {
            args: vec!["-r".to_string(), "/dir".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/dir/a.txt.gz").await);
        assert!(fs.exists("/dir/b.txt.gz").await);
        assert!(!fs.exists("/dir/a.txt").await);
        assert!(!fs.exists("/dir/b.txt").await);
    }

    #[tokio::test]
    async fn test_empty_file() {
        let ctx = make_ctx_str(vec!["-k", "/empty.txt"], "", vec![("/empty.txt", "")]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/empty.txt.gz").await);
        let compressed = fs.read_file_buffer("/empty.txt.gz").await.unwrap();
        assert!(is_gzip(&compressed));
    }

    #[tokio::test]
    async fn test_missing_file_error() {
        let ctx = make_ctx_str(vec!["/nonexistent.txt"], "", vec![]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("No such file or directory"));
    }

    #[tokio::test]
    async fn test_name_flag() {
        // The -N flag should use the original name from the gzip header if available
        // Without FNAME in header, it falls back to stripping suffix
        let original = b"name test data";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["-d", "-N", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Should decompress to /test (suffix stripped)
        assert!(fs.exists("/test").await);
    }

    #[tokio::test]
    async fn test_no_name_flag() {
        let original = b"no name test";
        let compressed = compress_bytes(original);
        let ctx = make_ctx(vec!["-d", "-n", "/test.gz"], "", vec![("/test.gz", &compressed)]).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/test").await);
    }

    #[tokio::test]
    async fn test_multiple_files() {
        let ctx = make_ctx_str(
            vec!["-k", "/a.txt", "/b.txt"],
            "",
            vec![("/a.txt", "file a"), ("/b.txt", "file b")],
        ).await;
        let fs = ctx.fs.clone();
        let result = GzipCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(fs.exists("/a.txt.gz").await);
        assert!(fs.exists("/b.txt.gz").await);
    }

    #[tokio::test]
    async fn test_decompress_without_gz_suffix_error() {
        let compressed = compress_bytes(b"test data");
        let ctx = make_ctx(vec!["-d", "/test.bin"], "", vec![("/test.bin", &compressed)]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("unknown suffix"));
    }

    #[tokio::test]
    async fn test_already_has_suffix() {
        let ctx = make_ctx_str(vec!["/test.gz"], "", vec![("/test.gz", "not really gzip")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("already has .gz suffix"));
    }

    #[tokio::test]
    async fn test_not_gzip_format_decompress() {
        let ctx = make_ctx_str(vec!["-d", "/test.gz"], "", vec![("/test.gz", "not gzip data")]).await;
        let result = GzipCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("not in gzip format"));
    }

    #[tokio::test]
    async fn test_is_gzip_helper() {
        assert!(is_gzip(&[0x1f, 0x8b, 0x08]));
        assert!(!is_gzip(&[0x00, 0x00]));
        assert!(!is_gzip(&[0x1f]));
        assert!(!is_gzip(&[]));
    }

    #[tokio::test]
    async fn test_get_uncompressed_size_helper() {
        // 4 bytes little-endian: 0x0B000000 = 11
        let data = vec![0x1f, 0x8b, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00];
        assert_eq!(get_uncompressed_size(&data), 11);
    }
}
