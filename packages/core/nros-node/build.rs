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
    // Emitted from the `needs-type-descriptors` capability feature (no dep
    // edge). Gates the descriptor-registration
    // schema-passing body + the `M: Message` super-bound for builds
    // where a descriptor-needing backend (Cyclone DDS) is linked. The
    // backend itself is brought into the link graph by the umbrella's
    // own `dep:nros-rmw-cyclonedds-sys`; the agnostic core only flips
    // this presence cfg.
    println!("cargo:rustc-check-cfg=cfg(rmw_needs_type_descriptors)");

    // Emit `has_rmw` when an RMW seam is compiled in.
    //
    // phase-347 W1 — this used to test four features:
    // `CARGO_FEATURE_RMW_{ZENOH,XRCE,CFFI,UORB}`. **Three of them do not
    // exist.** This crate declares exactly one `rmw-*` selection feature,
    // `rmw-cffi`; there is no `rmw-zenoh`, `rmw-xrce` or `rmw-uorb` on
    // `nros-node`, so cargo could never set those env vars and the three
    // disjuncts were dead — vestiges of the pre-phase-248 shape, before the
    // umbrella converged on the cffi vtable.
    //
    // Deleting them is not a behaviour change; it is the removal of three
    // backend NAMES from a core package (RFC-0071: core receives capabilities,
    // it does not detect backends). `rmw-cffi` is already the capability —
    // "an RMW vtable seam is present" — wearing a four-backend disguise.
    //
    // The comment here also claimed `has_rmw` was set "when compiling for
    // tests (unit tests use MockSession)". No such branch existed, in this
    // revision or any other reachable from it; the claim was removed rather
    // than implemented, because unit tests that need a session already select
    // `rmw-cffi` through dev-dependencies.
    if env::var("CARGO_FEATURE_RMW_CFFI").is_ok() {
        println!("cargo:rustc-cfg=has_rmw");
    }

    // phase-347 W4 — signalled by the CAPABILITY feature
    // `needs-type-descriptors`, which the lowering enables when the selected
    // backend declares `type-descriptors` in its `nros-rmw.toml`. It was
    // `__cyclonedds-link`: a core feature named after one backend, flipping a
    // seam that was already capability-shaped. No `DEP_CYCLONEDDS_*` `links=`
    // probe anymore: the agnostic core has no Cargo dep on the Cyclone
    // crates, so there is no direct edge for cargo's `DEP_*` env-var
    // hand-off. The descriptor registration is a generic vtable seam
    // (`nros_rmw::register_type_descriptor`); the Cyclone backend
    // installs its registrar at init from its own crate.
    if env::var("CARGO_FEATURE_NEEDS_TYPE_DESCRIPTORS").is_ok() {
        println!("cargo:rustc-cfg=rmw_needs_type_descriptors");
    }

    // --- Primary user-facing knobs ---
    let max_cbs = env_usize("NROS_EXECUTOR_MAX_CBS", 4);
    let max_sc = env_usize("NROS_EXECUTOR_MAX_SC", 8);
    // Phase 214.C.3 — default coordinated with
    // `packages/rmw/zenoh/nros-rmw-zenoh/build.rs::ZPICO_SUBSCRIBER_BUFFER_SIZE`
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
    // `0` is the Kconfig SENTINEL for "derive it" (zephyr/Kconfig:
    // NROS_EXECUTOR_ARENA_SIZE, "0 = derive"), and it has to be honoured HERE,
    // where the value is consumed.
    //
    // `nros_cargo_build.cmake` already knows the sentinel and deliberately does
    // not forward a literal 0 — "forwarding a literal 0 would hand it a
    // zero-byte arena rather than the derivation". That guard became INERT when
    // issue 0460 made `knob_usize` read `$DOTCONFIG` directly so knobs could
    // reach the Rust lane at all: build.rs now finds `CONFIG_..._ARENA_SIZE=0`
    // in `.config` whether or not cmake exported it, and took it literally.
    //
    // The result was a zero-byte arena on every Zephyr image built with the
    // default: the FIRST node registers, the second fails
    // `NodeError::BufferTooSmall`, and the entry panics. Kconfig's own help
    // predicted the shape — "too small fails at runtime, not at link".
    let arena_size = match env_usize("NROS_EXECUTOR_ARENA_SIZE", derived_arena) {
        0 => derived_arena,
        n => n,
    };

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

/// Read a usize knob: explicit env var, else Zephyr Kconfig, else `default`.
///
/// issue 0460 — a Zephyr RUST image never sees the `set(ENV{...})` knob exports
/// that `nros_cargo_build.cmake` writes, so every one of these compiled its
/// crate default whatever Kconfig said (measured: `.config` said
/// `CONFIG_NROS_EXECUTOR_MAX_CBS=16`, zero occurrences in `build.ninja`, crate
/// compiled 4). `nros_zephyr_build::knob_usize` is the ONE spelling of the
/// `$DOTCONFIG` fallback — the zenoh shim's knobs go through the same helper.
/// This crate's env names are the Kconfig names minus `CONFIG_`, so the pair is
/// derived rather than tabulated.
fn env_usize(name: &str, default: usize) -> usize {
    nros_zephyr_build::knob_usize(name, &format!("CONFIG_{name}"), default)
}
