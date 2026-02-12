use async_trait::async_trait;
use crate::commands::{Command, CommandContext, CommandResult};

const ALIAS_PREFIX: &str = "BASH_ALIAS_";

pub struct AliasCommand;

#[async_trait]
impl Command for AliasCommand {
    fn name(&self) -> &'static str { "alias" }

    async fn execute(&self, mut ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "alias - define or display aliases\n\nUsage: alias [name[=value] ...]\n".to_string()
            );
        }

        if ctx.args.is_empty() {
            let mut stdout = String::new();
            for (key, value) in &ctx.env {
                if let Some(name) = key.strip_prefix(ALIAS_PREFIX) {
                    stdout.push_str(&format!("alias {}='{}'\n", name, value));
                }
            }
            return CommandResult::success(stdout);
        }

        let args: Vec<_> = if ctx.args.first().map(|s| s.as_str()) == Some("--") {
            ctx.args[1..].to_vec()
        } else {
            ctx.args.clone()
        };

        for arg in args {
            if let Some(eq_idx) = arg.find('=') {
                let name = &arg[..eq_idx];
                let mut value = arg[eq_idx + 1..].to_string();
                if (value.starts_with('\'') && value.ends_with('\''))
                    || (value.starts_with('"') && value.ends_with('"'))
                {
                    value = value[1..value.len() - 1].to_string();
                }
                ctx.env.insert(format!("{}{}", ALIAS_PREFIX, name), value);
            } else {
                let key = format!("{}{}", ALIAS_PREFIX, arg);
                if let Some(value) = ctx.env.get(&key) {
                    return CommandResult::success(format!("alias {}='{}'\n", arg, value));
                } else {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("alias: {}: not found\n", arg),
                        1,
                    );
                }
            }
        }

        CommandResult::success(String::new())
    }
}
