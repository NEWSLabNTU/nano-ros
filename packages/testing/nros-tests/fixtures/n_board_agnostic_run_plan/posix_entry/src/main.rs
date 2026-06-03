//! Phase 212.O.3 — POSIX Entry pkg main.rs.
//!
//! Mirror the canonical Entry pkg shape from
//! `examples/qemu-arm-freertos/rust/*_entry/`: include the
//! codegen-emitted `run_plan(runtime)` body and drive it through
//! `<PosixBoard as BoardEntry>::run`.
//!
//! The `extern crate _` keeps the shared component pkg's `#[used]`
//! Node-symbols alive against `--gc-sections`; the codegen-emitted
//! `run_plan` calls `shared_node_pkg::register(runtime)` through the
//! pkg path, but cargo wouldn't otherwise pull the rlib into the
//! link graph since `main.rs` itself references no symbol from it.

extern crate shared_node_pkg as _;

use nros_board_posix::PosixBoard;
use nros_platform::BoardEntry;

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs`:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_platform::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    let outcome: Result<(), nros_platform::RuntimeError> =
        <PosixBoard as BoardEntry>::run(|runtime| run_plan(runtime));
    if outcome.is_err() {
        std::process::exit(1);
    }
}
