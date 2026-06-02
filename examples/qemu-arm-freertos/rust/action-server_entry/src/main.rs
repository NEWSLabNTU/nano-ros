//! Phase 212.N.7 step-2 — `freertos_rs_action_server_entry` Entry pkg.
//!
//! Mirrors `examples/native/rust/entry-poc/src/main.rs` but on the
//! MPS2-AN385 + FreeRTOS board.
//!
//! 1. `build.rs` calls `nros_build::generate_run_plan(launch)` →
//!    emits `$OUT_DIR/run_plan.rs` with one `<pkg>::register(runtime)?`
//!    call per launch-XML `<node>` entry. The step-2 sweep ships an
//!    empty launch file, so the emitted body is `Ok(())`.
//! 2. `main.rs` `include!`s the emitted file.
//! 3. `main.rs` invokes `<Mps2An385 as BoardEntry>::run(setup)` where
//!    `setup` is the codegen-emitted `run_plan` body.
//!
//! Per the N.7 contract, this Entry pkg is **board-agnostic at the
//! codegen layer** — the board choice (`Mps2An385` here) lives in the
//! `main.rs` `Board::run` call, not the codegen.

#![no_std]
#![no_main]

use nros_board_mps2_an385_freertos::Mps2An385;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_build::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    // The body of `<BoardEntry as Trait>::run` owns init → executor
    // open → setup callback → spin → exit. Our setup just delegates
    // to the codegen-emitted `run_plan`.
    let outcome: Result<(), nros_build::RuntimeError> =
        <Mps2An385 as BoardEntry>::run(|runtime| run_plan(runtime));
    match outcome {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
