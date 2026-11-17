//! RFC-0132 D3 -- compile the CAN frame driver's `socketcan` backend
//! (`c/can_socketcan.c`). This crate owns the source; the other Linux-process
//! boards compile the same file through the same helper.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    nros_board_common::can_socketcan::compile_socketcan_backend();
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    nros_cc_flags::header_deps::emit_header_deps(&out_dir);
}
