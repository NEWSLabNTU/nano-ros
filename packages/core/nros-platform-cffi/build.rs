//! Phase 121.4.a — compile `tests/c_stubs/platform_stubs.c` into a
//! small static lib for the C-stub integration test.
//!
//! Gated behind the `c-stub-test` Cargo feature so consumers that
//! vendor this crate without a C toolchain on the build host aren't
//! forced through the cc invocation.

fn main() {
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.h");

    #[cfg(feature = "c-stub-test")]
    cc::Build::new()
        .file("tests/c_stubs/platform_stubs.c")
        .include("tests/c_stubs")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_platform_stubs");
}
