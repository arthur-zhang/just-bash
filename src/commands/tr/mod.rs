// src/commands/tr/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};
use std::collections::HashSet;

pub struct TrCommand;

/// Parse a SET string into a list of characters.
/// Supports literal characters, ranges (a-z), and escape sequences (\n, \t).
fn parse_set(set: &str) -> Vec<char> {
    let mut chars: Vec<char> = Vec::new();
    let raw: Vec<char> = set.chars().collect();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == '\\' && i + 1 < raw.len() {
            match raw[i + 1] {
                'n' => chars.push('\n'),
                't' => chars.push('\t'),
                'r' => chars.push('\r'),
                '\\' => chars.push('\\'),
                other => {
                    chars.push('\\');
                    chars.push(other);
                }
            }
            i += 2;
        } else if i + 2 < raw.len() && raw[i + 1] == '-' {
            let start = raw[i] as u32;
            let end = raw[i + 2] as u32;
            if start <= end {
                for code in start..=end {
                    if let Some(c) = char::from_u32(code) {
                        chars.push(c);
                    }
                }
            } else {
                chars.push(raw[i]);
                chars.push('-');
                chars.push(raw[i + 2]);
            }
            i += 3;
        } else {
            chars.push(raw[i]);
            i += 1;
        }
    }
    chars
}

#[async_trait]
impl Command for TrCommand {
    fn name(&self) -> &'static str {
        "tr"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: tr [OPTION]... SET1 [SET2]\n\n\
                 Translate, squeeze, or delete characters from stdin.\n\n\
                 Options:\n\
                   -d, --delete         delete characters in SET1\n\
                   -s, --squeeze-repeats squeeze repeated characters in SET1\n\
                   -c, -C, --complement use complement of SET1\n\
                       --help           display this help and exit\n"
                    .to_string(),
            );
        }

        let mut delete = false;
        let mut squeeze = false;
        let mut complement = false;
        let mut sets: Vec<String> = Vec::new();

        for arg in &ctx.args {
            match arg.as_str() {
                "-d" | "--delete" => delete = true,
                "-s" | "--squeeze-repeats" => squeeze = true,
                "-c" | "-C" | "--complement" => complement = true,
                "-cd" | "-dc" => {
                    complement = true;
                    delete = true;
                }
                _ if !arg.starts_with('-') => sets.push(arg.clone()),
                _ => {}
            }
        }

        if sets.is_empty() {
            return CommandResult::error(
                "tr: missing operand\n".to_string(),
            );
        }

        if sets.len() < 2 && !delete && !squeeze {
            return CommandResult::error(
                "tr: missing operand after the first SET\n".to_string(),
            );
        }

        let set1_chars = parse_set(&sets[0]);
        let set1_hash: HashSet<char> = set1_chars.iter().cloned().collect();

        let input = &ctx.stdin;

        if delete {
            // Delete mode: remove characters in SET1 (or complement)
            let output: String = input
                .chars()
                .filter(|c| {
                    if complement {
                        set1_hash.contains(c) // keep chars IN set1
                    } else {
                        !set1_hash.contains(c) // keep chars NOT in set1
                    }
                })
                .collect();
            return CommandResult::success(output);
        }

        if squeeze && sets.len() < 2 {
            // Squeeze only (no translation): squeeze repeated chars in SET1
            let mut output = String::new();
            let mut last_char: Option<char> = None;
            for c in input.chars() {
                let in_set = if complement {
                    !set1_hash.contains(&c)
                } else {
                    set1_hash.contains(&c)
                };
                if in_set {
                    if last_char == Some(c) {
                        continue;
                    }
                }
                output.push(c);
                last_char = Some(c);
            }
            return CommandResult::success(output);
        }

        // Translation mode: translate SET1 chars to SET2 chars
        let set2_chars = parse_set(&sets[1]);
        let mut output = String::new();

        for c in input.chars() {
            let in_set1 = if complement {
                !set1_hash.contains(&c)
            } else {
                set1_hash.contains(&c)
            };

            if in_set1 {
                if complement {
                    // For complement, map all non-SET1 chars to last char of SET2
                    let replacement = set2_chars.last().copied().unwrap_or(c);
                    output.push(replacement);
                } else {
                    // Find position in set1_chars and map to set2_chars
                    if let Some(pos) = set1_chars.iter().position(|&sc| sc == c) {
                        let replacement = if pos < set2_chars.len() {
                            set2_chars[pos]
                        } else {
                            // SET2 shorter: use last char of SET2
                            *set2_chars.last().unwrap_or(&c)
                        };
                        output.push(replacement);
                    } else {
                        output.push(c);
                    }
                }
            } else {
                output.push(c);
            }
        }

        // If squeeze is also set, squeeze repeated chars that are in SET2
        if squeeze {
            let set2_hash: HashSet<char> = set2_chars.iter().cloned().collect();
            let mut squeezed = String::new();
            let mut last_char: Option<char> = None;
            for c in output.chars() {
                if set2_hash.contains(&c) && last_char == Some(c) {
                    continue;
                }
                squeezed.push(c);
                last_char = Some(c);
            }
            output = squeezed;
        }

        CommandResult::success(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_ctx(args: Vec<&str>, stdin: &str) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
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

    #[tokio::test]
    async fn test_tr_lower_to_upper() {
        let ctx = make_ctx(vec!["a-z", "A-Z"], "hello world\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "HELLO WORLD\n");
    }

    #[tokio::test]
    async fn test_tr_upper_to_lower() {
        let ctx = make_ctx(vec!["A-Z", "a-z"], "HELLO WORLD\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello world\n");
    }

    #[tokio::test]
    async fn test_tr_delete() {
        let ctx = make_ctx(vec!["-d", "aeiou"], "hello world\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hll wrld\n");
    }

    #[tokio::test]
    async fn test_tr_delete_newlines() {
        let ctx = make_ctx(vec!["-d", "\\n"], "a\nb\nc\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abc");
    }

    #[tokio::test]
    async fn test_tr_squeeze() {
        let ctx = make_ctx(vec!["-s", "a"], "aaabbbccc\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abbbccc\n");
    }

    #[tokio::test]
    async fn test_tr_translate_chars() {
        let ctx = make_ctx(vec!["abc", "xyz"], "aabbcc\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "xxyyzz\n");
    }

    #[tokio::test]
    async fn test_tr_space_to_underscore() {
        let ctx = make_ctx(vec![" ", "_"], "hello world\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello_world\n");
    }

    #[tokio::test]
    async fn test_tr_char_range() {
        let ctx = make_ctx(vec!["0-9", "X"], "abc123def456\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abcXXXdefXXX\n");
    }

    #[tokio::test]
    async fn test_tr_delete_digits() {
        let ctx = make_ctx(vec!["-d", "0-9"], "abc123def456\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abcdef\n");
    }

    #[tokio::test]
    async fn test_tr_missing_operand() {
        let ctx = make_ctx(vec![], "hello\n");
        let result = TrCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tr_missing_set2() {
        let ctx = make_ctx(vec!["abc"], "hello\n");
        let result = TrCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_tr_shorter_set2() {
        let ctx = make_ctx(vec!["abc", "x"], "aabbcc\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "xxxxxx\n");
    }

    #[tokio::test]
    async fn test_tr_complement_delete() {
        let ctx = make_ctx(vec!["-cd", "a-z\\n"], "Hello123World\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "elloorld\n");
    }

    #[tokio::test]
    async fn test_tr_squeeze_spaces() {
        let ctx = make_ctx(vec!["-s", " "], "hello    world\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello world\n");
    }

    #[tokio::test]
    async fn test_tr_number_range() {
        let ctx = make_ctx(vec!["1-5", "a-e"], "12345\n");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "abcde\n");
    }

    #[tokio::test]
    async fn test_tr_help() {
        let ctx = make_ctx(vec!["--help"], "");
        let result = TrCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Usage"));
        assert!(result.stdout.contains("--delete"));
    }
}