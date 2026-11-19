//! Two nano-ros images on the two cores of one chip, peering over RPMsg with
//! no router — the dual-core acceptance of phase-575 W4.
//!
//! **Bucket (phase-329 W4): matrix consumer.** The images are the C
//! workspace's `amp_bringup` pair, built by the Zephyr west lane as two rows at
//! the `(ZephyrQemuCortexM, C, Zenoh)` workspace coordinate
//! (`build-cortex-m-c-amp-{talker,listener}-zenoh`).
//!
//! QEMU's `mps2-an521` machine has two Cortex-M33 cores. cpu0's image is the
//! kernel; cpu1's is placed beside it with `-device loader`, and cpu0 releases
//! cpu1 once its RPMsg service has set up the shared memory. cpu0 runs the
//! talker and is the RPMsg master, so it CONNECTS; cpu1 runs the listener and
//! is the remote, so it LISTENS. Nothing else is in the session: no router, no
//! network, no host process. A sample the listener prints crossed from one
//! core to the other through the RPMsg link and nothing else.
//!
//! The emulator must be the pinned store QEMU. A distro QEMU 6.2 never ticks
//! this board's SysTick ("Timer with period zero"), so the images boot and
//! then stand still; `qemu_system_arm_cmd`'s ladder refuses rather than fall
//! back to a QEMU on `$PATH` (issue 2659).
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west lane builds both rows)
//! - the store QEMU (`nros setup`), whose `qemu-system-arm` has `mps2-an521`
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_amp_rpmsg_qemu`

use nros_tests::{
    fixtures::{QemuProcess, build_zephyr_cortex_m_amp_pair},
    matrix::PlatformId,
    output::{LISTENER_LOG_PREFIX, TALKER_LOG_PREFIX},
    qemu::qemu_system_has_machine,
    run::QemuBoot,
};
use std::time::Duration;

/// The machine both images are built for.
const MACHINE: &str = "mps2-an521";

/// Samples the listener must print. The talker publishes once a second, so
/// this is a few seconds of a healthy pair.
const SAMPLES: usize = 5;

/// Boot of both cores, the RPMsg name-service handshake and the zenoh
/// session's, then `SAMPLES` one-second periods, with room for a loaded host.
const BUDGET: Duration = Duration::from_secs(45);

/// The `N` of every `<prefix> … Hello World: N` line, in output order.
fn sequence(out: &str, prefix: &str) -> Vec<u32> {
    out.lines()
        .filter(|l| l.contains(prefix))
        .filter_map(|l| {
            let tail = l.split("Hello World: ").nth(1)?;
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect()
}

#[test]
fn zephyr_amp_rpmsg_pair_delivers_across_cores() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrQemuCortexM],
        "the Zephyr dual-core RPMsg pair",
    );
    if !qemu_system_has_machine("qemu-system-arm", MACHINE) {
        nros_tests::unmet!(
            "the resolved qemu-system-arm has no `{MACHINE}` machine -- provision the \
             pinned QEMU with `nros setup`"
        );
    }
    let (cpu0, cpu1) = build_zephyr_cortex_m_amp_pair().unwrap_or_else(|e| {
        panic!(
            "the amp_bringup pair did not resolve. The lane gate already asserted \
             these fixtures are built, so this is a real failure -- read the verdict \
             before rebuilding: {e:?}"
        )
    });

    let loader = format!("loader,file={}", cpu1.display());
    let boot = QemuBoot::table("arm", MACHINE)
        .arg("cpu", "cortex-m33")
        .arg("icount", "shift=auto")
        .then(["-device", loader.as_str()]);
    let mut qemu = QemuProcess::start(&cpu0, boot).expect("start QEMU with both images");
    let out = qemu.collect_until_count(LISTENER_LOG_PREFIX, SAMPLES, BUDGET);
    let line = qemu.command_line().to_string();
    qemu.kill();

    let heard = sequence(&out, LISTENER_LOG_PREFIX);
    let sent = sequence(&out, TALKER_LOG_PREFIX);
    assert!(
        heard.len() >= SAMPLES,
        "cpu1's listener printed {} of {SAMPLES} samples in {BUDGET:?} (cpu0's talker \
         published {}).\nQEMU: {line}\n--- output ---\n{out}",
        heard.len(),
        sent.len()
    );
    // Every sample heard was one the talker published, and they arrived in
    // the order sent: one link, one session, no reordering and no invention.
    for n in &heard {
        assert!(
            sent.contains(n),
            "cpu1 heard Hello World: {n}, which cpu0 never printed publishing.\n\
             QEMU: {line}\n--- output ---\n{out}"
        );
    }
    assert!(
        heard.windows(2).all(|w| w[0] < w[1]),
        "cpu1 heard the samples out of order: {heard:?}\nQEMU: {line}\n--- output ---\n{out}"
    );
}

/// RFC-0051 §7c -- the cases this file runs are exactly the Runtime cells
/// `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Config, Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_config_cases,
    };
    assert_consumer_config_cases(
        Consumer::ZephyrAmpRpmsg,
        &[(
            (
                P::ZephyrQemuCortexM,
                L::C,
                R::Zenoh,
                W::Pubsub,
                K::Workspace,
            ),
            &[Config::Rpmsg],
        )],
    );
}
