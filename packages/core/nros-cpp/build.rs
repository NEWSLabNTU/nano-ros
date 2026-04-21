//! Build script for nros-cpp
//!
//! 1. Reads `DEP_NROS_NODE_*` metadata (from nros-node's `links = "nros_node"`)
//!    to compute opaque storage for CppContext.
//! 2. Runs cbindgen to generate `include/nros_cpp_ffi.h`.
//!
//! The opaque storage size is an upper bound. A compile-time assertion in
//! lib.rs validates that `size_of::<CppContext>()` fits within this bound.

use std::env;
use std::path::PathBuf;

/// Get the target pointer size in bytes from `CARGO_CFG_TARGET_POINTER_WIDTH`.
fn target_pointer_bytes() -> usize {
    let width: usize = env::var("CARGO_CFG_TARGET_POINTER_WIDTH")
        .unwrap_or_else(|_| "64".to_string())
        .parse()
        .unwrap_or(64);
    width / 8
}

/// Round `n` up to the next multiple of `align`.
#[allow(clippy::manual_div_ceil)]
const fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) / align * align
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = env::var("OUT_DIR").unwrap();

    let probed = probe_nros_sizes();
    generate_config(&out_dir, &manifest_dir, &probed);
    generate_header(&manifest_dir);

    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

/// Probe sizes exported by the `nros` crate via `export_size!`.
///
/// Returns an empty map when the rlib is not yet available or no RMW feature
/// is active. Consumers pair each lookup with `unwrap_or(0)` so the build
/// still completes (and emits a warning) in that mode. The bulk of the logic
/// lives in `nros-sizes-build`.
fn probe_nros_sizes() -> std::collections::HashMap<String, u64> {
    use std::collections::HashMap;

    let rlib = match nros_sizes_build::find_dep_rlib("nros", "__NROS_SIZE_") {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning=nros-cpp probe: {e}");
            return HashMap::new();
        }
    };
    match nros_sizes_build::extract_sizes(&rlib, "__NROS_SIZE_") {
        Ok(map) => map,
        Err(e) => {
            println!(
                "cargo:warning=nros-cpp probe failed parsing {}: {e}",
                rlib.display()
            );
            HashMap::new()
        }
    }
}

/// Generate `nros_cpp_ffi_config.rs` with build-time constants for executor storage.
fn generate_config(
    out_dir: &str,
    manifest_dir: &std::path::Path,
    probed: &std::collections::HashMap<String, u64>,
) {
    // Read executor layout from nros-node via Cargo `links` metadata.
    // nros-node is the single source of truth.
    let max_cbs = dep_usize("DEP_NROS_NODE_MAX_CBS");
    let arena_size = dep_usize("DEP_NROS_NODE_ARENA_SIZE");

    // Upper bound for CppContext size (in bytes):
    //   CppContext = Executor + domain_id (u32) + padding
    //   Executor ≈ SessionStore + arena + entries + trigger + misc
    let session_upper = 512;
    let entries_upper = max_cbs * 80;
    let overhead = 512; // trigger + semantics + strings + halt_flag + domain_id + padding
    let total_bytes = session_upper + arena_size + entries_upper + overhead;
    let opaque_u64s = total_bytes.div_ceil(8);
    let storage_bytes = opaque_u64s * 8;

    // ── Target-aware struct layout ─────────────────────────────────────
    //
    // Compute struct sizes using the TARGET pointer width (not the host's).
    // build.rs runs on the host, but `CARGO_CFG_TARGET_POINTER_WIDTH` tells
    // us the cross-compilation target. This avoids the bug where 8-byte
    // host `usize` underestimates sizes for 4-byte ARM targets.
    let ptr_bytes = target_pointer_bytes();

    // PendingGoal { goal_id: GoalId(16), data: [u8; DEFAULT_RX_BUF_SIZE], data_len: usize, occupied: bool }
    // Rust lays out fields in declaration order for non-repr(C) structs,
    // but may reorder for alignment. Compute a safe upper bound using the
    // struct's natural alignment (= alignment of its most-aligned field).
    //
    // All size constants below are sourced from `nros-node` (rx_buf via
    // Cargo links metadata, layout caps via `nros_node::limits`). The 256
    // here is `MAX_ACTION_NAME_LEN` / `MAX_TYPE_NAME_LEN` and also acts
    // as an upper bound for `MAX_TYPE_HASH_LEN` (128).
    let action_buf_size = dep_usize("DEP_NROS_NODE_RX_BUF_SIZE");
    let max_pending_goals = 4usize; // = nros_node::limits::MAX_CONCURRENT_GOALS
    let pending_goal_size = align_up(16 + action_buf_size + ptr_bytes + 1, ptr_bytes);
    // CppActionServer { handle: Option<Handle>, pending: [PendingGoal; MAX_CONCURRENT_GOALS],
    //                    action_name: [u8; MAX_ACTION_NAME_LEN], _len: usize, ×3 for name/type/hash }
    let handle_size = align_up(ptr_bytes + 4, ptr_bytes); // Option<ActionServerRawHandle> ~ usize + tag
    let name_field_size = 256 + ptr_bytes; // MAX_ACTION_NAME_LEN + usize len
    // Add margin for Rust's flexible (non-repr(C)) struct layout — the compiler
    // may add inter-field padding that differs from our estimate. The compile-time
    // assertion in action.rs catches any undercount.
    let layout_padding = 8 * ptr_bytes;
    let action_server_bytes = align_up(
        handle_size
            + (pending_goal_size * max_pending_goals)
            + 3 * name_field_size
            + layout_padding,
        8, // align to u64 for storage
    );
    let action_server_opaque_u64s = action_server_bytes.div_ceil(8);
    let action_server_storage = action_server_opaque_u64s * 8;

    // CppActionClient { callbacks: CppActionClientCallbacks, arena_entry_index: i32,
    //                    executor_ptr: *mut, action_name: [u8; MAX_ACTION_NAME_LEN], _action_name_len: usize }
    // CppActionClientCallbacks = 3 Option<fn> + context ptr
    // Each Option<fn> is 2 × ptr_bytes (function pointer + discriminant, aligned)
    let action_client_callbacks = 3 * (2 * ptr_bytes) + ptr_bytes;
    let action_client_bytes = align_up(
        action_client_callbacks + 4 + ptr_bytes + 256 + ptr_bytes + 8 * ptr_bytes, // fields + layout padding (256 = MAX_ACTION_NAME_LEN)
        8,
    );
    let action_client_opaque_u64s = action_client_bytes.div_ceil(8);
    let action_client_storage = action_client_opaque_u64s * 8;

    // ── Publisher / Subscription / Service / GuardCondition sizes ──────
    //
    // Upper bounds derived from the same field-layout math used for
    // action server/client. CppPublisher / CppSubscription / etc. are
    // declared in publisher.rs / subscription.rs / service.rs /
    // guard_condition.rs; lib.rs has compile-time asserts that catch
    // any undercount, so these numbers are safe upper bounds.
    //
    // Field sizes come from `nros_node::limits`:
    //   MAX_TOPIC_LEN = 256, MAX_SERVICE_NAME_LEN = 256, MAX_TYPE_NAME_LEN = 256,
    //   MAX_TYPE_HASH_LEN = 128.
    let _rx_buf = action_buf_size; // DEP_NROS_NODE_RX_BUF_SIZE (unused after 87.6)
    let _name_buf = 256usize; // MAX_TOPIC_LEN == MAX_SERVICE_NAME_LEN (unused after 87.6)

    // Phase 87.6: Publisher is a thin wrapper — storage sized to
    // `size_of::<RmwPublisher>()` via `NROS_PUBLISHER_SIZE` (probed from
    // the nros rlib). No hand-math needed.

    // Phase 87.6: Subscription is a thin wrapper — storage sized to
    // `size_of::<RmwSubscriber>()` via `NROS_SUBSCRIBER_SIZE`. The rx
    // scratch buffer lives C++-side on the `nros::Subscription<M>` class.

    // Phase 87.6: Service server/client are thin wrappers — storage sized
    // to `size_of::<RmwServiceServer>()` / `size_of::<RmwServiceClient>()`
    // via `NROS_SERVICE_SERVER_SIZE` / `NROS_SERVICE_CLIENT_SIZE`.

    // Phase 87.6: GuardCondition is thin — storage sized to
    // `size_of::<GuardConditionHandle>()` via `NROS_GUARD_CONDITION_SIZE`.

    let contents = format!(
        "/// Inline opaque storage for `CppContext` (in u64 units).\n\
         /// Upper bound derived from nros-node's MAX_CBS and ARENA_SIZE.\n\
         /// Validated at compile time by `size_of::<CppContext>()` assertion.\n\
         pub const CPP_EXECUTOR_OPAQUE_U64S: usize = {opaque_u64s};\n\
         \n\
         /// Inline opaque storage for `CppActionServer` (in u64 units).\n\
         pub const CPP_ACTION_SERVER_OPAQUE_U64S: usize = {action_server_opaque_u64s};\n\
         \n\
         /// Inline opaque storage for `CppActionClient` (in u64 units).\n\
         pub const CPP_ACTION_CLIENT_OPAQUE_U64S: usize = {action_client_opaque_u64s};\n\
         \n\
         // Byte-value views of the C-side storage macros. lib.rs uses\n\
         // these to assert at compile time that `size_of::<T>()` fits\n\
         // within the generated C macro — if the Rust type grows past\n\
         // the estimate, the build fails loudly instead of silently\n\
         // overflowing caller-provided storage.\n\
"
    );

    std::fs::write(
        std::path::Path::new(out_dir).join("nros_cpp_ffi_config.rs"),
        contents,
    )
    .unwrap();

    // --- Phase 87: probe-derived sizes (Rust-as-SSoT) ------------------------
    //
    // These mirror the `NROS_*_SIZE` macros emitted by `nros-c`. During the
    // Phase 87.3 transition they live alongside the hand-math `NROS_CPP_*`
    // values; once the thin-wrapper refactor (87.6) lands the hand-math
    // consumers can switch to these macros directly.
    let probe_executor = probed.get("EXECUTOR_SIZE").copied().unwrap_or(0) as usize;
    let probe_guard = probed.get("GUARD_CONDITION_SIZE").copied().unwrap_or(0) as usize;
    let probe_publisher = probed.get("PUBLISHER_SIZE").copied().unwrap_or(0) as usize;
    let probe_subscriber = probed.get("SUBSCRIBER_SIZE").copied().unwrap_or(0) as usize;
    let probe_service_client = probed.get("SERVICE_CLIENT_SIZE").copied().unwrap_or(0) as usize;
    let probe_service_server = probed.get("SERVICE_SERVER_SIZE").copied().unwrap_or(0) as usize;

    // Transition invariant: hand-math `NROS_CPP_EXECUTOR_STORAGE_SIZE` must
    // envelope the exact Rust `size_of::<Executor>()`. Build fails with a
    // clear message if this ever flips.
    if probe_executor > 0 {
        assert!(
            probe_executor <= storage_bytes,
            "nros-cpp: probed size_of::<Executor>()={probe_executor} exceeds hand-math \
             upper bound {storage_bytes}. Raise hand-math (nros-cpp/build.rs) or \
             drop it per Phase 87.4."
        );
    }

    // Generate C++ config header with all storage sizes (local include/nros/)
    let cpp_header = format!(
        "/* Auto-generated by nros-cpp build.rs — do not edit */\n\
         #ifndef NROS_CPP_CONFIG_GENERATED_H\n\
         #define NROS_CPP_CONFIG_GENERATED_H\n\
         \n\
         /** Inline opaque storage size (bytes) for nros::Executor. */\n\
         #define NROS_CPP_EXECUTOR_STORAGE_SIZE {storage_bytes}\n\
         \n\
         /** Inline opaque storage size (bytes) for nros::ActionServer<A>. */\n\
         #define NROS_CPP_ACTION_SERVER_STORAGE_SIZE {action_server_storage}\n\
         \n\
         /** Inline opaque storage size (bytes) for nros::ActionClient<A>. */\n\
         #define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE {action_client_storage}\n\
         \n\
         /* ── Phase 87: probe-derived sizes (Rust is the single source of truth) ─\n\
          * `size_of::<T>()` per Rust type, extracted from the compiled `nros`\n\
          * rlib by nros-sizes-build. During the 87.3 transition these exist\n\
          * alongside the hand-math macros above; 87.4 drops hand-math and\n\
          * 87.6 switches nros::Publisher<M> etc. to use NROS_PUBLISHER_SIZE\n\
          * directly (thin-wrapper refactor).\n\
          */\n\
         #define NROS_EXECUTOR_SIZE {probe_executor}\n\
         #define NROS_GUARD_CONDITION_SIZE {probe_guard}\n\
         #define NROS_PUBLISHER_SIZE {probe_publisher}\n\
         #define NROS_SUBSCRIBER_SIZE {probe_subscriber}\n\
         #define NROS_SERVICE_CLIENT_SIZE {probe_service_client}\n\
         #define NROS_SERVICE_SERVER_SIZE {probe_service_server}\n\
         \n\
         #endif /* NROS_CPP_CONFIG_GENERATED_H */\n"
    );
    let include_dir = manifest_dir.join("include/nros");
    std::fs::create_dir_all(&include_dir).ok();
    std::fs::write(include_dir.join("nros_cpp_config_generated.h"), cpp_header).unwrap();
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

/// Generate `include/nros_cpp_ffi.h` using cbindgen.
fn generate_header(manifest_dir: &std::path::Path) {
    let config_path = manifest_dir.join("cbindgen.toml");
    let output_path = manifest_dir.join("include/nros_cpp_ffi.h");

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
            // Ensure include directory exists
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            bindings.write_to_file(&output_path);
        }
        Err(e) => {
            println!("cargo:warning=cbindgen header generation skipped: {e}");
        }
    }
}
