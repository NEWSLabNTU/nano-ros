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

use crate::shared::{
    compile_c_stub, dep_usize, env_usize, generate_cbindgen_header, probe_nros_sizes,
    write_header_to_corrosion, write_header_to_target_dir,
};

pub fn run() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let probed = probe_nros_sizes("nros-c");
    // issue 0369 — the anchor suffix is derived from the exact header's SIZE
    // values (generate_config computes both), so it must be emitted after.
    // `variant_symbol.rs` is written unconditionally either way: `lib.rs`
    // `include!`s it, so a probe-less build (cargo check/doc) still compiles
    // — it just carries no anchor, matching its absent header.
    let variant_suffix = generate_config(&out_dir, &manifest_dir, &probed);
    emit_variant_symbol(&out_dir, variant_suffix.as_deref());
    // RFC-0090 / phase-429 W1 — the codegen-version anchors. Unconditional: the
    // accepted range is a property of this checkout's runtime, not of the
    // feature set and not of whether a size probe ran.
    generate_header(&manifest_dir);

    // Weak fallbacks for the platform log ABI (`nros_platform_log_*`), for
    // no-platform link paths (workspace test / metadata builds). A real platform
    // crate overrides them with strong defs. (The weak default of
    // `nros_app_register_backends` that once lived here was removed in
    // phase-249 P4a — registration is now the cmake strong stub from
    // `nano_ros_link_rmw`, a link error if absent.)
    compile_c_stub(
        &manifest_dir,
        "c-stubs/weak_platform_log_stubs.c",
        None,
        "nros_c_weak_stubs",
        true,
    );

    // Phase 88.12 — `nros_log_emit_fmt` C shim. Implemented in C
    // because Rust's `c_variadic` feature is still unstable. The shim
    // vsnprintfs and forwards to the Rust-side `nros_log_emit`.
    compile_c_stub(
        &manifest_dir,
        "c-stubs/log_fmt.c",
        Some(&manifest_dir.join("include")),
        "nros_c_log_fmt",
        true,
    );

    // Phase 257 (Stage-3) — the legacy `nros_board_native_run` C-FFI adapter
    // (a no-op sleep-spin with no executor, for the retired declarative entry)
    // is deleted. The typed C entry uses `nros_board_native_run_components`
    // (defined in nros-cpp), driving the real executor.

    // Re-run if source files change (for library rebuild + header regen)
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    // Phase 221.C: templates feed the generated config + header.
    println!("cargo:rerun-if-changed=templates/nros_c_config.rs.template");
    println!("cargo:rerun-if-changed=templates/nros_config_generated.h.template");
    println!("cargo:rerun-if-changed=templates/nros_config_generated_exact.h.template");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    // issue 0805 — NOT `rerun-if-env-changed=CORROSION_BUILD_DIR`.
    //
    // That is a PATH variable, and watching one as TEXT is the defect issue
    // 0491 records. It was previously safe here on a premise that no longer
    // holds: every cmake build dir owned its own cargo target dir, so two
    // spellings could never meet in one `.fingerprint/`. Sharing the target dir
    // across leaves put ~70 different spellings in ONE fingerprint namespace,
    // so every leaf invalidated the previous leaf's build script and
    // recompiled `nros-c` + `nros-cpp`. Measured: a repeated invocation is
    // 0.07 s, the same invocation after another leaf's is 7.59 s, and the
    // warm rebuild of one platform was 459 s of almost nothing but that.
    //
    // The header this script writes into `$CORROSION_BUILD_DIR` is ALSO written
    // to `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/`, which is leaf-independent
    // and is what the zephyr lane already consumes. cmake mirrors from there.
}

/// Generate `nros_c_config.rs` with build-time configurable constants.
fn generate_config(
    out_dir: &str,
    manifest_dir: &Path,
    probed: &std::collections::HashMap<String, u64>,
) -> Option<String> {
    // --- Executor layout from nros-node (via Cargo `links` metadata) ---
    // nros-node is the single source of truth for these values.
    // MESSAGE_BUFFER_SIZE must equal nrs-node's RX_BUF_SIZE because the
    // C API places entries (SubRawEntry<MESSAGE_BUFFER_SIZE>) into the
    // nros-node arena, which is sized for RX_BUF_SIZE entries.
    let max_cbs = dep_usize("DEP_NROS_NODE_MAX_CBS");
    let arena_size = dep_usize("DEP_NROS_NODE_ARENA_SIZE");
    let message_buffer_size = dep_usize("DEP_NROS_NODE_RX_BUF_SIZE");

    // --- C API knobs (nros-c only, not shared with nros-node) ---
    // phase-400 W6 — the `[knobs.runtime]` rungs for the LET buffer.
    //
    // `NROS_SERVICE_TIMEOUT_MS` below is deliberately NOT on the ladder: it is
    // read here and in `nros-rmw-zenoh/build.rs` because two artifacts embed
    // it, and a migrated knob may have exactly one reader
    // (`check-knob-single-reader`). Migrating it means giving the pair one
    // emission point first.
    let rungs = nros_platform_config::platform_config::BuildRungs::from_build_env();
    let let_buffer_size = env_usize(
        "NROS_LET_BUFFER_SIZE",
        rungs
            .as_ref()
            .and_then(|r| r.runtime_rungs().let_buffer_size)
            .unwrap_or(512),
    );
    // Phase 192.4 — default service-client RPC timeout. Read the same env var
    // and use the same default (30000) as the zenoh backend
    // (`nros-rmw-zenoh/build.rs`) so the two paths agree.
    //
    // Phase 214.C.1 — the 30_000 literal lives in TWO build.rs files
    // (here + `nros-rmw-zenoh/build.rs:25`). Single source = the
    // `NROS_SERVICE_TIMEOUT_MS` env var contract — both sites read it
    // with the same default. The literal is duplicated as a fallback;
    // when changing the default, update BOTH sites (and the doc strings
    // at line 200 here + line 42 there) in lockstep. Cross-doc
    // pointer: `nros-rmw-zenoh/build.rs:15` carries the rationale
    // for the 30_000 ms choice (Phase 160.C.2 — bumped from 10 s
    // because zenoh handshake under qemu slirp can drop early
    // packets).
    let service_timeout_ms = env_usize("NROS_SERVICE_TIMEOUT_MS", 30_000);

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
    // `$CARGO_TARGET_DIR/nros-c-generated/nros/...` (FLAT; issue 0360)
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
    // issue 0961 — see the note this emits into the header: the STACK cost is
    // the bare value, not the value-plus-backing `probe_executor` above.
    let probe_executor_value = probed.get("EXECUTOR_VALUE_SIZE").copied().unwrap_or(0) as usize;
    let executor_stack_min = probe_executor_value.saturating_mul(2);

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
    let probe_raw_action_server =
        probed.get("RAW_ACTION_SERVER_SIZE").copied().unwrap_or(0) as usize;
    let probe_raw_action_client =
        probed.get("RAW_ACTION_CLIENT_SIZE").copied().unwrap_or(0) as usize;

    // Phase 221.C: extracted to `templates/nros_c_config.rs.template`.
    // `@PLACEHOLDER@` follows the CMake `configure_file()` convention used
    // elsewhere in the tree (cmake/*.in, templates/overlay-board/*.template).
    //
    // Emitted AFTER the probes (issue 0472). It used to be written above, before
    // any of them was read, which is why the Rust side could compare only the
    // executor against what the header states — the other probe values were not
    // yet in scope. The `_opaque` widths a C caller allocates come from these
    // probes; `opaque_sizes.rs` now asserts each against its type's real
    // `size_of`, so a wrong number is a build error rather than a short buffer.
    let contents = read_template(manifest_dir, "nros_c_config.rs.template")
        .replace("@NROS_EXECUTOR_MAX_HANDLES@", &max_cbs.to_string())
        .replace("@LET_BUFFER_SIZE@", &let_buffer_size.to_string())
        .replace(
            "@SERVICE_DEFAULT_TIMEOUT_MS@",
            &service_timeout_ms.to_string(),
        )
        .replace("@MESSAGE_BUFFER_SIZE@", &message_buffer_size.to_string())
        .replace("@EXECUTOR_OPAQUE_U64S@", &executor_opaque_u64s.to_string())
        // Same `div_ceil(8)` the header macros below are built from, so the
        // guard compares the number the header ACTUALLY states, not a
        // re-derivation that could differ.
        .replace(
            "@PROBE_SESSION_U64S@",
            &probe_session.div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_PUBLISHER_U64S@",
            &probe_publisher.div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_GUARD_HANDLE_U64S@",
            &probe_guard.div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_LIFECYCLE_CTX_U64S@",
            &probe_lifecycle_ctx.div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_RAW_SUBSCRIPTION_U64S@",
            &probe_raw_subscription.max(8).div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_RAW_SERVICE_SERVER_U64S@",
            &probe_raw_service_server.max(8).div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_RAW_SERVICE_CLIENT_U64S@",
            &probe_raw_service_client.max(8).div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_RAW_ACTION_SERVER_U64S@",
            &probe_raw_action_server.max(8).div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_RAW_ACTION_CLIENT_U64S@",
            &probe_raw_action_client.max(8).div_ceil(8).to_string(),
        )
        .replace(
            "@PROBE_ACTION_SERVER_RAW_HANDLE_U64S@",
            &probe_action_server_raw_handle.div_ceil(8).to_string(),
        );

    std::fs::write(Path::new(out_dir).join("nros_c_config.rs"), contents).unwrap();

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

    // Phase 221.C: extracted to
    // `templates/nros_config_generated.h.template`. `@PLACEHOLDER@` follows
    // the CMake `configure_file()` convention used elsewhere in the tree
    // (cmake/*.in, templates/overlay-board/*.template).
    let c_header = read_template(manifest_dir, "nros_config_generated.h.template")
        .replace(
            "@EXECUTOR_STORAGE_BYTES@",
            &executor_storage_bytes.to_string(),
        )
        .replace("@PROBE_EXECUTOR@", &probe_executor.to_string())
        .replace("@PROBE_EXECUTOR_VALUE@", &probe_executor_value.to_string())
        .replace("@EXECUTOR_STACK_MIN@", &executor_stack_min.to_string())
        .replace("@PROBE_GUARD@", &probe_guard.to_string())
        .replace("@PROBE_PUBLISHER@", &probe_publisher.to_string())
        .replace("@PROBE_SUBSCRIBER@", &probe_subscriber.to_string())
        .replace("@PROBE_SERVICE_CLIENT@", &probe_service_client.to_string())
        .replace("@PROBE_SERVICE_SERVER@", &probe_service_server.to_string())
        .replace("@PROBE_SESSION@", &probe_session.to_string())
        .replace("@PROBE_LIFECYCLE_CTX@", &probe_lifecycle_ctx.to_string())
        .replace(
            "@PROBE_ACTION_SERVER_INTERNAL@",
            &probe_action_server_internal.to_string(),
        )
        .replace("@SESSION_OPAQUE_U64S@", &session_opaque_u64s.to_string())
        .replace(
            "@PUBLISHER_OPAQUE_U64S@",
            &publisher_opaque_u64s.to_string(),
        )
        .replace("@EXECUTOR_OPAQUE_U64S@", &executor_opaque_u64s.to_string())
        .replace(
            "@GUARD_HANDLE_OPAQUE_U64S@",
            &guard_handle_opaque_u64s.to_string(),
        )
        .replace(
            "@LIFECYCLE_CTX_OPAQUE_U64S@",
            &lifecycle_ctx_opaque_u64s.to_string(),
        )
        .replace("@RAW_HANDLE_U64S@", &raw_handle_u64s.to_string())
        // RFC-0090 / phase-429 — the accepted codegen range, straight from
        // `nros_core::codegen_version` (this crate `include!`s it). Generated
        // C compares against these with the preprocessor.
        .replace(
            "@CODEGEN_VERSION@",
            &crate::codegen_version::NROS_CODEGEN_VERSION.to_string(),
        )
        .replace(
            "@CODEGEN_VERSION_MIN@",
            &crate::codegen_version::NROS_CODEGEN_VERSION_MIN.to_string(),
        );
    // Phase 119.3: source-tree header is now a committed STUB that
    // `#error`s. Real header gets written PER-BUILD to
    // `$CARGO_TARGET_DIR/nros-c-generated/nros/` (FLAT; issue 0360).
    // `nros_c_setup()` CMake function finds it.
    let _ = (manifest_dir, c_header);
    if probe_executor == 0 {
        // `cargo check --no-default-features` / `cargo doc` — no probe
        // result, skip writing.
        return None;
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
    let exact_raw_action_server_u64s = probe_raw_action_server.max(8).div_ceil(8);
    let exact_raw_action_client_u64s = probe_raw_action_client.max(8).div_ceil(8);
    // Phase 221.C: extracted to
    // `templates/nros_config_generated_exact.h.template`. Variant-exact
    // header — see the upper-bound template above for the placeholder
    // convention rationale.
    let exact_header = read_template(manifest_dir, "nros_config_generated_exact.h.template")
        .replace(
            "@EXACT_EXECUTOR_STORAGE@",
            &exact_executor_storage.to_string(),
        )
        .replace("@PROBE_EXECUTOR@", &probe_executor.to_string())
        .replace("@PROBE_EXECUTOR_VALUE@", &probe_executor_value.to_string())
        .replace("@EXECUTOR_STACK_MIN@", &executor_stack_min.to_string())
        .replace("@PROBE_GUARD@", &probe_guard.to_string())
        .replace("@PROBE_PUBLISHER@", &probe_publisher.to_string())
        .replace("@PROBE_SUBSCRIBER@", &probe_subscriber.to_string())
        .replace("@PROBE_SERVICE_CLIENT@", &probe_service_client.to_string())
        .replace("@PROBE_SERVICE_SERVER@", &probe_service_server.to_string())
        .replace("@PROBE_SESSION@", &probe_session.to_string())
        .replace("@PROBE_LIFECYCLE_CTX@", &probe_lifecycle_ctx.to_string())
        .replace(
            "@PROBE_ACTION_SERVER_INTERNAL@",
            &probe_action_server_internal.to_string(),
        )
        .replace("@EXACT_SESSION_U64S@", &exact_session_u64s.to_string())
        .replace("@EXACT_PUBLISHER_U64S@", &exact_publisher_u64s.to_string())
        .replace("@EXACT_EXECUTOR_U64S@", &exact_executor_u64s.to_string())
        .replace("@EXACT_GUARD_U64S@", &exact_guard_u64s.to_string())
        .replace("@EXACT_LIFECYCLE_U64S@", &exact_lifecycle_u64s.to_string())
        .replace(
            "@EXACT_RAW_SUBSCRIPTION_U64S@",
            &exact_raw_subscription_u64s.to_string(),
        )
        .replace(
            "@EXACT_RAW_SERVICE_SERVER_U64S@",
            &exact_raw_service_server_u64s.to_string(),
        )
        .replace(
            "@EXACT_RAW_SERVICE_CLIENT_U64S@",
            &exact_raw_service_client_u64s.to_string(),
        )
        .replace(
            "@EXACT_RAW_ACTION_SERVER_U64S@",
            &exact_raw_action_server_u64s.to_string(),
        )
        .replace(
            "@EXACT_RAW_ACTION_CLIENT_U64S@",
            &exact_raw_action_client_u64s.to_string(),
        )
        .replace(
            "@EXACT_RAW_HANDLE_U64S@",
            &exact_raw_handle_u64s.to_string(),
        )
        // RFC-0090 — the EXACT template is the one written as
        // `nros_config_generated.h` (see `write_header_to_corrosion` below), so
        // the version macros have to be here as well as in the probe template.
        // Missing them here is not silent: generated C hits the fail-closed
        // `#ifndef` and says the config header did not define them.
        .replace(
            "@CODEGEN_VERSION@",
            &crate::codegen_version::NROS_CODEGEN_VERSION.to_string(),
        )
        .replace(
            "@CODEGEN_VERSION_MIN@",
            &crate::codegen_version::NROS_CODEGEN_VERSION_MIN.to_string(),
        );

    // Issue 0360 — stamp the variant into the header, and emit the matching
    // symbol into the archive, so a mismatch is a link error. issue 0369 —
    // the SYMBOL derives from the size values this header ships (not the
    // cargo feature spelling), so the two nros-c builds of a mixed C+C++
    // workspace — same sizes, different rmw-feature spellings — agree on
    // the anchor by construction. The human-readable NROS_CONFIG_VARIANT
    // string keeps the feature slug for debugging.
    let sizes: [(&str, usize); 22] = [
        ("EXACT_EXECUTOR_STORAGE", exact_executor_storage),
        ("PROBE_EXECUTOR", probe_executor),
        ("PROBE_EXECUTOR_VALUE", probe_executor_value),
        ("PROBE_GUARD", probe_guard),
        ("PROBE_PUBLISHER", probe_publisher),
        ("PROBE_SUBSCRIBER", probe_subscriber),
        ("PROBE_SERVICE_CLIENT", probe_service_client),
        ("PROBE_SERVICE_SERVER", probe_service_server),
        ("PROBE_SESSION", probe_session),
        ("PROBE_LIFECYCLE_CTX", probe_lifecycle_ctx),
        ("PROBE_ACTION_SERVER_INTERNAL", probe_action_server_internal),
        ("EXACT_SESSION_U64S", exact_session_u64s),
        ("EXACT_PUBLISHER_U64S", exact_publisher_u64s),
        ("EXACT_EXECUTOR_U64S", exact_executor_u64s),
        ("EXACT_GUARD_U64S", exact_guard_u64s),
        ("EXACT_LIFECYCLE_U64S", exact_lifecycle_u64s),
        ("EXACT_RAW_SUBSCRIPTION_U64S", exact_raw_subscription_u64s),
        (
            "EXACT_RAW_SERVICE_SERVER_U64S",
            exact_raw_service_server_u64s,
        ),
        (
            "EXACT_RAW_SERVICE_CLIENT_U64S",
            exact_raw_service_client_u64s,
        ),
        ("EXACT_RAW_ACTION_SERVER_U64S", exact_raw_action_server_u64s),
        ("EXACT_RAW_ACTION_CLIENT_U64S", exact_raw_action_client_u64s),
        ("EXACT_RAW_HANDLE_U64S", exact_raw_handle_u64s),
    ];
    let variant_suffix = crate::shared::variant_suffix_from_sizes(&sizes);
    let exact_header = exact_header
        .replace("@VARIANT_SLUG@", &crate::shared::variant_slug())
        .replace("@VARIANT_SYMBOL@", &variant_suffix);

    // Phase 119.3: two stable per-build locations (see nros-cpp/build.rs
    // for rationale).
    write_header_to_target_dir(
        &["nros-c-generated", "nros", "nros_config_generated.h"],
        &exact_header,
    );
    write_header_to_corrosion("nros_config_generated.h", &exact_header);
    Some(variant_suffix)
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
    crate::shared::write_atomic(path, new_contents);
}

fn read_template(manifest_dir: &Path, name: &str) -> String {
    let path = manifest_dir.join("templates").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("nros-c: read template {}: {err}", path.display()))
}

/// Generate `include/nros/nros_generated.h` using cbindgen.
///
/// cbindgen reads Rust source files and generates C header declarations
/// for all `#[repr(C)]` structs, enums, type aliases, constants, and
/// `extern "C"` functions. The generated header is the single source of
/// truth for C/Rust type layout compatibility.
fn generate_header(manifest_dir: &Path) {
    generate_cbindgen_header(
        manifest_dir,
        "cbindgen.toml",
        "include/nros/nros_generated.h",
    );
}

/// Issue 0360 — write the archive side of the variant stamp.
///
/// Included by `nros-c/src/lib.rs` exactly like the existing
/// `c_surface_anchor.rs`. The symbol NAME carries the feature slug, so an
/// archive built with a different feature set simply does not define what a
/// consumer's header asked for, and the linker says so.
fn emit_variant_symbol(out_dir: &str, suffix: Option<&str>) {
    // `lib.rs` `include!`s this file unconditionally, so it must exist even
    // on probe-less builds (cargo check/doc) — those carry no anchor,
    // matching their absent header.
    let out = std::path::PathBuf::from(out_dir);
    std::fs::write(
        out.join("variant_symbol.rs"),
        "// Generated by nros-build-helpers (issues 0360/0369). Do not edit.\n\
         // The variant anchor is a WEAK C definition (below, via cc) — a\n\
         // Rust `#[no_mangle] static` is strong, and a mixed C+C++ image\n\
         // links TWO nros-c builds (the C half's and the one embedded in\n\
         // nros-cpp): with the issue-0369 size-derived suffix they agree on\n\
         // the name, so two strong defs would collide. N identical weak\n\
         // defs merge; a consumer holding a DIFFERENT-sized header still\n\
         // fails to resolve its extern.\n",
    )
    .expect("write variant_symbol.rs");
    let Some(suffix) = suffix else {
        return;
    };
    // phase-340 W6 / issue 0904 — the TU is TRACKED, not generated into
    // `OUT_DIR`, and only its symbol NAME varies. A generated TU carries its
    // absolute `OUT_DIR` into `__FILE__` and the debug-info compilation dir,
    // which is what made otherwise-identical `libnros_c.a` artifacts differ
    // between two target dirs — the single carrier W6 measured.
    //
    // W6 masked that with `-ffile-prefix-map`. On the NuttX cross build the
    // flag was NOT taking effect (0 `/nros-out` hits, 2 real-path hits) while
    // `is_flag_supported` reported the flag SUPPORTED, so the determinism it
    // promised had quietly stopped holding with nothing going red. A tracked
    // source removes the dependence rather than masking it: the path is this
    // checkout's, identical in every target dir.
    //
    // `CARGO_MANIFEST_DIR` is expanded at THIS crate's compile time, so it names
    // nros-build-helpers' own directory, not the consumer's.
    let c_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("csrc/variant_symbol.c");
    println!("cargo:rerun-if-changed={}", c_src.display());
    let mut build = cc::Build::new();
    // issue 0383 — implicit-function-declaration / int-conversion as errors.
    nros_cc_flags::strict_decls(&mut build);
    build.file(&c_src).warnings(false);
    build.define(
        "NROS_VARIANT_SYMBOL",
        format!("nros_config_variant_{suffix}").as_str(),
    );
    crate::shared::apply_baremetal_libc(&mut build);
    build.compile("nros_variant_symbol");
}
