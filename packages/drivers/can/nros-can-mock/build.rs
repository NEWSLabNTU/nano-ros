//! Compile the frame-check shim (`c/frame_check.c`) against the platform
//! headers. Nothing to do on a `target_os = "none"` build: the crate is a
//! `compile_error!` there, and no cross C compiler is assumed.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=c/frame_check.c");
    println!(
        "cargo:rerun-if-changed=../../../platform/nros-platform-api/include/nros/platform_can.h"
    );
    println!("cargo:rerun-if-changed=../../../platform/nros-platform-api/include/nros/platform.h");
    if matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("none" | "nuttx")
    ) {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let include = manifest.join("../../../platform/nros-platform-api/include");
    let mut build = cc::Build::new();
    nros_cc_flags::strict_decls(&mut build);
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build
        .file(manifest.join("c/frame_check.c"))
        .include(include)
        .warnings_into_errors(true)
        .compile("nros_can_mock_frame_check");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    nros_cc_flags::header_deps::emit_header_deps(&out_dir);
}
