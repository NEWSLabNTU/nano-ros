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
    emit_section_link_flags();
}

/// Phase 128.A.1 — anchor the `.nros_rmw_init` linker section so the
/// runtime walker (`nros_rmw_cffi_walk_init_section`) can iterate
/// every backend init entry. Each backend crate emits one entry into
/// the section via `#[link_section = ".nros_rmw_init"]`; the
/// `__start_/__stop_` encapsulation symbols come from the
/// `cmake/nros-rmw-section.ld` fragment INSERT'd before `.rodata` on
/// hosted ELF targets. Mach-O has its own native `__section_start_/
/// __section_stop_` aliases — skipped here. Bare-metal / RTOS users
/// INCLUDE the same fragment from their own linker script (see
/// `book/src/reference/rmw-backends.md`).
fn emit_section_link_flags() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let hosted_elf = matches!(
        target_os.as_str(),
        "linux" | "freebsd" | "netbsd" | "openbsd" | "android"
    ) && target_env != "msvc";
    if !hosted_elf {
        // Bare-metal (`target_os = "none"`) and embedded RTOS targets
        // come with their own linker script; the fragment is INCLUDE'd
        // from there, not injected by this build script.
        return;
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let script = format!("{manifest_dir}/cmake/nros-rmw-section.ld");
    println!("cargo:rerun-if-changed=cmake/nros-rmw-section.ld");
    println!("cargo:rustc-link-arg=-Wl,-T,{script}");
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
