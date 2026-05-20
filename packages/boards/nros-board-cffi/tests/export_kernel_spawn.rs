//! Phase 173.4 — prove `nros_board_export!` also serves a **kernel-spawn**
//! board: one that impls `BoardEntry` *directly* (custom `run` body)
//! rather than via the `DirectExec` blanket. Separate test binary so its
//! `nros_board_*` symbols don't collide with `export_compiles.rs`.

#![allow(dead_code)]

use core::ffi::c_void;

use nros_board_cffi::nros_board_export;
use nros_board_common::{BoardEntry, BoardExit, BoardInit, BoardPrint};

struct KernelConfig {
    _domain_id: u32,
}

struct DummyKernelBoard;

impl BoardInit for DummyKernelBoard {
    type Config = KernelConfig;
    fn init_hardware(_cfg: &KernelConfig) {}
}

impl BoardPrint for DummyKernelBoard {
    fn println(_args: core::fmt::Arguments<'_>) {}
}

impl BoardExit for DummyKernelBoard {
    fn exit_success() -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
    fn exit_failure() -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
}

// Kernel-spawn board: NOT `DirectExec`. It owns its `run` body — here a
// stand-in for "allocate task + start scheduler" that a real FreeRTOS /
// ThreadX overlay would call into. The macro still wires `nros_board_run`
// to this via `BoardEntry::run`.
impl BoardEntry for DummyKernelBoard {
    fn run<F, E>(_cfg: Self::Config, _f: F) -> !
    where
        F: FnOnce(&Self::Config) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        // Real impl: spawn an app task carrying `_f`, start the scheduler.
        loop {
            core::hint::spin_loop();
        }
    }
}

nros_board_export!(DummyKernelBoard);

#[test]
fn kernel_spawn_board_exports_run() {
    let symbols: [*const c_void; 5] = [
        nros_board_init_hardware as *const c_void,
        nros_board_println as *const c_void,
        nros_board_exit_success as *const c_void,
        nros_board_exit_failure as *const c_void,
        nros_board_run as *const c_void,
    ];
    assert!(symbols.iter().all(|p| !p.is_null()));
}
