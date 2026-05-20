//! Phase 173.4 — compile-link proof that `nros_board_export!` expands
//! against a real `Board` impl and emits the five `nros_board_*`
//! symbols. The symbols are never invoked here (the exit fns diverge);
//! the value is that the macro body type-checks and links.

#![allow(dead_code)]

use core::ffi::c_void;

use nros_board_cffi::nros_board_export;
use nros_board_common::{BoardExit, BoardInit, BoardPrint, DirectExec};

struct DummyConfig {
    _domain_id: u32,
}

struct DummyBoard;

impl BoardInit for DummyBoard {
    type Config = DummyConfig;
    fn init_hardware(_cfg: &DummyConfig) {}
}

impl BoardPrint for DummyBoard {
    fn println(_args: core::fmt::Arguments<'_>) {}
}

impl BoardExit for DummyBoard {
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

// Direct-exec board: the `DirectExec` marker grants `BoardEntry` for
// free, so the macro's `nros_board_run` routes through the common run.
impl DirectExec for DummyBoard {}

nros_board_export!(DummyBoard);

#[test]
fn exported_symbols_are_addressable() {
    // Take the address of each emitted symbol to force the linker to
    // keep them; never call (they diverge / would never return).
    let symbols: [*const c_void; 5] = [
        nros_board_init_hardware as *const c_void,
        nros_board_println as *const c_void,
        nros_board_exit_success as *const c_void,
        nros_board_exit_failure as *const c_void,
        nros_board_run as *const c_void,
    ];
    assert!(symbols.iter().all(|p| !p.is_null()));
}
