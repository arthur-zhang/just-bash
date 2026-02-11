// src/commands/chmod/mod.rs
use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct ChmodCommand;

#[async_trait]
impl Command for ChmodCommand {
    fn name(&self) -> &'static str { "chmod" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = &ctx.args;
        if args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "Usage: chmod [OPTIONS] MODE FILE...\n\nchange file mode bits\n\nOptions:\n  -R  change files recursively\n  -v  verbose\n      --help  display this help\n".into()
            );
        }
        if args.len() < 2 {
            return CommandResult::error("chmod: missing operand\n".into());
        }

        let mut recursive = false;
        let mut verbose = false;
        let mut idx = 0;
        while idx < args.len() && args[idx].starts_with('-') {
            match args[idx].as_str() {
                "-R" | "--recursive" => { recursive = true; idx += 1; }
                "-v" | "--verbose" => { verbose = true; idx += 1; }
                "-Rv" | "-vR" => { recursive = true; verbose = true; idx += 1; }
                "--" => { idx += 1; break; }
                s if is_mode_like(s) => break,
                _ => {
                    return CommandResult::with_exit_code("".into(), format!("chmod: invalid option -- '{}'\n", &args[idx][1..]), 1);
                }
            }
        }
        if args.len() - idx < 2 {
            return CommandResult::error("chmod: missing operand\n".into());
        }

        let mode_arg = &args[idx];
        let files = &args[idx + 1..];
        let is_numeric = mode_arg.chars().all(|c| c >= '0' && c <= '7');
        let numeric_mode = if is_numeric { Some(u32::from_str_radix(mode_arg, 8).unwrap_or(0)) } else { None };
        if !is_numeric {
            if parse_mode(mode_arg, 0o644).is_err() {
                return CommandResult::with_exit_code("".into(), format!("chmod: invalid mode: '{}'\n", mode_arg), 1);
            }
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut err = false;

        for file in files {
            let fp = ctx.fs.resolve_path(&ctx.cwd, file);
            let mv = if is_numeric { numeric_mode.unwrap_or(0) } else {
                match ctx.fs.stat(&fp).await {
                    Ok(st) => parse_mode(mode_arg, st.mode).unwrap_or(0),
                    Err(_) => { stderr.push_str(&format!("chmod: cannot access '{}': No such file or directory\n", file)); err = true; continue; }
                }
            };
            if let Err(_) = ctx.fs.chmod(&fp, mv).await {
                stderr.push_str(&format!("chmod: cannot access '{}': No such file or directory\n", file));
                err = true; continue;
            }
            if verbose { stdout.push_str(&format!("mode of '{}' changed to {:04o}\n", file, mv)); }
            if recursive {
                if let Ok(st) = ctx.fs.stat(&fp).await {
                    if st.is_directory { chmod_rec(&ctx, &fp, is_numeric, numeric_mode, mode_arg, verbose, &mut stdout).await; }
                }
            }
        }
        CommandResult::with_exit_code(stdout, stderr, if err { 1 } else { 0 })
    }
}

fn is_mode_like(s: &str) -> bool {
    let r1 = regex_lite::Regex::new(r"^[+-]?[rwxugo]+").unwrap();
    let r2 = regex_lite::Regex::new(r"^\d+$").unwrap();
    r1.is_match(s) || r2.is_match(s)
}

async fn chmod_rec(ctx: &CommandContext, dir: &str, is_numeric: bool, nm: Option<u32>, sm: &str, v: bool, out: &mut String) {
    let entries = match ctx.fs.readdir(dir).await { Ok(e) => e, Err(_) => return };
    for entry in &entries {
        let fp = if dir == "/" { format!("/{}", entry) } else { format!("{}/{}", dir, entry) };
        let mv = if is_numeric { nm.unwrap_or(0) } else {
            ctx.fs.stat(&fp).await.map(|s| parse_mode(sm, s.mode).unwrap_or(0o644)).unwrap_or(0o644)
        };
        let _ = ctx.fs.chmod(&fp, mv).await;
        if v { out.push_str(&format!("mode of '{}' changed to {:04o}\n", fp, mv)); }
        if let Ok(st) = ctx.fs.stat(&fp).await {
            if st.is_directory { Box::pin(chmod_rec(ctx, &fp, is_numeric, nm, sm, v, out)).await; }
        }
    }
}

fn parse_mode(mode_str: &str, current_mode: u32) -> Result<u32, String> {
    if mode_str.chars().all(|c| c >= '0' && c <= '7') {
        return Ok(u32::from_str_radix(mode_str, 8).unwrap_or(0));
    }
    let mut mode = current_mode & 0o7777;
    for part in mode_str.split(',') {
        let re = regex_lite::Regex::new(r"^([ugoa]*)([+\-=])([rwxXst]*)$").unwrap();
        let caps = re.captures(part).ok_or_else(|| format!("Invalid mode: {}", mode_str))?;
        let mut who = caps.get(1).map_or("a", |m| m.as_str()).to_string();
        let op = caps.get(2).map_or("", |m| m.as_str());
        let perms = caps.get(3).map_or("", |m| m.as_str());
        if who == "a" || who.is_empty() { who = "ugo".to_string(); }
        let mut pb: u32 = 0;
        if perms.contains('r') { pb |= 4; }
        if perms.contains('w') { pb |= 2; }
        if perms.contains('x') || perms.contains('X') { pb |= 1; }
        let mut sb: u32 = 0;
        if perms.contains('s') { if who.contains('u') { sb |= 0o4000; } if who.contains('g') { sb |= 0o2000; } }
        if perms.contains('t') { sb |= 0o1000; }
        for w in who.chars() {
            let sh: u32 = match w { 'u' => 6, 'g' => 3, 'o' => 0, _ => continue };
            let bits = pb << sh;
            match op { "+" => mode |= bits, "-" => mode &= !bits, "=" => { mode &= !(7 << sh); mode |= bits; } _ => {} }
        }
        match op { "+" => mode |= sb, "-" => mode &= !sb, "=" => { if perms.contains('s') { if who.contains('u') { mode = (mode & !0o4000) | (sb & 0o4000); } if who.contains('g') { mode = (mode & !0o2000) | (sb & 0o2000); } } if perms.contains('t') { mode = (mode & !0o1000) | (sb & 0o1000); } } _ => {} }
    }
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, InMemoryFs};
    use std::sync::Arc;
    use std::collections::HashMap;

    async fn make_ctx(args: Vec<&str>, files: Vec<(&str, &str)>) -> CommandContext {
        let fs = Arc::new(InMemoryFs::new());
        for (p, c) in files { fs.write_file(p, c.as_bytes()).await.unwrap(); }
        CommandContext { args: args.into_iter().map(String::from).collect(), stdin: String::new(), cwd: "/".into(), env: HashMap::new(), fs, exec_fn: None, fetch_fn: None }
    }

    #[tokio::test]
    async fn test_chmod_octal() { let c = make_ctx(vec!["755", "/t.txt"], vec![("/t.txt", "h")]).await; assert_eq!(ChmodCommand.execute(c).await.exit_code, 0); }
    #[tokio::test]
    async fn test_chmod_missing() { let c = make_ctx(vec![], vec![]).await; let r = ChmodCommand.execute(c).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("missing")); }
    #[tokio::test]
    async fn test_chmod_nofile() { let c = make_ctx(vec!["755", "/x"], vec![]).await; let r = ChmodCommand.execute(c).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("No such file")); }
    #[tokio::test]
    async fn test_chmod_invalid() { let c = make_ctx(vec!["xyz", "/t.txt"], vec![("/t.txt", "h")]).await; let r = ChmodCommand.execute(c).await; assert_eq!(r.exit_code, 1); assert!(r.stderr.contains("invalid mode")); }
    #[tokio::test]
    async fn test_chmod_help() { let c = make_ctx(vec!["--help"], vec![]).await; let r = ChmodCommand.execute(c).await; assert!(r.stdout.contains("chmod")); assert_eq!(r.exit_code, 0); }
    #[test]
    fn test_parse_numeric() { assert_eq!(parse_mode("755", 0).unwrap(), 0o755); assert_eq!(parse_mode("644", 0).unwrap(), 0o644); }
    #[test]
    fn test_parse_symbolic() { assert_eq!(parse_mode("u+x", 0o644).unwrap(), 0o744); assert_eq!(parse_mode("a+x", 0o644).unwrap(), 0o755); assert_eq!(parse_mode("g-w", 0o664).unwrap(), 0o644); assert_eq!(parse_mode("u=rwx", 0o644).unwrap(), 0o744); }
}
