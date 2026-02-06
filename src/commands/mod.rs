// src/commands/mod.rs
pub mod basename;
pub mod cat;
pub mod dirname;
pub mod head;
pub mod mkdir;
pub mod registry;
pub mod tail;
pub mod wc;
pub mod types;
pub mod utils;

pub use registry::CommandRegistry;
pub use types::{Command, CommandContext, CommandResult};
