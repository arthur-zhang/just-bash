// src/commands/tar/options.rs

#[derive(Debug, Clone, PartialEq)]
pub enum TarOperation {
    Create,  // -c
    Extract, // -x
    List,    // -t
    Append,  // -r
    Update,  // -u
}

#[derive(Debug, Clone)]
pub struct TarOptions {
    pub operation: Option<TarOperation>,
    pub file: Option<String>,
    pub directory: Option<String>,
    pub verbose: bool,
    pub gzip: bool,
    pub auto_compress: bool,
    pub to_stdout: bool,
    pub keep_old_files: bool,
    pub touch: bool,
    pub preserve: bool,
    pub files_from: Option<String>,
    pub exclude: Vec<String>,
    pub exclude_from: Option<String>,
    pub strip: usize,
    pub wildcards: bool,
    pub files: Vec<String>,
}

impl Default for TarOptions {
    fn default() -> Self {
        Self {
            operation: None,
            file: None,
            directory: None,
            verbose: false,
            gzip: false,
            auto_compress: false,
            to_stdout: false,
            keep_old_files: false,
            touch: false,
            preserve: false,
            files_from: None,
            exclude: Vec::new(),
            exclude_from: None,
            strip: 0,
            wildcards: false,
            files: Vec::new(),
        }
    }
}
/// Detect gzip from file extension for auto-compress mode
fn detect_gzip_from_extension(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".taz")
}

/// Options that consume a value: f, C, T, X
fn is_value_option(c: char) -> bool {
    matches!(c, 'f' | 'C' | 'T' | 'X')
}

fn set_value_option(
    opts: &mut TarOptions,
    c: char,
    value: String,
) {
    match c {
        'f' => opts.file = Some(value),
        'C' => opts.directory = Some(value),
        'T' => opts.files_from = Some(value),
        'X' => opts.exclude_from = Some(value),
        _ => {}
    }
}

fn apply_short_flag(opts: &mut TarOptions, c: char) -> Result<(), String> {
    match c {
        'c' => opts.operation = Some(TarOperation::Create),
        'x' => opts.operation = Some(TarOperation::Extract),
        't' => opts.operation = Some(TarOperation::List),
        'r' => opts.operation = Some(TarOperation::Append),
        'u' => opts.operation = Some(TarOperation::Update),
        'z' => opts.gzip = true,
        'a' => opts.auto_compress = true,
        'v' => opts.verbose = true,
        'O' => opts.to_stdout = true,
        'k' => opts.keep_old_files = true,
        'm' => opts.touch = true,
        'p' => opts.preserve = true,
        _ => {
            return Err(format!(
                "tar: unrecognized option '-{}'\n",
                c
            ));
        }
    }
    Ok(())
}

pub fn parse_options(args: &[String]) -> Result<TarOptions, String> {
    let mut opts = TarOptions::default();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].clone();

        // Combined short options (e.g., -cvzf archive.tar)
        if arg.starts_with('-')
            && !arg.starts_with("--")
            && arg.len() > 2
        {
            // Check if it's a negative number
            if arg[1..].chars().all(|c| c.is_ascii_digit()) {
                opts.files.push(arg);
                i += 1;
                continue;
            }

            let chars: Vec<char> = arg[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                let c = chars[j];
                if is_value_option(c) {
                    // Rest of this arg is the value, or next arg
                    if j < chars.len() - 1 {
                        let value: String =
                            chars[j + 1..].iter().collect();
                        set_value_option(&mut opts, c, value);
                        j = chars.len(); // stop processing
                    } else {
                        i += 1;
                        if i >= args.len() {
                            return Err(format!(
                                "tar: option requires an argument -- '{}'\n",
                                c
                            ));
                        }
                        set_value_option(
                            &mut opts,
                            c,
                            args[i].clone(),
                        );
                    }
                } else {
                    apply_short_flag(&mut opts, c)?;
                }
                j += 1;
            }
            i += 1;
            continue;
        }

        // Long options and single short options
        match arg.as_str() {
            "-c" | "--create" => {
                opts.operation = Some(TarOperation::Create)
            }
            "-x" | "--extract" | "--get" => {
                opts.operation = Some(TarOperation::Extract)
            }
            "-t" | "--list" => {
                opts.operation = Some(TarOperation::List)
            }
            "-r" | "--append" => {
                opts.operation = Some(TarOperation::Append)
            }
            "-u" | "--update" => {
                opts.operation = Some(TarOperation::Update)
            }
            "-z" | "--gzip" | "--gunzip" => opts.gzip = true,
            "-a" | "--auto-compress" => opts.auto_compress = true,
            "-v" | "--verbose" => opts.verbose = true,
            "-O" | "--to-stdout" => opts.to_stdout = true,
            "-k" | "--keep-old-files" => {
                opts.keep_old_files = true
            }
            "-m" | "--touch" => opts.touch = true,
            "-p" | "--preserve" | "--preserve-permissions" => {
                opts.preserve = true
            }
            "--wildcards" => opts.wildcards = true,
            "-f" | "--file" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "tar: option requires an argument -- 'f'\n"
                            .to_string(),
                    );
                }
                opts.file = Some(args[i].clone());
            }
            s if s.starts_with("--file=") => {
                opts.file = Some(s["--file=".len()..].to_string());
            }
            "-C" | "--directory" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "tar: option requires an argument -- 'C'\n"
                            .to_string(),
                    );
                }
                opts.directory = Some(args[i].clone());
            }
            s if s.starts_with("--directory=") => {
                opts.directory =
                    Some(s["--directory=".len()..].to_string());
            }
            s if s.starts_with("--strip-components=")
                || s.starts_with("--strip=") =>
            {
                let val = if s.starts_with("--strip-components=") {
                    &s["--strip-components=".len()..]
                } else {
                    &s["--strip=".len()..]
                };
                let num: usize = val.parse().map_err(|_| {
                    format!(
                        "tar: invalid number for --strip: '{}'\n",
                        val
                    )
                })?;
                opts.strip = num;
            }
            s if s.starts_with("--exclude=") => {
                opts.exclude
                    .push(s["--exclude=".len()..].to_string());
            }
            "--exclude" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "tar: option '--exclude' requires an argument\n"
                            .to_string(),
                    );
                }
                opts.exclude.push(args[i].clone());
            }
            "-T" | "--files-from" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "tar: option requires an argument -- 'T'\n"
                            .to_string(),
                    );
                }
                opts.files_from = Some(args[i].clone());
            }
            s if s.starts_with("--files-from=") => {
                opts.files_from =
                    Some(s["--files-from=".len()..].to_string());
            }
            "-X" | "--exclude-from" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "tar: option requires an argument -- 'X'\n"
                            .to_string(),
                    );
                }
                opts.exclude_from = Some(args[i].clone());
            }
            s if s.starts_with("--exclude-from=") => {
                opts.exclude_from =
                    Some(s["--exclude-from=".len()..].to_string());
            }
            "--" => {
                opts.files
                    .extend(args[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') => {
                return Err(format!(
                    "tar: unrecognized option '{}'\n",
                    s
                ));
            }
            _ => {
                opts.files.push(arg);
            }
        }
        i += 1;
    }

    // Handle auto-compress: detect gzip from file extension
    if opts.auto_compress {
        if let Some(ref f) = opts.file {
            if detect_gzip_from_extension(f) {
                opts.gzip = true;
            }
        }
    }

    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn test_parse_cvzf() {
        let opts =
            parse_options(&args(&["-cvzf", "archive.tar.gz", "dir/"]))
                .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Create));
        assert!(opts.verbose);
        assert!(opts.gzip);
        assert_eq!(opts.file, Some("archive.tar.gz".to_string()));
        assert_eq!(opts.files, vec!["dir/"]);
    }

    #[test]
    fn test_parse_xf() {
        let opts =
            parse_options(&args(&["-xf", "archive.tar"])).unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Extract));
        assert_eq!(opts.file, Some("archive.tar".to_string()));
        assert!(!opts.verbose);
        assert!(!opts.gzip);
    }

    #[test]
    fn test_parse_tf() {
        let opts =
            parse_options(&args(&["-tf", "archive.tar"])).unwrap();
        assert_eq!(opts.operation, Some(TarOperation::List));
        assert_eq!(opts.file, Some("archive.tar".to_string()));
    }

    #[test]
    fn test_parse_long_options() {
        let opts = parse_options(&args(&[
            "--create",
            "--gzip",
            "--file=archive.tar.gz",
        ]))
        .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Create));
        assert!(opts.gzip);
        assert_eq!(opts.file, Some("archive.tar.gz".to_string()));
    }

    #[test]
    fn test_parse_directory_option() {
        let opts = parse_options(&args(&[
            "-C", "/tmp", "-xf", "archive.tar",
        ]))
        .unwrap();
        assert_eq!(opts.directory, Some("/tmp".to_string()));
        assert_eq!(opts.operation, Some(TarOperation::Extract));
        assert_eq!(opts.file, Some("archive.tar".to_string()));
    }

    #[test]
    fn test_parse_exclude() {
        let opts = parse_options(&args(&[
            "--exclude=*.log",
            "-cf",
            "archive.tar",
            "dir/",
        ]))
        .unwrap();
        assert_eq!(opts.exclude, vec!["*.log"]);
        assert_eq!(opts.operation, Some(TarOperation::Create));
        assert_eq!(opts.files, vec!["dir/"]);
    }

    #[test]
    fn test_parse_strip_components() {
        let opts = parse_options(&args(&[
            "--strip-components=2",
            "-xf",
            "archive.tar",
        ]))
        .unwrap();
        assert_eq!(opts.strip, 2);
        assert_eq!(opts.operation, Some(TarOperation::Extract));
    }

    #[test]
    fn test_combined_short_cvf() {
        let opts =
            parse_options(&args(&["-cvf", "archive.tar", "a", "b"]))
                .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Create));
        assert!(opts.verbose);
        assert_eq!(opts.file, Some("archive.tar".to_string()));
        assert_eq!(opts.files, vec!["a", "b"]);
    }

    #[test]
    fn test_missing_operation_is_none() {
        let opts =
            parse_options(&args(&["-f", "archive.tar"])).unwrap();
        assert_eq!(opts.operation, None);
    }

    #[test]
    fn test_auto_compress_from_extension() {
        let opts = parse_options(&args(&[
            "-acf",
            "archive.tar.gz",
            "dir/",
        ]))
        .unwrap();
        assert!(opts.auto_compress);
        assert!(opts.gzip);
    }

    #[test]
    fn test_auto_compress_tgz() {
        let opts = parse_options(&args(&[
            "-a", "-cf", "archive.tgz", "dir/",
        ]))
        .unwrap();
        assert!(opts.auto_compress);
        assert!(opts.gzip);
    }

    #[test]
    fn test_inline_f_value() {
        let opts =
            parse_options(&args(&["-cfmy_archive.tar", "dir/"]))
                .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Create));
        assert_eq!(
            opts.file,
            Some("my_archive.tar".to_string())
        );
    }

    #[test]
    fn test_double_dash_stops_options() {
        let opts = parse_options(&args(&[
            "-cf",
            "archive.tar",
            "--",
            "-weird-file",
        ]))
        .unwrap();
        assert_eq!(opts.files, vec!["-weird-file"]);
    }

    #[test]
    fn test_unknown_option_error() {
        let result = parse_options(&args(&["-Q"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unrecognized option"));
    }

    #[test]
    fn test_missing_f_argument() {
        let result = parse_options(&args(&["-f"]));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("requires an argument")
        );
    }

    #[test]
    fn test_wildcards_flag() {
        let opts = parse_options(&args(&[
            "--wildcards",
            "-xf",
            "archive.tar",
        ]))
        .unwrap();
        assert!(opts.wildcards);
    }

    #[test]
    fn test_preserve_permissions() {
        let opts = parse_options(&args(&[
            "--preserve-permissions",
            "-xf",
            "archive.tar",
        ]))
        .unwrap();
        assert!(opts.preserve);
    }

    #[test]
    fn test_exclude_separate_arg() {
        let opts = parse_options(&args(&[
            "--exclude",
            "*.tmp",
            "-cf",
            "archive.tar",
            "dir/",
        ]))
        .unwrap();
        assert_eq!(opts.exclude, vec!["*.tmp"]);
    }

    #[test]
    fn test_files_from_option() {
        let opts = parse_options(&args(&[
            "-T",
            "filelist.txt",
            "-cf",
            "archive.tar",
        ]))
        .unwrap();
        assert_eq!(
            opts.files_from,
            Some("filelist.txt".to_string())
        );
    }

    #[test]
    fn test_exclude_from_option() {
        let opts = parse_options(&args(&[
            "-X",
            "excludes.txt",
            "-cf",
            "archive.tar",
        ]))
        .unwrap();
        assert_eq!(
            opts.exclude_from,
            Some("excludes.txt".to_string())
        );
    }

    #[test]
    fn test_long_directory_equals() {
        let opts = parse_options(&args(&[
            "--directory=/tmp",
            "-xf",
            "archive.tar",
        ]))
        .unwrap();
        assert_eq!(opts.directory, Some("/tmp".to_string()));
    }

    #[test]
    fn test_append_operation() {
        let opts =
            parse_options(&args(&["-rf", "archive.tar", "newfile"]))
                .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Append));
        assert_eq!(opts.files, vec!["newfile"]);
    }

    #[test]
    fn test_update_operation() {
        let opts =
            parse_options(&args(&["-uf", "archive.tar", "file"]))
                .unwrap();
        assert_eq!(opts.operation, Some(TarOperation::Update));
    }

    #[test]
    fn test_keep_old_files() {
        let opts =
            parse_options(&args(&["-xkf", "archive.tar"])).unwrap();
        assert!(opts.keep_old_files);
        assert_eq!(opts.operation, Some(TarOperation::Extract));
    }

    #[test]
    fn test_touch_flag() {
        let opts =
            parse_options(&args(&["-xmf", "archive.tar"])).unwrap();
        assert!(opts.touch);
    }

    #[test]
    fn test_to_stdout_flag() {
        let opts =
            parse_options(&args(&["-xOf", "archive.tar"])).unwrap();
        assert!(opts.to_stdout);
    }
}
