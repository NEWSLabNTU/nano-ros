//! Phase 212.N.7 step-2 — ThreadX-Linux Rust listener Entry pkg.
//!
//! Pairs with the sibling Component pkg `threadx_linux_rs_listener`.
//! Board choice (`ThreadxLinux`) lives here; the codegen-emitted
//! `run_plan` stays board-agnostic so the same Component pkg
//! `register` fn links under any tier-1 board impl.

use nros_board_threadx_linux::ThreadxLinux;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. Step-2 ships an empty stub.
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    let outcome: Result<(), nros_build::RuntimeError> =
        <ThreadxLinux as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        eprintln!("threadx_linux_rs_listener_entry: run_plan failed: {err}");
        std::process::exit(1);
    }
}
