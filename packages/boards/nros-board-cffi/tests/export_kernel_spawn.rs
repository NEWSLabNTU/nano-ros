//! Phase 313 (was 173.4) — prove `nros_board_export!` serves a **kernel-spawn**
//! board with the SAME macro: the only difference from the direct-exec case
//! (`export_compiles.rs`) is the board's own `run` body — here a stand-in for
//! "allocate an app task carrying `app` + start the scheduler" that a real
//! FreeRTOS / ThreadX overlay would call into. Separate test binary so its
//! `nros_board_*` symbols don't collide with `export_compiles.rs`.

#![allow(dead_code)]

use core::ffi::c_void;

use nros_board_cffi::nros_board_export;

struct KernelConfig {
    _domain_id: u32,
}

fn init_hardware(_cfg: &KernelConfig) {}

fn board_print(_s: &str) {}

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

// Kernel-spawn board: unlike the direct-exec case, this does NOT run `app`
// inline — a real overlay spawns an app task carrying `app` and starts the
// scheduler (which never returns). The macro wires `nros_board_run` to this
// identically; the family shape is entirely this function's concern.
fn run<F: FnOnce() -> Result<(), i32>>(_cfg: KernelConfig, _app: F) -> ! {
    // Real impl: spawn app task carrying `_app`, start the scheduler.
    loop {
        core::hint::spin_loop();
    }
}

nros_board_export! {
    config       = KernelConfig,
    init         = init_hardware,
    println      = board_print,
    exit_success = exit_success,
    exit_failure = exit_failure,
    run          = run,
}

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
