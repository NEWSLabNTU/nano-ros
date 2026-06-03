//! Phase 212.O.5 fixture — demo_entry main.rs (host / native_sim).
//!
//! Mirrors the H.3 firmware shape one-for-one but targets `NativeBoard`
//! instead of `Mps2An385`. `build.rs` emits `$OUT_DIR/run_plan.rs`;
//! this `main.rs` `include!`s it and feeds the codegen-emitted
//! `run_plan(runtime)` into `<NativeBoard as BoardEntry>::run`.

// Phase 212.O.5 — keep the per-pkg Component rlibs alive against
// `--gc-sections`. The codegen `run_plan(runtime)` body calls
// `<pkg>::register(runtime)` through the pkg path, so cargo only pulls
// the rlibs into the link graph when the binary references *some*
// symbol from each — `extern crate _` is the canonical no-cost
// reference.
extern crate primary_node as _;
extern crate secondary_node as _;

use nros_board_native::NativeBoard;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_platform::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    let outcome: Result<(), nros_platform::RuntimeError> =
        <NativeBoard as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        eprintln!("demo_entry: BoardEntry::run failed: {err:?}");
        std::process::exit(1);
    }
}
