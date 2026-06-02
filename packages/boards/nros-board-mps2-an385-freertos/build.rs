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
    // Phase 208.B Track A — paths come from `nros-build-paths`
    // (walks up to `nros-sdk-index.toml`); env vars stay as overrides.
    let freertos_dir = nros_build_paths::freertos_dir();
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let lwip_dir = nros_build_paths::lwip_dir();
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir.clone());

    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    let lan9118_dir = nros_build_paths::nros_lan9118_lwip_dir();

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
        let tband_dir = nros_build_paths::tband_dir();
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
    // Phase 212.M-F.10.3 — emit_nros_app_config's TU `#include <nros/app_config.h>`
    // (the canonical-path wrapper from M-F.10.1 `c8aafd6ff`).
    glue.include(
        manifest_dir
            .parent() // packages/boards/
            .and_then(|p| p.parent()) // packages/
            .expect("workspace layout")
            .join("core/nros-c/include"),
    );
    if nros_trace {
        let tband_dir = nros_build_paths::tband_dir();
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

    // Phase 212.M-F.10.3 — emit the universal `NROS_APP_CONFIG`
    // symbol into the board staticlib. Values mirror
    // `nros_board_freertos::Config::default()` so the
    // user-facing read API (`NROS_APP_CONFIG.zenoh.locator` /
    // `.network.ip` / `.scheduling.poll_interval_ms`) is identical
    // regardless of whether the TU resolves the symbol via the
    // cmake-codegen `static const` (transition path, M-F.10.5
    // retires it) or via this `extern` definition (post-M-F.10.5,
    // and any pure-cargo path that never invokes cmake codegen).
    let app_config_path = emit_nros_app_config(&out_dir);
    glue.file(&app_config_path);

    glue.compile("startup");

    // --- Link order ---
    // Only the per-board archives compiled in THIS build script
    // get explicit link-lib lines. The four archives produced by
    // `nros-board-freertos` (nros_platform_freertos, freertos_glue,
    // lwip, freertos) propagate via cargo's normal dep chain — its
    // `cc::Build::compile()` already emitted matching link-lib
    // directives. Re-emitting them here causes cargo to bundle the
    // same `.a` into BOTH rlibs (Phase 166.A duplicate-symbol root
    // cause).
    println!("cargo:rustc-link-lib=static=startup");
    println!("cargo:rustc-link-lib=static=lan9118_lwip");

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
    println!("cargo:rerun-if-env-changed=NROS_LAN9118_LWIP_DIR");
    println!("cargo:rerun-if-env-changed=TBAND_DIR");
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

/// Phase 212.M-F.10.3 — emit the per-board `NROS_APP_CONFIG`
/// definition into a generated TU, baked into the board staticlib.
///
/// Values mirror `nros_board_freertos::Config::default()` for this
/// board (MPS2-AN385 + FreeRTOS + lwIP). The struct layout matches
/// the `nros_app_config_t` shipped under
/// `packages/core/nros-c/include/nros/zephyr/app_config.h`; we inline
/// the typedef in the emitted TU so the file is self-contained (no
/// dependency on a non-Zephyr include-path remap of that header).
///
/// During the M-F.10.3 → M-F.10.5 transition the cmake codegen path
/// may still emit a per-binary `<nros/app_config.h>` with its own
/// `static const` initialiser earlier on the include path; that TU-
/// local symbol shadows this `extern` definition for any consumer
/// TU compiled with the codegen header reachable. The two paths
/// coexist until M-F.10.5 retires the codegen.
///
/// Returns the path to the generated `.c` file so the caller can
/// pass it to `cc::Build::file`.
fn emit_nros_app_config(out_dir: &Path) -> PathBuf {
    let out_path = out_dir.join("nros_app_config_def.c");
    // Mirrors `nros_board_freertos::Config::default()`
    // (`packages/boards/nros-board-freertos/src/config.rs`). Keep these
    // values in sync when the board's defaults change.
    let body = r#"/* Auto-generated by nros-board-mps2-an385-freertos/build.rs.
 *
 * Phase 212.M-F.10.3 (Path C) — board-emitted NROS_APP_CONFIG.
 *
 * Mirrors `nros_board_freertos::Config::default()`:
 *   - MAC:     02:00:00:00:00:00
 *   - IP:      192.0.3.10 / 24
 *   - Gateway: 192.0.3.1
 *   - Locator: tcp/192.0.3.1:7447
 *   - Domain:  0
 *   - Scheduling defaults: see board-freertos config.rs
 *
 * Includes the canonical-path wrapper at
 * `nros-c/include/nros/app_config.h` (introduced by Phase
 * 212.M-F.10.1 in `c8aafd6ff`) which re-includes the shipped
 * `nros/zephyr/app_config.h` for the struct type. Keeps a single
 * source-of-truth definition; no inlined-typedef sync obligation.
 */

#include <stdint.h>
#include <nros/app_config.h>

const nros_app_config_t NROS_APP_CONFIG = {
    .zenoh = {
        .locator   = "tcp/192.0.3.1:7447",
        .domain_id = 0,
    },
    .network = {
        .ip      = { 192, 0, 3, 10 },
        .mac     = { 0x02, 0x00, 0x00, 0x00, 0x00, 0x00 },
        .gateway = { 192, 0, 3, 1 },
        .netmask = { 255, 255, 255, 0 },
        .prefix  = 24,
    },
    .scheduling = {
        .app_priority            = 12,
        .zenoh_read_priority     = 16,
        .zenoh_lease_priority    = 16,
        .poll_priority           = 16,
        .app_stack_bytes         = 262144u,
        .zenoh_read_stack_bytes  = 5120u,
        .zenoh_lease_stack_bytes = 5120u,
        .poll_interval_ms        = 5u,
    },
};
"#;
    File::create(&out_path)
        .expect("failed to create nros_app_config_def.c")
        .write_all(body.as_bytes())
        .expect("failed to write nros_app_config_def.c");
    out_path
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

