use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct HelpCommand;

const CATEGORIES: &[(&str, &[&str])] = &[
    ("File operations", &["ls", "cat", "head", "tail", "wc", "touch", "mkdir", "rm", "cp", "mv", "ln", "chmod", "stat", "readlink"]),
    ("Text processing", &["grep", "sed", "awk", "sort", "uniq", "cut", "tr", "tee", "diff"]),
    ("Search", &["find"]),
    ("Navigation & paths", &["pwd", "basename", "dirname", "tree", "du"]),
    ("Environment & shell", &["echo", "printf", "env", "printenv", "export", "alias", "unalias", "history", "clear", "true", "false", "bash", "sh"]),
    ("Data processing", &["xargs", "jq", "base64", "date"]),
    ("Network", &["curl"]),
];

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &'static str { "help" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help" || a == "-h") {
            return CommandResult::success(
                "help - display available commands\n\nUsage: help [command]\n\nOptions:\n  -h, --help    Show this help message\n".to_string()
            );
        }

        if !ctx.args.is_empty() {
            if let Some(exec_fn) = &ctx.exec_fn {
                let cmd_name = &ctx.args[0];
                return exec_fn(
                    format!("{} --help", cmd_name),
                    String::new(),
                    ctx.cwd.clone(),
                    ctx.env.clone(),
                    ctx.fs.clone(),
                ).await;
            }
        }

        let mut stdout = String::from("Available commands:\n\n");

        for (category, cmds) in CATEGORIES {
            stdout.push_str(&format!("  {}:\n", category));
            stdout.push_str(&format!("    {}\n\n", cmds.join(", ")));
        }

        stdout.push_str("Use '<command> --help' for details on a specific command.\n");

        CommandResult::success(stdout)
    }
}
