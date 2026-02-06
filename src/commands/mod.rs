// src/commands/mod.rs
pub mod registry;
pub mod types;

pub use registry::CommandRegistry;
pub use types::{Command, CommandContext, CommandResult};
