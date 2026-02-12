use async_trait::async_trait;
use regex_lite::Regex;
use std::collections::HashSet;
use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::FileSystem;

pub struct RgCommand;

struct RgOptions {
    patterns: Vec<String>,
    ignore_case: bool,
    fixed_strings: bool,
    word_regexp: bool,
    line_regexp: bool,
    invert_match: bool,
    count: bool,
    files_with_matches: bool,
    files_without_match: bool,
    only_matching: bool,
    max_count: Option<usize>,
    line_number: bool,
    no_filename: bool,
    hidden: bool,
    no_ignore: bool,
    max_depth: Option<usize>,
    context_before: usize,
    context_after: usize,
    globs: Vec<String>,
    type_filters: Vec<String>,
    type_not_filters: Vec<String>,
}

impl Default for RgOptions {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            ignore_case: false,
            fixed_strings: false,
            word_regexp: false,
            line_regexp: false,
            invert_match: false,
            count: false,
            files_with_matches: false,
            files_without_match: false,
            only_matching: false,
            max_count: None,
            line_number: true,
            no_filename: false,
            hidden: false,
            no_ignore: false,
            max_depth: None,
            context_before: 0,
            context_after: 0,
            globs: Vec::new(),
            type_filters: Vec::new(),
            type_not_filters: Vec::new(),
        }
    }
}

fn get_type_extensions(type_name: &str) -> Option<Vec<&'static str>> {
    match type_name {
        "js" => Some(vec![".js", ".mjs", ".cjs"]),
        "ts" => Some(vec![".ts", ".tsx", ".mts", ".cts"]),
        "py" => Some(vec![".py", ".pyi"]),
        "rs" => Some(vec![".rs"]),
        "go" => Some(vec![".go"]),
        "java" => Some(vec![".java"]),
        "c" => Some(vec![".c", ".h"]),
        "cpp" => Some(vec![".cpp", ".cc", ".cxx", ".hpp", ".hh", ".hxx"]),
        "html" => Some(vec![".html", ".htm"]),
        "css" => Some(vec![".css"]),
        "json" => Some(vec![".json"]),
        "yaml" | "yml" => Some(vec![".yaml", ".yml"]),
        "xml" => Some(vec![".xml"]),
        "md" => Some(vec![".md", ".markdown"]),
        "sh" => Some(vec![".sh", ".bash"]),
        "txt" => Some(vec![".txt"]),
        _ => None,
    }
}

fn matches_type(path: &str, type_filters: &[String], type_not_filters: &[String]) -> bool {
    if type_filters.is_empty() && type_not_filters.is_empty() {
        return true;
    }

    for t in type_not_filters {
        if let Some(exts) = get_type_extensions(t) {
            for ext in exts {
                if path.ends_with(ext) {
                    return false;
                }
            }
        }
    }

    if type_filters.is_empty() {
        return true;
    }

    for t in type_filters {
        if let Some(exts) = get_type_extensions(t) {
            for ext in exts {
                if path.ends_with(ext) {
                    return true;
                }
            }
        }
    }
    false
}

fn matches_glob(path: &str, globs: &[String]) -> bool {
    if globs.is_empty() {
        return true;
    }
    let filename = path.rsplit('/').next().unwrap_or(path);
    for glob in globs {
        let pattern = glob.replace("*", ".*").replace("?", ".");
        if let Ok(re) = Regex::new(&format!("^{}$", pattern)) {
            if re.is_match(filename) {
                return true;
            }
        }
    }
    false
}

#[async_trait]
impl Command for RgCommand {
    fn name(&self) -> &'static str { "rg" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "rg - recursively search for a pattern\n\nUsage: rg [OPTIONS] PATTERN [PATH ...]\n\nOptions:\n  -i, --ignore-case       case-insensitive search\n  -F, --fixed-strings     treat pattern as literal\n  -w, --word-regexp       match whole words\n  -v, --invert-match      select non-matching lines\n  -c, --count             print match count per file\n  -l, --files-with-matches  print only filenames\n  -o, --only-matching     print only matching parts\n  -n, --line-number       print line numbers (default)\n  -N, --no-line-number    suppress line numbers\n  --hidden                search hidden files\n  -g, --glob GLOB         include files matching GLOB\n  -t, --type TYPE         search only TYPE files\n  -A NUM                  print NUM lines after match\n  -B NUM                  print NUM lines before match\n  -C NUM                  print NUM context lines\n".to_string()
            );
        }

        let mut opts = RgOptions::default();
        let mut paths: Vec<String> = Vec::new();
        let mut i = 0;
        let args = &ctx.args;

        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "-i" | "--ignore-case" => opts.ignore_case = true,
                "-F" | "--fixed-strings" => opts.fixed_strings = true,
                "-w" | "--word-regexp" => opts.word_regexp = true,
                "-x" | "--line-regexp" => opts.line_regexp = true,
                "-v" | "--invert-match" => opts.invert_match = true,
                "-c" | "--count" => opts.count = true,
                "-l" | "--files-with-matches" => opts.files_with_matches = true,
                "--files-without-match" => opts.files_without_match = true,
                "-o" | "--only-matching" => opts.only_matching = true,
                "-n" | "--line-number" => opts.line_number = true,
                "-N" | "--no-line-number" => opts.line_number = false,
                "-I" | "--no-filename" => opts.no_filename = true,
                "--hidden" => opts.hidden = true,
                "--no-ignore" => opts.no_ignore = true,
                "-e" | "--regexp" => {
                    i += 1;
                    if i < args.len() {
                        opts.patterns.push(args[i].clone());
                    }
                }
                "-g" | "--glob" => {
                    i += 1;
                    if i < args.len() {
                        opts.globs.push(args[i].clone());
                    }
                }
                "-t" | "--type" => {
                    i += 1;
                    if i < args.len() {
                        opts.type_filters.push(args[i].clone());
                    }
                }
                "-T" | "--type-not" => {
                    i += 1;
                    if i < args.len() {
                        opts.type_not_filters.push(args[i].clone());
                    }
                }
                "-m" | "--max-count" => {
                    i += 1;
                    if i < args.len() {
                        opts.max_count = args[i].parse().ok();
                    }
                }
                "-d" | "--max-depth" => {
                    i += 1;
                    if i < args.len() {
                        opts.max_depth = args[i].parse().ok();
                    }
                }
                "-A" => {
                    i += 1;
                    if i < args.len() {
                        opts.context_after = args[i].parse().unwrap_or(0);
                    }
                }
                "-B" => {
                    i += 1;
                    if i < args.len() {
                        opts.context_before = args[i].parse().unwrap_or(0);
                    }
                }
                "-C" => {
                    i += 1;
                    if i < args.len() {
                        let n = args[i].parse().unwrap_or(0);
                        opts.context_before = n;
                        opts.context_after = n;
                    }
                }
                _ if arg.starts_with("-") => {}
                _ => {
                    if opts.patterns.is_empty() {
                        opts.patterns.push(arg.clone());
                    } else {
                        paths.push(arg.clone());
                    }
                }
            }
            i += 1;
        }

        if opts.patterns.is_empty() {
            return CommandResult::error("rg: no pattern given\n".to_string());
        }

        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let pattern_str = if opts.fixed_strings {
            regex_lite::escape(&opts.patterns.join("|"))
        } else {
            opts.patterns.join("|")
        };

        let pattern_str = if opts.word_regexp {
            format!(r"\b({})\b", pattern_str)
        } else if opts.line_regexp {
            format!("^({})$", pattern_str)
        } else {
            pattern_str
        };

        let pattern_str = if opts.ignore_case {
            format!("(?i){}", pattern_str)
        } else {
            pattern_str
        };

        let regex = match Regex::new(&pattern_str) {
            Ok(r) => r,
            Err(e) => return CommandResult::error(format!("rg: invalid pattern: {}\n", e)),
        };

        let mut stdout = String::new();
        let mut found_match = false;
        let mut files_to_search = Vec::new();

        for path in &paths {
            let full_path = ctx.fs.resolve_path(&ctx.cwd, path);
            collect_files(&ctx.fs, &full_path, &opts, 0, &mut files_to_search).await;
        }

        let single_file = files_to_search.len() == 1;

        for file_path in files_to_search {
            let content = match ctx.fs.read_file(&file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = 0;
            let mut matched_lines: Vec<(usize, String)> = Vec::new();

            for (line_num, line) in lines.iter().enumerate() {
                let is_match = regex.is_match(line);
                let should_output = if opts.invert_match { !is_match } else { is_match };

                if should_output {
                    if let Some(max) = opts.max_count {
                        if file_matches >= max {
                            break;
                        }
                    }
                    file_matches += 1;
                    found_match = true;

                    if opts.only_matching && !opts.invert_match {
                        for m in regex.find_iter(line) {
                            matched_lines.push((line_num + 1, m.as_str().to_string()));
                        }
                    } else {
                        matched_lines.push((line_num + 1, line.to_string()));
                    }
                }
            }

            if opts.files_with_matches {
                if file_matches > 0 {
                    stdout.push_str(&file_path);
                    stdout.push('\n');
                }
            } else if opts.files_without_match {
                if file_matches == 0 {
                    stdout.push_str(&file_path);
                    stdout.push('\n');
                }
            } else if opts.count {
                if opts.no_filename || single_file {
                    stdout.push_str(&format!("{}\n", file_matches));
                } else {
                    stdout.push_str(&format!("{}:{}\n", file_path, file_matches));
                }
            } else {
                for (line_num, line) in matched_lines {
                    if opts.no_filename || single_file {
                        if opts.line_number {
                            stdout.push_str(&format!("{}:{}\n", line_num, line));
                        } else {
                            stdout.push_str(&format!("{}\n", line));
                        }
                    } else if opts.line_number {
                        stdout.push_str(&format!("{}:{}:{}\n", file_path, line_num, line));
                    } else {
                        stdout.push_str(&format!("{}:{}\n", file_path, line));
                    }
                }
            }
        }

        CommandResult::with_exit_code(stdout, String::new(), if found_match { 0 } else { 1 })
    }
}

async fn collect_files(
    fs: &std::sync::Arc<dyn FileSystem>,
    path: &str,
    opts: &RgOptions,
    depth: usize,
    files: &mut Vec<String>,
) {
    if let Some(max_depth) = opts.max_depth {
        if depth > max_depth {
            return;
        }
    }

    let stat = match fs.stat(path).await {
        Ok(s) => s,
        Err(_) => return,
    };

    if stat.is_file {
        let filename = path.rsplit('/').next().unwrap_or(path);
        if !opts.hidden && filename.starts_with('.') {
            return;
        }
        if !matches_type(path, &opts.type_filters, &opts.type_not_filters) {
            return;
        }
        if !matches_glob(path, &opts.globs) {
            return;
        }
        files.push(path.to_string());
    } else if stat.is_directory {
        let dirname = path.rsplit('/').next().unwrap_or(path);
        if !opts.hidden && dirname.starts_with('.') && depth > 0 {
            return;
        }

        if let Ok(entries) = fs.readdir(path).await {
            let mut sorted_entries: Vec<_> = entries.into_iter().collect();
            sorted_entries.sort();
            for entry in sorted_entries {
                let child_path = if path.ends_with('/') {
                    format!("{}{}", path, entry)
                } else {
                    format!("{}/{}", path, entry)
                };
                Box::pin(collect_files(fs, &child_path, opts, depth + 1, files)).await;
            }
        }
    }
}
