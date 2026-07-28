//! Phase 313 (was 173.4) — compile-link proof that `nros_board_export!`
//! expands against a **direct-exec** board's plain functions and emits the
//! five `nros_board_*` symbols. The symbols are never invoked here (the exit
//! fns diverge); the value is that the macro body type-checks and links.
//!
//! No trait impls: the board supplies free functions, and its own `run`
//! encodes the direct-exec shape (init → app → exit, inline).

#![allow(dead_code)]

use core::ffi::c_void;

use nros_board_cffi::nros_board_export;

struct DummyConfig {
    _domain_id: u32,
}

fn init_hardware(_cfg: &DummyConfig) {}

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

// Direct-exec board: run `app` inline on the boot stack, then exit. No task
// spawn — the family shape lives entirely in this one function.
fn run<F: FnOnce() -> Result<(), i32>>(cfg: DummyConfig, app: F) -> ! {
    init_hardware(&cfg);
    match app() {
        Ok(()) => exit_success(),
        Err(_) => exit_failure(),
    }
}

nros_board_export! {
    config       = DummyConfig,
    init         = init_hardware,
    println      = board_print,
    exit_success = exit_success,
    exit_failure = exit_failure,
    run          = run,
}

#[test]
fn exported_symbols_are_addressable() {
    // Take the address of each emitted symbol to force the linker to keep
    // them; never call (they diverge / would never return).
    let symbols: [*const c_void; 5] = [
        nros_board_init_hardware as *const c_void,
        nros_board_println as *const c_void,
        nros_board_exit_success as *const c_void,
        nros_board_exit_failure as *const c_void,
        nros_board_run as *const c_void,
    ];
    assert!(symbols.iter().all(|p| !p.is_null()));
}
