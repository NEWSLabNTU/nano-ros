//! RFC-0132 D3 -- compile the CAN frame driver's `socketcan` backend
//! (`c/can_socketcan.c`). This crate owns the source; the other Linux-process
//! boards compile the same file through the same helper.
//!
//! A CROSS build (a Linux board that names a target triple, such as
//! `linux-aarch64-musl`) also states the stack its app runs on, the process
//! main thread, for `nros build`'s stack check, and passes the number to the
//! crate, which holds the main thread's stack limit to it at boot. The host
//! board's build (`TARGET == HOST`) states nothing: its stack is the host's.

use std::{env, path::PathBuf};

/// The stack a cross Linux board states for its main thread when the image
/// sets no `memory.app_stack_bytes`. The talker template peaks at 106,616 B
/// on a dev build and 47,584 B on a release build under QEMU user mode, so
/// this is about 2.4 times the larger.
const CROSS_APP_STACK_DEFAULT_BYTES: u32 = 256 * 1024;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(nros_linux_stated_stack)");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let target = env::var("TARGET").unwrap_or_default();
    let host = env::var("HOST").unwrap_or_default();
    if !target.is_empty() && target != host {
        let bytes =
            nros_board_common::threadx_qemu_riscv64_build::app_stack_bytes_from_build_env_or(
                CROSS_APP_STACK_DEFAULT_BYTES,
            );
        nros_board_common::stack_stated::emit(
            &PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")),
            &[nros_board_common::stack_stated::Stated {
                role: "OPEN",
                root: "main",
                bytes,
                knob: "NROS_MEMORY_APP_STACK_BYTES={bytes_kib}",
            }],
        );
        println!("cargo:rustc-cfg=nros_linux_stated_stack");
        println!("cargo:rustc-env=NROS_LINUX_APP_STACK_BYTES={bytes}");
    }
    nros_board_common::can_socketcan::compile_socketcan_backend();
}
