//! Build script for nros-cpp
//!
//! 1. Reads `DEP_NROS_NODE_*` metadata (from nros-node's `links = "nros_node"`)
//!    to compute opaque storage for CppContext.
//! 2. Runs cbindgen to generate `include/nros_cpp_ffi.h`.
//!
//! The opaque storage size is an upper bound. A compile-time assertion in
//! lib.rs validates that `size_of::<CppContext>()` fits within this bound.

use std::{env, path::PathBuf};

use crate::shared::{
    compile_c_stub, dep_usize, generate_cbindgen_header, non_zero_or, probe_nros_sizes,
    target_pointer_bytes, write_header_to_corrosion, write_header_to_target_dir,
};

// Phase 87.11: `target_pointer_bytes()` and `align_up()` removed —
// nros-cpp's storage sizes are now sourced from `nros::sizes` probes
// instead of pointer-width hand-math.

pub fn run() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = env::var("OUT_DIR").unwrap();

    let probed = probe_nros_sizes("nros-cpp");
    generate_config(&out_dir, &manifest_dir, &probed);
    generate_header(&manifest_dir);

    // Weak fallbacks for the platform log ABI (mirrors nros-c). The weak default
    // of `nros_app_register_backends` was removed in phase-249 P4a.
    compile_c_stub(
        &manifest_dir,
        "c-stubs/weak_platform_log_stubs.c",
        None,
        "nros_cpp_weak_stubs",
        true,
    );

    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    println!("cargo:rerun-if-env-changed=CORROSION_BUILD_DIR");
}

/// Generate `nros_cpp_ffi_config.rs` with build-time constants for executor storage.
fn generate_config(
    out_dir: &str,
    manifest_dir: &std::path::Path,
    probed: &std::collections::HashMap<String, u64>,
) {
    // Read executor layout from nros-node via Cargo `links` metadata.
    // Kept for `cargo:rerun-if-env-changed` plumbing now that hand-math
    // is gone (Phase 118.B closure of Phase 87.6).
    let _ = dep_usize("DEP_NROS_NODE_MAX_CBS");
    let _ = dep_usize("DEP_NROS_NODE_ARENA_SIZE");

    // CppContext = Executor + domain_id (u32) + padding. Phase 118.B —
    // sourced from `nros::sizes::EXECUTOR_SIZE` via the probe; the
    // hand-math upper bound that lived here is gone.
    //
    // `CppContext` adds a `u32 domain_id` field after the embedded
    // `CppExecutor`. With `repr(Rust)` and align=8 (Executor's max
    // alignment), the trailing field rounds the struct size up by one
    // u64 word: `size_of::<CppContext>() = size_of::<Executor>() + 8`
    // — verified by the const-assert in `lib.rs`. Probe = 0 only
    // happens on `cargo check --no-default-features` (no RMW).
    // Phase 119.3: merge is gone. Use probe values directly — each
    // cargo invocation gets its own per-build header in
    // `$CARGO_TARGET_DIR/nros-cpp-generated/<variant_slug>/`, so the
    // source-tree merging from 119.1 is unnecessary.
    const CPP_CONTEXT_OVERHEAD: usize = 8;
    let probe_executor_pre = probed.get("EXECUTOR_SIZE").copied().unwrap_or(0) as usize;
    if probe_executor_pre == 0 {
        println!(
            "cargo:warning=nros-cpp: EXECUTOR_SIZE probe returned 0 — \
             likely a `cargo check --no-default-features` run. The emitted \
             `CPP_EXECUTOR_OPAQUE_U64S` will be 1; do not link the \
             resulting rlib."
        );
    }
    let storage_bytes = probe_executor_pre.max(8) + CPP_CONTEXT_OVERHEAD;
    let opaque_u64s = storage_bytes.div_ceil(8);

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
    // Phase 119.3: probe values feed the per-build header directly.
    let exact_executor = probe_executor_pre;
    let exact_storage_bytes = storage_bytes;
    let exact_action_server = action_server_storage;
    let exact_action_client = action_client_storage;
    let exact_guard = probed.get("GUARD_CONDITION_SIZE").copied().unwrap_or(0) as usize;
    let exact_publisher = probed.get("PUBLISHER_SIZE").copied().unwrap_or(0) as usize;
    let exact_subscriber = probed.get("SUBSCRIBER_SIZE").copied().unwrap_or(0) as usize;
    let exact_service_client = probed.get("SERVICE_CLIENT_SIZE").copied().unwrap_or(0) as usize;
    let exact_service_server = probed.get("SERVICE_SERVER_SIZE").copied().unwrap_or(0) as usize;
    // Phase 122.3.d — L1 polling-mode Raw* storage. Probes from
    // `nros::sizes` (added in .c.3 / .c.6.a). Used by future
    // C++-class polling-mode storage fields and by the new
    // `nros_cpp_action_*_init_polling` FFI surface.
    let exact_raw_subscription = probed.get("RAW_SUBSCRIPTION_SIZE").copied().unwrap_or(0) as usize;
    let exact_raw_service_server =
        probed.get("RAW_SERVICE_SERVER_SIZE").copied().unwrap_or(0) as usize;
    let exact_raw_service_client =
        probed.get("RAW_SERVICE_CLIENT_SIZE").copied().unwrap_or(0) as usize;
    let exact_raw_action_server =
        probed.get("RAW_ACTION_SERVER_SIZE").copied().unwrap_or(0) as usize;
    let exact_raw_action_client =
        probed.get("RAW_ACTION_CLIENT_SIZE").copied().unwrap_or(0) as usize;
    let exact_raw_subscription_u64s = exact_raw_subscription.max(8).div_ceil(8);
    let exact_raw_service_server_u64s = exact_raw_service_server.max(8).div_ceil(8);
    let exact_raw_service_client_u64s = exact_raw_service_client.max(8).div_ceil(8);
    let exact_raw_action_server_u64s = exact_raw_action_server.max(8).div_ceil(8);
    let exact_raw_action_client_u64s = exact_raw_action_client.max(8).div_ceil(8);
    let _ = (manifest_dir, action_client_fallback);

    // Phase 119.3: source-tree header is a committed STUB that errors
    // out (see `include/nros/nros_cpp_config_generated.h`). Real header
    // gets written PER-BUILD to a variant-slug subdir under the cargo
    // target directory; `nros_cpp_setup()` CMake function finds it.
    if exact_executor == 0 {
        // `cargo check --no-default-features` / `cargo doc` path —
        // probe yielded nothing. Skip writing the per-build header.
        // Consumer code that #includes nros_cpp_config_generated.h
        // hits the stub `#error`, which is the desired behavior (no
        // RMW backend means no executor sizes to ship).
        return;
    }

    let exact_header = format!(
        "\
/* Auto-generated by nros-cpp build.rs — do not edit (Phase 119.3 per-build variant header) */
#ifndef NROS_CPP_CONFIG_GENERATED_H
#define NROS_CPP_CONFIG_GENERATED_H

/** Inline opaque storage size (bytes) for nros::Executor. */
#define NROS_CPP_EXECUTOR_STORAGE_SIZE {exact_storage_bytes}

/** Inline opaque storage size (bytes) for nros::ActionServer<A>. */
#define NROS_CPP_ACTION_SERVER_STORAGE_SIZE {exact_action_server}

/** Inline opaque storage size (bytes) for nros::ActionClient<A>. */
#define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE {exact_action_client}

#define NROS_EXECUTOR_SIZE {exact_executor}
#define NROS_GUARD_CONDITION_SIZE {exact_guard}
#define NROS_PUBLISHER_SIZE {exact_publisher}
#define NROS_SUBSCRIBER_SIZE {exact_subscriber}
#define NROS_SERVICE_CLIENT_SIZE {exact_service_client}
#define NROS_SERVICE_SERVER_SIZE {exact_service_server}

/* Phase 122.3.d — Layer-1 polling-mode raw handle storage. Sized to
 * `Raw*` types in `nros-node` (the same probes nros-c emits in its
 * variant header). Future C++ class polling fields read these.
 */
#define NROS_CPP_RAW_SUBSCRIPTION_OPAQUE_U64S {exact_raw_subscription_u64s}
#define NROS_CPP_RAW_SERVICE_SERVER_OPAQUE_U64S {exact_raw_service_server_u64s}
#define NROS_CPP_RAW_SERVICE_CLIENT_OPAQUE_U64S {exact_raw_service_client_u64s}
#define NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S {exact_raw_action_server_u64s}
#define NROS_CPP_RAW_ACTION_CLIENT_OPAQUE_U64S {exact_raw_action_client_u64s}

#endif /* NROS_CPP_CONFIG_GENERATED_H */
"
    );

    // Phase 119.3: write the per-build header to two stable locations.
    // 1. `$CARGO_TARGET_DIR/nros-cpp-generated/nros/...` — Zephyr (and
    //    any in-tree caller that explicitly sets CARGO_TARGET_DIR)
    //    prepends `$CARGO_TARGET_DIR/nros-cpp-generated` to its
    //    include path.
    // 2. `$CORROSION_BUILD_DIR/nros_cpp_config_generated.h` — cmake-
    //    corrosion's install rule reads from CMAKE_CURRENT_BINARY_DIR
    //    (== CORROSION_BUILD_DIR) and installs to
    //    `include/nros_cpp_<variant>/nros/`.
    // Within ANY single build context only one cargo invocation runs
    // against its target dir, so no variant slug is needed — each
    // build context owns its own target dir.
    write_header_to_target_dir(
        &["nros-cpp-generated", "nros", "nros_cpp_config_generated.h"],
        &exact_header,
    );
    write_header_to_corrosion("nros_cpp_config_generated.h", &exact_header);

    // Phase 119.3: also write a nros-c-format header to
    // `$CARGO_TARGET_DIR/nros-c-generated/nros/...`. nros-cpp's user
    // code transitively includes the C-side `<nros/parameter.h>` →
    // `<nros/types.h>` → `<nros/nros_config_generated.h>`, but in
    // Zephyr CPP-only builds nros-c isn't compiled (its build.rs
    // doesn't run), so without this fallback the C header would
    // resolve to the source-tree stub. The cpp + c sizes are
    // identical (same nros rlib), so it's safe to emit both from
    // nros-cpp's build script.
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
    let exec_storage_c = exact_executor.max(8);
    let exec_u64s_c = exec_storage_c.div_ceil(8);
    let session_u64s_c = probe_session.div_ceil(8);
    let publisher_u64s_c = exact_publisher.div_ceil(8);
    let guard_u64s_c = exact_guard.div_ceil(8);
    let lifecycle_u64s_c = probe_lifecycle_ctx.div_ceil(8);
    let raw_handle_u64s_c = probe_action_server_raw_handle.div_ceil(8);
    let c_format_header = format!(
        "\
/* Auto-generated by nros-cpp build.rs (nros-c-format companion) — do not edit */
#ifndef NROS_CONFIG_GENERATED_H
#define NROS_CONFIG_GENERATED_H

#include <stdint.h>

#define NROS_EXECUTOR_STORAGE_SIZE {exec_storage_c}

#define NROS_EXECUTOR_SIZE {exact_executor}
#define NROS_GUARD_CONDITION_SIZE {exact_guard}
#define NROS_PUBLISHER_SIZE {exact_publisher}
#define NROS_SUBSCRIBER_SIZE {exact_subscriber}
#define NROS_SERVICE_CLIENT_SIZE {exact_service_client}
#define NROS_SERVICE_SERVER_SIZE {exact_service_server}
#define NROS_SESSION_SIZE {probe_session}
#define NROS_LIFECYCLE_CTX_SIZE {probe_lifecycle_ctx}
#define NROS_ACTION_SERVER_INTERNAL_SIZE {probe_action_server_internal}

#define SESSION_OPAQUE_U64S {session_u64s_c}
#define PUBLISHER_OPAQUE_U64S {publisher_u64s_c}
#define EXECUTOR_OPAQUE_U64S {exec_u64s_c}
#define GUARD_HANDLE_OPAQUE_U64S {guard_u64s_c}
#define NROS_LIFECYCLE_CTX_OPAQUE_U64S {lifecycle_u64s_c}

/* Phase 122.3.d — C++-only Zephyr builds still include the C API's
 * nros_generated.h, whose public handle structs need the C-side raw
 * opaque storage macros. nros-c's build.rs emits these in C builds; mirror
 * them here because nros-c is not compiled in CPP-only build contexts.
 */
#undef SUBSCRIPTION_OPAQUE_U64S
#define SUBSCRIPTION_OPAQUE_U64S {exact_raw_subscription_u64s}
#undef SERVICE_SERVER_OPAQUE_U64S
#define SERVICE_SERVER_OPAQUE_U64S {exact_raw_service_server_u64s}
#undef SERVICE_CLIENT_OPAQUE_U64S
#define SERVICE_CLIENT_OPAQUE_U64S {exact_raw_service_client_u64s}
#undef ACTION_SERVER_OPAQUE_U64S
#define ACTION_SERVER_OPAQUE_U64S {exact_raw_action_server_u64s}
#undef ACTION_CLIENT_OPAQUE_U64S
#define ACTION_CLIENT_OPAQUE_U64S {exact_raw_action_client_u64s}

#ifdef __cplusplus
extern \"C\" {{
#endif
typedef struct ActionServerRawHandle {{
    uint64_t _opaque[{raw_handle_u64s_c}];
}} ActionServerRawHandle;
#ifdef __cplusplus
}}
#endif

#endif /* NROS_CONFIG_GENERATED_H */
"
    );
    write_header_to_target_dir(
        &["nros-c-generated", "nros", "nros_config_generated.h"],
        &c_format_header,
    );
}

/// Generate `include/nros/nros_cpp_ffi.h` using cbindgen.
fn generate_header(manifest_dir: &std::path::Path) {
    generate_cbindgen_header(manifest_dir, "cbindgen.toml", "include/nros/nros_cpp_ffi.h");
}
