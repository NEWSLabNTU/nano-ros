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
    add_c_files_in(build, &threadx_dir.join("ports").join(port_subpath).join("src"))
}

/// Phase 152.2.B — wire `nros-platform-threadx`'s C port into a
/// pre-configured `cc::Build`. Adds the cffi include dir + the
/// three platform C files (`platform.c`, `net.c`, `timer.c`) and
/// emits the matching `cargo:rerun-if-changed` lines.
///
/// Caller's `build` must already carry the architecture / cflags
/// + ThreadX kernel + NetX includes — those differ per overlay
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
/// nros_board_common::threadx_sources::add_nros_platform_threadx_build(
///     &mut platform,
///     &workspace_root,
/// );
/// platform.compile("nros_platform_threadx");
/// ```
pub fn add_nros_platform_threadx_build(build: &mut cc::Build, workspace_root: &Path) {
    let src_dir = workspace_root.join("packages/core/nros-platform-threadx/src");
    let cffi_include = workspace_root.join("packages/core/nros-platform-cffi/include");

    build.include(&cffi_include);
    build.file(src_dir.join("platform.c"));
    build.file(src_dir.join("net.c"));
    build.file(src_dir.join("timer.c"));

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", cffi_include.display());
}

fn add_c_files_in(build: &mut cc::Build, dir: &Path) -> usize {
    let mut count = 0;
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "nros-board-common: read_dir({}): {e}",
            dir.display()
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "c") {
            build.file(&path);
            count += 1;
        }
    }
    count
}
