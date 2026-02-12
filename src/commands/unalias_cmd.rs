use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

const ALIAS_PREFIX: &str = "BASH_ALIAS_";

pub struct UnaliasCommand;

#[async_trait]
impl Command for UnaliasCommand {
    fn name(&self) -> &'static str { "unalias" }

    async fn execute(&self, mut ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "unalias - remove alias definitions\n\nUsage: unalias [-a] name [name ...]\n".to_string()
            );
        }

        if ctx.args.is_empty() {
            return CommandResult::with_exit_code(
                String::new(),
                "unalias: usage: unalias [-a] name [name ...]\n".to_string(),
                1,
            );
        }

        if ctx.args.first().map(|s| s.as_str()) == Some("-a") {
            let keys_to_remove: Vec<_> = ctx.env.keys()
                .filter(|k| k.starts_with(ALIAS_PREFIX))
                .cloned()
                .collect();
            for key in keys_to_remove {
                ctx.env.remove(&key);
            }
            return CommandResult::success(String::new());
        }

        let args: Vec<_> = if ctx.args.first().map(|s| s.as_str()) == Some("--") {
            ctx.args[1..].to_vec()
        } else {
            ctx.args.clone()
        };

        let mut stderr = String::new();
        let mut any_error = false;

        for name in args {
            let key = format!("{}{}", ALIAS_PREFIX, name);
            if ctx.env.contains_key(&key) {
                ctx.env.remove(&key);
            } else {
                stderr.push_str(&format!("unalias: {}: not found\n", name));
                any_error = true;
            }
        }

        CommandResult::with_exit_code(String::new(), stderr, if any_error { 1 } else { 0 })
    }
}
