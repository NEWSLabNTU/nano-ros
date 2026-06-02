//! Phase 212.N.7 step-1 — Entry pkg POC.
//!
//! Demonstrates the Phase 212.N target user-facing shape:
//!
//! 1. `build.rs` calls `nros_build::generate_run_plan(launch)` →
//!    emits `$OUT_DIR/run_plan.rs` with one `<pkg>::register(runtime)?`
//!    call per launch-XML `<node>` entry.
//! 2. `main.rs` `include!`s the emitted file.
//! 3. `main.rs` invokes `<Board as BoardEntry>::run(setup)` where
//!    `setup` is the codegen-emitted `run_plan` body.
//!
//! Per the N.7 contract, this Entry pkg is **board-agnostic at the
//! codegen layer** — the board choice (`NativeBoard` here) lives in
//! the `main.rs` Board::run call, not the codegen. Swapping in a
//! different per-board crate is one-line: replace
//! `nros_board_native::NativeBoard` with the target's tier-1 ZST.
//!
//! ## Status
//!
//! Step-1 (this commit) ships the shape with an EMPTY launch file —
//! `run_plan` body is `Ok(())`. The wave-4 sweep migrates each
//! Component pkg to expose a `pub fn register(runtime)` and lands a
//! real launch.xml + populated `run_plan`.

use nros_board_native::NativeBoard;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_platform::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    // The body of `<BoardEntry as Trait>::run` owns init → executor
    // open → setup callback → spin → exit. Our setup just delegates
    // to the codegen-emitted `run_plan`.
    let outcome: Result<(), nros_platform::RuntimeError> =
        <NativeBoard as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        eprintln!("entry-poc: run_plan failed: {err}");
        std::process::exit(1);
    }
}
