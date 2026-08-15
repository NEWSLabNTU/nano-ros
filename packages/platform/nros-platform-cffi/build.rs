//! Build-time C compilation for the in-crate integration tests.
//!
//! Two mutually-exclusive features pull in a different platform-ABI
//! provider:
//!
//! - `c-stub-test` (Phase 121.4.a) — counter-bumping no-op stubs in
//!   `tests/c_stubs/platform_stubs.c`. Used to exercise the Rust
//!   extern declarations + macro emission without depending on real
//!   POSIX behaviour.
//! - `posix-c-port` (Phase 121.3.posix / 123.A.1.x.2) — the native C
//!   port living in `../nros-platform-posix/src/platform.c` (was
//!   `nros-platform-posix-c` until the Rust crate of the same name
//!   was deleted). Used by
//!   `tests/c_port_posix.rs` to verify the reference implementation
//!   round-trips through `CffiPlatform`.
//!
//! Both produce the same symbol set; enabling both at once would emit
//! duplicate `#[no_mangle]` definitions. The feature gate enforces
//! one or the other (or neither, for non-test consumers).

fn main() {
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/platform_stubs.h");
    // phase-364 W2 — the ABI HEADER is an input to every C file below, and was
    // not watched. Editing `<nros/platform.h>` therefore did not rebuild the
    // port, which is how a deliberately-broken `_Static_assert` bound went
    // undetected while being tested: cargo had nothing to rebuild. Same shape as
    // issue 0196 (a probe blind to one of its inputs).
    println!("cargo:rerun-if-changed=../nros-platform-api/include/nros/platform.h");
    println!("cargo:rerun-if-changed=../nros-platform-api/include/nros/platform_net.h");
    println!("cargo:rerun-if-changed=../nros-platform-api/include/nros/platform_timer.h");
    println!("cargo:rerun-if-changed=../nros-platform-posix/src/platform.c");
    println!("cargo:rerun-if-changed=../nros-platform-posix/src/net.c");
    println!("cargo:rerun-if-changed=../nros-platform-posix/src/timer.c");

    #[cfg(all(feature = "c-stub-test", feature = "posix-c-port"))]
    compile_error!(
        "features `c-stub-test` and `posix-c-port` are mutually exclusive — \
         both define the canonical `nros_platform_*` symbols"
    );

    #[cfg(feature = "c-stub-test")]
    // issue 0383 — implicit-function-declaration / int-conversion as errors.
    nros_cc_flags::strict_decls(&mut cc::Build::new())
        .file("tests/c_stubs/platform_stubs.c")
        .include("tests/c_stubs")
        // The stub's own header says it "defines every `nros_platform_*` symbol
        // declared in `<nros/platform.h>`" — but that header was never on the
        // include path, so the TU hand-declared each signature instead. Any type
        // added upstream was then simply unknown to it: phase-359 W10's
        // `nros_platform_task_attr_t` broke `c-stub-test` outright, and nothing
        // noticed because no default build enables the feature. Compiling
        // against the real declarations makes a signature change a compile
        // error here instead of a silent mirror drift.
        .include("../nros-platform-api/include")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_platform_stubs");

    #[cfg(feature = "posix-c-port")]
    {
        // issue 0383 — implicit-function-declaration / int-conversion as errors.
        nros_cc_flags::strict_decls(&mut cc::Build::new())
            .file("../nros-platform-posix/src/platform.c")
            .file("../nros-platform-posix/src/net.c")
            .file("../nros-platform-posix/src/timer.c")
            // phase-241 B.2 — the canonical `<nros/platform.h>` moved to
            // nros-platform-api; this crate's own include/ is now empty.
            .include("../nros-platform-api/include")
            .warnings(true)
            .extra_warnings(true)
            .flag_if_supported("-Wpedantic")
            .define("_POSIX_C_SOURCE", "200809L")
            .compile("nros_platform_posix");
        // pthread + librt for downstream test binaries (rt supplies timer_*).
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=rt");
    }
}
