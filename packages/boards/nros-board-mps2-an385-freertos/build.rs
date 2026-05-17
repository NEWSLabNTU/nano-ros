//! Build script for nros-board-mps2-an385-freertos
//!
//! Compiles the FreeRTOS kernel, lwIP stack, lwIP FreeRTOS sys_arch,
//! LAN9118 lwIP netif driver, and a small C startup/glue layer into
//! a single static library linked into the final firmware.
//!
//! Required environment variables:
//!   FREERTOS_DIR       — FreeRTOS kernel source root
//!   FREERTOS_PORT      — portable layer, e.g. "GCC/ARM_CM3"
//!   LWIP_DIR           — lwIP source root
//!   FREERTOS_CONFIG_DIR — (optional) directory with FreeRTOSConfig.h + lwipopts.h
//!                         Defaults to this crate's config/ directory.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // --- Linker script ---
    File::create(out_dir.join("mps2_an385.ld"))
        .unwrap()
        .write_all(include_bytes!("config/mps2_an385.ld"))
        .unwrap();
    // Make the linker script discoverable by the final binary.
    // The binary's .cargo/config.toml specifies `-Tmps2_an385.ld` via rustflags.
    println!("cargo:rustc-link-search={}", out_dir.display());

    // --- Environment variables ---
    let freertos_dir = env_path("FREERTOS_DIR");
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let lwip_dir = env_path("LWIP_DIR");
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir.clone());

    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    let lan9118_dir = manifest_dir.join("../../drivers/lan9118-lwip");

    // --- Trace opt-in (NROS_TRACE=1) ---
    let nros_trace = env::var("NROS_TRACE").unwrap_or_default() == "1";
    println!("cargo:rerun-if-env-changed=NROS_TRACE");

    // Phase 152.1.B.4 — FreeRTOS kernel + lwIP + nros-platform-freertos
    // are now compiled by `nros-board-freertos/build.rs` (the generic
    // crate this overlay depends on). Its `cargo:rustc-link-lib=static=...`
    // lines propagate transitively into this binary's link. Overlay
    // only needs the per-board pieces below.

    // --- Build LAN9118 lwIP netif driver ---
    let mut lan9118 = cc::Build::new();
    configure_arm_cm3(&mut lan9118);
    add_freertos_includes(&mut lan9118, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut lan9118, &lwip_dir);
    lan9118.include(lan9118_dir.join("include"));
    lan9118.file(lan9118_dir.join("src/lan9118_lwip.c"));
    lan9118.compile("lan9118_lwip");

    // --- Tonbandgeraet trace library (opt-in via NROS_TRACE=1) ---
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");

        let mut tband = cc::Build::new();
        configure_arm_cm3(&mut tband);
        add_freertos_includes(&mut tband, &freertos_dir, &port_dir, &freertos_config_dir);
        tband.include(tband_dir.join("inc"));
        tband.include(&trace_config_dir);
        tband.define("NROS_TRACE", "1");
        tband.file(tband_dir.join("src/tband.c"));
        tband.file(tband_dir.join("src/tband_freertos.c"));
        tband.file(tband_dir.join("src/tband_backend.c"));
        tband.compile("tband");
        println!("cargo:rustc-link-lib=static=tband");
        println!("cargo:rustc-cfg=nros_trace");
    }

    // --- Build startup/glue C code ---
    let mut glue = cc::Build::new();
    configure_arm_cm3(&mut glue);
    add_freertos_includes(&mut glue, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut glue, &lwip_dir);
    glue.include(lan9118_dir.join("include"));
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");
        glue.include(tband_dir.join("inc"));
        glue.include(&trace_config_dir);
        glue.define("NROS_TRACE", "1");
    }

    // Phase 152.1.B.4 — overlay glue carries only board-specific
    // C: MPS2-AN385 vector table + Reset_Handler + LAN9118 diag +
    // trace_dump (always compiled; stubs when NROS_TRACE off).
    // Generic FreeRTOS / lwIP / nros-platform-freertos pieces moved
    // to `nros-board-freertos/build.rs`.
    glue.file(manifest_dir.join("c/board_mps2.c"));
    glue.file(manifest_dir.join("trace/trace_dump.c"));

    glue.compile("startup");

    // --- Link order ---
    // Phase 152.1.B.4 — overlay re-emits the link-lib lines for
    // the four archives the generic `nros-board-freertos` crate
    // produces. `cargo:rustc-link-search` propagates transitively
    // (the generic crate's OUT_DIR ends up on `-L` automatically)
    // but `cargo:rustc-link-lib` does NOT propagate cleanly
    // through a regular `[dependencies]` chain in rust-lld's
    // ordering, so the overlay names them explicitly. Order
    // matters for static-archive symbol resolution; per-board
    // archives first (so the overlay's strong `nros_board_*`
    // overrides win), then generic kernel + lwIP + glue +
    // platform-port archives.
    println!("cargo:rustc-link-lib=static=startup");
    println!("cargo:rustc-link-lib=static=lan9118_lwip");
    println!("cargo:rustc-link-lib=static=nros_platform_freertos");
    println!("cargo:rustc-link-lib=static=freertos_glue");
    println!("cargo:rustc-link-lib=static=lwip");
    println!("cargo:rustc-link-lib=static=freertos");

    // --- Newlib (libc + nosys stubs for bare-metal) ---
    // zenoh-pico and lwIP use standard C library functions (atoi, strtoul, snprintf, etc.)
    // Use --print-file-name to discover multilib-correct paths (--print-sysroot is empty
    // on some distros).
    let libc_path = gcc_print_file("libc.a");
    let libc_dir = Path::new(&libc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libc_dir.display());
    // GCC's own library (libgcc.a) for ARM intrinsics
    let libgcc_path = gcc_print_file("libgcc.a");
    let libgcc_dir = Path::new(&libgcc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libgcc_dir.display());
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=nosys");
    println!("cargo:rustc-link-lib=static=gcc");

    // --- Rerun triggers ---
    println!("cargo:rerun-if-changed=config/FreeRTOSConfig.h");
    println!("cargo:rerun-if-changed=config/lwipopts.h");
    println!("cargo:rerun-if-changed=config/mps2_an385.ld");
    println!("cargo:rerun-if-changed=c/board_mps2.c");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CFLAGS");
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(
        env::var(name).unwrap_or_else(|_| panic!("{name} not set — run `just setup-freertos`")),
    )
}

/// Phase 152.1.B.3 — generic FreeRTOS+lwIP compiler-flag setup.
///
/// Reads `FREERTOS_CFLAGS` env var (space-separated flag list) +
/// applies each via `cc::Build::flag`. The MPS2-AN385 board's
/// reference `.cargo/config.toml` sets
/// `FREERTOS_CFLAGS = "-mcpu=cortex-m3 -mthumb"`; future overlays
/// (Cortex-M4F, etc.) set their own. Generic crate's build.rs
/// (152.1.B.4) reads the same env var so kernel + lwIP + per-board
/// glue all see consistent flags.
///
/// `-ffunction-sections` / `-fdata-sections` / `-O2` / `warnings off`
/// stay built-in defaults — every FreeRTOS+lwIP consumer wants them.
fn configure_arm_cm3(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    let cflags = env::var("FREERTOS_CFLAGS").unwrap_or_else(|_| {
        // Backward-compat default for MPS2-AN385 consumers that
        // don't yet set the env var. Future PR removes the
        // fallback after every example bumps its `.cargo/config.toml`.
        "-mcpu=cortex-m3 -mthumb".to_string()
    });
    for flag in cflags.split_whitespace() {
        build.flag(flag);
    }
}

fn add_freertos_includes(
    build: &mut cc::Build,
    freertos_dir: &Path,
    port_dir: &Path,
    config_dir: &Path,
) {
    build
        .include(config_dir)
        .include(freertos_dir.join("include"))
        .include(port_dir);
}

fn add_lwip_includes(build: &mut cc::Build, lwip_dir: &Path) {
    build
        .include(lwip_dir.join("src/include"))
        .include(lwip_dir.join("contrib/ports/freertos/include"));
}

fn gcc_print_file(name: &str) -> String {
    let out = std::process::Command::new("arm-none-eabi-gcc")
        .args(["-mcpu=cortex-m3", "-mthumb", &format!("--print-file-name={name}")])
        .output()
        .expect("arm-none-eabi-gcc not found");
    let path = String::from_utf8(out.stdout).unwrap();
    let path = path.trim().to_string();
    // If GCC can't resolve the file it echoes the bare name back
    assert!(
        Path::new(&path).is_absolute(),
        "arm-none-eabi-gcc could not locate {name}"
    );
    path
}

