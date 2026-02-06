// src/commands/mod.rs
pub mod basename;
pub mod cat;
pub mod cut;
pub mod cp;
pub mod dirname;
pub mod grep;
pub mod head;
pub mod join;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod nl;
pub mod paste;
pub mod registry;
pub mod rm;
pub mod sort;
pub mod tail;
pub mod test_cmd;
pub mod touch;
pub mod tr;
pub mod wc;
pub mod uniq;
pub mod types;
pub mod utils;

pub use registry::{CommandRegistry, register_batch_a, register_batch_b, create_batch_a_registry, create_batch_ab_registry};
pub use types::{Command, CommandContext, CommandResult};
