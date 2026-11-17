//! RFC-0132 D3 -- compile the CAN frame driver's `socketcan` backend
//! (`c/can_socketcan.c`). This crate owns the source; the other Linux-process
//! boards compile the same file through the same helper.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    nros_board_common::can_socketcan::compile_socketcan_backend();
}
