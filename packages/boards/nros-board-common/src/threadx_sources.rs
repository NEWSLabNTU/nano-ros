//! Shared ThreadX kernel + port source helpers.
//!
//! Both `nros-board-threadx-linux` and `nros-board-threadx-qemu-riscv64`
//! (and the future generic `nros-board-threadx` crate) enumerate the
//! same ThreadX kernel directory (`common/src/*.c`) and a portable
//! layer under `ports/<arch>/gnu/src/*.c`. Centralising avoids
//! drift when a ThreadX-kernel submodule bump adds new files.
//!
//! Use from `build.rs`:
//! ```ignore
//! use nros_board_common::threadx_sources::{
//!     add_threadx_kernel_sources, add_threadx_port_sources,
//! };
//!
//! let mut build = cc::Build::new();
//! configure_arch(&mut build);
//! add_threadx_kernel_sources(&mut build, &threadx_dir);
//! add_threadx_port_sources(&mut build, &threadx_dir, "linux/gnu");
//! build.compile("threadx");
//! ```
//!
//! `cc` is a transitive dep of `nros-board-common`'s consumers but
//! NOT a direct dep of this crate — these helpers take a `&mut
//! cc::Build` so the caller owns the cc dependency edge.

use std::path::Path;

/// Add every `.c` file under `<threadx_dir>/common/src/` to a
/// `cc::Build`. Mirrors the loop both ThreadX overlays use today.
///
/// Returns the number of source files added — useful for build-time
/// sanity asserts (e.g. "ThreadX kernel set unexpectedly empty").
pub fn add_threadx_kernel_sources(build: &mut cc::Build, threadx_dir: &Path) -> usize {
    add_c_files_in(build, &threadx_dir.join("common/src"))
}

/// Add every `.c` file under `<threadx_dir>/ports/<port_subpath>/src/`
/// to a `cc::Build`. `port_subpath` is e.g. `linux/gnu` or
/// `risc-v64/gnu`. Caller is responsible for adding any matching
/// assembly files separately — `cc::Build::file` handles `.S` /
/// `.s` extensions natively but the search loop below filters to
/// `.c` to preserve the existing per-overlay split (assembly stays
/// per-overlay so each crate can pick its own toolchain prefix).
pub fn add_threadx_port_sources(
    build: &mut cc::Build,
    threadx_dir: &Path,
    port_subpath: &str,
) -> usize {
    add_c_files_in(
        build,
        &threadx_dir.join("ports").join(port_subpath).join("src"),
    )
}

/// Wire `nros-platform-threadx`'s C port into a
/// pre-configured `cc::Build`. Adds the cffi include dir + the
/// three platform C files (`platform.c`, `net.c`, `timer.c`) and
/// emits the matching `cargo:rerun-if-changed` lines.
///
/// Caller's `build` must already carry the architecture / cflags
/// plus ThreadX kernel + NetX includes — those differ per overlay
/// (NSOS shim vs full NetX-Duo) so they can't be lifted here.
///
/// `workspace_root` is the directory containing `packages/` —
/// typically `manifest_dir.parent().parent().parent()`.
///
/// # Example
/// ```ignore
/// let mut platform = cc::Build::new();
/// configure_arch(&mut platform);
/// add_threadx_includes(&mut platform, &threadx_dir, &port_dir, &config_dir);
/// add_netx_includes(&mut platform, &netx_dir);
/// nros_board_common::threadx_sources::add_nros_platform_threadx_build(&mut platform);
/// platform.compile("nros_platform_threadx");
/// ```
/// Generic `tx_application_define` stub +
/// shared FFI setters (`nros_threadx_set_app_callback`,
/// `nros_threadx_set_app_main`) + app-thread plumbing.
/// Materialises `threadx_hooks.c` into `OUT_DIR` so consumers do
/// not need to know `nros-board-common`'s own manifest path, then
/// adds the file to `build` and emits the matching
/// `cargo:rerun-if-changed`.
///
/// The overlay supplies the divergent bits via these weak hooks:
///
///   - `void nros_board_log(const char *)` — diagnostic print
///     (overlay maps to `printf` / `uart_puts` / etc.).
///   - `int nros_board_init_eth(void)` — per-board network init.
///     Linux/NSOS overlay no-ops; the RISC-V QEMU overlay runs
///     the full NetX-Duo + virtio-net sequence.
///   - `void nros_board_compute_rng_seed(uint32_t *out)` —
///     IP/MAC-derived seed so zenoh-pico session IDs do not
///     collide across simulations.
///
/// And these weak-`const` knobs (defaults match the Linux overlay):
///
///   - `nros_board_app_stack_size` (default 64 KB)
///   - `nros_board_app_priority`   (default 4)
///
/// # Example
/// ```ignore
/// let mut glue = cc::Build::new();
/// configure_arch(&mut glue);
/// add_threadx_includes(&mut glue, &threadx_dir, &port_dir, &config_dir);
/// nros_board_common::threadx_sources::add_threadx_hooks_source(&mut glue, 64 * 1024);
/// glue.file(manifest_dir.join("c/board_threadx_linux.c"));
/// glue.compile("glue");
/// ```
/// `THREADX_CFLAGS` env wiring.
///
/// Appends every space-separated flag from the `THREADX_CFLAGS`
/// environment variable to `build` and emits the matching
/// `cargo:rerun-if-env-changed=THREADX_CFLAGS` so a future
/// generic `nros-board-threadx` `build.rs` (152.2.B.3) and the
/// existing per-overlay `build.rs` files share one canonical
/// extension point for arch flags.
///
/// Empty / unset = no-op (host-native builds need no extra flags).
/// Typical overlay-set values:
///
///   - `nros-board-threadx-linux`: unset (host gcc).
///   - `nros-board-threadx-qemu-riscv64`:
///     `-march=rv64gc -mabi=lp64d -mcmodel=medany`
///   - Future Cortex-M ThreadX overlay:
///     `-mcpu=cortex-m7 -mthumb -mfpu=fpv5-d16 -mfloat-abi=hard`
///
/// Overlays typically set the var in their `.cargo/config.toml
/// [env]` block so every `cargo` invocation sees it.
pub fn apply_threadx_cflags(build: &mut cc::Build) {
    if let Ok(cflags) = std::env::var("THREADX_CFLAGS") {
        for flag in cflags.split_whitespace() {
            build.flag(flag);
        }
    }
    println!("cargo:rerun-if-env-changed=THREADX_CFLAGS");
}

/// Compile `c/threadx_hooks.c` into `build`, with the byte pool sized for an
/// image whose app thread takes `app_stack_bytes`.
pub fn add_threadx_hooks_source(build: &mut cc::Build, app_stack_bytes: usize) {
    const HOOKS_C: &str = include_str!("../c/threadx_hooks.c");
    let out_dir = std::path::PathBuf::from(
        std::env::var_os("OUT_DIR").expect("nros-board-common: OUT_DIR not set"),
    );
    let dest = out_dir.join("nros_threadx_hooks.c");
    std::fs::write(&dest, HOOKS_C)
        .unwrap_or_else(|e| panic!("nros-board-common: write({}): {e}", dest.display()));
    build.file(&dest);
    define_byte_pool(build, app_stack_bytes);
    println!(
        "cargo:rerun-if-changed={}/c/threadx_hooks.c",
        env!("CARGO_MANIFEST_DIR")
    );
}

// RFC-0140 Appendix A.1, phase-581 W5 (the pool is its own knob).
// Issue 2624.
/// Size the byte pool `threadx_hooks.c` reserves.
///
/// `threadx.byte_pool_bytes` (env `NROS_THREADX_BYTE_POOL_BYTES`) when stated,
/// else a board's or platform's `[knobs.memory] heap_bytes` rung; else
/// [`crate::threadx_pool::default_byte_pool_bytes`]: the app stack plus
/// the RMW's measured working set. The RMW is this board crate's own feature:
/// with no `rmw-zenoh` the image runs a DDS backend, which takes the larger
/// working set (Cyclone's; XRCE's is unmeasured and smaller).
///
/// The pool is its own knob, not `memory.heap_bytes`: it is the port's whole
/// allocator, not the heap alone.
///
/// A stated pool below the app stack plus the measured working set is refused
/// here, naming the knob, and the C file's `_Static_assert` refuses it again
/// on a road that skips this (cmake). Returns the pool's size.
pub fn define_byte_pool(build: &mut cc::Build, app_stack_bytes: usize) -> usize {
    use crate::threadx_pool::{PoolRmw, default_byte_pool_bytes, least_byte_pool_bytes};
    const POOL_ENV: &str = "NROS_THREADX_BYTE_POOL_BYTES";
    let rmw = if std::env::var_os("CARGO_FEATURE_RMW_ZENOH").is_some() {
        PoolRmw::Zenoh
    } else {
        PoolRmw::Cyclone
    };
    let derived = default_byte_pool_bytes(app_stack_bytes, rmw);
    crate::platform_config::watch_knob_env(POOL_ENV);
    let stated = crate::platform_config::knob_env(&|k| std::env::var(k).ok(), POOL_ENV).map(|v| {
        v.trim()
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("{POOL_ENV} must be a decimal byte count, got `{v}`"))
    });
    let pool = match stated {
        Some(bytes) => bytes,
        // The board's or platform's `[knobs.memory] heap_bytes` rung; a bare
        // `cargo build` names no platform and has none.
        None => crate::platform_config::BuildRungs::from_build_env()
            .and_then(|rungs| rungs.memory_rungs().heap_bytes)
            .unwrap_or(derived),
    };
    let least = least_byte_pool_bytes(app_stack_bytes, rmw);
    if pool < least {
        panic!(
            "the ThreadX byte pool is {pool} bytes, below the {least} this image needs \
             (its {app_stack_bytes}-byte app stack plus the {rmw:?} working set measured \
             on this port). Raise `[image.<id>.knobs.threadx] byte_pool_bytes` ({POOL_ENV}) \
             to at least {least}."
        );
    }
    if rmw == PoolRmw::Cyclone {
        build.define("NROS_THREADX_POOL_DDS", "1");
    }
    build.define(
        "NROS_THREADX_APP_STACK_BYTES",
        app_stack_bytes.to_string().as_str(),
    );
    build.define("NROS_THREADX_BYTE_POOL_BYTES", pool.to_string().as_str());
    pool
}

pub fn add_nros_platform_threadx_build(build: &mut cc::Build) {
    // Phase 208.B Track A — paths come from `nros-build-paths` which
    // walks up from `CARGO_MANIFEST_DIR` to the repo's
    // `nros-sdk-index.toml` sentinel and defaults to the in-tree
    // location, dropping the `direnv allow`/`just` precondition. Env
    // vars stay valid as out-of-tree overrides (the helper emits the
    // matching `cargo:rerun-if-env-changed` directive).
    let src_dir = nros_build_paths::nros_platform_threadx_src();
    let cffi_include = nros_build_paths::nros_platform_cffi_include();

    build.include(&cffi_include);
    for f in ["platform.c", "net.c", "timer.c"] {
        build.file(src_dir.join(f));
    }
    // issue 1580 — no `rerun-if-changed` here any more. These three sources
    // and every header they open are declared by the compiler's own depfiles:
    // the caller applies `nros_cc_flags::header_deps::track_header_deps` to
    // `build` and calls `emit_header_deps` after compiling it. The per-file
    // and per-directory lines this used to print named the sources and the
    // cffi include dir, and nothing the port includes from anywhere else.
    nros_cc_flags::header_deps::track_header_deps(build);
}

fn add_c_files_in(build: &mut cc::Build, dir: &Path) -> usize {
    let mut count = 0;
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("nros-board-common: read_dir({}): {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "c") {
            build.file(&path);
            count += 1;
        }
    }
    count
}
