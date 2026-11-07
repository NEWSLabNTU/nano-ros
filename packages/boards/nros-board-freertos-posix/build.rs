//! phase-548 W1 — the C half of a Rust image on the FreeRTOS POSIX board.
//!
//! Mirrors `cmake/board/nano-ros-board-freertos-posix.cmake` (the C/C++ road)
//! file for file, so both languages' images run the same kernel and port:
//!
//! - the kernel with upstream's `GCC_POSIX` port (`portable/ThirdParty/GCC/
//!   Posix`) plus its `utils/wait_for_event.c`, and **heap_3** (host `malloc`):
//!   heap_4 would take `vTaskSuspendAll` from Cyclone's host threads
//!   (issue 1769);
//! - the FreeRTOS platform C port's `platform.c` + `timer.c` with
//!   `NROS_FREERTOS_HEAP_3=1`, and WITHOUT `net.c` (lwIP) or
//!   `cyclonedds_compat.c` (bare-metal fill-ins glibc already provides);
//! - the family task glue, the board's kernel hooks, and the Rust-only glue.
//!
//! The family crate `nros-board-freertos` compiles none of this on a host
//! target (`skip_cross_build`), which is why the board does.

use std::{env, path::PathBuf};

use nros_board_common::nros_build_paths;

const CRATE: &str = "nros-board-freertos-posix";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");

    // The POSIX port is a Linux-host port. Any other target (a cross check, a
    // host-tooling probe on another OS) gets the Rust shell only.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        println!(
            "cargo:warning={CRATE}: target OS is not linux; skipping the FreeRTOS POSIX-port \
             compile (the crate still builds as a Rust shell)"
        );
        return;
    }
    // Issue 1779 — one `nros_platform_*` provider per linked graph.
    if nros_board_common::platform_port::defer_to_graph_provider(CRATE, "FreeRTOS") {
        return;
    }

    let freertos_dir = match nros_build_paths::locate::try_source("freertos-kernel") {
        Ok(d) => d,
        Err(why) => {
            println!(
                "cargo:warning={CRATE}: skipping the kernel compile — {}",
                why.replace('\n', "; ")
            );
            return;
        }
    };
    if !freertos_dir.join("tasks.c").exists() {
        println!(
            "cargo:warning={CRATE}: the FreeRTOS kernel at {} has no tasks.c; skipping the \
             kernel compile. A link of this board will fail until it is provisioned.",
            freertos_dir.display()
        );
        nros_build_paths::watch_skip_cause(&freertos_dir);
        return;
    }

    // The board descriptor's `cargo_config` states both (RFC-0103 D1). A build
    // without them is not a build FOR this board -- a `cargo build --workspace`,
    // say -- and gets the Rust shell, as the family crate does; an image of this
    // board that somehow lacks them fails at link with this warning above it.
    let port = env::var("FREERTOS_PORT").unwrap_or_default();
    let Some(config_dir) = nros_build_paths::env_path("FREERTOS_CONFIG_DIR") else {
        println!(
            "cargo:warning={CRATE}: no FREERTOS_CONFIG_DIR (the board descriptor's cargo_config \
             [env] states it; `nros build` renders it); skipping the kernel compile"
        );
        return;
    };
    // Another board's port (the root workspace's own `[env]` names mps2's
    // `GCC/ARM_CM3`) is equally not a build for this board.
    if port != "GCC_POSIX" {
        println!(
            "cargo:warning={CRATE}: FREERTOS_PORT is `{port}`, not this board's GCC_POSIX; \
             skipping the kernel compile"
        );
        return;
    }
    let port_dir = nros_board_common::freertos_build::port_dir(CRATE, &freertos_dir, &port);

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let family_c = manifest_dir.join("../nros-board-freertos/c");
    let common_c = manifest_dir.join("../nros-board-common/c");
    let platform_src = nros_build_paths::nros_platform_freertos_src();
    let platform_include = nros_build_paths::nros_platform_cffi_include();

    // Host flags only: the family's `configure_cflags` resolves Cortex `[arch]`
    // flags, which a host compiler rejects.
    let base = |build: &mut cc::Build| {
        build
            .opt_level(2)
            .flag("-ffunction-sections")
            .flag("-fdata-sections")
            .warnings(false)
            .include(&config_dir)
            .include(freertos_dir.join("include"))
            .include(&port_dir)
            .include(port_dir.join("utils"));
        nros_cc_flags::strict_decls(build);
        // The link lines are written below, in dependency order; cc-rs would
        // emit them in COMPILE order, which puts the kernel before its callers.
        build.cargo_metadata(false);
    };

    // ---- kernel: GCC_POSIX port + heap_3 ----------------------------------
    let mut kernel = cc::Build::new();
    base(&mut kernel);
    for src in [
        "tasks.c",
        "queue.c",
        "list.c",
        "timers.c",
        "event_groups.c",
        "stream_buffer.c",
    ] {
        kernel.file(freertos_dir.join(src));
    }
    kernel
        .file(port_dir.join("port.c"))
        .file(port_dir.join("utils/wait_for_event.c"))
        .file(freertos_dir.join("portable/MemMang/heap_3.c"));
    nros_cc_flags::header_deps::track_header_deps(&mut kernel);
    kernel.compile("freertos_posix_kernel");

    // ---- the FreeRTOS platform C port, host-network shape -----------------
    let mut platform = cc::Build::new();
    base(&mut platform);
    platform
        .include(&platform_include)
        .define("NROS_FREERTOS_HEAP_3", "1")
        .file(platform_src.join("platform.c"))
        .file(platform_src.join("timer.c"));
    nros_cc_flags::header_deps::track_header_deps(&mut platform);
    platform.compile("nros_platform_freertos");

    // ---- glue: family task glue, board hooks, Rust-only glue --------------
    let mut glue = cc::Build::new();
    base(&mut glue);
    glue.include(&platform_include)
        .file(family_c.join("freertos_task_glue.c"))
        .file(family_c.join("freertos_run_sched_tasks.c"))
        .file(common_c.join("nros_rtos_run_components.c"))
        .file(manifest_dir.join("c/freertos_posix_hooks.c"))
        .file(manifest_dir.join("c/freertos_posix_rust.c"));
    nros_cc_flags::header_deps::track_header_deps(&mut glue);
    glue.compile("freertos_posix_glue");

    nros_cc_flags::header_deps::emit_header_deps(&PathBuf::from(
        env::var("OUT_DIR").expect("OUT_DIR"),
    ));

    // Dependency order for a single-pass linker: each archive before the ones
    // it calls into (glue -> platform -> kernel), then the host thread library
    // the POSIX port is built on.
    println!(
        "cargo:rustc-link-search=native={}",
        env::var("OUT_DIR").expect("OUT_DIR")
    );
    for lib in [
        "freertos_posix_glue",
        "nros_platform_freertos",
        "freertos_posix_kernel",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    println!("cargo:rustc-link-lib=pthread");
    // The entry is `#![no_std]`, so nothing else names libc: the C code above
    // (stdio, malloc via heap_3, the POSIX port's pthreads/signals) needs it,
    // and glibc's crt0 is what calls the entry's C-ABI `main` (threadx-linux
    // links it for the same reason).
    println!("cargo:rustc-link-lib=c");
}
