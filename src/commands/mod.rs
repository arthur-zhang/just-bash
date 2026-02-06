// src/commands/mod.rs
pub mod basename;
pub mod cat;
pub mod cp;
pub mod dirname;
pub mod head;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod registry;
pub mod rm;
pub mod tail;
pub mod touch;
pub mod wc;
pub mod types;
pub mod utils;

pub use registry::CommandRegistry;
pub use types::{Command, CommandContext, CommandResult};
