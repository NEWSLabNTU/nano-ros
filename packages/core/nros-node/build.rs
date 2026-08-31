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
    // issue 0790 — shutdown-hook slots, PER PHASE: the executor keeps one table
    // this size for pre-shutdown hooks and a second for on-shutdown hooks.
    //
    // Deliberately SMALL. Issue 0460 is the precedent: a per-entity static slot
    // is a measurable cost paid by every image, including the ones that never
    // register a single hook, so the default must not assume everyone wants
    // them. Two is enough for the canonical shape (park the actuator, release
    // the bus) and costs 2 x 2 x sizeof(fn ptr + ctx ptr) = 64 bytes on a
    // 64-bit host, 32 on a 32-bit target. Raise it with
    // `NROS_EXECUTOR_MAX_SHUTDOWN_CBS` (or `CONFIG_NROS_EXECUTOR_MAX_SHUTDOWN_CBS`
    // on Zephyr) when an image genuinely has more things to park.
    let max_shutdown_cbs = env_usize("NROS_EXECUTOR_MAX_SHUTDOWN_CBS", 2);
    // issue 0900 — how many of the MAX_CBS slots may hold an ACTION CLIENT,
    // the entity the arena derivation below budgets every slot at.
    //
    // Defaults to `max_cbs`, which reproduces the old `max_cbs * worst_case`
    // arithmetic byte for byte, so no existing image moves. It is a COUNT and
    // not a "which entity is heaviest" enum because Kconfig knobs are ints and
    // `knob_usize` is the one spelling that reaches the Zephyr Rust lane
    // (issue 0460); an enum would need a second reader shape for no gain.
    //
    // Setting it to 0 on a pub/sub-only image is the whole point: 74,240 bytes
    // becomes 16,384 at the defaults, and that arena is INLINE ON THE TASK
    // STACK, not in `.bss`.
    //
    // Too small fails at REGISTRATION (`NodeError::BufferTooSmall`), not at
    // link — same caveat `NROS_EXECUTOR_ARENA_SIZE` carries, and the reason
    // `Executor::arena_used()` plus the first-spin advisory landed first.
    let action_clients = env_usize("NROS_EXECUTOR_ACTION_CLIENTS", max_cbs).min(max_cbs);

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
    // issue 0900 — the budget for the two entry SHAPES, summed over how many
    // slots each may occupy, instead of charging every slot the larger one.
    // With `action_clients == max_cbs` (the default) the second term is zero
    // and this is byte-identical to the old formula.
    const PUBSUB_SUB_BUFS: usize = 3;
    const PUBSUB_ENTRY_OVERHEAD: usize = 512;
    let action_client_entry = ACTION_CLIENT_SERVICES * ACTION_CLIENT_PER_SERVICE
        + ACTION_CLIENT_FEEDBACK_SUBS * rx_buf_size
        + ACTION_CLIENT_SUB_OVERHEAD;
    let pubsub_entry = PUBSUB_SUB_BUFS * rx_buf_size + PUBSUB_ENTRY_OVERHEAD;
    let derived_arena = (action_clients * action_client_entry
        + max_cbs.saturating_sub(action_clients) * pubsub_entry
        + ARENA_BASE_OVERHEAD)
        .max(ARENA_FLOOR);
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
         /// Executor arena size in bytes (derived from MAX_CBS, RX_BUF_SIZE \
         and ACTION_CLIENTS).\n\
         pub const ARENA_SIZE: usize = {arena_size};\n\
         \n\
         /// How many callback slots the arena derivation budgeted at \
         ActionClient size (set via NROS_EXECUTOR_ACTION_CLIENTS, default \
         MAX_CBS). Issue 0900.\n\
         pub const ARENA_ACTION_CLIENTS: usize = {action_clients};\n\
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
         pub const MAX_NODES: usize = {max_nodes};\n\
         \n\
         /// Shutdown-hook slots PER PHASE -- one table this size for \
         pre-shutdown hooks and a second for on-shutdown hooks \
         (set via NROS_EXECUTOR_MAX_SHUTDOWN_CBS, default 2). Issue 0790.\n\
         pub const MAX_SHUTDOWN_CBS: usize = {max_shutdown_cbs};\n"
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
/// The platform and board rungs of the RFC-0049 ladder for `[knobs.executor]`,
/// resolved once.
///
/// phase-400 W6. This crate deliberately has NO `platform-*` cargo feature
/// (phase-248 C2: the core executor is platform-agnostic and reaches the
/// platform through the vtable), so the build script cannot know its platform
/// from a `cfg`. It learns it the way every other build-script fact travels
/// here: the lane exports a value, and a POINTER to the file that carries the
/// rest. `nros ws board-facts` emits `NROS_PLATFORM_NAME` and
/// `NROS_BOARD_TOML`, and `corrosion_set_env_vars` attaches them to the
/// target's own build command — which is what actually runs cargo, unlike
/// cmake's `set(ENV{...})` (issue 0460).
///
/// Absent pointer (a bare `cargo build` with no lane, an out-of-tree consumer)
/// → every rung is `None` and the front-end below decides, exactly as before.
/// With no board named there IS no platform rung to resolve, so that is the
/// right answer rather than a degradation.
fn executor_rungs() -> nros_board_common::platform_config::ExecutorKnobs {
    use nros_board_common::platform_config as pc;

    let empty = pc::ExecutorKnobs::default();
    let Some(platform) = std::env::var("NROS_PLATFORM_NAME")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return empty;
    };
    println!("cargo:rerun-if-env-changed=NROS_PLATFORM_NAME");

    // The board rung. `watch_path` fingerprints the file's CONTENT — issue
    // 0491: `rerun-if-env-changed` on a variable naming a PATH compares the
    // spelling, and one directory has three spellings here.
    // Every failure below is FATAL, never a fall-through to defaults.
    //
    // A silently empty tree resolves every knob to a builtin and produces a
    // wrong image with no diagnostic — `nros-zpico-build` says exactly this
    // about the same tree, and it is not a hypothetical: the first version of
    // this function used `.ok()`, and a platform file with one rejected key
    // compiled at the crate defaults while reporting success. The lane named a
    // platform; being unable to honour it is an error.
    let board = nros_build_paths::env_path("NROS_BOARD_TOML").map(|p| {
        nros_build_paths::watch_path(&p);
        pc::BoardKnobsFile::load(&p)
            .unwrap_or_else(|e| panic!("NROS_BOARD_TOML={}: {e}", p.display()))
            .knobs
            .executor
    });

    let search = pc::PlatformsTree::default_search_path(
        &nros_build_paths::repo_root(),
        std::env::var("NROS_PLATFORMS_DIR").ok().as_deref(),
    );
    for dir in &search {
        nros_build_paths::watch_path(dir);
    }
    let tree = pc::PlatformsTree::load_search_path(&search)
        .unwrap_or_else(|e| panic!("platform search path {search:?}: {e}"));
    let plat = tree
        .platform_executor_rungs(&platform)
        .unwrap_or_else(|e| panic!("NROS_PLATFORM_NAME={platform}: {e}"));

    // Board over platform, per RFC-0049. `None` at both leaves the front-end
    // and the built-in default in charge.
    let b = board.unwrap_or_default();
    pc::ExecutorKnobs {
        max_cbs: b.max_cbs.or(plat.max_cbs),
        max_sc: b.max_sc.or(plat.max_sc),
        max_nodes: b.max_nodes.or(plat.max_nodes),
        max_shutdown_cbs: b.max_shutdown_cbs.or(plat.max_shutdown_cbs),
        action_clients: b.action_clients.or(plat.action_clients),
        arena_size: b.arena_size.or(plat.arena_size),
        subscription_buffer_size: b.subscription_buffer_size.or(plat.subscription_buffer_size),
        param_service_buffer_size: b
            .param_service_buffer_size
            .or(plat.param_service_buffer_size),
    }
}

/// The knob a front-end env name belongs to, derived from the one table that
/// maps the other way rather than retyped.
fn knob_for_env(name: &str) -> Option<&'static str> {
    nros_board_common::platform_config::EXECUTOR_KNOBS
        .iter()
        .copied()
        .find(|k| nros_board_common::platform_config::executor_env_key(k) == name)
}

fn rung_value(
    rungs: &nros_board_common::platform_config::ExecutorKnobs,
    knob: &str,
) -> Option<usize> {
    match knob {
        "max_cbs" => rungs.max_cbs,
        "max_sc" => rungs.max_sc,
        "max_nodes" => rungs.max_nodes,
        "max_shutdown_cbs" => rungs.max_shutdown_cbs,
        "action_clients" => rungs.action_clients,
        "arena_size" => rungs.arena_size,
        "subscription_buffer_size" => rungs.subscription_buffer_size,
        "param_service_buffer_size" => rungs.param_service_buffer_size,
        _ => None,
    }
}

/// One executor knob: env → Kconfig → board → platform → built-in default.
///
/// The front-end keeps winning. Migrating a knob into the ladder must not take
/// an operator's override away, which is half of this wave's own gate.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    if let Some(v) = std::env::var(name).ok().and_then(|v| v.trim().parse().ok()) {
        return v;
    }
    if let Some(v) = nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{name}")) {
        return v;
    }
    static RUNGS: std::sync::OnceLock<nros_board_common::platform_config::ExecutorKnobs> =
        std::sync::OnceLock::new();
    let rungs = RUNGS.get_or_init(executor_rungs);
    knob_for_env(name)
        .and_then(|k| rung_value(rungs, k))
        .unwrap_or(default)
}
