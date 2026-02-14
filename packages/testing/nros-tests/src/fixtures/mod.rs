//! rstest fixtures for integration tests
//!
//! Provides `#[rstest::fixture]` functions for:
//! - `zenohd`, `zenohd_unique` - Managed zenohd router
//! - `qemu_binary`, `talker_binary`, `listener_binary` - Binary build fixtures
//!
//! Also re-exports utilities from sibling modules for convenience.

mod binaries;
#[allow(hidden_glob_reexports)] // rstest fixture creates a module matching the fn name
mod xrce_agent;
mod zenohd_router;

pub use binaries::*;
pub use xrce_agent::*;
pub use zenohd_router::*;

// Re-export utilities for backwards compatibility
pub use crate::esp32::*;
pub use crate::process::*;
pub use crate::qemu::*;
pub use crate::ros2::*;
pub use crate::zephyr::*;
