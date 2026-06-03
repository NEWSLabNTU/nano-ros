//! Phase 212.O.3 — FreeRTOS Entry pkg main.rs.
//!
//! Mirrors `../posix_entry/src/main.rs` modulo the Board impl + the
//! `#![no_std] / #![no_main] / extern "C" fn main` shape required by
//! the embedded MPS2-AN385 target. The same `shared_node_pkg` rlib
//! linked into a different Board surface — that's the proof.

#![no_std]
#![no_main]

extern crate shared_node_pkg as _;

use nros_board_mps2_an385_freertos::Mps2An385;
use nros_platform::BoardEntry;
use panic_semihosting as _;

// Phase 212.N.4 — codegen-emitted body. Identical signature to the
// posix sibling.
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
