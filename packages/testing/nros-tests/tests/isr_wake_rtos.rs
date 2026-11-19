//! Issue 2696 -- an interrupt handler wakes a parked executor.
//!
//! `isr-wake-freertos-mps2` parks its executor in `spin_once` with a 1 s
//! budget, and CMSDK TIMER1's handler triggers a guard condition every 20 ms
//! through `GuardCondition::trigger_from_isr`. Each guard callback reports how
//! long after its interrupt it ran, read off TIMER1's own counter.
//!
//! What the run must show:
//! - every interrupt is served: the callback count keeps up with the
//!   interrupt count (a guard is a level, so a late wake would fold several
//!   interrupts into one callback);
//! - callbacks run within [`BOUND_US`] of their interrupt (83 to 97 us at a
//!   fixed `-icount shift=5` on a quiet host), with the idle-clock allowance
//!   described on [`ON_TIME_SHARE`].
//!
//! Mutations, both red: the thread `trigger()` in the handler trips FreeRTOS's
//! `configASSERT` (`port.c`, a critical section entered from an interrupt),
//! and the image never finishes; and an interrupt path that sets the flag but
//! skips the port's `xSemaphoreGiveFromISR` served 50 callbacks for 2462
//! interrupts, the worst 19.7 ms late.

use nros_tests::{
    fixtures::{
        QemuProcess, RequireFixture, ZenohRouter, build_isr_wake_baremetal_mps2,
        build_isr_wake_freertos_mps2, build_isr_wake_threadx_riscv64,
        build_isr_wake_zephyr_native_sim, or_unmet,
    },
    matrix::PlatformId,
    output::{ISR_RACE_DONE, ISR_RACE_SAMPLE, ISR_WAKE_DONE, ISR_WAKE_NOT_IN_ISR, ISR_WAKE_SAMPLE},
    wait_for_port,
};
use std::time::Duration;

/// How late a guard callback may run after its interrupt: the design's
/// bound, measured at 83 to 97 us on a quiet host.
const BOUND_US: u64 = 500;

/// The executor parks in `wfi` between interrupts, and QEMU's `-icount
/// sleep=on` (which the network needs) runs an idle guest's clock at wall
/// speed, so a loaded host delays the timer's interrupt itself and the
/// latency read off the timer carries that delay (1.4 ms seen once under a
/// full gate). That is the emulator's, not the wake path's: this share of
/// samples must meet [`BOUND_US`], and every one must stay under
/// [`CAP_US`], half the 20 ms period, so a late wake is never read as an
/// early one. Do not loosen [`BOUND_US`] to absorb this, or tighten these
/// to make the bound look stricter.
const ON_TIME_SHARE: f64 = 0.95;
const CAP_US: u64 = 10_000;

#[test]
fn a_timer_interrupt_wakes_the_parked_executor_on_freertos_mps2() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS interrupt-wake image",
    );
    let binary = build_isr_wake_freertos_mps2()
        .require("isr-wake-freertos-mps2 — run `just build freertos`");
    run_and_check(
        binary,
        nros_tests::alloc::FREERTOS_ISR_WAKE_PORT,
        "FreeRTOS",
        ISR_WAKE_DONE,
    );
}

/// Issue 2706 -- the bare-metal twin. The executor parks in the board's
/// tickless `wfi` idle, and the platform's interrupt-safe signal is the wake
/// flag that idle's critical section reads.
///
/// Then the race probe: the image pends an interrupt exactly between the
/// park's flag read and its idle, [`RACE_ROUNDS`] times, with nothing else
/// running to end a park. Each round's wake must arrive within
/// [`RACE_BOUND_US`].
///
/// Mutation, the park reading the flag before the idle's critical section (as
/// it did before issue 2706): 6 of 10 rounds lost their wake and slept 326 to
/// 671 ms, in each of 3 runs. Red every run.
#[test]
fn a_timer_interrupt_wakes_the_parked_executor_on_baremetal_mps2() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::QemuBaremetal],
        "the bare-metal interrupt-wake image",
    );
    let binary = build_isr_wake_baremetal_mps2()
        .require("isr-wake-baremetal-mps2 — run `just build baremetal`");
    let output = run_and_check(
        binary,
        nros_tests::alloc::BAREMETAL_ISR_WAKE_PORT,
        "bare-metal",
        ISR_RACE_DONE,
    );
    let rounds: u64 = output
        .lines()
        .find_map(|l| l.split(ISR_RACE_DONE).nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("the race probe never finished:\n{output}"));
    let race: Vec<u64> = output
        .lines()
        .filter_map(|l| l.split(ISR_RACE_SAMPLE).nth(1))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    eprintln!("bare-metal park race probe: {rounds} rounds, latency {race:?} us");
    assert_eq!(
        (rounds, race.len() as u64),
        (RACE_ROUNDS, RACE_ROUNDS),
        "every race-probe round must be served:\n{output}"
    );
    let late: Vec<&u64> = race.iter().filter(|&&us| us > RACE_BOUND_US).collect();
    assert!(
        late.is_empty(),
        "an interrupt pended between the park's flag read and its idle was served late \
         ({late:?} us > {RACE_BOUND_US} us): the park lost the wake and slept on"
    );
}

/// The race probe's rounds (the image's `RACE_ROUNDS`).
const RACE_ROUNDS: u64 = 10;
/// How late a race-probe wake may be. The pended interrupt ends the `wfi` at
/// once, so this is the dispatch path alone; a lost wake sleeps to the park's
/// deadline, which is far above it.
const RACE_BOUND_US: u64 = 5_000;

/// Boot `binary` against a router on `port` and hold its run to the bound.
fn run_and_check(binary: &std::path::Path, port: u16, port_name: &str, finish: &str) -> String {
    let _zenohd = or_unmet(ZenohRouter::start_slirp(port));
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "zenohd not reachable on the interrupt-wake port"
    );
    let mut qemu = QemuProcess::start_mps2_an385_networked_measured(binary)
        .unwrap_or_else(|e| panic!("start QEMU for the {port_name} interrupt-wake image: {e:?}"));
    let output = qemu.collect_until(finish, Duration::from_secs(120));
    qemu.kill();

    check_isr_wake_output(&output, port_name);
    output
}

/// Issue 2706 -- the Zephyr twin, on native_sim. A `k_timer` expiry runs in
/// the system clock's interrupt handler and calls `trigger_from_isr`, which
/// reaches the port's `k_sem_give`. The image also counts any expiry that did
/// not run in interrupt context, which must be none: otherwise this would test
/// a thread wake under an interrupt's name.
///
/// Mutation, the port refusing again (`nros_platform_wake_signal_from_isr`
/// returning -1): measured in issue 2706.
#[test]
fn a_timer_interrupt_wakes_the_parked_executor_on_zephyr_native_sim() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNativeSim],
        "the Zephyr interrupt-wake image",
    );
    let binary = build_isr_wake_zephyr_native_sim()
        .require("isr-wake-zephyr-native-sim — run `just zephyr build-fixtures`");
    let router = or_unmet(ZenohRouter::start_unique());
    let mut zephyr = nros_tests::zephyr::ZephyrProcess::start_with_locator(
        binary,
        nros_tests::zephyr::ZephyrPlatform::NativeSim,
        &router.locator(),
    )
    .expect("start the Zephyr interrupt-wake image");
    let output = zephyr.wait_for_pattern(ISR_WAKE_NOT_IN_ISR, Duration::from_secs(120));
    zephyr.kill();
    let not_in_isr: u64 = output
        .lines()
        .find_map(|l| l.split(ISR_WAKE_NOT_IN_ISR).nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("the image never finished (a lost wake?):\n{output}"));
    assert_eq!(
        not_in_isr, 0,
        "the timer expiry ran outside interrupt context, so this did not test an interrupt wake"
    );
    check_isr_wake_output(&output, "Zephyr");
}

/// Issue 2706 -- the ThreadX twin, on the QEMU RISC-V `virt` machine. The
/// machine-timer trap runs ThreadX's tick and then the image's tick hook,
/// which triggers the guard every second tick (20 ms); the port's signal is
/// `tx_semaphore_ceiling_put`, taken at the trap's context restore.
///
/// Mutation, the port refusing (`nros_platform_wake_signal_from_isr`
/// returning -1): measured in issue 2706.
#[test]
fn a_timer_interrupt_wakes_the_parked_executor_on_threadx_riscv64() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxRiscv64],
        "the ThreadX interrupt-wake image",
    );
    if !nros_tests::fixtures::is_qemu_riscv64_available() {
        nros_tests::unmet!("qemu-system-riscv64 not available");
    }
    let binary = build_isr_wake_threadx_riscv64()
        .require("isr-wake-threadx-riscv64 — run `just threadx_riscv64 build-fixtures`");
    let port = nros_tests::alloc::THREADX_ISR_WAKE_PORT;
    let _zenohd = or_unmet(ZenohRouter::start_slirp(port));
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "zenohd not reachable on the interrupt-wake port"
    );
    let mut qemu = QemuProcess::start_riscv64_virt(binary, 0)
        .unwrap_or_else(|e| panic!("start QEMU for the ThreadX interrupt-wake image: {e:?}"));
    let output = qemu.collect_until(ISR_WAKE_DONE, Duration::from_secs(120));
    qemu.kill();
    check_isr_wake_output(&output, "ThreadX");
}

/// The bounds every interrupt-wake image is held to, over its `ISR_WAKE*`
/// lines.
fn check_isr_wake_output(output: &str, port_name: &str) {
    let done = output
        .lines()
        .find_map(|l| l.split(ISR_WAKE_DONE).nth(1))
        .unwrap_or_else(|| {
            panic!("the image never finished (an assert or a lost wake?):\n{output}")
        });
    let mut it = done
        .split_whitespace()
        .map(|v| v.parse::<u64>().unwrap_or(0));
    let (fires, callbacks, refused) = (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(u64::MAX),
    );
    let samples: Vec<u64> = output
        .lines()
        .filter_map(|l| l.split(ISR_WAKE_SAMPLE).nth(1))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    eprintln!(
        "{port_name} interrupt wake: {fires} interrupts, {callbacks} callbacks, latency min {:?} max {:?} us",
        samples.iter().min(),
        samples.iter().max()
    );
    assert!(
        callbacks >= 50 && samples.len() as u64 >= 50,
        "too few callbacks:\n{output}"
    );
    assert_eq!(
        refused, 0,
        "{port_name}'s interrupt-safe wake must take every trigger (`trigger_from_isr` returned false)"
    );
    assert!(
        fires <= callbacks + 1,
        "{fires} interrupts were served by only {callbacks} callbacks: the interrupt's wake did not end the park"
    );
    let on_time = samples.iter().filter(|&&us| us <= BOUND_US).count();
    assert!(
        on_time as f64 >= ON_TIME_SHARE * samples.len() as f64,
        "only {on_time} of {} callbacks within {BOUND_US} us of their interrupt: {samples:?}",
        samples.len()
    );
    let over_cap: Vec<&u64> = samples.iter().filter(|&&us| us > CAP_US).collect();
    assert!(
        over_cap.is_empty(),
        "callbacks later than {CAP_US} us after their interrupt: {over_cap:?}"
    );
}
