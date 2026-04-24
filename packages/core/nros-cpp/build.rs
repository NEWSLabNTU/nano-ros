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

// Phase 87.11: `target_pointer_bytes()` and `align_up()` removed —
// nros-cpp's storage sizes are now sourced from `nros::sizes` probes
// instead of pointer-width hand-math.

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

    // Phase 87.11: action server/client storage sizes are now sourced
    // from `nros::sizes::CppActionServerLayout` /
    // `CppActionClientLayout` via the same probe path used for everything
    // else. The probe values land in `nros_cpp_config_generated.h` as
    // `NROS_CPP_ACTION_SERVER_SIZE` / `NROS_CPP_ACTION_CLIENT_SIZE`.
    // Rust-side asserts in `nros-cpp/src/action.rs` ensure the layout
    // mirror in `nros::sizes` stays byte-equivalent to the real
    // `CppActionServer` / `CppActionClient`.
    //
    // Phase 77.23: fat LTO (workspace release profile) makes `nros`
    // emit bitcode-only rlibs, so `object::parse` returns no symbols
    // and the probe silently yields 0 for every entry. Until the probe
    // learns to read bitcode (or the workspace LTO policy changes),
    // fall back to hand-math for the action storage values — they
    // drive C++ opaque-storage arrays and cannot be 0 or the
    // `ActionClient<A>::storage_[0]` array aliases into the next
    // field, causing memory corruption at `send_goal` time.
    let ptr_bytes = target_pointer_bytes();
    // CppActionServerLayout real size (verified against probe with LTO
    // disabled, see below): on 64-bit it is 72 bytes — inner
    // `Option<ActionServerRawHandle>` niche-optimises into
    // `ActionServerRawHandle` (which is `usize + 5 fn pointers` = 6*ptr =
    // 48 bytes), then +3 pointer-sized fields (goal_cb, cancel_cb,
    // cb_ctx). On 32-bit that's 6*4 + 3*4 = 36 bytes.
    let action_server_fallback = ptr_bytes * 9; // 9*ptr = 72 on 64-bit, 36 on 32-bit
    // CppActionClientLayout: 4-pointer callbacks (32 on 64-bit, 16 on
    // 32-bit) + i32 + pad + *mut c_void. Alignment pads to 8 on 64-bit.
    let action_client_fallback = ptr_bytes * 5 + if ptr_bytes == 8 { 8 } else { 4 };

    let action_server_storage = non_zero_or(
        probed.get("CPP_ACTION_SERVER_SIZE").copied().unwrap_or(0) as usize,
        action_server_fallback,
    );
    let action_client_storage = non_zero_or(
        probed.get("CPP_ACTION_CLIENT_SIZE").copied().unwrap_or(0) as usize,
        action_client_fallback,
    );

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
         // Phase 87.11: `CPP_ACTION_SERVER_OPAQUE_U64S` and\n\
         // `CPP_ACTION_CLIENT_OPAQUE_U64S` removed. C++ ActionServer/\n\
         // ActionClient storage now sized from `nros::sizes::CPP_ACTION_*_SIZE`\n\
         // via the probe; see action.rs for the byte-equivalence asserts.\n\
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
    let header_path = include_dir.join("nros_cpp_config_generated.h");
    // Phase 77.24: if the probe silently returned 0 (LTO bitcode rlib),
    // keep the checked-in committed header rather than clobbering it
    // with zeros. `action_server_storage` / `action_client_storage` are
    // safe even on probe failure because `nros-cpp/build.rs` has the
    // hand-math fallback for those two — the `NROS_*_SIZE` macros on
    // the other hand have no fallback and would silently produce
    // `_opaque[0]` arrays.
    let probe_failed = probe_executor == 0;
    if probe_failed && header_path.exists() {
        println!(
            "cargo:warning=nros-cpp: probe returned all-zero sizes (LTO \
             bitcode rlib?); keeping existing committed header at {}",
            header_path.display()
        );
    } else if probe_failed {
        panic!(
            "nros-cpp: probe returned all-zero sizes and no committed \
             header exists at {}. Run a non-LTO build (e.g. debug \
             profile) once to seed the header.",
            header_path.display()
        );
    } else {
        std::fs::write(header_path, cpp_header).unwrap();
    }
}

/// Phase 77.23: target pointer width in bytes, for hand-math fallback
/// when the rlib probe returns 0 (see `generate_config`).
fn target_pointer_bytes() -> usize {
    match env::var("CARGO_CFG_TARGET_POINTER_WIDTH").ok().as_deref() {
        Some("32") => 4,
        Some("64") => 8,
        _ => core::mem::size_of::<*const ()>(),
    }
}

/// Phase 77.23: return `probe` if non-zero, else the hand-math fallback.
fn non_zero_or(probe: usize, fallback: usize) -> usize {
    if probe != 0 { probe } else { fallback }
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
