//! Build script for nros-c
//!
//! 1. Reads nros-c-specific env vars and `DEP_NROS_NODE_*` metadata (from
//!    nros-node's `links = "nros_node"`) to generate `nros_c_config.rs`.
//! 2. Generates a C config header with the opaque executor storage size.
//! 3. Runs cbindgen to generate `include/nros/nros_generated.h`.
//!
//! The opaque storage size is an upper bound computed from the executor's
//! arena, entries, and overhead. A compile-time assertion in executor.rs
//! validates that `size_of::<Executor>()` fits within this bound — if the
//! estimate drifts, the build fails instead of silently corrupting memory.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let probed = probe_nros_sizes();
    generate_config(&out_dir, &manifest_dir, &probed);
    generate_header(&manifest_dir);

    // Re-run if source files change (for library rebuild + header regen)
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

/// Probe sizes exported by the `nros` crate via `export_size!`.
///
/// Returns an empty map when no RMW backend is active (e.g. `cargo check`
/// without features) — the downstream consumer pairs each probe lookup with
/// `unwrap_or(0)` so the build still completes in that mode. Returning early
/// with a warning also keeps the rlib-less first-pass `cargo check` runs
/// working during incremental builds.
fn probe_nros_sizes() -> std::collections::HashMap<String, u64> {
    use std::collections::HashMap;

    let rlib = match nros_sizes_build::find_dep_rlib("nros", "__NROS_SIZE_") {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning=nros-c probe: {e}");
            return HashMap::new();
        }
    };
    match nros_sizes_build::extract_sizes(&rlib, "__NROS_SIZE_") {
        Ok(map) => map,
        Err(e) => {
            println!(
                "cargo:warning=nros-c probe failed parsing {}: {e}",
                rlib.display()
            );
            HashMap::new()
        }
    }
}

/// Generate `nros_c_config.rs` with build-time configurable constants.
fn generate_config(
    out_dir: &str,
    manifest_dir: &Path,
    probed: &std::collections::HashMap<String, u64>,
) {
    // --- Executor layout from nros-node (via Cargo `links` metadata) ---
    // nros-node is the single source of truth for these values.
    // MESSAGE_BUFFER_SIZE must equal nrs-node's RX_BUF_SIZE because the
    // C API places entries (SubRawEntry<MESSAGE_BUFFER_SIZE>) into the
    // nros-node arena, which is sized for RX_BUF_SIZE entries.
    let max_cbs = dep_usize("DEP_NROS_NODE_MAX_CBS");
    let arena_size = dep_usize("DEP_NROS_NODE_ARENA_SIZE");
    let message_buffer_size = dep_usize("DEP_NROS_NODE_RX_BUF_SIZE");

    // --- C API knobs (nros-c only, not shared with nros-node) ---
    let let_buffer_size = env_usize("NROS_LET_BUFFER_SIZE", 512);

    // --- Opaque storage upper bound ---
    // This MUST be >= size_of::<Executor>(). We use a generous estimate;
    // the compile-time assertion in executor.rs catches any undercount.
    //
    // Layout: SessionStore + arena + entries + trigger + misc fields
    let session_upper = 512; // SessionStore enum (Owned or Borrowed)
    let entries_upper = max_cbs * 80; // Option<CallbackMeta> ~72 bytes, rounded up
    let overhead = 512; // Trigger + semantics + node_name + namespace + halt_flag + padding
    let executor_bytes = session_upper + arena_size + entries_upper + overhead;
    let executor_opaque_u64s = executor_bytes.div_ceil(8);
    let executor_storage_bytes = executor_opaque_u64s * 8;

    // Phase 87.5 (full): all four `*Internal` shim types are now
    // `#[repr(C)]` and embedded directly in their outer `nros_*_t`
    // structs. No hand-math storage upper bounds needed.

    let contents = format!(
        "/// Maximum number of handles in an executor \
         (derived from NROS_EXECUTOR_MAX_CBS via nros-node).\n\
         pub const NROS_EXECUTOR_MAX_HANDLES: usize = {max_cbs};\n\
         \n\
         /// Buffer size for LET semantics per handle \
         (set via NROS_LET_BUFFER_SIZE, default 512).\n\
         pub const LET_BUFFER_SIZE: usize = {let_buffer_size};\n\
         \n\
         /// Maximum buffer size for subscription/service data \
         (derived from NROS_SUBSCRIPTION_BUFFER_SIZE via nros-node).\n\
         pub const MESSAGE_BUFFER_SIZE: usize = {message_buffer_size};\n\
         \n\
         /// Inline opaque storage for `Executor` inside `nros_executor_t` (in u64 units).\n\
         /// Upper bound derived from nros-node's MAX_CBS and ARENA_SIZE.\n\
         /// Validated at compile time by `size_of::<Executor>()` assertion.\n\
         pub const EXECUTOR_OPAQUE_U64S: usize = {executor_opaque_u64s};\n\
         \n\
         "
    );

    std::fs::write(Path::new(out_dir).join("nros_c_config.rs"), contents).unwrap();

    // --- Phase 87: probe-derived sizes (Rust-as-SSoT) ------------------------
    //
    // These come from `nros`'s `export_size!` symbols, read out of the rlib by
    // `nros_sizes_build`. During the Phase 87 transition they exist alongside
    // the hand-math values above — once every downstream consumer is switched
    // over (Phase 87.4 / 87.6), the hand-math and its assertions will be
    // deleted.
    let probe_executor = probed.get("EXECUTOR_SIZE").copied().unwrap_or(0) as usize;
    let probe_guard = probed.get("GUARD_CONDITION_SIZE").copied().unwrap_or(0) as usize;
    let probe_publisher = probed.get("PUBLISHER_SIZE").copied().unwrap_or(0) as usize;
    let probe_subscriber = probed.get("SUBSCRIBER_SIZE").copied().unwrap_or(0) as usize;
    let probe_service_client = probed.get("SERVICE_CLIENT_SIZE").copied().unwrap_or(0) as usize;
    let probe_service_server = probed.get("SERVICE_SERVER_SIZE").copied().unwrap_or(0) as usize;
    let probe_session = probed.get("SESSION_SIZE").copied().unwrap_or(0) as usize;
    let probe_lifecycle_ctx = probed.get("LIFECYCLE_CTX_SIZE").copied().unwrap_or(0) as usize;
    let probe_action_server_internal = probed
        .get("ACTION_SERVER_INTERNAL_SIZE")
        .copied()
        .unwrap_or(0) as usize;
    let probe_action_server_raw_handle = probed
        .get("ACTION_SERVER_RAW_HANDLE_SIZE")
        .copied()
        .unwrap_or(0) as usize;

    // Invariant during the transition: the existing hand-math upper bound for
    // Executor must envelope the exact Rust size. If this ever flips the build
    // should fail loudly — that's the 32-bit ARM under-count we are replacing.
    if probe_executor > 0 {
        assert!(
            probe_executor <= executor_storage_bytes,
            "nros-c: probed size_of::<Executor>()={probe_executor} exceeds hand-math \
             upper bound {executor_storage_bytes}. Raise hand-math (nros-c/build.rs) \
             or drop it per Phase 87.4."
        );
    }

    // Inline opaque storage in u64 units. cbindgen-generated nros_generated.h
    // references these by name (`uint64_t _opaque[SESSION_OPAQUE_U64S]`,
    // `uint64_t _opaque[EXECUTOR_OPAQUE_U64S]`, …) but cannot evaluate the
    // Rust-side expressions that compute them, so we emit the post-probe
    // values here as plain C #defines. cbindgen.toml's `[export.exclude]`
    // suppresses the placeholder versions cbindgen would otherwise emit.
    let session_opaque_u64s = probe_session.div_ceil(8);
    let publisher_opaque_u64s = probe_publisher.div_ceil(8);
    let guard_handle_opaque_u64s = probe_guard.div_ceil(8);
    let lifecycle_ctx_opaque_u64s = probe_lifecycle_ctx.div_ceil(8);

    // u64-aligned size for the opaque ActionServerRawHandle storage.
    let raw_handle_u64s = probe_action_server_raw_handle.div_ceil(8);

    // Generate C config header with opaque storage sizes
    let c_header = format!(
        "/* Auto-generated by nros-c build.rs — do not edit */\n\
         #ifndef NROS_CONFIG_GENERATED_H\n\
         #define NROS_CONFIG_GENERATED_H\n\
         \n\
         #include <stdint.h>\n\
         \n\
         /** Inline opaque storage size (bytes) for nros_executor_t. */\n\
         #define NROS_EXECUTOR_STORAGE_SIZE {executor_storage_bytes}\n\
         \n\
         /* ── Phase 87: probe-derived sizes (Rust is the single source of truth) ─\n\
          * Values below are `size_of::<T>()` for each Rust type, extracted from\n\
          * the compiled `nros` rlib by nros-sizes-build. They coexist with the\n\
          * hand-math macros above during the Phase 87.3 transition; once\n\
          * downstream consumers are migrated (Phase 87.4/87.6) the hand-math\n\
          * macros are deleted.\n\
          */\n\
         /** `size_of::<nros_node::Executor>()` */\n\
         #define NROS_EXECUTOR_SIZE {probe_executor}\n\
         /** `size_of::<nros_node::GuardConditionHandle>()` */\n\
         #define NROS_GUARD_CONDITION_SIZE {probe_guard}\n\
         /** `size_of::<RmwPublisher>()` */\n\
         #define NROS_PUBLISHER_SIZE {probe_publisher}\n\
         /** `size_of::<RmwSubscriber>()` */\n\
         #define NROS_SUBSCRIBER_SIZE {probe_subscriber}\n\
         /** `size_of::<RmwServiceClient>()` */\n\
         #define NROS_SERVICE_CLIENT_SIZE {probe_service_client}\n\
         /** `size_of::<RmwServiceServer>()` */\n\
         #define NROS_SERVICE_SERVER_SIZE {probe_service_server}\n\
         /** `size_of::<RmwSession>()` */\n\
         #define NROS_SESSION_SIZE {probe_session}\n\
         /** `size_of::<LifecyclePollingNodeCtx>()` */\n\
         #define NROS_LIFECYCLE_CTX_SIZE {probe_lifecycle_ctx}\n\
         /** Layout-mirror size for `ActionServerInternal` (Phase 87.5). */\n\
         #define NROS_ACTION_SERVER_INTERNAL_SIZE {probe_action_server_internal}\n\
         \n\
         /* ── *_OPAQUE_U64S macros for cbindgen-emitted struct fields ──────\n\
          * Phase 91.C1: cbindgen generates `uint64_t _opaque[N]` array fields\n\
          * that reference these macros by name. The Rust definitions live in\n\
          * src/opaque_sizes.rs / src/constants.rs and would either evaluate\n\
          * to placeholder 1 (when cbindgen runs without an active RMW\n\
          * feature) or fail to evaluate at all (when the value is a\n\
          * `u64s_for::<T>()` / `size_of::<T>()` expression cbindgen can't\n\
          * fold). build.rs emits the real, post-probe values here so the\n\
          * cbindgen output is self-contained when included from C / C++.\n\
          */\n\
         #define SESSION_OPAQUE_U64S {session_opaque_u64s}\n\
         #define PUBLISHER_OPAQUE_U64S {publisher_opaque_u64s}\n\
         #define EXECUTOR_OPAQUE_U64S {executor_opaque_u64s}\n\
         #define GUARD_HANDLE_OPAQUE_U64S {guard_handle_opaque_u64s}\n\
         #define NROS_LIFECYCLE_CTX_OPAQUE_U64S {lifecycle_ctx_opaque_u64s}\n\
         \n\
         /* ── Type-compatible opaque definition of nros_node::ActionServerRawHandle ──\n\
          * Phase 91.C1: cbindgen emits `ActionServerRawHandle handle;` as an\n\
          * inline field of `ActionServerInternal` (parse_deps=false means it\n\
          * doesn't recurse into nros-node to see the body). We provide a\n\
          * size-equivalent opaque definition here so the cbindgen header is\n\
          * self-contained. The nros-c Rust side still uses the typed\n\
          * `nros_node::ActionServerRawHandle`; only the C side sees opaque\n\
          * bytes — safe because the C API never lets callers reach into\n\
          * `_internal.handle` directly.\n\
          */\n\
         #ifdef __cplusplus\n\
         extern \"C\" {{\n\
         #endif\n\
         typedef struct ActionServerRawHandle {{\n\
             uint64_t _opaque[{raw_handle_u64s}];\n\
         }} ActionServerRawHandle;\n\
         #ifdef __cplusplus\n\
         }}\n\
         #endif\n\
         \n\
         #endif /* NROS_CONFIG_GENERATED_H */\n"
    );
    let config_header_path = manifest_dir.join("include/nros/nros_config_generated.h");
    let probe_failed = probe_executor == 0; // see write_header_preserve_nonzero below
    write_header_preserve_nonzero(&config_header_path, &c_header, "nros-c", probe_failed);
}

/// Phase 77.24: write `c_header` to `path`, but only if the probe produced
/// meaningful sizes. When the workspace's fat-LTO release profile is
/// active, `nros-sizes-build` can't extract symbol sizes from the
/// bitcode-only rlib — the probe returns 0 for every entry and writing
/// zeros would corrupt the checked-in header (which acts as the
/// last-known-good snapshot). In that case keep the existing committed
/// file and warn the consumer instead. `probe_failed` is passed in by
/// the caller because only it knows which probe keys are expected to be
/// populated. See Phase 77.24 in
/// `docs/roadmap/phase-77-async-action-client.md`.
fn write_header_preserve_nonzero(
    path: &std::path::Path,
    new_contents: &str,
    crate_label: &str,
    probe_failed: bool,
) {
    if probe_failed && path.exists() {
        println!(
            "cargo:warning={crate_label}: probe returned all-zero sizes \
             (LTO bitcode rlib?); keeping existing committed header at {}",
            path.display()
        );
        return;
    }
    if probe_failed {
        panic!(
            "{crate_label}: probe returned all-zero sizes and no committed \
             header exists at {}. Run a non-LTO build (e.g. debug profile) \
             once to seed the header, or switch the workspace release \
             profile to `lto = false`.",
            path.display()
        );
    }
    std::fs::write(path, new_contents).unwrap();
}

/// Generate `include/nros/nros_generated.h` using cbindgen.
///
/// cbindgen reads Rust source files and generates C header declarations
/// for all `#[repr(C)]` structs, enums, type aliases, constants, and
/// `extern "C"` functions. The generated header is the single source of
/// truth for C/Rust type layout compatibility.
fn generate_header(manifest_dir: &Path) {
    let config_path = manifest_dir.join("cbindgen.toml");
    let output_path = manifest_dir.join("include/nros/nros_generated.h");

    let config = match cbindgen::Config::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            println!("cargo:warning=Failed to load cbindgen config: {e}");
            return;
        }
    };

    let result = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate();

    match result {
        Ok(bindings) => {
            bindings.write_to_file(&output_path);
        }
        Err(e) => {
            // cbindgen may fail if dependencies aren't available (e.g.,
            // during no-default-features builds). This is expected —
            // the generated header is only needed for builds with an
            // RMW backend enabled.
            println!("cargo:warning=cbindgen header generation skipped: {e}");
        }
    }
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Read a usize from a `DEP_*` environment variable (Cargo `links` metadata).
///
/// Panics if the variable is missing — this means nros-node's `links` export
/// is broken, which is a build system bug that should fail loudly.
fn dep_usize(name: &str) -> usize {
    env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} not set — is nros-node's `links = \"nros_node\"` configured?")
        })
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a valid usize"))
}
