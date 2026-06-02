//! Phase 212.N.7 step-2 — Entry pkg for the NuttX QEMU ARM service-client.
//!
//! Sibling to the `service-client` Component pkg. See the talker_entry
//! crate docs for the full lifecycle + NuttX caveat.

#![cfg_attr(not(target_os = "nuttx"), allow(dead_code))]

include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

#[cfg(target_os = "nuttx")]
fn main() {
    use nros_board_nuttx_qemu_arm::QemuArmVirt;
    use nros_platform::BoardEntry;

    let outcome: Result<(), nros_build::RuntimeError> =
        <QemuArmVirt as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        eprintln!("nuttx_rs_service_client_entry: run_plan failed: {err:?}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "nuttx"))]
fn main() {
    eprintln!(
        "nuttx_rs_service_client_entry: this binary only runs on target_os = \"nuttx\"; \
         cross-build for armv7a-nuttx-eabihf to exercise the real Entry path."
    );
}
