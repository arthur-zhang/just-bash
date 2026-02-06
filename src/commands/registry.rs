// src/commands/registry.rs
use std::collections::HashMap;
use super::types::Command;

pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.insert(cmd.name().to_string(), cmd);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(|c| c.as_ref())
    }

    pub fn names(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

use super::basename::BasenameCommand;
use super::dirname::DirnameCommand;
use super::cat::CatCommand;
use super::head::HeadCommand;
use super::tail::TailCommand;
use super::wc::WcCommand;
use super::mkdir::MkdirCommand;
use super::touch::TouchCommand;
use super::rm::RmCommand;
use super::cp::CpCommand;
use super::mv::MvCommand;
use super::ls::LsCommand;
use super::grep::GrepCommand;
use super::test_cmd::{TestCommand, BracketCommand};

/// 注册批次 A 的所有命令
pub fn register_batch_a(registry: &mut CommandRegistry) {
    registry.register(Box::new(BasenameCommand));
    registry.register(Box::new(DirnameCommand));
    registry.register(Box::new(CatCommand));
    registry.register(Box::new(HeadCommand));
    registry.register(Box::new(TailCommand));
    registry.register(Box::new(WcCommand));
    registry.register(Box::new(MkdirCommand));
    registry.register(Box::new(TouchCommand));
    registry.register(Box::new(RmCommand));
    registry.register(Box::new(CpCommand));
    registry.register(Box::new(MvCommand));
    registry.register(Box::new(LsCommand));
    registry.register(Box::new(GrepCommand));
    registry.register(Box::new(TestCommand));
    registry.register(Box::new(BracketCommand));
}

/// 创建包含批次 A 命令的注册表
pub fn create_batch_a_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    registry
}
