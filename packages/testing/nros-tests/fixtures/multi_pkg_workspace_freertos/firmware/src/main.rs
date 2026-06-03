//! Phase 212.N.7 step-5 firmware bin — Entry pkg shape.
//!
//! Replaces the M.5.a `freertos-qemu-mps2-an385-bsp::nros_run()` baker
//! path. The bin now drives the Phase 212.N shape:
//!
//! 1. `build.rs` calls `nros_build::generate_run_plan(launch)` to emit
//!    `$OUT_DIR/run_plan.rs` with one `<pkg>::register(runtime)?` call
//!    per launch-XML `<node>` entry.
//! 2. `main.rs` `include!`s the emitted file.
//! 3. `main.rs` invokes `<Mps2An385 as BoardEntry>::run(setup)` where
//!    `setup` is the codegen-emitted `run_plan` body.

#![no_std]
#![no_main]

use nros_board_mps2_an385_freertos::Mps2An385;
use nros_platform::BoardEntry;
use panic_semihosting as _;

// Phase 212.M.5.a.3 — keep the per-pkg Node crates alive against
// `--gc-sections`. `nros::node!()` emits its symbols with
// `#[unsafe(no_mangle)]` + a `#[used]` presence marker, but cargo only
// pulls the rlibs into the link graph when the binary references *some*
// symbol from each — `extern crate _` is the canonical no-cost
// reference. Required even with step-5 because the codegen-emitted
// `run_plan` calls `<pkg>::register(runtime)` through the pkg path, not
// the mangled symbol; without `extern crate _` the rlib could still be
// dropped before path resolution.
extern crate listener_pkg as _;
extern crate talker_pkg as _;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_platform::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let outcome: Result<(), nros_platform::RuntimeError> =
        <Mps2An385 as BoardEntry>::run(|runtime| run_plan(runtime));
    match outcome {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
