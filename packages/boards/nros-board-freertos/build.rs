//! Phase 152.1.B.4 — generic FreeRTOS + lwIP + nros-platform-freertos
//! build pipeline carved out of `nros-board-mps2-an385-freertos/build.rs`.
//!
//! Compiles four static archives that any FreeRTOS + lwIP overlay
//! links transitively:
//!
//! | Archive                  | Contents                                          |
//! |--------------------------|---------------------------------------------------|
//! | `libfreertos.a`          | kernel core + port + heap_4                       |
//! | `liblwip.a`              | core + IPv4 + API + netif + ethernet + sys_arch   |
//! | `libnros_platform_freertos.a` | C port providing the `nros_platform_*` ABI   |
//! | `libfreertos_glue.a`     | `c/freertos_hooks.c` + `c/network_glue.c`         |
//!
//! Required env vars (read from the user's environment; the
//! per-board overlay's `.cargo/config.toml [env]` block typically
//! sets them):
//!
//! | Var | Purpose | Required |
//! |---|---|---|
//! | `FREERTOS_DIR`        | FreeRTOS kernel source root | yes |
//! | `FREERTOS_PORT`       | Portable layer (e.g. `GCC/ARM_CM3`) | defaults to `GCC/ARM_CM3` |
//! | `LWIP_DIR`            | lwIP source root | yes |
//! | `FREERTOS_CONFIG_DIR` | Directory with `FreeRTOSConfig.h` + `lwipopts.h` | yes |
//! | `FREERTOS_CFLAGS`     | Space-separated cflags (`-mcpu=cortex-m3 -mthumb` etc.) | defaults to Cortex-M3 |
//!
//! The overlay's `build.rs` shrinks to: linker script write +
//! board driver build (LAN9118 / STM ETH / NXP ENET / …) +
//! `c/board_<name>.c` (vector table + Reset + diag) + libc/libgcc
//! discovery + `cargo:rustc-link-search` for any per-board search
//! path the linker needs.

use std::{
    env,
    path::{Path, PathBuf},
};

use nros_board_common::arch_flags;

/// The platform whose `[arch.*]` profiles supply this board family's cflags —
/// `config/freertos-lwip/nros-platform.toml`.
const PLATFORM: &str = "freertos-lwip";

fn main() {
    // issue 0288 — the FREERTOS_DIR guard below does NOT cover host tooling:
    // an example's `.cargo/config.toml [env]` sets that var for a host probe
    // just as for a firmware build, so the guard passes and the ARM flags reach
    // the host compiler (`cc: error: unrecognized command-line option
    // '-mthumb'`). Only the target says what we are building FOR.
    if nros_board_common::host_probe::skip_cross_build("nros-board-freertos", &["thumb", "arm"]) {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Skip the heavy lift unless the build is actually targeting a
    // FreeRTOS overlay. A `cargo check` of just this crate (no
    // downstream consumer + no env vars) panics today — gate the
    // env-var reads on the presence of `FREERTOS_DIR` so the bare
    // `cargo check -p nros-board-freertos` keeps working.
    if env::var("FREERTOS_DIR").is_err() {
        // Document via a build warning so an overlay author that
        // forgot to set the env var sees a clear hint instead of a
        // confusing missing-symbol error at link time.
        println!(
            "cargo:warning=nros-board-freertos: FREERTOS_DIR not set; \
             skipping kernel / lwIP / glue compile. Set it in your \
             overlay's `.cargo/config.toml [env]` block."
        );
        return;
    }

    let freertos_dir = env_path("FREERTOS_DIR");
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let lwip_dir = env_path("LWIP_DIR");
    let freertos_config_dir = env_path("FREERTOS_CONFIG_DIR");
    let port_dir = freertos_dir.join("portable").join(&freertos_port);

    // --- Build FreeRTOS kernel ---
    let mut freertos = cc::Build::new();
    configure_cflags(&mut freertos);
    add_freertos_includes(
        &mut freertos,
        &freertos_dir,
        &port_dir,
        &freertos_config_dir,
    );
    // Phase 204.6 — right-size the FreeRTOS heap (heap_4 `ucHeap`, the dominant
    // bss; this is the only TU that sizes it). FreeRTOSConfig.h defaults to a
    // cyclone-safe 3 MiB. Two overrides, env wins:
    //   1. explicit `NROS_FREERTOS_HEAP_KB` build env (any value), else
    //   2. the `rmw-zenoh` feature (forwarded from the board) → 2 MiB. The
    //      FreeRTOS task stacks are allocated *from* this heap (heap_4), so it
    //      must hold the `nros_app` task stack (now 384 KiB — the Phase 212
    //      Entry / run-plan Executor open exceeds the old 256 KiB) PLUS lwIP
    //      (netconns/pbufs/socket semaphores) PLUS zenoh-pico's working set.
    //      512 KiB sufficed for the old *direct* talker but the Entry path
    //      MALLOC-FAILs at it (issue #46); 2 MiB boots cleanly through Executor
    //      + network on the qemu MPS2-AN385 (4 MiB SRAM, ample headroom). Still
    //      below the cyclone DDS-discovery default; cyclone/xrce don't enable
    //      this feature on the base crate, so they keep the 3 MiB default; tune
    //      via the env (`xPortGetMinimumEverFreeHeapSize()` high-water).
    let heap_kb = env::var("NROS_FREERTOS_HEAP_KB")
        .ok()
        .or_else(|| (env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()).then(|| "2048".to_string()));
    if let Some(kb) = heap_kb {
        freertos.define("NROS_FREERTOS_HEAP_KB", kb.as_str());
    }
    println!("cargo:rerun-if-env-changed=NROS_FREERTOS_HEAP_KB");
    for src in &[
        "tasks.c",
        "queue.c",
        "list.c",
        "timers.c",
        "event_groups.c",
        "stream_buffer.c",
    ] {
        freertos.file(freertos_dir.join(src));
    }
    freertos.file(port_dir.join("port.c"));
    freertos.file(freertos_dir.join("portable/MemMang/heap_4.c"));
    freertos.compile("freertos");

    // --- Build lwIP ---
    let mut lwip = cc::Build::new();
    configure_cflags(&mut lwip);
    add_freertos_includes(&mut lwip, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut lwip, &lwip_dir);
    for src in &[
        // Core
        "src/core/init.c",
        "src/core/def.c",
        "src/core/dns.c",
        "src/core/inet_chksum.c",
        "src/core/ip.c",
        "src/core/mem.c",
        "src/core/memp.c",
        "src/core/netif.c",
        "src/core/pbuf.c",
        "src/core/raw.c",
        "src/core/stats.c",
        "src/core/sys.c",
        "src/core/tcp.c",
        "src/core/tcp_in.c",
        "src/core/tcp_out.c",
        "src/core/timeouts.c",
        "src/core/udp.c",
        // IPv4 + IGMP for RTPS SPDP multicast
        "src/core/ipv4/etharp.c",
        "src/core/ipv4/icmp.c",
        "src/core/ipv4/ip4.c",
        "src/core/ipv4/ip4_addr.c",
        "src/core/ipv4/ip4_frag.c",
        "src/core/ipv4/igmp.c",
        // API (sockets)
        "src/api/api_lib.c",
        "src/api/api_msg.c",
        "src/api/err.c",
        "src/api/if_api.c",
        "src/api/netbuf.c",
        "src/api/netdb.c",
        "src/api/netifapi.c",
        "src/api/sockets.c",
        "src/api/tcpip.c",
        // Netif + FreeRTOS sys_arch
        "src/netif/ethernet.c",
        "contrib/ports/freertos/sys_arch.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    lwip.compile("lwip");

    // --- Build nros-platform-freertos C port ---
    // First-party sibling C source/headers, located via env vars (defaults in
    // just/sdk-env.just / .envrc) — not a build.rs repo-layout walk-up (192.3).
    let nros_platform_freertos_dir = env_path("NROS_PLATFORM_FREERTOS_SRC");
    let nros_platform_cffi_include = env_path("NROS_PLATFORM_CFFI_INCLUDE");
    let mut platform = cc::Build::new();
    configure_cflags(&mut platform);
    add_freertos_includes(
        &mut platform,
        &freertos_dir,
        &port_dir,
        &freertos_config_dir,
    );
    add_lwip_includes(&mut platform, &lwip_dir);
    platform.include(&nros_platform_cffi_include);
    platform.file(nros_platform_freertos_dir.join("platform.c"));
    platform.file(nros_platform_freertos_dir.join("net.c"));
    platform.file(nros_platform_freertos_dir.join("timer.c"));
    platform.compile("nros_platform_freertos");
    println!(
        "cargo:rerun-if-changed={}",
        nros_platform_freertos_dir.display()
    );

    // --- Generic glue (freertos_hooks + network_glue + freertos_run_tiers) ---
    // `c/freertos_hooks.c` provides the FreeRTOS task hooks +
    // semihosting helpers. `c/network_glue.c` provides the lwIP
    // init + FFI surface Rust calls; both invoke
    // `nros_board_*` weak hooks the overlay implements (152.1.B.2).
    // `c/freertos_run_tiers.c` (Phase 274.W3) implements
    // `nros_board_freertos_run_tiers` for embedded C/C++ multi-tier entries
    // (RFC-0015 Model 1): one FreeRTOS task per tier over one shared session.
    // nros-cpp C FFI symbols are forward-declared and resolved at link time.
    let mut glue = cc::Build::new();
    configure_cflags(&mut glue);
    add_freertos_includes(&mut glue, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut glue, &lwip_dir);
    glue.file(manifest_dir.join("c/freertos_hooks.c"));
    glue.file(manifest_dir.join("c/network_glue.c"));
    glue.file(manifest_dir.join("c/freertos_run_tiers.c"));
    glue.compile("freertos_glue");

    // --- Link order (link-lib propagates transitively to overlays + final binary) ---
    println!("cargo:rustc-link-lib=static=nros_platform_freertos");
    println!("cargo:rustc-link-lib=static=freertos_glue");
    println!("cargo:rustc-link-lib=static=lwip");
    println!("cargo:rustc-link-lib=static=freertos");

    // --- Rerun triggers ---
    println!("cargo:rerun-if-changed=c/freertos_hooks.c");
    println!("cargo:rerun-if-changed=c/network_glue.c");
    println!("cargo:rerun-if-changed=c/freertos_run_tiers.c");
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        freertos_config_dir.join("FreeRTOSConfig.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        freertos_config_dir.join("lwipopts.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        freertos_config_dir.join("arch/cc.h").display()
    );
    println!("cargo:rerun-if-changed={}", freertos_dir.display());
    println!("cargo:rerun-if-changed={}", lwip_dir.display());
    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CFLAGS");
    println!("cargo:rerun-if-env-changed=NROS_PLATFORM_FREERTOS_SRC");
    println!("cargo:rerun-if-env-changed=NROS_PLATFORM_CFFI_INCLUDE");
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env::var(name).unwrap_or_else(|_| {
        panic!(
            "{name} not set — overlays should set it via \
             `.cargo/config.toml [env]` or the user must export it"
        )
    }))
}

/// Shared cflag setup. Reads `FREERTOS_CFLAGS` env var
/// (space-separated). Default cortex-m3 fallback matches the
/// pre-152.1.B.3 behaviour for existing examples that haven't
/// bumped their `.cargo/config.toml` yet.
fn configure_cflags(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);
    // issue 0383 — implicit-function-declaration / int-conversion as ERRORS.
    // Safe next to `warnings(false)`: that only makes cc-rs OMIT `-Wall`/
    // `-Wextra`, it passes no `-w`, and gcc enables both diagnostics by
    // default — so the gate is live on the pinned arm-none-eabi-gcc 13.2.
    nros_cc_flags::strict_decls(build);
    // Phase 195 audit (b) — applying cortex-m3 flags to any other ARM-M target
    // (thumbv7em = M4/M7, thumbv6m = M0, …) yields a wrong-CPU / wrong-FPU-ABI
    // binary, so a mismatch must never be silent.
    //
    // phase-338 W4 — but it must not be a PANIC either. `config/freertos-lwip/
    // nros-platform.toml` already declares the arch profiles with their cflags
    // (`arch = ["cortex-m3", "cortex-m7"]`), and this function used to ignore
    // them and hard-fail for anything that was not thumbv7m — so FreeRTOS+lwIP
    // stayed unbuildable on Cortex-M4F/M7 *after* the M7 profile had been
    // added. Every industrial FreeRTOS board this project targets is M4F/M7
    // (the NXP S32K344 is an M7), so the panic was the whole blocker.
    //
    // Resolution order, so the ladder still runs board-over-platform:
    //   1. `FREERTOS_CFLAGS` — the board's explicit override (RFC-0049 rung 1).
    //   2. the matching `[arch.*]` profile for this TARGET.
    //   3. loud failure naming the profiles that exist and what they admit.
    // A non-`thumb*` target (host `cargo check`, the source-metadata probe)
    // skips all of it — there is no embedded compile of substance there.
    let cflags = match env::var("FREERTOS_CFLAGS") {
        Ok(v) => v,
        Err(_) => {
            let target = env::var("TARGET").unwrap_or_default();
            if !target.starts_with("thumb") {
                "-mcpu=cortex-m3 -mthumb".to_string()
            } else {
                let config_root = arch_flags::config_root().unwrap_or_else(|| {
                    panic!(
                        "nros-board-freertos: TARGET=`{target}` needs arch cflags but the \
                         nano-ros config/ tree was not found walking up from CARGO_MANIFEST_DIR. \
                         Out-of-tree consumer? Set FREERTOS_CFLAGS explicitly."
                    )
                });
                match arch_flags::cflags_for_target(&config_root, PLATFORM, &target) {
                    Ok(Some(flags)) => flags.join(" "),
                    Ok(None) => panic!(
                        "nros-board-freertos: no [arch.*] profile of platform `{PLATFORM}` \
                         admits TARGET=`{target}`.\n  declared: {}\n  Either add an [arch.*] \
                         block to config/{PLATFORM}/nros-platform.toml, or set FREERTOS_CFLAGS \
                         in the board's .cargo/config.toml [env] — e.g. `-mcpu=cortex-m4 \
                         -mthumb -mfpu=fpv4-sp-d16 -mfloat-abi=hard` for a Cortex-M4F.",
                        arch_flags::describe_profiles(&config_root, PLATFORM)
                    ),
                    Err(e) => panic!("nros-board-freertos: reading arch profiles: {e}"),
                }
            }
        }
    };
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
