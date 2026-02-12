use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct FileCommand;

const HELP: &str = "file - determine file type

Usage: file [OPTION]... FILE...

Options:
  -b, --brief      do not prepend filenames to output
  -i, --mime       output MIME type strings
  -L               follow symlinks (default)
  --help           display this help and exit";

fn get_extension(filename: &str) -> Option<&str> {
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    if basename.starts_with('.') && !basename[1..].contains('.') {
        return Some(basename);
    }
    basename.rfind('.').map(|i| &basename[i..])
}

fn detect_type_by_extension(ext: &str) -> (&'static str, &'static str) {
    match ext.to_lowercase().as_str() {
        ".js" | ".mjs" | ".cjs" => ("JavaScript source", "text/javascript"),
        ".ts" | ".tsx" => ("TypeScript source", "text/typescript"),
        ".jsx" => ("JavaScript JSX source", "text/javascript"),
        ".py" => ("Python script", "text/x-python"),
        ".rb" => ("Ruby script", "text/x-ruby"),
        ".go" => ("Go source", "text/x-go"),
        ".rs" => ("Rust source", "text/x-rust"),
        ".c" => ("C source", "text/x-c"),
        ".h" => ("C header", "text/x-c"),
        ".cpp" | ".cc" | ".cxx" => ("C++ source", "text/x-c++"),
        ".hpp" => ("C++ header", "text/x-c++"),
        ".java" => ("Java source", "text/x-java"),
        ".sh" | ".bash" => ("Bourne-Again shell script", "text/x-shellscript"),
        ".json" => ("JSON data", "application/json"),
        ".yaml" | ".yml" => ("YAML data", "text/yaml"),
        ".xml" => ("XML document", "application/xml"),
        ".html" | ".htm" => ("HTML document", "text/html"),
        ".css" => ("CSS stylesheet", "text/css"),
        ".md" | ".markdown" => ("Markdown document", "text/markdown"),
        ".txt" => ("ASCII text", "text/plain"),
        ".toml" => ("TOML data", "text/toml"),
        ".csv" => ("CSV text", "text/csv"),
        ".svg" => ("SVG image", "image/svg+xml"),
        ".png" => ("PNG image data", "image/png"),
        ".jpg" | ".jpeg" => ("JPEG image data", "image/jpeg"),
        ".gif" => ("GIF image data", "image/gif"),
        ".webp" => ("WebP image data", "image/webp"),
        ".pdf" => ("PDF document", "application/pdf"),
        ".zip" => ("Zip archive data", "application/zip"),
        ".gz" | ".gzip" => ("gzip compressed data", "application/gzip"),
        ".tar" => ("POSIX tar archive", "application/x-tar"),
        ".mp3" => ("Audio file with ID3", "audio/mpeg"),
        ".mp4" => ("ISO Media, MPEG-4", "video/mp4"),
        _ => ("data", "application/octet-stream"),
    }
}

fn detect_type_by_content(content: &str, filename: &str) -> (&'static str, &'static str) {
    if content.is_empty() {
        return ("empty", "inode/x-empty");
    }

    if content.starts_with("#!") {
        let first_line = content.lines().next().unwrap_or("");
        if first_line.contains("python") {
            return ("Python script, ASCII text executable", "text/x-python");
        }
        if first_line.contains("node") || first_line.contains("bun") || first_line.contains("deno") {
            return ("JavaScript script, ASCII text executable", "text/javascript");
        }
        if first_line.contains("bash") {
            return ("Bourne-Again shell script, ASCII text executable", "text/x-shellscript");
        }
        if first_line.contains("sh") {
            return ("POSIX shell script, ASCII text executable", "text/x-shellscript");
        }
        if first_line.contains("ruby") {
            return ("Ruby script, ASCII text executable", "text/x-ruby");
        }
        return ("script, ASCII text executable", "text/plain");
    }

    let trimmed = content.trim_start();
    if trimmed.starts_with("<?xml") {
        return ("XML document", "application/xml");
    }
    if trimmed.to_lowercase().starts_with("<!doctype html") || trimmed.to_lowercase().starts_with("<html") {
        return ("HTML document", "text/html");
    }

    if let Some(ext) = get_extension(filename) {
        let (desc, mime) = detect_type_by_extension(ext);
        if desc != "data" {
            return (desc, mime);
        }
    }

    let has_unicode = content.chars().take(8192).any(|c| c as u32 > 127);
    if has_unicode {
        ("UTF-8 Unicode text", "text/plain; charset=utf-8")
    } else {
        ("ASCII text", "text/plain")
    }
}

#[async_trait]
impl Command for FileCommand {
    fn name(&self) -> &'static str {
        "file"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let mut brief = false;
        let mut mime_mode = false;
        let mut files = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "--help" => return CommandResult::success(format!("{}\n", HELP)),
                "--brief" | "-b" => brief = true,
                "--mime" | "--mime-type" | "-i" => mime_mode = true,
                "-L" | "--dereference" => {}
                s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                    for c in s[1..].chars() {
                        match c {
                            'b' => brief = true,
                            'i' => mime_mode = true,
                            'L' => {}
                            _ => return CommandResult::error(format!("file: invalid option -- '{}'\n", c)),
                        }
                    }
                }
                _ => files.push(arg.clone()),
            }
        }

        if files.is_empty() {
            return CommandResult::error("Usage: file [-bLi] FILE...\n".to_string());
        }

        let mut output = String::new();
        let mut exit_code = 0;

        for file in &files {
            let path = ctx.fs.resolve_path(&ctx.cwd, file);
            match ctx.fs.stat(&path).await {
                Ok(stat) => {
                    if stat.is_directory {
                        let result = if mime_mode { "inode/directory" } else { "directory" };
                        if brief {
                            output.push_str(&format!("{}\n", result));
                        } else {
                            output.push_str(&format!("{}: {}\n", file, result));
                        }
                    } else {
                        let content = ctx.fs.read_file(&path).await.unwrap_or_default();
                        let (desc, mime) = detect_type_by_content(&content, file);
                        let result = if mime_mode { mime } else { desc };
                        if brief {
                            output.push_str(&format!("{}\n", result));
                        } else {
                            output.push_str(&format!("{}: {}\n", file, result));
                        }
                    }
                }
                Err(_) => {
                    if brief {
                        output.push_str("cannot open\n");
                    } else {
                        output.push_str(&format!("{}: cannot open (No such file or directory)\n", file));
                    }
                    exit_code = 1;
                }
            }
        }

        CommandResult::with_exit_code(output, String::new(), exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::{InMemoryFs, FileSystem};

    fn create_ctx(args: Vec<&str>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = FileCommand.execute(ctx).await;
        assert!(result.stdout.contains("file"));
        assert!(result.stdout.contains("-b"));
    }

    #[tokio::test]
    async fn test_no_args() {
        let ctx = create_ctx(vec![]);
        let result = FileCommand.execute(ctx).await;
        assert!(result.stderr.contains("Usage"));
    }

    #[tokio::test]
    async fn test_text_file() {
        let mut ctx = create_ctx(vec!["/test.txt"]);
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/test.txt", b"hello world").await.unwrap();
        ctx.fs = fs;
        let result = FileCommand.execute(ctx).await;
        assert!(result.stdout.contains("text"));
    }

    #[tokio::test]
    async fn test_directory() {
        let mut ctx = create_ctx(vec!["/"]);
        let fs = Arc::new(InMemoryFs::new());
        ctx.fs = fs;
        let result = FileCommand.execute(ctx).await;
        assert!(result.stdout.contains("directory"));
    }

    #[tokio::test]
    async fn test_not_found() {
        let ctx = create_ctx(vec!["/nonexistent"]);
        let result = FileCommand.execute(ctx).await;
        assert!(result.stdout.contains("cannot open"));
    }

    #[test]
    fn test_get_extension() {
        assert_eq!(get_extension("test.txt"), Some(".txt"));
        assert_eq!(get_extension("file.tar.gz"), Some(".gz"));
        assert_eq!(get_extension(".hidden"), Some(".hidden"));
    }

    #[test]
    fn test_detect_type_by_extension() {
        assert_eq!(detect_type_by_extension(".js").0, "JavaScript source");
        assert_eq!(detect_type_by_extension(".py").0, "Python script");
        assert_eq!(detect_type_by_extension(".json").0, "JSON data");
    }
}
