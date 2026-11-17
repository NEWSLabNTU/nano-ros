// RFC-0132 D3.

//! The `socketcan` backend of the CAN frame driver, compiled by every board
//! that is a Linux process.
//!
//! The source is ONE file, `packages/boards/nros-board-linux/c/can_socketcan.c`:
//! `nros-board-linux` owns it (the board layer is the only layer allowed to be
//! Linux-only), and `nros-board-freertos-posix` and `nros-board-threadx-linux`
//! compile the same file by its workspace path, with no Cargo dependency on
//! `nros-board-linux` (which would bring `nros-platform-posix` into a FreeRTOS
//! or ThreadX image).
//!
//! The archive is pulled on demand: an image that never calls
//! `nros_platform_can_*` links none of it.
//!
//! The rebuild edge is declared here, after this compile: `emit_header_deps`
//! consumes every depfile in `OUT_DIR`, so a caller that compiles other
//! sources calls this AFTER its own `emit_header_deps`.

use std::{env, path::PathBuf};

/// The backend source, by its workspace path.
pub fn socketcan_source() -> PathBuf {
    nros_build_paths::repo_root().join("packages/boards/nros-board-linux/c/can_socketcan.c")
}

/// Compile the SocketCAN backend into `libnros_can_socketcan.a` and link it.
/// Call only for a Linux target: the source includes `<linux/can.h>`.
pub fn compile_socketcan_backend() {
    let src = socketcan_source();
    println!("cargo:rerun-if-changed={}", src.display());
    let mut build = cc::Build::new();
    nros_cc_flags::strict_decls(&mut build);
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build
        .file(&src)
        .include(nros_build_paths::nros_platform_cffi_include())
        .warnings_into_errors(true)
        .compile("nros_can_socketcan");
    nros_cc_flags::header_deps::emit_header_deps(&PathBuf::from(
        env::var("OUT_DIR").expect("OUT_DIR"),
    ));
}
