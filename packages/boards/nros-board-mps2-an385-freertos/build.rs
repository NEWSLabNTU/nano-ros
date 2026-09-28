//! Build script for nros-board-mps2-an385-freertos
//!
//! Per-board wiring only. phase-471 W2 — the recipe itself is
//! `nros_board_common::freertos_build::run_overlay`, the runner the FreeRTOS
//! family gained to match the one NuttX and ThreadX-RISCV have had since
//! phase-337; this file states only what is true of THIS board. Everything
//! generic (cflag resolution, FreeRTOS/lwIP include dirs, the
//! `NROS_APP_CONFIG` emitter, newlib discovery for the right multilib) lives
//! there; the FreeRTOS kernel, lwIP, `nros-platform-freertos` and the generic
//! C glue are compiled by `nros-board-freertos/build.rs` and propagate
//! transitively.
//!
//! What is genuinely this board's, and is therefore still here: the LAN9118
//! lwIP netif driver, and the opt-in Tonbandgeraet trace library. phase-471 W0
//! ruled the `cargo:rustc-cfg=nros_trace` this board emits LEGITIMATELY
//! DIFFERENT rather than duplication — it is the only board with tband wiring,
//! so it is the only board that can emit that cfg, and absorbing it into the
//! runner would put one board's optional feature in every board's recipe. It
//! stays in the leaf, beside the compile that earns it.
//!
//! Environment: see `nros-board-freertos/build.rs` for `FREERTOS_DIR` /
//! `FREERTOS_PORT` / `LWIP_DIR` / `FREERTOS_CONFIG_DIR` / `FREERTOS_CFLAGS`.

use std::env;

use nros_board_common::freertos_build::{
    Overlay, OverlayEnv, add_freertos_includes, add_lwip_includes, configure_cflags, run_overlay,
};

/// The Tonbandgeraet opt-in. One spelling, read by both hooks below — the
/// value decides an archive in one and include dirs + a define in the other,
/// and two reads of one variable is how those two halves drift apart.
fn trace_enabled() -> bool {
    env::var("NROS_TRACE").unwrap_or_default() == "1"
}

fn main() {
    run_overlay(&Overlay {
        // Compiled before the board glue, against the runner's resolved paths.
        extra_archives: Some(&|env: &OverlayEnv| {
            println!("cargo:rerun-if-env-changed=NROS_TRACE");

            // --- LAN9118 lwIP netif driver ---
            let lan9118_dir = nros_build_paths::nros_lan9118_lwip_dir();
            let mut lan9118 = cc::Build::new();
            configure_cflags(&mut lan9118);
            add_freertos_includes(
                &mut lan9118,
                &env.freertos_dir,
                &env.port_dir,
                &env.freertos_config_dir,
            );
            add_lwip_includes(&mut lan9118, &env.lwip_dir);
            lan9118.include(lan9118_dir.join("include"));
            lan9118.file(lan9118_dir.join("src/lan9118_lwip.c"));
            // issue 0478 — cc-rs would hand arm-none-eabi-gcc the clang-only
            // `-mno-omit-leaf-frame-pointer`, which gcc REJECTS.
            nros_cc_flags::gcc_safe_frame_pointer(&mut lan9118);
            lan9118.compile("lan9118_lwip");
            // issue 0491 — the driver tree is first-party, so it is watched by
            // CONTENT rather than fingerprinted as an env string.
            nros_build_paths::watch_path(&lan9118_dir);

            // --- Tonbandgeraet trace library (opt-in via NROS_TRACE=1) ---
            if trace_enabled() {
                let tband_dir = nros_build_paths::tband_dir();
                let mut tband = cc::Build::new();
                configure_cflags(&mut tband);
                add_freertos_includes(
                    &mut tband,
                    &env.freertos_dir,
                    &env.port_dir,
                    &env.freertos_config_dir,
                );
                tband.include(tband_dir.join("inc"));
                tband.include(env.manifest_dir.join("trace"));
                tband.define("NROS_TRACE", "1");
                tband.file(tband_dir.join("src/tband.c"));
                tband.file(tband_dir.join("src/tband_freertos.c"));
                tband.file(tband_dir.join("src/tband_backend.c"));
                nros_cc_flags::gcc_safe_frame_pointer(&mut tband);
                tband.compile("tband");
                println!("cargo:rustc-link-lib=static=tband");
                println!("cargo:rustc-cfg=nros_trace");
            }
        }),
        // The board glue reaches the netif header for its strong overrides, and
        // `trace/trace_dump.c` compiles either way — stubs when tband is off.
        configure_glue: Some(&|env: &OverlayEnv, build: &mut cc::Build| {
            build.include(nros_build_paths::nros_lan9118_lwip_dir().join("include"));
            if trace_enabled() {
                build.include(nros_build_paths::tband_dir().join("inc"));
                build.include(env.manifest_dir.join("trace"));
                build.define("NROS_TRACE", "1");
            }
        }),
        // Phase 166.A — only the archives compiled in THIS build script get an
        // explicit link-lib line. The four from `nros-board-freertos` propagate
        // via cargo's dep chain; re-emitting them bundles the same `.a` into
        // both rlibs.
        extra_link_libs: &["lan9118_lwip"],
        ..Overlay::new(
            "nros-board-mps2-an385-freertos",
            "mps2_an385.ld",
            "GCC/ARM_CM3",
            // MPS2-AN385 vector table + Reset_Handler + the LAN9118 netif
            // registration, then `trace_dump` (always compiled).
            &["c/board_mps2.c", "trace/trace_dump.c"],
        )
    });
}
