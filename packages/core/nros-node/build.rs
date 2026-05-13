//! Build script for nros-node
//!
//! Reads NROS_* environment variables and generates `nros_node_config.rs`
//! with compile-time configurable constants for executor and subscription sizing.
//!
//! Exports values via `links = "nros_node"` so dependents (nros-c, nros-cpp)
//! can read them as `DEP_NROS_NODE_*` environment variables.

use std::{env, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    println!("cargo:rustc-check-cfg=cfg(has_rmw)");

    // Emit `has_rmw` cfg when any RMW backend feature is active, or
    // when compiling for tests (unit tests use MockSession).
    let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()
        || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()
        || env::var("CARGO_FEATURE_RMW_DDS").is_ok()
        || env::var("CARGO_FEATURE_RMW_CFFI").is_ok()
        || env::var("CARGO_FEATURE_RMW_UORB").is_ok();
    if has_rmw {
        println!("cargo:rustc-cfg=has_rmw");
    }

    // --- Primary user-facing knobs ---
    let max_cbs = env_usize("NROS_EXECUTOR_MAX_CBS", 4);
    let max_sc = env_usize("NROS_EXECUTOR_MAX_SC", 8);
    let rx_buf_size = env_usize("NROS_SUBSCRIPTION_BUFFER_SIZE", 1024);
    let param_svc_buf = env_usize("NROS_PARAM_SERVICE_BUFFER_SIZE", 4096);
    // Phase 104.C.2 — multi-Node-per-Executor (rclcpp `add_node`
    // pattern). Most apps run a single Node per Executor; bridge
    // nodes typically need 2 (ingress + egress). Default 4 leaves
    // headroom for multi-Node services with shared spin.
    let max_nodes = env_usize("NROS_EXECUTOR_MAX_NODES", 4);

    // --- Derived arena size ---
    // Arena must hold MAX_CBS entries. Worst-case entry is an
    // ActionClient: 3 CffiServiceClients (each carries a 4096-byte
    // `pending_request` blocking-fallback buffer + ~256 of header) +
    // 1 CffiSubscriber + 3 × rx_buf (goal/result/feedback) + ~256
    // entry overhead. Subscription / service entries are strictly
    // smaller, so budget every slot at the action-client size.
    // Per entry: 3 × (4096 + 384) + 3 × rx_buf + 1536 ≈ 14976 + 3·rx_buf
    //
    // Embedded targets that never instantiate an `ActionClient` can
    // override the derived size with `NROS_EXECUTOR_ARENA_SIZE`. A
    // pub/sub-only workload only needs `3 × rx_buf + 512` per entry.
    let per_entry = 3 * 4480 + 3 * rx_buf_size + 1536;
    let derived_arena = (max_cbs * per_entry + 2048).max(8192);
    let arena_size = env_usize("NROS_EXECUTOR_ARENA_SIZE", derived_arena);

    let contents = format!(
        "/// Maximum number of executor callback slots \
         (set via NROS_EXECUTOR_MAX_CBS, default 4).\n\
         pub const MAX_CBS: usize = {max_cbs};\n\
         \n\
         /// Maximum number of `SchedContext` slots per executor \
         (set via NROS_EXECUTOR_MAX_SC, default 8). Phase 110.B.\n\
         pub const MAX_SC: usize = {max_sc};\n\
         \n\
         /// Executor arena size in bytes (derived from MAX_CBS and RX_BUF_SIZE).\n\
         pub const ARENA_SIZE: usize = {arena_size};\n\
         \n\
         /// Default subscription receive buffer size in bytes \
         (set via NROS_SUBSCRIPTION_BUFFER_SIZE, default 1024).\n\
         pub const DEFAULT_RX_BUF_SIZE: usize = {rx_buf_size};\n\
         \n\
         /// Parameter service request/reply buffer size in bytes \
         (set via NROS_PARAM_SERVICE_BUFFER_SIZE, default 4096).\n\
         pub const PARAM_SERVICE_BUFFER_SIZE: usize = {param_svc_buf};\n\
         \n\
         /// Maximum number of Nodes attached to a single Executor \
         (set via NROS_EXECUTOR_MAX_NODES, default 4). Phase 104.C.2.\n\
         pub const MAX_NODES: usize = {max_nodes};\n"
    );

    std::fs::write(Path::new(&out_dir).join("nros_node_config.rs"), contents).unwrap();

    // Export via `links = "nros_node"` so dependents (nros-c, nros-cpp)
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
