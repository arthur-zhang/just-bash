// src/commands/mod.rs
pub mod basename;
pub mod cat;
pub mod dirname;
pub mod registry;
pub mod types;
pub mod utils;

pub use registry::CommandRegistry;
pub use types::{Command, CommandContext, CommandResult};
