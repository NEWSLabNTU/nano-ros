//! 194.3c.2 — shared NuttX platform-lib build (the C platform port:
//! `nros-platform-posix/src/{platform.c,net.c}` compiled against the board's
//! NuttX export). This was per-board, arm-hardcoded in
//! `nros-board-nuttx-qemu-arm/build.rs` (compiler `arm-none-eabi-gcc`, cflags
//! `-mcpu=cortex-a7 …`, includes `arch/arm/src/{chip,common,armv7-a}`). Hoisted
//! here and parameterized so a new-arch NuttX board (riscv) reuses it with its
//! own `NUTTX_*` env. Defaults reproduce the arm build byte-for-byte.

use std::env;

/// Compile the NuttX C platform port into `libnros_platform_nuttx.a` and emit
/// the `cargo:rustc-link-lib` directive. All arch-specifics come from env (the
/// board overlay sets them); defaults are the qemu-arm cortex-a7 hardfloat
/// values used before 194.3c.
///
/// Env (all optional, arm defaults):
/// - `NUTTX_CROSS` — cross C compiler (default `arm-none-eabi-gcc`).
/// - `NUTTX_PLATFORM_CFLAGS` — arch flags for the platform.c/net.c compile
///   (default `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4`). Kept
///   DISTINCT from the FFI app's `NUTTX_ARCH_CFLAGS` (vfpv3-d16) because the
///   arm platform port has always compiled with neon-vfpv4; `-std=c11` is
///   always appended (generic to the C port).
/// - `NUTTX_ARCH_INCLUDES` — space-separated dirs relative to `NUTTX_DIR`
///   (default `arch/arm/src/{chip,common,armv7-a}`). Same var the FFI helper
///   reads, so a board sets it once.
pub fn run_platform() {
    let nuttx_dir = nros_build_paths::nuttx_dir();
    // Match the legacy guard: skip when the NuttX tree isn't populated yet
    // (a clean checkout before `make export`).
    if !nuttx_dir.join("include").exists() {
        return;
    }

    let cffi_include = nros_build_paths::nros_platform_cffi_include();
    let platform_src = nros_build_paths::nros_platform_posix_src();

    let nuttx_cross = env::var("NUTTX_CROSS").unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());
    let cflags: Vec<String> = env::var("NUTTX_PLATFORM_CFLAGS")
        .unwrap_or_else(|_| "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    let arch_includes: Vec<String> = env::var("NUTTX_ARCH_INCLUDES")
        .unwrap_or_else(|_| {
            "arch/arm/src/chip arch/arm/src/common arch/arm/src/armv7-a".to_string()
        })
        .split_whitespace()
        .map(String::from)
        .collect();

    let mut platform = cc::Build::new();
    platform.compiler(&nuttx_cross);
    for f in &cflags {
        platform.flag(f);
    }
    platform.flag("-std=c11");
    platform.define("__NuttX__", None);
    platform.include(&cffi_include);
    platform.include(nuttx_dir.join("include"));
    for inc in &arch_includes {
        platform.include(nuttx_dir.join(inc));
    }
    platform.include(nuttx_dir.join("sched"));
    platform.file(platform_src.join("platform.c"));
    platform.file(platform_src.join("net.c"));
    platform.compile("nros_platform_nuttx");

    println!("cargo:rustc-link-lib=static=nros_platform_nuttx");
    println!("cargo:rerun-if-changed={}", platform_src.display());
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    println!("cargo:rerun-if-env-changed=NUTTX_CROSS");
    println!("cargo:rerun-if-env-changed=NUTTX_PLATFORM_CFLAGS");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH_INCLUDES");
}
