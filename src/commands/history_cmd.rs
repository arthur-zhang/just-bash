use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

const HISTORY_KEY: &str = "BASH_HISTORY";

pub struct HistoryCommand;

#[async_trait]
impl Command for HistoryCommand {
    fn name(&self) -> &'static str { "history" }

    async fn execute(&self, mut ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "history - display command history\n\nUsage: history [n]\n\nOptions:\n  -c      clear the history list\n".to_string()
            );
        }

        let history_str = ctx.env.get(HISTORY_KEY).cloned().unwrap_or_else(|| "[]".to_string());
        let history: Vec<String> = serde_json::from_str(&history_str).unwrap_or_default();

        if ctx.args.first().map(|s| s.as_str()) == Some("-c") {
            ctx.env.insert(HISTORY_KEY.to_string(), "[]".to_string());
            return CommandResult::success(String::new());
        }

        let count = if let Some(arg) = ctx.args.first() {
            arg.parse::<usize>().unwrap_or(history.len()).min(history.len())
        } else {
            history.len()
        };

        let start = history.len().saturating_sub(count);
        let mut stdout = String::new();
        for (i, cmd) in history.iter().enumerate().skip(start) {
            stdout.push_str(&format!("{:5}  {}\n", i + 1, cmd));
        }

        CommandResult::success(stdout)
    }
}
