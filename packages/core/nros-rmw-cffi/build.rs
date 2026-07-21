//! Build script. Two responsibilities:
//!
//! 1. Phase 104.B.1 — read `NROS_RMW_MAX_BACKENDS` (env var, default
//!    8) and re-emit it as a `cargo:rustc-env` so the crate's source
//!    can read it via `env!("NROS_RMW_MAX_BACKENDS")`. Mirrors the
//!    `NROS_EXECUTOR_MAX_CBS` / `NROS_LET_BUFFER_SIZE` pattern in
//!    `nros-node`. Cortex-M0+ users can drop to 2; bridge users on
//!    companion-class hardware can bump to 16. Range [1, 64];
//!    values outside the range fail the build.
//!
//! 2. Phase 115.G.4 — compile `tests/c_stubs/c_stub_transport.c`
//!    into a small static lib for the second-language smoke test.
//!    Gated behind the `c-stub-test` Cargo feature so consumers of
//!    `nros-rmw-cffi` that vendor it without a C toolchain on the
//!    build host aren't forced through the cc invocation.

fn main() {
    emit_max_backends();
    maybe_build_c_stub();
}

fn emit_max_backends() {
    println!("cargo:rerun-if-env-changed=NROS_RMW_MAX_BACKENDS");

    let raw = std::env::var("NROS_RMW_MAX_BACKENDS").unwrap_or_else(|_| "8".to_string());
    let parsed: usize = raw.trim().parse().unwrap_or_else(|err| {
        panic!(
            "NROS_RMW_MAX_BACKENDS=\"{raw}\" is not a valid usize: {err}. \
             Set a positive integer (default 8)."
        )
    });

    if !(1..=64).contains(&parsed) {
        panic!(
            "NROS_RMW_MAX_BACKENDS={parsed} out of range [1, 64]. Bump \
             the build script's upper bound if a larger value is truly \
             needed."
        );
    }

    println!("cargo:rustc-env=NROS_RMW_MAX_BACKENDS={parsed}");
}

fn maybe_build_c_stub() {
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.h");
    println!("cargo:rerun-if-changed=tests/c_stubs/abi_layout_check.c");
    println!("cargo:rerun-if-changed=include/nros");

    if std::env::var_os("CARGO_FEATURE_C_STUB_TEST").is_none() {
        return;
    }

    cc::Build::new()
        .file("tests/c_stubs/c_stub_transport.c")
        .include("tests/c_stubs")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_c_stub_transport");

    // ABI-layout single-source-of-truth (issue #238 / #239): a header
    // TU of `_Static_assert`s that pin the C-side widths of the RMW
    // mirror. Its Rust counterpart is the `abi_layout` const-assert
    // block in `src/lib.rs`. If either side's layout drifts, exactly
    // one guard fails the build. Compiled against the public headers.
    cc::Build::new()
        .file("tests/c_stubs/abi_layout_check.c")
        .include("include")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_abi_layout_check");
}
