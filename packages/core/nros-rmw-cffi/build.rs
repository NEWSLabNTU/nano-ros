//! Phase 115.G.4 — compile `tests/c_stubs/c_stub_transport.c` into a
//! small static lib for the second-language smoke test.
//!
//! Gated behind the `c-stub-test` Cargo feature so consumers of
//! `nros-rmw-cffi` that vendor it without a C toolchain on the build
//! host aren't forced through the cc invocation.

fn main() {
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.h");

    if std::env::var_os("CARGO_FEATURE_C_STUB_TEST").is_none() {
        return;
    }

    cc::Build::new()
        .file("tests/c_stubs/c_stub_transport.c")
        .include("tests/c_stubs")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_c_stub_transport");
}
