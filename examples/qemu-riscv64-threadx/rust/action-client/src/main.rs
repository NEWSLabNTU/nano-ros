//! ThreadX QEMU RISC-V Action Client — app-node entry (Phase 245).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[package.metadata.nros.entry] deploy = "threadx-qemu-riscv64"`, resolves the
//! board (`nros-board-threadx-qemu-riscv64`), and emits the `target_os = "none"`
//! boot scaffold that runs this crate's `register` (from `lib.rs`'s
//! `nros::node!(FibonacciClient)`). The board owns executor open + RMW + spin;
//! the deploy overlay threads the locator/domain. The CycloneDDS/CMake path uses
//! `lib.rs::app_main` instead.

#![no_std]
#![no_main]

nros::main!();
