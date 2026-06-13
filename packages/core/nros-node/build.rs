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
    // Phase 248 (C2) — emitted from the private `__cyclonedds-link`
    // marker feature (no dep edge). Gates the descriptor-registration
    // schema-passing body + the `M: Message` super-bound for builds
    // where a descriptor-needing backend (Cyclone DDS) is linked. The
    // backend itself is brought into the link graph by the umbrella's
    // own `dep:nros-rmw-cyclonedds-sys`; the agnostic core only flips
    // this presence cfg.
    println!("cargo:rustc-check-cfg=cfg(rmw_cyclonedds_present)");

    // Emit `has_rmw` cfg when any RMW backend feature is active, or
    // when compiling for tests (unit tests use MockSession).
    let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()
        || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()
        || env::var("CARGO_FEATURE_RMW_CFFI").is_ok()
        || env::var("CARGO_FEATURE_RMW_UORB").is_ok();
    if has_rmw {
        println!("cargo:rustc-cfg=has_rmw");
    }

    // Phase 248 (C2) — descriptor-needing-backend presence is signalled
    // purely by the `__cyclonedds-link` marker feature (which the
    // umbrella `nros/rmw-cyclonedds` activates alongside its own
    // `dep:nros-rmw-cyclonedds-sys`). No `DEP_CYCLONEDDS_*` `links=`
    // probe anymore: the agnostic core has no Cargo dep on the Cyclone
    // crates, so there is no direct edge for cargo's `DEP_*` env-var
    // hand-off. The descriptor registration is a generic vtable seam
    // (`nros_rmw::register_type_descriptor`); the Cyclone backend
    // installs its registrar at init from its own crate.
    if env::var("CARGO_FEATURE___CYCLONEDDS_LINK").is_ok() {
        println!("cargo:rustc-cfg=rmw_cyclonedds_present");
    }

    // --- Primary user-facing knobs ---
    let max_cbs = env_usize("NROS_EXECUTOR_MAX_CBS", 4);
    let max_sc = env_usize("NROS_EXECUTOR_MAX_SC", 8);
    // Phase 214.C.3 — default coordinated with
    // `packages/zpico/nros-rmw-zenoh/build.rs::ZPICO_SUBSCRIBER_BUFFER_SIZE`
    // (also 1024). If you change one, change the other — they share the
    // wire-format expectation. Both can be overridden independently via
    // their respective env vars.
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
    //
    // Phase 214.C.4 — magic-number breakdown for `4480` and friends:
    //   ACTION_CLIENT_SERVICE_BUF   = 4096  // pending_request blocking-fallback buf
    //   ACTION_CLIENT_HEADER_OVERHD =  384  // ~256 hdr + alignment slack
    //   ACTION_CLIENT_PER_SERVICE   = 4480  // = SERVICE_BUF + HEADER_OVERHD
    //   ACTION_CLIENT_SERVICES      =    3  // goal_send + cancel + get_result
    //   ACTION_CLIENT_SUB_OVERHEAD  = 1536  // 1 CffiSubscriber + ~256 entry slop
    const ACTION_CLIENT_PER_SERVICE: usize = 4096 + 384;
    const ACTION_CLIENT_SERVICES: usize = 3;
    const ACTION_CLIENT_FEEDBACK_SUBS: usize = 3; // goal + result + feedback rx
    const ACTION_CLIENT_SUB_OVERHEAD: usize = 1536;
    const ARENA_BASE_OVERHEAD: usize = 2048;
    const ARENA_FLOOR: usize = 8192;
    let per_entry = ACTION_CLIENT_SERVICES * ACTION_CLIENT_PER_SERVICE
        + ACTION_CLIENT_FEEDBACK_SUBS * rx_buf_size
        + ACTION_CLIENT_SUB_OVERHEAD;
    let derived_arena = (max_cbs * per_entry + ARENA_BASE_OVERHEAD).max(ARENA_FLOOR);
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
