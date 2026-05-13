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

use std::{
    env,
    path::{Path, PathBuf},
};

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

    // --- Opaque storage from probe (Phase 118.B closure of Phase 87.6) ---
    // `EXECUTOR_SIZE` comes from `nros::sizes::EXECUTOR_SIZE` exported via
    // the `__NROS_SIZE_*` symbols. The hand-math upper bound that used to
    // live here (Phase 87.4) is gone — the probe is now the single source
    // of truth.
    //
    // Phase 119.1: merge against any prior header in the package source
    // tree so multi-variant cmake builds end up with the MAX across
    // variants (a safe upper bound that every variant fits into).
    // Without this the last cmake build's target-specific sizes
    // pollute every other variant's installed header → opaque-storage
    // overflow at runtime.
    // Phase 119.3: merge is gone. Each cargo invocation writes its
    // per-build header to
    // `$CARGO_TARGET_DIR/nros-c-generated/<variant_slug>/nros/...`
    // and `nros_c_setup()` CMake function finds it.
    let probe_executor = probed.get("EXECUTOR_SIZE").copied().unwrap_or(0) as usize;
    if probe_executor == 0 {
        println!(
            "cargo:warning=nros-c: EXECUTOR_SIZE probe returned 0 — \
             likely a `cargo check --no-default-features` run. The emitted \
             `EXECUTOR_OPAQUE_U64S` will be 1; do not link the resulting \
             rlib."
        );
    }
    let executor_storage_bytes = probe_executor.max(8);
    let executor_opaque_u64s = executor_storage_bytes.div_ceil(8);

    // Phase 87.5 (full): all four `*Internal` shim types are now
    // `#[repr(C)]` and embedded directly in their outer `nros_*_t`
    // structs. No hand-math storage upper bounds needed.

    // `max_cbs` is consumed below in `NROS_EXECUTOR_MAX_HANDLES`;
    // `arena_size` is no longer referenced now that hand-math is gone but
    // the `dep_usize` call still triggers Cargo's
    // `cargo:rerun-if-env-changed` plumbing on `DEP_NROS_NODE_ARENA_SIZE`.
    let _ = arena_size;

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
    // Phase 118.B: hand-math upper bound for `EXECUTOR_SIZE` deleted —
    // `executor_storage_bytes` above now reads directly from the probe.
    // The other sizes have always been probe-only.
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
    // Phase 122.3.c.3 — L1 polling-mode handle storage. Sized from
    // `nros::sizes::RAW_*_SIZE` (concrete instantiations at
    // `DEFAULT_RX_BUF_SIZE` == `MESSAGE_BUFFER_SIZE`).
    let probe_raw_subscription = probed.get("RAW_SUBSCRIPTION_SIZE").copied().unwrap_or(0) as usize;
    let probe_raw_service_server =
        probed.get("RAW_SERVICE_SERVER_SIZE").copied().unwrap_or(0) as usize;
    let probe_raw_service_client =
        probed.get("RAW_SERVICE_CLIENT_SIZE").copied().unwrap_or(0) as usize;

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

    // Generate C config header with opaque storage sizes.
    //
    // NOTE: this string uses raw line literals (no `\` continuation) so
    // leading whitespace inside C block comments (` * Each constant ...`)
    // is preserved exactly. `\` continuation strips leading whitespace on
    // the next line, which would produce clang-format violations on the
    // ` *` comment alignment.
    let c_header = format!(
        "\
/* Auto-generated by nros-c build.rs — do not edit */
#ifndef NROS_CONFIG_GENERATED_H
#define NROS_CONFIG_GENERATED_H

#include <stdint.h>

/** Inline opaque storage size (bytes) for nros_executor_t. */
#define NROS_EXECUTOR_STORAGE_SIZE {executor_storage_bytes}

/* ── Probe-derived inline storage sizes ──────────────────────
 * Each constant below is the byte size of the corresponding
 * runtime type, extracted from the compiled runtime by the
 * build script. They size the `_opaque` storage of the public
 * C handle types so callers can declare them on the stack or
 * inside their own structs without dynamic allocation.
 */
#define NROS_EXECUTOR_SIZE {probe_executor}
#define NROS_GUARD_CONDITION_SIZE {probe_guard}
#define NROS_PUBLISHER_SIZE {probe_publisher}
#define NROS_SUBSCRIBER_SIZE {probe_subscriber}
#define NROS_SERVICE_CLIENT_SIZE {probe_service_client}
#define NROS_SERVICE_SERVER_SIZE {probe_service_server}
#define NROS_SESSION_SIZE {probe_session}
#define NROS_LIFECYCLE_CTX_SIZE {probe_lifecycle_ctx}
#define NROS_ACTION_SERVER_INTERNAL_SIZE {probe_action_server_internal}

/* ── *_OPAQUE_U64S macros — sized opaque storage for the
 *    handle structs declared in <nros/nros_generated.h>.
 *    Each value is `ceil(size_of_type / 8)` u64 slots so the
 *    handle's storage is u64-aligned.
 */
#define SESSION_OPAQUE_U64S {session_opaque_u64s}
#define PUBLISHER_OPAQUE_U64S {publisher_opaque_u64s}
#define EXECUTOR_OPAQUE_U64S {executor_opaque_u64s}
#define GUARD_HANDLE_OPAQUE_U64S {guard_handle_opaque_u64s}
#define NROS_LIFECYCLE_CTX_OPAQUE_U64S {lifecycle_ctx_opaque_u64s}

/* ── Type-compatible opaque definition of ActionServerRawHandle ──
 * Public C handle struct. The runtime owns the body; the C API
 * never lets callers reach into `_internal.handle` directly,
 * so an opaque, size-equivalent declaration is sufficient.
 */
#ifdef __cplusplus
extern \"C\" {{
#endif
typedef struct ActionServerRawHandle {{
    uint64_t _opaque[{raw_handle_u64s}];
}} ActionServerRawHandle;
#ifdef __cplusplus
}}
#endif

#endif /* NROS_CONFIG_GENERATED_H */
"
    );
    // Phase 119.3: source-tree header is now a committed STUB that
    // `#error`s. Real header gets written PER-BUILD to
    // `$CARGO_TARGET_DIR/nros-c-generated/<variant_slug>/nros/`.
    // `nros_c_setup()` CMake function finds it.
    let _ = (manifest_dir, c_header);
    if probe_executor == 0 {
        // `cargo check --no-default-features` / `cargo doc` — no probe
        // result, skip writing.
        return;
    }
    let exact_executor_storage = probe_executor.max(8);
    let exact_executor_u64s = exact_executor_storage.div_ceil(8);
    let exact_session_u64s = probe_session.div_ceil(8);
    let exact_publisher_u64s = probe_publisher.div_ceil(8);
    let exact_guard_u64s = probe_guard.div_ceil(8);
    let exact_lifecycle_u64s = probe_lifecycle_ctx.div_ceil(8);
    let exact_raw_handle_u64s = probe_action_server_raw_handle.div_ceil(8);
    let exact_raw_subscription_u64s = probe_raw_subscription.max(8).div_ceil(8);
    let exact_raw_service_server_u64s = probe_raw_service_server.max(8).div_ceil(8);
    let exact_raw_service_client_u64s = probe_raw_service_client.max(8).div_ceil(8);
    let exact_header = format!(
        "\
/* Auto-generated by nros-c build.rs — do not edit (Phase 119.3 per-build variant header) */
#ifndef NROS_CONFIG_GENERATED_H
#define NROS_CONFIG_GENERATED_H

#include <stdint.h>

#define NROS_EXECUTOR_STORAGE_SIZE {exact_executor_storage}

#define NROS_EXECUTOR_SIZE {probe_executor}
#define NROS_GUARD_CONDITION_SIZE {probe_guard}
#define NROS_PUBLISHER_SIZE {probe_publisher}
#define NROS_SUBSCRIBER_SIZE {probe_subscriber}
#define NROS_SERVICE_CLIENT_SIZE {probe_service_client}
#define NROS_SERVICE_SERVER_SIZE {probe_service_server}
#define NROS_SESSION_SIZE {probe_session}
#define NROS_LIFECYCLE_CTX_SIZE {probe_lifecycle_ctx}
#define NROS_ACTION_SERVER_INTERNAL_SIZE {probe_action_server_internal}

#define SESSION_OPAQUE_U64S {exact_session_u64s}
#define PUBLISHER_OPAQUE_U64S {exact_publisher_u64s}
#define EXECUTOR_OPAQUE_U64S {exact_executor_u64s}
#define GUARD_HANDLE_OPAQUE_U64S {exact_guard_u64s}
#define NROS_LIFECYCLE_CTX_OPAQUE_U64S {exact_lifecycle_u64s}

/* Phase 122.3.c.3 — L1 polling-mode handle storage. Override the
 * placeholder values cbindgen emits in nros_generated.h
 * (cbindgen picks the `#[cfg(not(feature = \"rmw-cffi\"))]`
 * branch in opaque_sizes.rs, so its output is always 1). The
 * variant header is included AFTER nros_generated.h, so these
 * `#define`s win.
 */
#undef SUBSCRIPTION_OPAQUE_U64S
#define SUBSCRIPTION_OPAQUE_U64S {exact_raw_subscription_u64s}
#undef SERVICE_SERVER_OPAQUE_U64S
#define SERVICE_SERVER_OPAQUE_U64S {exact_raw_service_server_u64s}
#undef SERVICE_CLIENT_OPAQUE_U64S
#define SERVICE_CLIENT_OPAQUE_U64S {exact_raw_service_client_u64s}

#ifdef __cplusplus
extern \"C\" {{
#endif
typedef struct ActionServerRawHandle {{
    uint64_t _opaque[{exact_raw_handle_u64s}];
}} ActionServerRawHandle;
#ifdef __cplusplus
}}
#endif

#endif /* NROS_CONFIG_GENERATED_H */
"
    );

    // Phase 119.3: two stable per-build locations (see nros-cpp/build.rs
    // for rationale).
    let write_to = |dest: PathBuf| {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("nros-c: create per-build header dir");
        }
        std::fs::write(&dest, &exact_header).expect("nros-c: write per-build header");
    };
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        write_to(
            PathBuf::from(target_dir)
                .join("nros-c-generated")
                .join("nros")
                .join("nros_config_generated.h"),
        );
    } else if let Ok(td) = nros_sizes_build::cargo_target_dir() {
        write_to(
            td.join("nros-c-generated")
                .join("nros")
                .join("nros_config_generated.h"),
        );
    }
    if let Ok(corrosion_dir) = env::var("CORROSION_BUILD_DIR") {
        write_to(PathBuf::from(corrosion_dir).join("nros_config_generated.h"));
    }
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
#[allow(dead_code)] // Phase 119.3: kept for direct-cargo source-tree fallback in future
fn write_header_preserve_nonzero(
    path: &std::path::Path,
    new_contents: &str,
    crate_label: &str,
    probe_failed: bool,
) {
    if probe_failed && path.exists() {
        // Expected on `cargo doc` / `cargo check --workspace` (LTO bitcode
        // rlib has no readable layout) — fall back to the committed header.
        // Use `eprintln!` so this doesn't surface as a yellow `warning:`
        // line on every workspace build (Phase 77.24 stopgap).
        eprintln!(
            "{crate_label}: probe returned all-zero sizes (LTO bitcode rlib?); \
             keeping existing committed header at {}",
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
