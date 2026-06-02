//! Phase 212.N.7 step-2 — ThreadX-Linux Rust talker Entry pkg.
//!
//! Pairs with the sibling Component pkg `threadx_linux_rs_talker`.
//! Board choice (`ThreadxLinux`) lives here; the codegen-emitted
//! `run_plan` stays board-agnostic so the same Component pkg
//! `register` fn links under any tier-1 board impl.

use nros_board_threadx_linux::ThreadxLinux;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_build::RuntimeError>;
//
// Step-2 ships an empty stub; the §212.N.4 follow-up wires the
// sibling Component pkg's `register` into a real `run_plan` body
// from a launch.xml.
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    // `<ThreadxLinux as BoardEntry>::run` owns the ThreadX kernel
    // bring-up + executor open + spin loop; our `setup` closure
    // delegates to the codegen-emitted `run_plan`.
    let outcome: Result<(), nros_build::RuntimeError> =
        <ThreadxLinux as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        // `BoardExit::exit_failure` is invoked by `BoardEntry::run`
        // on infrastructure failure; reaching this branch means the
        // setup closure itself returned `Err`. Surface the error
        // before the process exits.
        eprintln!("threadx_linux_rs_talker_entry: run_plan failed: {err}");
        std::process::exit(1);
    }
}
