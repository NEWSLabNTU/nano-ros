//! Build script for nros-board-mps2-an385-freertos
//!
//! phase-337 W5 — the overlay's build script is now per-board WIRING only:
//! linker scripts into `OUT_DIR`, the LAN9118 netif driver, the board's own C
//! translation unit, the `NROS_APP_CONFIG` symbol, and libc/libgcc discovery.
//! Everything generic (cflag resolution, FreeRTOS/lwIP include dirs, the
//! `NROS_APP_CONFIG` emitter) comes from
//! `nros_board_common::freertos_build`; the FreeRTOS kernel, lwIP,
//! `nros-platform-freertos` and the generic C glue are compiled by
//! `nros-board-freertos/build.rs` and propagate transitively.
//!
//! Environment: see `nros-board-freertos/build.rs` for `FREERTOS_DIR` /
//! `FREERTOS_PORT` / `LWIP_DIR` / `FREERTOS_CONFIG_DIR` / `FREERTOS_CFLAGS`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use nros_board_common::{
    BaseConfig, FreertosScheduling,
    freertos_build::{
        add_freertos_includes, add_lwip_includes, app_stack_bytes_from_build_env, configure_cflags,
        emit_app_config_tu,
    },
};

fn main() {
    // issue 0288 — skip the ARM cross-compile when host tooling builds this
    // crate (the source-metadata probe). Without it the host `cc` is handed
    // `-mthumb -mcpu=cortex-m3` and dies before rustc runs.
    if nros_board_common::host_probe::skip_cross_build(
        "nros-board-mps2-an385-freertos",
        &["thumb", "arm"],
    ) {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");
    // phase-337 W5.a/W5.e — the shared config headers + section layout live in
    // the family crate; the board keeps only the numbers.
    let shared_config_dir = manifest_dir
        .parent()
        .expect("workspace layout")
        .join("nros-board-freertos/config");

    // --- Linker scripts ---
    // The board script `INCLUDE`s the shared one, and `INCLUDE` resolves
    // against the linker's search path — so BOTH land in OUT_DIR, which the
    // `rustc-link-search` below puts on that path. The binary's
    // `.cargo/config.toml` names `-Tmps2_an385.ld` in rustflags.
    for (src, name) in [
        (config_dir.join("mps2_an385.ld"), "mps2_an385.ld"),
        (
            shared_config_dir.join("nros-freertos-cortex-m.ld"),
            "nros-freertos-cortex-m.ld",
        ),
    ] {
        fs::copy(&src, out_dir.join(name))
            .unwrap_or_else(|e| panic!("copying {} into OUT_DIR: {e}", src.display()));
        println!("cargo:rerun-if-changed={}", src.display());
    }
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
    configure_cflags(&mut lan9118);
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
        configure_cflags(&mut tband);
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
    configure_cflags(&mut glue);
    add_freertos_includes(&mut glue, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut glue, &lwip_dir);
    glue.include(lan9118_dir.join("include"));
    // Phase 212.M-F.10.3 — the emitted TU `#include`s <nros/app_config.h>
    // (the canonical-path wrapper from M-F.10.1 `c8aafd6ff`).
    // Issue 0365 — nros-c moved to `packages/api/nros-c` in phase-321 W2.e; this
    // join was left at the old `core/nros-c`, so the TU could not find the header.
    // Assert existence so a future move fails loud here, not deep in `cc`.
    let nros_c_include = manifest_dir
        .parent() // packages/boards/
        .and_then(|p| p.parent()) // packages/
        .expect("workspace layout")
        .join("api/nros-c/include");
    assert!(
        nros_c_include.join("nros/app_config.h").exists(),
        "nros-c header not at {} — did nros-c move again? (issue 0365)",
        nros_c_include.display()
    );
    glue.include(nros_c_include);
    if nros_trace {
        let tband_dir = nros_build_paths::tband_dir();
        let trace_config_dir = manifest_dir.join("trace");
        glue.include(tband_dir.join("inc"));
        glue.include(&trace_config_dir);
        glue.define("NROS_TRACE", "1");
    }

    // Phase 152.1.B.4 — overlay glue carries only board-specific C:
    // MPS2-AN385 vector table + Reset_Handler + the LAN9118 netif
    // registration + trace_dump (always compiled; stubs when NROS_TRACE off).
    // Generic FreeRTOS / lwIP / nros-platform-freertos pieces moved to
    // `nros-board-freertos/build.rs`.
    glue.file(manifest_dir.join("c/board_mps2.c"));
    glue.file(manifest_dir.join("trace/trace_dump.c"));

    // phase-337 W5.d — the `NROS_APP_CONFIG` symbol, emitted from the board's
    // `BaseConfig` + `FreertosScheduling` rather than the 57-line hand-written
    // C-string mirror this replaced (which had drifted 128 KiB on the app
    // stack). The C/C++ application entry reads it for network bring-up and
    // task sizing; on the pure-Rust path `Config` carries the same values.
    let sched = FreertosScheduling {
        app_stack_bytes: app_stack_bytes_from_build_env(),
        ..FreertosScheduling::default()
    };
    glue.file(emit_app_config_tu(&out_dir, &BaseConfig::default(), &sched));

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
    println!("cargo:rerun-if-changed=config/arch/cc.h");
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

fn gcc_print_file(name: &str) -> String {
    let out = std::process::Command::new("arm-none-eabi-gcc")
        .args([
            "-mcpu=cortex-m3",
            "-mthumb",
            &format!("--print-file-name={name}"),
        ])
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
