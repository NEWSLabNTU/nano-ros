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

    // --- Build FreeRTOS kernel ---
    let mut freertos = cc::Build::new();
    configure_arm_cm3(&mut freertos);
    add_freertos_includes(&mut freertos, &freertos_dir, &port_dir, &freertos_config_dir);
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");
        freertos.include(tband_dir.join("inc"));
        freertos.include(&trace_config_dir);
        freertos.define("NROS_TRACE", "1");
    }

    // Kernel core
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
    // Portable layer
    freertos.file(port_dir.join("port.c"));
    // Memory manager
    freertos.file(freertos_dir.join("portable/MemMang/heap_4.c"));

    freertos.compile("freertos");

    // --- Build lwIP ---
    let mut lwip = cc::Build::new();
    configure_arm_cm3(&mut lwip);
    add_freertos_includes(&mut lwip, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut lwip, &lwip_dir);

    // Core
    for src in &[
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
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // IPv4
    for src in &[
        "src/core/ipv4/etharp.c",
        "src/core/ipv4/icmp.c",
        "src/core/ipv4/ip4.c",
        "src/core/ipv4/ip4_addr.c",
        "src/core/ipv4/ip4_frag.c",
        // Phase 97.1.kconfig.freertos — IGMP for RTPS SPDP multicast.
        "src/core/ipv4/igmp.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // API (required for sockets)
    for src in &[
        "src/api/api_lib.c",
        "src/api/api_msg.c",
        "src/api/err.c",
        "src/api/if_api.c",
        "src/api/netbuf.c",
        "src/api/netdb.c",
        "src/api/netifapi.c",
        "src/api/sockets.c",
        "src/api/tcpip.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // Netif
    lwip.file(lwip_dir.join("src/netif/ethernet.c"));
    // FreeRTOS sys_arch
    lwip.file(lwip_dir.join("contrib/ports/freertos/sys_arch.c"));

    lwip.compile("lwip");

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

    // Phase 149.1.B.1 — startup C split into three checked-in
    // files under `c/`. Mechanical split; behaviour unchanged.
    // - freertos_hooks.c — generic FreeRTOS hooks + semihosting
    //   (candidate for promotion into nros-board-freertos at 149.1.B.4)
    // - network_glue.c   — lwIP init + FFI surface Rust calls
    //   (candidate for promotion once 149.1.B.2 lifts the
    //   nros_board_init_eth weak-hook contract)
    // - board_mps2.c     — MPS2-AN385 vector table + Reset +
    //   LAN9118 register-level diagnostic (stays per-board)
    glue.file(manifest_dir.join("c/freertos_hooks.c"));
    glue.file(manifest_dir.join("c/network_glue.c"));
    glue.file(manifest_dir.join("c/board_mps2.c"));

    // Trace dump (always compiled — stubs when NROS_TRACE not defined)
    glue.file(manifest_dir.join("trace/trace_dump.c"));

    glue.compile("startup");

    // --- Phase 121.3 — nros-platform-freertos C port ---
    // The native C port (`packages/core/nros-platform-freertos/src/`)
    // provides the canonical `nros_platform_*` symbols against the
    // FreeRTOS kernel + lwIP. Built in-tree by the board because the
    // C port headers (`<FreeRTOS.h>`, `<lwip/sockets.h>`) come from
    // this build's already-configured includes.
    let nros_platform_freertos_dir =
        manifest_dir.join("../../../packages/core/nros-platform-freertos/src");
    let nros_platform_cffi_include =
        manifest_dir.join("../../../packages/core/nros-platform-cffi/include");
    let mut platform = cc::Build::new();
    configure_arm_cm3(&mut platform);
    add_freertos_includes(&mut platform, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut platform, &lwip_dir);
    platform.include(&nros_platform_cffi_include);
    platform.file(nros_platform_freertos_dir.join("platform.c"));
    platform.file(nros_platform_freertos_dir.join("net.c"));
    platform.file(nros_platform_freertos_dir.join("timer.c"));
    platform.compile("nros_platform_freertos");
    println!("cargo:rerun-if-changed={}", nros_platform_freertos_dir.display());

    // --- Link order ---
    println!("cargo:rustc-link-lib=static=nros_platform_freertos");
    println!("cargo:rustc-link-lib=static=startup");
    println!("cargo:rustc-link-lib=static=lan9118_lwip");
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
    println!("cargo:rerun-if-changed=c/freertos_hooks.c");
    println!("cargo:rerun-if-changed=c/network_glue.c");
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

/// Phase 149.1.B.3 — generic FreeRTOS+lwIP compiler-flag setup.
///
/// Reads `FREERTOS_CFLAGS` env var (space-separated flag list) +
/// applies each via `cc::Build::flag`. The MPS2-AN385 board's
/// reference `.cargo/config.toml` sets
/// `FREERTOS_CFLAGS = "-mcpu=cortex-m3 -mthumb"`; future overlays
/// (Cortex-M4F, etc.) set their own. Generic crate's build.rs
/// (149.1.B.4) reads the same env var so kernel + lwIP + per-board
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

