//! Build-time C compilation for the in-crate integration tests.
//!
//! Two mutually-exclusive features pull in a different platform-ABI
//! provider:
//!
//! - `c-stub-test` (Phase 121.4.a) — counter-bumping no-op stubs in
//!   `tests/c_stubs/platform_stubs.c`. Used to exercise the Rust
//!   extern declarations + macro emission without depending on real
//!   POSIX behaviour.
//! - `posix-c-port` (Phase 121.3.posix) — the native C port living in
//!   `../nros-platform-posix-c/src/platform.c`. Used by
//!   `tests/c_port_posix.rs` to verify the reference implementation
//!   round-trips through `CffiPlatform`.
//!
//! Both produce the same symbol set; enabling both at once would emit
//! duplicate `#[no_mangle]` definitions. The feature gate enforces
//! one or the other (or neither, for non-test consumers).

fn main() {
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.h");
    println!("cargo:rerun-if-changed=../nros-platform-posix-c/src/platform.c");
    println!("cargo:rerun-if-changed=../nros-platform-posix-c/src/net.c");
    println!("cargo:rerun-if-changed=../nros-platform-posix-c/src/timer.c");

    #[cfg(all(feature = "c-stub-test", feature = "posix-c-port"))]
    compile_error!(
        "features `c-stub-test` and `posix-c-port` are mutually exclusive — \
         both define the canonical `nros_platform_*` symbols"
    );

    #[cfg(feature = "c-stub-test")]
    cc::Build::new()
        .file("tests/c_stubs/platform_stubs.c")
        .include("tests/c_stubs")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_platform_stubs");

    #[cfg(feature = "posix-c-port")]
    {
        cc::Build::new()
            .file("../nros-platform-posix-c/src/platform.c")
            .file("../nros-platform-posix-c/src/net.c")
            .file("../nros-platform-posix-c/src/timer.c")
            .include("include")
            .warnings(true)
            .extra_warnings(true)
            .flag_if_supported("-Wpedantic")
            .define("_POSIX_C_SOURCE", "200809L")
            .compile("nros_platform_posix_c");
        // pthread + librt for downstream test binaries (rt supplies timer_*).
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=rt");
    }
}
