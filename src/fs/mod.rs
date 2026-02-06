//! File System Module
//!
//! Provides virtual file system abstractions for the bash environment.
//! Supports multiple implementations:
//! - InMemoryFs: Pure in-memory file system (default)
//! - OverlayFs: Copy-on-write over real filesystem (future)

pub mod types;
pub mod in_memory_fs;

pub use types::*;
pub use in_memory_fs::InMemoryFs;
