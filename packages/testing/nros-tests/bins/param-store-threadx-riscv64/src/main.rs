//! Issue 1706 — entry of the rv-virt-threadx parameter-store image.
//! `nros::main!()` reads the board from `system.toml`
//! (`[image.rv-virt-threadx] board = "rv-virt-threadx"`) and delegates to the
//! board's `BoardEntry::run_with_deploy`, which boots ThreadX + NetX Duo, opens
//! the executor and spins.
#![no_std]
#![no_main]

// rustc's staticlib/LTO DCE drops a dependency's `#[no_mangle]` exports without
// a direct reference; this anchor keeps the board (boot asm, allocator,
// critical section) in the image. The board is reached through the generated
// selection package (issue 1305), which re-exports it.
extern crate param_store_threadx_riscv64_nros_selection as _;

nros::main!(panic = "platform");
