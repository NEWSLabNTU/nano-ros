//! Phase 212.N.7 step-2 — Entry pkg for the NuttX QEMU ARM talker.
//!
//! Sibling to the `talker` Component pkg. Same shape as
//! `examples/native/rust/entry-poc/src/main.rs`, swapping
//! `NativeBoard` for `QemuArmVirt` (NuttX tier-1).
//!
//! ## NuttX caveat
//!
//! `<QemuArmVirt as BoardEntry>::run` is gated `target_os = "nuttx"`
//! in `nros-board-nuttx-qemu-arm` (see N.3 entry_212n.rs cfg note —
//! activating the `reference-qemu-arm` feature on the family driver
//! would create a cargo package cycle). On host `cargo check` the
//! trait impl is absent; the other three impls (BoardInit/BoardPrint/
//! BoardExit) still type-check so the dep graph stays validated.
//! Cross-build for `armv7a-nuttx-eabihf` materialises the real
//! `BoardEntry::run` body that delegates to
//! `nros_board_nuttx::run_entry`.
//!
//! ## Status
//!
//! Step-2 (this commit) ships the Entry pkg shape with an EMPTY launch
//! file — `run_plan` body is `Ok(())` (the `build.rs` falls back to a
//! stub when no `launch/system.launch.xml` is present). The body
//! becomes real once codegen wires `nuttx_rs_talker::register(runtime)`
//! into `run_plan` per launch XML.

#![cfg_attr(not(target_os = "nuttx"), allow(dead_code))]

// Phase 212.N.4 — codegen-emitted body. `$OUT_DIR/run_plan.rs` defines:
//
//   pub fn run_plan(
//       runtime: &mut ::nros_platform::RuntimeCtx<'_>,
//   ) -> Result<(), ::nros_platform::RuntimeError>;
include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

#[cfg(target_os = "nuttx")]
fn main() {
    use nros_board_nuttx_qemu_arm::QemuArmVirt;
    use nros_platform::BoardEntry;

    let outcome: Result<(), nros_platform::RuntimeError> =
        <QemuArmVirt as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        // NuttX hosts `std`; eprintln routes through the NuttX serial
        // console (same path the M.5.a baker uses for its own
        // diagnostics).
        eprintln!("nuttx_rs_talker_entry: run_plan failed: {err:?}");
        std::process::exit(1);
    }
}

/// Host-target `main` stub.
///
/// `<QemuArmVirt as BoardEntry>::run` is only impl'd `cfg(target_os =
/// "nuttx")` (see crate docs), so on a host `cargo check` we need a
/// `main` body that does NOT call it. The stub keeps the crate
/// link-compatible everywhere; the real Entry path is the NuttX-target
/// `main` above.
#[cfg(not(target_os = "nuttx"))]
fn main() {
    eprintln!(
        "nuttx_rs_talker_entry: this binary only runs on target_os = \"nuttx\"; \
         cross-build for armv7a-nuttx-eabihf to exercise the real Entry path."
    );
}
