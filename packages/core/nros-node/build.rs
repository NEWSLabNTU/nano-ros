//! Build script for nros-node
//!
//! Reads NROS_* environment variables and generates `nros_node_config.rs`
//! with compile-time configurable constants for executor and subscription sizing.
//!
//! Exports values via `links = "nros_node"` so dependents (nros-c, nros-cpp-ffi)
//! can read them as `DEP_NROS_NODE_*` environment variables.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    println!("cargo:rustc-check-cfg=cfg(has_rmw)");

    // Emit `has_rmw` cfg when any RMW backend feature is active, or
    // when compiling for tests (unit tests use MockSession).
    let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()
        || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()
        || env::var("CARGO_FEATURE_RMW_CFFI").is_ok();
    if has_rmw {
        println!("cargo:rustc-cfg=has_rmw");
    }

    // --- Primary user-facing knobs ---
    let max_cbs = env_usize("NROS_EXECUTOR_MAX_CBS", 4);
    let rx_buf_size = env_usize("NROS_SUBSCRIPTION_BUFFER_SIZE", 1024);

    // --- Derived arena size ---
    // Arena must hold MAX_CBS entries, each with rx+reply buffers + per-entry overhead.
    // This is always computed from MAX_CBS and RX_BUF_SIZE to prevent inconsistency.
    let arena_size = (max_cbs * (rx_buf_size * 2 + 256) + 1024).max(4096);

    let contents = format!(
        "/// Maximum number of executor callback slots \
         (set via NROS_EXECUTOR_MAX_CBS, default 4).\n\
         pub const MAX_CBS: usize = {max_cbs};\n\
         \n\
         /// Executor arena size in bytes (derived from MAX_CBS and RX_BUF_SIZE).\n\
         pub const ARENA_SIZE: usize = {arena_size};\n\
         \n\
         /// Default subscription receive buffer size in bytes \
         (set via NROS_SUBSCRIPTION_BUFFER_SIZE, default 1024).\n\
         pub const DEFAULT_RX_BUF_SIZE: usize = {rx_buf_size};\n"
    );

    std::fs::write(Path::new(&out_dir).join("nros_node_config.rs"), contents).unwrap();

    // Export via `links = "nros_node"` so dependents (nros-c, nros-cpp-ffi)
    // can read these as DEP_NROS_NODE_MAX_CBS, DEP_NROS_NODE_ARENA_SIZE, etc.
    println!("cargo:max_cbs={max_cbs}");
    println!("cargo:arena_size={arena_size}");
    println!("cargo:rx_buf_size={rx_buf_size}");
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
