//! # nros-board-threadx-linux
//!
//! Board crate for running nros on Linux with ThreadX + NetX Duo.
//!
//! ThreadX runs as pthreads via its Linux simulation port, and NetX Duo
//! uses a raw-socket Linux driver for real Ethernet over TAP interfaces.
//! This mirrors the FreeRTOS board crate pattern but is simpler since we
//! have `std`.
//!
//! Users call [`run()`] with a closure that receives `&Config` and creates
//! an `Executor` for full API access (publishers, subscriptions, services,
//! actions, timers, callbacks).

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

// Phase 152.2.B — canonical overlay trait impls. Generic-crate
// `run<B>` lift deferred; the traits are in place so future
// `nros_board_threadx::run::<ThreadxLinux, _, _>` shape compiles
// without further overlay-side work.
use nros_board_common::{BoardExit, BoardInit, BoardPrint};

/// Per-board marker for trait dispatch.
pub struct ThreadxLinux;

impl BoardInit for ThreadxLinux {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        init_hardware(cfg);
    }
}

impl BoardPrint for ThreadxLinux {
    fn println(args: core::fmt::Arguments<'_>) {
        // std `println!` on the host. ThreadX-linux is `std`-host,
        // so this dispatches to libc stdout via Rust's println!.
        println!("{}", args);
    }
}

impl BoardExit for ThreadxLinux {
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}
