//! Phase 149.2.B — shared ThreadX kernel + port source helpers.
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
