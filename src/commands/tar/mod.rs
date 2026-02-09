// src/commands/tar/mod.rs
pub mod archive;
pub mod options;

use async_trait::async_trait;
use std::sync::Arc;

use crate::commands::{Command, CommandContext, CommandResult};
use crate::fs::types::{FileSystem, MkdirOptions};

use archive::{
    compress_gzip, create_archive, decompress_gzip, is_gzip, parse_archive,
    TarEntry,
};
use options::{parse_options, TarOperation};

pub struct TarCommand;

/// Simple glob matching: `*` matches any chars except `/`, `?` matches single char.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_inner(&pat, &txt)
}

fn glob_match_inner(pat: &[char], txt: &[char]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = None;

    while ti < txt.len() {
        if pi < pat.len() && pat[pi] == '?' {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == '*' {
            star_pi = Some(pi);
            star_ti = Some(ti);
            pi += 1;
        } else if pi < pat.len() && pat[pi] == txt[ti] {
            pi += 1;
            ti += 1;
        } else if let (Some(sp), Some(st)) = (star_pi, star_ti) {
            pi = sp + 1;
            let new_st = st + 1;
            star_ti = Some(new_st);
            ti = new_st;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }

    pi == pat.len()
}

/// Check if a path matches any exclude pattern.
fn matches_exclude(path: &str, patterns: &[String]) -> bool {
    let basename = if let Some(pos) = path.rfind('/') {
        &path[pos + 1..]
    } else {
        path
    };

    for pattern in patterns {
        // Check full path match
        if glob_match(pattern, path) {
            return true;
        }
        // Check if path starts with pattern/
        let with_slash = format!("{}/", pattern);
        if glob_match(&with_slash, path) || path.starts_with(&with_slash) {
            return true;
        }
        // Check basename match (for patterns like *.log)
        if !pattern.contains('/') && glob_match(pattern, basename) {
            return true;
        }
    }
    false
}

/// Strip N leading path components.
fn strip_components(path: &str, count: usize) -> String {
    if count == 0 {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() <= count {
        return String::new();
    }
    parts[count..].join("/")
}

/// Format file mode for verbose output (like ls -l).
fn format_mode(mode: u32, is_dir: bool) -> String {
    let prefix = if is_dir { 'd' } else { '-' };
    let perms = [
        if mode & 0o400 != 0 { 'r' } else { '-' },
        if mode & 0o200 != 0 { 'w' } else { '-' },
        if mode & 0o100 != 0 { 'x' } else { '-' },
        if mode & 0o040 != 0 { 'r' } else { '-' },
        if mode & 0o020 != 0 { 'w' } else { '-' },
        if mode & 0o010 != 0 { 'x' } else { '-' },
        if mode & 0o004 != 0 { 'r' } else { '-' },
        if mode & 0o002 != 0 { 'w' } else { '-' },
        if mode & 0o001 != 0 { 'x' } else { '-' },
    ];
    let mut s = String::with_capacity(10);
    s.push(prefix);
    for c in &perms {
        s.push(*c);
    }
    s
}

/// Format a unix timestamp for verbose output.
fn format_mtime(mtime: u64) -> String {
    // Simple date formatting from unix timestamp
    let secs = mtime;
    // Calculate date components from unix timestamp
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let mut y = 1970i64;
    let mut remaining_days = days as i64;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md {
            month = i;
            break;
        }
        remaining_days -= md;
    }

    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        month + 1,
        day,
        hours,
        minutes
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Convert SystemTime to unix timestamp (seconds since epoch).
fn system_time_to_unix(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Recursively collect files from the virtual filesystem.
async fn collect_files(
    fs: &Arc<dyn FileSystem>,
    base_path: &str,
    relative_path: &str,
    exclude: &[String],
    entries: &mut Vec<TarEntry>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let full_path = fs.resolve_path(base_path, relative_path);

    if matches_exclude(relative_path, exclude) {
        return errors;
    }

    let stat = match fs.stat(&full_path).await {
        Ok(s) => s,
        Err(e) => {
            errors.push(format!("tar: {}: {}", relative_path, e));
            return errors;
        }
    };

    if stat.is_directory {
        // Add directory entry
        entries.push(TarEntry {
            path: relative_path.to_string(),
            content: Vec::new(),
            mode: stat.mode,
            size: 0,
            mtime: system_time_to_unix(stat.mtime),
            is_directory: true,
            is_symlink: false,
            link_target: String::new(),
        });

        // Read directory contents
        let items = match fs.readdir(&full_path).await {
            Ok(items) => items,
            Err(e) => {
                errors.push(format!("tar: {}: {}", relative_path, e));
                return errors;
            }
        };

        let mut sorted_items = items;
        sorted_items.sort();

        for item in sorted_items {
            let child_rel = if relative_path.is_empty() {
                item.clone()
            } else {
                format!("{}/{}", relative_path, item)
            };
            let child_errors = Box::pin(collect_files(
                fs,
                base_path,
                &child_rel,
                exclude,
                entries,
            ))
            .await;
            errors.extend(child_errors);
        }
    } else if stat.is_file {
        let content = match fs.read_file_buffer(&full_path).await {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("tar: {}: {}", relative_path, e));
                return errors;
            }
        };
        entries.push(TarEntry {
            path: relative_path.to_string(),
            content: content.clone(),
            mode: stat.mode,
            size: content.len() as u64,
            mtime: system_time_to_unix(stat.mtime),
            is_directory: false,
            is_symlink: false,
            link_target: String::new(),
        });
    }

    errors
}

/// Read and decompress an archive from file or stdin.
async fn read_archive(
    ctx: &CommandContext,
    file: &Option<String>,
    use_gzip: bool,
) -> Result<Vec<TarEntry>, CommandResult> {
    let archive_data = if let Some(ref f) = file {
        if f == "-" {
            ctx.stdin.chars().map(|c| c as u8).collect::<Vec<u8>>()
        } else {
            let archive_path = ctx.fs.resolve_path(&ctx.cwd, f);
            match ctx.fs.read_file_buffer(&archive_path).await {
                Ok(data) => data,
                Err(_) => {
                    return Err(CommandResult::with_exit_code(
                        String::new(),
                        format!(
                            "tar: {}: Cannot open: No such file or directory\n",
                            f
                        ),
                        2,
                    ));
                }
            }
        }
    } else {
        ctx.stdin.chars().map(|c| c as u8).collect::<Vec<u8>>()
    };

    // Decompress if needed
    let data = if use_gzip || is_gzip(&archive_data) {
        match decompress_gzip(&archive_data) {
            Ok(d) => d,
            Err(e) => {
                return Err(CommandResult::with_exit_code(
                    String::new(),
                    format!("tar: gzip decompression error: {}\n", e),
                    2,
                ));
            }
        }
    } else {
        archive_data
    };

    match parse_archive(&data) {
        Ok(entries) => Ok(entries),
        Err(e) => Err(CommandResult::with_exit_code(
            String::new(),
            format!("tar: {}\n", e),
            2,
        )),
    }
}

#[async_trait]
impl Command for TarCommand {
    fn name(&self) -> &'static str {
        "tar"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let opts = match parse_options(&ctx.args) {
            Ok(o) => o,
            Err(e) => return CommandResult::error(e),
        };

        let operation = match &opts.operation {
            Some(op) => op.clone(),
            None => {
                return CommandResult::with_exit_code(
                    String::new(),
                    "tar: You must specify one of -c, -r, -u, -x, or -t\n"
                        .to_string(),
                    2,
                );
            }
        };

        // Handle files-from: read file list from a file
        let mut files = opts.files.clone();
        if let Some(ref files_from) = opts.files_from {
            let ff_path = ctx.fs.resolve_path(&ctx.cwd, files_from);
            match ctx.fs.read_file(&ff_path).await {
                Ok(content) => {
                    let additional: Vec<String> = content
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .collect();
                    files.extend(additional);
                }
                Err(_) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!(
                            "tar: {}: Cannot open: No such file or directory\n",
                            files_from
                        ),
                        2,
                    );
                }
            }
        }

        // Handle exclude-from: read exclude patterns from a file
        let mut exclude = opts.exclude.clone();
        if let Some(ref exclude_from) = opts.exclude_from {
            let ef_path = ctx.fs.resolve_path(&ctx.cwd, exclude_from);
            match ctx.fs.read_file(&ef_path).await {
                Ok(content) => {
                    let additional: Vec<String> = content
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .collect();
                    exclude.extend(additional);
                }
                Err(_) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!(
                            "tar: {}: Cannot open: No such file or directory\n",
                            exclude_from
                        ),
                        2,
                    );
                }
            }
        }

        match operation {
            TarOperation::Create => {
                self.create_archive(&ctx, &opts, &files, &exclude).await
            }
            TarOperation::Extract => {
                self.extract_archive(&ctx, &opts, &files, &exclude).await
            }
            TarOperation::List => {
                self.list_archive(&ctx, &opts, &files, &exclude).await
            }
            TarOperation::Append => {
                self.append_archive(&ctx, &opts, &files, &exclude).await
            }
            TarOperation::Update => {
                self.update_archive(&ctx, &opts, &files, &exclude).await
            }
        }
    }
}

impl TarCommand {
    async fn create_archive(
        &self,
        ctx: &CommandContext,
        opts: &options::TarOptions,
        files: &[String],
        exclude: &[String],
    ) -> CommandResult {
        if files.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "tar: Cowardly refusing to create an empty archive\n"
                    .to_string(),
                2,
            );
        }

        let work_dir = if let Some(ref dir) = opts.directory {
            ctx.fs.resolve_path(&ctx.cwd, dir)
        } else {
            ctx.cwd.clone()
        };

        let mut all_entries: Vec<TarEntry> = Vec::new();
        let mut all_errors: Vec<String> = Vec::new();
        let mut verbose_output = String::new();

        for file in files {
            let mut entries = Vec::new();
            let errors = collect_files(
                &ctx.fs,
                &work_dir,
                file,
                exclude,
                &mut entries,
            )
            .await;
            all_errors.extend(errors);

            if opts.verbose {
                for entry in &entries {
                    if entry.is_directory {
                        verbose_output
                            .push_str(&format!("{}/\n", entry.path));
                    } else {
                        verbose_output
                            .push_str(&format!("{}\n", entry.path));
                    }
                }
            }

            all_entries.extend(entries);
        }

        if all_entries.is_empty() && !all_errors.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                format!("{}\n", all_errors.join("\n")),
                2,
            );
        }

        // Create archive
        let archive_data = create_archive(&all_entries);

        // Compress if needed
        let final_data = if opts.gzip {
            match compress_gzip(&archive_data, 6) {
                Ok(d) => d,
                Err(e) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("tar: gzip compression error: {}\n", e),
                        2,
                    );
                }
            }
        } else {
            archive_data
        };

        // Write archive
        let stdout = if let Some(ref f) = opts.file {
            if f == "-" {
                final_data.iter().map(|&b| b as char).collect::<String>()
            } else {
                let archive_path = ctx.fs.resolve_path(&ctx.cwd, f);
                if let Err(e) =
                    ctx.fs.write_file(&archive_path, &final_data).await
                {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("tar: {}: {}\n", f, e),
                        2,
                    );
                }
                String::new()
            }
        } else {
            // Output to stdout as binary (latin1 chars)
            final_data.iter().map(|&b| b as char).collect::<String>()
        };

        let mut stderr = verbose_output;
        if !all_errors.is_empty() {
            stderr.push_str(&format!("{}\n", all_errors.join("\n")));
        }

        CommandResult::with_exit_code(
            stdout,
            stderr,
            if all_errors.is_empty() { 0 } else { 2 },
        )
    }

    async fn extract_archive(
        &self,
        ctx: &CommandContext,
        opts: &options::TarOptions,
        specific_files: &[String],
        exclude: &[String],
    ) -> CommandResult {
        let parsed_entries =
            match read_archive(ctx, &opts.file, opts.gzip).await {
                Ok(e) => e,
                Err(r) => return r,
            };

        let work_dir = if let Some(ref dir) = opts.directory {
            ctx.fs.resolve_path(&ctx.cwd, dir)
        } else {
            ctx.cwd.clone()
        };

        // Create target directory if needed
        if opts.directory.is_some() && !opts.to_stdout {
            let _ = ctx
                .fs
                .mkdir(&work_dir, &MkdirOptions { recursive: true })
                .await;
        }

        let mut verbose_output = String::new();
        let mut stdout_content = String::new();
        let mut errors: Vec<String> = Vec::new();

        for entry in &parsed_entries {
            // Apply strip-components
            let name = strip_components(&entry.path, opts.strip);
            if name.is_empty() {
                continue;
            }

            // Remove trailing slash for display/matching
            let display_name = if name.ends_with('/') {
                &name[..name.len() - 1]
            } else {
                &name
            };

            // Check if this file should be extracted
            if !specific_files.is_empty() {
                let matches = specific_files.iter().any(|f| {
                    name == *f
                        || display_name == f.as_str()
                        || name.starts_with(&format!("{}/", f))
                });
                if !matches {
                    continue;
                }
            }

            // Check exclude patterns
            if matches_exclude(&name, exclude) {
                continue;
            }

            let target_path = ctx.fs.resolve_path(&work_dir, &name);

            if entry.is_directory {
                if opts.to_stdout {
                    continue;
                }
                if let Err(e) = ctx
                    .fs
                    .mkdir(
                        &target_path,
                        &MkdirOptions { recursive: true },
                    )
                    .await
                {
                    errors.push(format!("tar: {}: {}", name, e));
                    continue;
                }
                if opts.verbose {
                    verbose_output.push_str(&format!("{}\n", name));
                }
            } else {
                // Handle -O (extract to stdout)
                if opts.to_stdout {
                    stdout_content.push_str(
                        &String::from_utf8_lossy(&entry.content),
                    );
                    if opts.verbose {
                        verbose_output.push_str(&format!("{}\n", name));
                    }
                    continue;
                }

                // Check -k (keep old files)
                if opts.keep_old_files {
                    if ctx.fs.stat(&target_path).await.is_ok() {
                        if opts.verbose {
                            verbose_output.push_str(&format!(
                                "{}: not overwritten, file exists\n",
                                name
                            ));
                        }
                        continue;
                    }
                }

                // Ensure parent directory exists
                if let Some(pos) = target_path.rfind('/') {
                    let parent = &target_path[..pos];
                    if !parent.is_empty() {
                        let _ = ctx
                            .fs
                            .mkdir(
                                parent,
                                &MkdirOptions { recursive: true },
                            )
                            .await;
                    }
                }

                if let Err(e) = ctx
                    .fs
                    .write_file(&target_path, &entry.content)
                    .await
                {
                    errors.push(format!("tar: {}: {}", name, e));
                    continue;
                }

                // Preserve permissions
                if opts.preserve {
                    let _ =
                        ctx.fs.chmod(&target_path, entry.mode).await;
                }

                if opts.verbose {
                    verbose_output.push_str(&format!("{}\n", name));
                }
            }
        }

        let mut stderr = verbose_output;
        if !errors.is_empty() {
            stderr.push_str(&format!("{}\n", errors.join("\n")));
        }

        CommandResult::with_exit_code(
            stdout_content,
            stderr,
            if errors.is_empty() { 0 } else { 2 },
        )
    }

    async fn list_archive(
        &self,
        ctx: &CommandContext,
        opts: &options::TarOptions,
        specific_files: &[String],
        exclude: &[String],
    ) -> CommandResult {
        let parsed_entries =
            match read_archive(ctx, &opts.file, opts.gzip).await {
                Ok(e) => e,
                Err(r) => return r,
            };

        let mut stdout = String::new();

        for entry in &parsed_entries {
            let name = strip_components(&entry.path, opts.strip);
            if name.is_empty() {
                continue;
            }

            let display_name = if name.ends_with('/') {
                &name[..name.len() - 1]
            } else {
                &name
            };

            // Check if this file should be listed
            if !specific_files.is_empty() {
                let matches = specific_files.iter().any(|f| {
                    name == *f
                        || display_name == f.as_str()
                        || name.starts_with(&format!("{}/", f))
                });
                if !matches {
                    continue;
                }
            }

            // Check exclude patterns
            if matches_exclude(&name, exclude) {
                continue;
            }

            if opts.verbose {
                let mode_str =
                    format_mode(entry.mode, entry.is_directory);
                let size = if entry.is_directory {
                    0
                } else {
                    entry.size
                };
                let date = format_mtime(entry.mtime);
                stdout.push_str(&format!(
                    "{} 0/0 {:>8} {} {}\n",
                    mode_str, size, date, name
                ));
            } else {
                stdout.push_str(&format!("{}\n", name));
            }
        }

        CommandResult::success(stdout)
    }

    async fn append_archive(
        &self,
        ctx: &CommandContext,
        opts: &options::TarOptions,
        files: &[String],
        exclude: &[String],
    ) -> CommandResult {
        if opts.file.is_none() || opts.file.as_deref() == Some("-") {
            return CommandResult::with_exit_code(
                String::new(),
                "tar: Cannot append to stdin/stdout\n".to_string(),
                2,
            );
        }

        if files.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "tar: Cowardly refusing to append nothing to archive\n"
                    .to_string(),
                2,
            );
        }

        // Read existing archive
        let existing_entries =
            match read_archive(ctx, &opts.file, false).await {
                Ok(e) => e,
                Err(r) => return r,
            };

        let work_dir = if let Some(ref dir) = opts.directory {
            ctx.fs.resolve_path(&ctx.cwd, dir)
        } else {
            ctx.cwd.clone()
        };

        // Collect new entries
        let mut new_entries: Vec<TarEntry> = Vec::new();
        let mut all_errors: Vec<String> = Vec::new();
        let mut verbose_output = String::new();

        for file in files {
            let mut entries = Vec::new();
            let errors = collect_files(
                &ctx.fs,
                &work_dir,
                file,
                exclude,
                &mut entries,
            )
            .await;
            all_errors.extend(errors);

            if opts.verbose {
                for entry in &entries {
                    if entry.is_directory {
                        verbose_output
                            .push_str(&format!("{}/\n", entry.path));
                    } else {
                        verbose_output
                            .push_str(&format!("{}\n", entry.path));
                    }
                }
            }

            new_entries.extend(entries);
        }

        // Combine existing and new entries
        let mut all_entries = existing_entries;
        all_entries.extend(new_entries);

        // Create new archive
        let archive_data = create_archive(&all_entries);

        // Write archive
        let f = opts.file.as_ref().unwrap();
        let archive_path = ctx.fs.resolve_path(&ctx.cwd, f);
        if let Err(e) =
            ctx.fs.write_file(&archive_path, &archive_data).await
        {
            return CommandResult::with_exit_code(
                String::new(),
                format!("tar: {}: {}\n", f, e),
                2,
            );
        }

        let mut stderr = verbose_output;
        if !all_errors.is_empty() {
            stderr.push_str(&format!("{}\n", all_errors.join("\n")));
        }

        CommandResult::with_exit_code(
            String::new(),
            stderr,
            if all_errors.is_empty() { 0 } else { 2 },
        )
    }

    async fn update_archive(
        &self,
        ctx: &CommandContext,
        opts: &options::TarOptions,
        files: &[String],
        exclude: &[String],
    ) -> CommandResult {
        if opts.file.is_none() || opts.file.as_deref() == Some("-") {
            return CommandResult::with_exit_code(
                String::new(),
                "tar: Cannot update stdin/stdout\n".to_string(),
                2,
            );
        }

        if files.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "tar: Cowardly refusing to update with nothing\n"
                    .to_string(),
                2,
            );
        }

        // Read existing archive
        let existing_entries =
            match read_archive(ctx, &opts.file, false).await {
                Ok(e) => e,
                Err(r) => return r,
            };

        // Build mtime map from existing entries
        let mut existing_mtimes: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for entry in &existing_entries {
            existing_mtimes
                .insert(entry.path.clone(), entry.mtime);
        }

        let work_dir = if let Some(ref dir) = opts.directory {
            ctx.fs.resolve_path(&ctx.cwd, dir)
        } else {
            ctx.cwd.clone()
        };

        // Collect new entries, but only if they're newer
        let mut newer_entries: Vec<TarEntry> = Vec::new();
        let mut all_errors: Vec<String> = Vec::new();
        let mut verbose_output = String::new();

        for file in files {
            let mut entries = Vec::new();
            let errors = collect_files(
                &ctx.fs,
                &work_dir,
                file,
                exclude,
                &mut entries,
            )
            .await;
            all_errors.extend(errors);

            for entry in entries {
                let existing_mtime =
                    existing_mtimes.get(&entry.path).copied();
                // Only include if it doesn't exist in archive or is newer
                if existing_mtime.is_none()
                    || entry.mtime > existing_mtime.unwrap()
                {
                    if opts.verbose {
                        if entry.is_directory {
                            verbose_output.push_str(&format!(
                                "{}/\n",
                                entry.path
                            ));
                        } else {
                            verbose_output.push_str(&format!(
                                "{}\n",
                                entry.path
                            ));
                        }
                    }
                    newer_entries.push(entry);
                }
            }
        }

        if newer_entries.is_empty() {
            let mut stderr = String::new();
            if !all_errors.is_empty() {
                stderr =
                    format!("{}\n", all_errors.join("\n"));
            }
            return CommandResult::with_exit_code(
                String::new(),
                stderr,
                if all_errors.is_empty() { 0 } else { 2 },
            );
        }

        // Filter out existing entries that are being updated
        let updated_names: std::collections::HashSet<String> =
            newer_entries.iter().map(|e| e.path.clone()).collect();
        let mut all_entries: Vec<TarEntry> = existing_entries
            .into_iter()
            .filter(|e| !updated_names.contains(&e.path))
            .collect();
        all_entries.extend(newer_entries);

        // Create new archive
        let archive_data = create_archive(&all_entries);

        // Write archive
        let f = opts.file.as_ref().unwrap();
        let archive_path = ctx.fs.resolve_path(&ctx.cwd, f);
        if let Err(e) =
            ctx.fs.write_file(&archive_path, &archive_data).await
        {
            return CommandResult::with_exit_code(
                String::new(),
                format!("tar: {}: {}\n", f, e),
                2,
            );
        }

        let mut stderr = verbose_output;
        if !all_errors.is_empty() {
            stderr.push_str(&format!("{}\n", all_errors.join("\n")));
        }

        CommandResult::with_exit_code(
            String::new(),
            stderr,
            if all_errors.is_empty() { 0 } else { 2 },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::collections::HashMap;

    async fn make_ctx(
        args: Vec<&str>,
        stdin: &str,
        files: Vec<(&str, &[u8])>,
    ) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (path, content) in files {
            // Ensure parent directories exist
            if let Some(pos) = path.rfind('/') {
                let parent = &path[..pos];
                if !parent.is_empty() {
                    let _ = fs
                        .mkdir(parent, &MkdirOptions { recursive: true })
                        .await;
                }
            }
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
        let byte_files: Vec<(&str, &[u8])> =
            files.iter().map(|(p, c)| (*p, c.as_bytes())).collect();
        make_ctx(args, stdin, byte_files).await
    }

    #[tokio::test]
    async fn test_create_tar_single_file() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello, World!")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        assert!(data.len() > 512);
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
        assert_eq!(entries[0].content, b"Hello, World!");
    }

    #[tokio::test]
    async fn test_create_tar_directory_tree() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "project"],
            "",
            vec![
                ("/project/main.rs", "fn main() {}"),
                ("/project/lib.rs", "pub fn hello() {}"),
            ],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert!(entries.len() >= 3);
        assert!(entries[0].is_directory);
    }

    #[tokio::test]
    async fn test_extract_tar_to_filesystem() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello, World!")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xf".to_string(),
                "archive.tar".to_string(),
                "-C".to_string(),
                "/output".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/output/hello.txt").await.unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_create_and_extract_round_trip() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "a.txt", "b.txt"],
            "",
            vec![("/a.txt", "aaa"), ("/b.txt", "bbb")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xf".to_string(),
                "archive.tar".to_string(),
                "-C".to_string(),
                "/out".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs.read_file("/out/a.txt").await.unwrap(), "aaa");
        assert_eq!(fs.read_file("/out/b.txt").await.unwrap(), "bbb");
    }

    #[tokio::test]
    async fn test_create_tar_gz() {
        let ctx = make_ctx_str(
            vec!["-czf", "archive.tar.gz", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello, World!")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar.gz").await.unwrap();
        assert!(is_gzip(&data));
    }

    #[tokio::test]
    async fn test_extract_tar_gz() {
        let ctx = make_ctx_str(
            vec!["-czf", "archive.tar.gz", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello, World!")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xzf".to_string(),
                "archive.tar.gz".to_string(),
                "-C".to_string(),
                "/out".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            fs.read_file("/out/hello.txt").await.unwrap(),
            "Hello, World!"
        );
    }

    #[tokio::test]
    async fn test_list_archive_contents() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "a.txt", "b.txt"],
            "",
            vec![("/a.txt", "aaa"), ("/b.txt", "bbb")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec!["-tf".to_string(), "archive.tar".to_string()],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("a.txt"));
        assert!(result.stdout.contains("b.txt"));
    }

    #[tokio::test]
    async fn test_list_verbose_output() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello!")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-tvf".to_string(),
                "archive.tar".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello.txt"));
        assert!(result.stdout.contains("0/0"));
        assert!(result.stdout.contains("-r"));
    }

    #[tokio::test]
    async fn test_auto_compress_from_extension() {
        let ctx = make_ctx_str(
            vec!["-acf", "archive.tar.gz", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello!")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar.gz").await.unwrap();
        assert!(is_gzip(&data));
    }

    #[tokio::test]
    async fn test_exclude_patterns() {
        let ctx = make_ctx_str(
            vec![
                "--exclude=*.log",
                "-cf",
                "archive.tar",
                "a.txt",
                "b.log",
            ],
            "",
            vec![("/a.txt", "aaa"), ("/b.log", "bbb")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a.txt");
    }

    #[tokio::test]
    async fn test_strip_components_on_extract() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "dir"],
            "",
            vec![("/dir/sub/file.txt", "content")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xf".to_string(),
                "archive.tar".to_string(),
                "--strip-components=2".to_string(),
                "-C".to_string(),
                "/out".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            fs.read_file("/out/file.txt").await.unwrap(),
            "content"
        );
    }

    #[tokio::test]
    async fn test_extract_to_stdout() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello, World!")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xOf".to_string(),
                "archive.tar".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Hello, World!");
    }

    #[tokio::test]
    async fn test_keep_old_files() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "new content")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        fs.write_file("/out/hello.txt", b"old content")
            .await
            .unwrap();

        let ctx2 = CommandContext {
            args: vec![
                "-xkf".to_string(),
                "archive.tar".to_string(),
                "-C".to_string(),
                "/out".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            fs.read_file("/out/hello.txt").await.unwrap(),
            "old content"
        );
    }

    #[tokio::test]
    async fn test_change_directory() {
        let ctx = make_ctx_str(
            vec!["-cf", "/archive.tar", "-C", "/src", "hello.txt"],
            "",
            vec![("/src/hello.txt", "Hello!")],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[tokio::test]
    async fn test_append_to_archive() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "a.txt"],
            "",
            vec![("/a.txt", "aaa")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        fs.write_file("/b.txt", b"bbb").await.unwrap();
        let ctx2 = CommandContext {
            args: vec![
                "-rf".to_string(),
                "archive.tar".to_string(),
                "b.txt".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);

        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "a.txt");
        assert_eq!(entries[1].path, "b.txt");
    }

    #[tokio::test]
    async fn test_update_archive() {
        // Create initial archive with a file
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "a.txt"],
            "",
            vec![("/a.txt", "old")],
        )
        .await;
        let fs = ctx.fs.clone();
        TarCommand.execute(ctx).await;

        // Add a new file that doesn't exist in the archive (always added by -u)
        fs.write_file("/b.txt", b"new_file").await.unwrap();

        let ctx2 = CommandContext {
            args: vec![
                "-uf".to_string(),
                "archive.tar".to_string(),
                "b.txt".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);

        // Verify the archive now has both files
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 2);
        let a_entry = entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert_eq!(a_entry.content, b"old");
        let b_entry = entries.iter().find(|e| e.path == "b.txt").unwrap();
        assert_eq!(b_entry.content, b"new_file");
    }

    #[tokio::test]
    async fn test_files_from() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar", "-T", "filelist.txt"],
            "",
            vec![
                ("/filelist.txt", "a.txt\nb.txt\n"),
                ("/a.txt", "aaa"),
                ("/b.txt", "bbb"),
            ],
        )
        .await;
        let fs = ctx.fs.clone();
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_verbose_output_during_create() {
        let ctx = make_ctx_str(
            vec!["-cvf", "archive.tar", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello!")],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stderr.contains("hello.txt"));
    }

    #[tokio::test]
    async fn test_missing_archive_file_error() {
        let ctx = make_ctx_str(
            vec!["-xf", "nonexistent.tar"],
            "",
            vec![],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Cannot open"));
    }

    #[tokio::test]
    async fn test_empty_directory_in_archive() {
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/emptydir", &MkdirOptions { recursive: true })
            .await
            .unwrap();
        let ctx = CommandContext {
            args: vec![
                "-cf".to_string(),
                "archive.tar".to_string(),
                "emptydir".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let data = fs.read_file_buffer("/archive.tar").await.unwrap();
        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_directory);
    }

    #[tokio::test]
    async fn test_preserve_permissions() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/script.sh", b"#!/bin/bash")
            .await
            .unwrap();
        fs.chmod("/script.sh", 0o755).await.unwrap();

        let ctx = CommandContext {
            args: vec![
                "-cf".to_string(),
                "archive.tar".to_string(),
                "script.sh".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        TarCommand.execute(ctx).await;

        let ctx2 = CommandContext {
            args: vec![
                "-xpf".to_string(),
                "archive.tar".to_string(),
                "-C".to_string(),
                "/out".to_string(),
            ],
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: fs.clone(),
            exec_fn: None,
            fetch_fn: None,
        };
        let result = TarCommand.execute(ctx2).await;
        assert_eq!(result.exit_code, 0);
        let stat = fs.stat("/out/script.sh").await.unwrap();
        assert_eq!(stat.mode, 0o755);
    }

    #[tokio::test]
    async fn test_no_operation_error() {
        let ctx = make_ctx_str(
            vec!["-f", "archive.tar"],
            "",
            vec![],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("You must specify"));
    }

    #[tokio::test]
    async fn test_empty_archive_error() {
        let ctx = make_ctx_str(
            vec!["-cf", "archive.tar"],
            "",
            vec![],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Cowardly refusing"));
    }

    #[tokio::test]
    async fn test_append_requires_file() {
        let ctx = make_ctx_str(
            vec!["-r", "a.txt"],
            "",
            vec![("/a.txt", "aaa")],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("Cannot append"));
    }

    #[tokio::test]
    async fn test_create_to_stdout() {
        let ctx = make_ctx_str(
            vec!["-c", "hello.txt"],
            "",
            vec![("/hello.txt", "Hello!")],
        )
        .await;
        let result = TarCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.is_empty());
        let bytes: Vec<u8> =
            result.stdout.chars().map(|c| c as u8).collect();
        let entries = parse_archive(&bytes).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn test_glob_match_basic() {
        assert!(glob_match("*.txt", "hello.txt"));
        assert!(!glob_match("*.txt", "hello.rs"));
        assert!(glob_match("hello.*", "hello.txt"));
        assert!(glob_match("?", "a"));
        assert!(!glob_match("?", "ab"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn test_strip_components_fn() {
        assert_eq!(strip_components("a/b/c", 0), "a/b/c");
        assert_eq!(strip_components("a/b/c", 1), "b/c");
        assert_eq!(strip_components("a/b/c", 2), "c");
        assert_eq!(strip_components("a/b/c", 3), "");
        assert_eq!(strip_components("a/b/c", 10), "");
    }

    #[test]
    fn test_format_mode_fn() {
        assert_eq!(format_mode(0o644, false), "-rw-r--r--");
        assert_eq!(format_mode(0o755, true), "drwxr-xr-x");
        assert_eq!(format_mode(0o777, false), "-rwxrwxrwx");
        assert_eq!(format_mode(0o000, false), "----------");
    }

    #[test]
    fn test_matches_exclude_fn() {
        let patterns = vec!["*.log".to_string()];
        assert!(matches_exclude("test.log", &patterns));
        assert!(!matches_exclude("test.txt", &patterns));
        assert!(matches_exclude("dir/test.log", &patterns));
    }
}
