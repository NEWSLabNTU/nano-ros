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
    fixtures::{QemuProcess, RequireFixture, ZenohRouter, build_isr_wake_freertos_mps2, or_unmet},
    matrix::PlatformId,
    output::{ISR_WAKE_DONE, ISR_WAKE_SAMPLE},
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
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        nros_tests::alloc::FREERTOS_ISR_WAKE_PORT,
    ));
    assert!(
        wait_for_port(
            nros_tests::alloc::FREERTOS_ISR_WAKE_PORT,
            Duration::from_secs(5)
        ),
        "zenohd not reachable on the interrupt-wake port"
    );

    let mut qemu = QemuProcess::start_mps2_an385_networked_measured(binary)
        .expect("start QEMU for the FreeRTOS interrupt-wake image");
    let output = qemu.collect_until(ISR_WAKE_DONE, Duration::from_secs(120));
    qemu.kill();

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
        "interrupt wake: {fires} interrupts, {callbacks} callbacks, latency min {:?} max {:?} us",
        samples.iter().min(),
        samples.iter().max()
    );

    assert!(
        callbacks >= 50 && samples.len() as u64 >= 50,
        "too few callbacks:\n{output}"
    );
    assert_eq!(
        refused, 0,
        "FreeRTOS's interrupt-safe wake must take every trigger (`trigger_from_isr` returned false)"
    );
    assert!(
        fires <= callbacks + 1,
        "{fires} interrupts were served by only {callbacks} callbacks: the \
         interrupt's wake did not end the park"
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
