//! Build script for nros-c
//!
//! Reads NROS_* environment variables and generates `nros_c_config.rs`
//! with compile-time configurable constants for the executor and action API.
//!
//! The C headers in include/nros/ are manually maintained and define the
//! same constants independently. If you change an env var here, update
//! the corresponding `#define` in the C header for consistency.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Executor limits
    let executor_max_handles = env_usize("NROS_EXECUTOR_MAX_HANDLES", 16);
    let max_subscriptions = env_usize("NROS_MAX_SUBSCRIPTIONS", 8);
    let max_timers = env_usize("NROS_MAX_TIMERS", 8);
    let max_services = env_usize("NROS_MAX_SERVICES", 4);
    let let_buffer_size = env_usize("NROS_LET_BUFFER_SIZE", 512);
    let message_buffer_size = env_usize("NROS_MESSAGE_BUFFER_SIZE", 4096);

    // Action limits
    let max_concurrent_goals = env_usize("NROS_MAX_CONCURRENT_GOALS", 4);

    let contents = format!(
        "/// Maximum number of handles in an executor \
         (set via NROS_EXECUTOR_MAX_HANDLES, default 16).\n\
         pub const NROS_EXECUTOR_MAX_HANDLES: usize = {executor_max_handles};\n\
         \n\
         /// Maximum number of subscriptions in an executor \
         (set via NROS_MAX_SUBSCRIPTIONS, default 8).\n\
         pub const NROS_MAX_SUBSCRIPTIONS: usize = {max_subscriptions};\n\
         \n\
         /// Maximum number of timers in an executor \
         (set via NROS_MAX_TIMERS, default 8).\n\
         pub const NROS_MAX_TIMERS: usize = {max_timers};\n\
         \n\
         /// Maximum number of services in an executor \
         (set via NROS_MAX_SERVICES, default 4).\n\
         pub const NROS_MAX_SERVICES: usize = {max_services};\n\
         \n\
         /// Buffer size for LET semantics per handle \
         (set via NROS_LET_BUFFER_SIZE, default 512).\n\
         pub const LET_BUFFER_SIZE: usize = {let_buffer_size};\n\
         \n\
         /// Maximum buffer size for subscription/service data \
         (set via NROS_MESSAGE_BUFFER_SIZE, default 4096).\n\
         pub const MESSAGE_BUFFER_SIZE: usize = {message_buffer_size};\n\
         \n\
         /// Maximum number of concurrent goals per action server \
         (set via NROS_MAX_CONCURRENT_GOALS, default 4).\n\
         pub const NROS_MAX_CONCURRENT_GOALS: usize = {max_concurrent_goals};\n"
    );

    std::fs::write(Path::new(&out_dir).join("nros_c_config.rs"), contents).unwrap();

    // Re-run if source files change (for library rebuild)
    println!("cargo:rerun-if-changed=src/");
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
