//! Phase 152.2.B — shared ThreadX kernel + port source helpers.
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

/// Phase 152.2.B — wire `nros-platform-threadx`'s C port into a
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
/// Phase 152.2.B.1 — generic `tx_application_define` stub +
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
/// nros_board_common::threadx_sources::add_threadx_hooks_source(&mut glue);
/// glue.file(manifest_dir.join("c/board_threadx_linux.c"));
/// glue.compile("glue");
/// ```
/// Phase 152.2.B.2 — `THREADX_CFLAGS` env wiring.
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

pub fn add_threadx_hooks_source(build: &mut cc::Build) {
    const HOOKS_C: &str = include_str!("../c/threadx_hooks.c");
    let out_dir = std::path::PathBuf::from(
        std::env::var_os("OUT_DIR").expect("nros-board-common: OUT_DIR not set"),
    );
    let dest = out_dir.join("nros_threadx_hooks.c");
    std::fs::write(&dest, HOOKS_C)
        .unwrap_or_else(|e| panic!("nros-board-common: write({}): {e}", dest.display()));
    build.file(&dest);
    println!(
        "cargo:rerun-if-changed={}/c/threadx_hooks.c",
        env!("CARGO_MANIFEST_DIR")
    );
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
        let path = src_dir.join(f);
        build.file(&path);
        // Per-FILE rerun trigger: a directory-level `rerun-if-changed`
        // only fires when entries are added/removed (the dir mtime), not
        // when a watched file's *contents* change, so editing platform.c
        // would otherwise reuse a stale object.
        println!("cargo:rerun-if-changed={}", path.display());
    }

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", cffi_include.display());
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
