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
//! - on native_sim, callbacks run within [`BOUND_US`] of their interrupt, with
//!   the allowance described on [`ON_TIME_SHARE`]. On the QEMU boards every
//!   sample is taken across an idle `wfi`, where the guest's clock is the
//!   host's, so there each callback must run before [`CAP_US`] and no earlier
//!   than its interrupt ([`Latency::CapOnly`] says why).
//!
//! Mutations, both red: the thread `trigger()` in the handler trips FreeRTOS's
//! `configASSERT` (`port.c`, a critical section entered from an interrupt),
//! and the image never finishes; and an interrupt path that sets the flag but
//! skips the port's `xSemaphoreGiveFromISR` served 50 callbacks for 2462
//! interrupts, the worst 19.7 ms late.

use nros_tests::{
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, ZenohRouter, build_isr_wake_baremetal_mps2,
        build_isr_wake_freertos_mps2, build_isr_wake_nuttx_qemu_arm, build_isr_wake_threadx_linux,
        build_isr_wake_threadx_riscv64, build_isr_wake_zephyr_native_sim, or_unmet, threadx_linux,
    },
    matrix::PlatformId,
    output::{
        ISR_PARK_DONE, ISR_PARK_SAMPLE, ISR_RACE_DONE, ISR_RACE_SAMPLE, ISR_WAKE_DONE,
        ISR_WAKE_NOT_IN_ISR, ISR_WAKE_READY_PERIOD, ISR_WAKE_SAMPLE,
    },
    wait_for_port,
};
use std::time::Duration;

/// How late a guard callback may run after its interrupt: the design's
/// bound, measured at 83 to 97 us on a quiet host.
const BOUND_US: u64 = 500;

/// The share of samples a [`Latency::Bounded`] image (native_sim) must keep
/// within [`BOUND_US`]; every sample must also stay under [`CAP_US`], half
/// the 20 ms period, so a late wake is never read as an early one. The QEMU
/// images were held to this too, until it measured the host instead of the
/// wake path: [`Latency::CapOnly`] has the numbers. Do not loosen
/// [`BOUND_US`] to absorb host delay, or tighten these to make the bound look
/// stricter.
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

    check_isr_wake_output(&output, port_name, Latency::CapOnly);
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
    check_isr_wake_output(&output, "Zephyr", Latency::Bounded);
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
    check_isr_wake_output(&output, "ThreadX", Latency::CapOnly);
}

/// Issue 2745 -- the HOSTED ThreadX twin. This port's interrupt is its timer
/// thread, which runs `_tx_timer_interrupt` between the port's context save
/// and restore; the image wraps that call (`--wrap=_tx_timer_interrupt`) and
/// signals the interrupt-safe wake from inside it every fifth tick (50 ms),
/// while the app thread parks on the wake with a 1 s budget. It opens no
/// session, so it needs no router and no veth bridge: it drives the wake
/// object `GuardCondition::trigger_from_isr` reaches, which the executor
/// parks on.
///
/// Mutation, the hosted arm refusing (`nros_platform_wake_signal_from_isr`
/// returning -1, as before this issue): 0 wakes for every interrupt, and the
/// image ends on its timeout budget. Measured in issue 2745.
#[test]
fn a_timer_interrupt_wakes_a_parked_waiter_on_threadx_linux() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxLinux],
        "the hosted ThreadX interrupt-wake image",
    );
    if !threadx_linux::is_threadx_available() {
        nros_tests::unmet!("no ThreadX kernel tree provisioned");
    }
    let binary = build_isr_wake_threadx_linux()
        .require("isr-wake-threadx-linux — run `just build threadx-linux`");
    let mut proc = ManagedProcess::spawn(binary, &[], "isr-wake-threadx-linux")
        .expect("spawn the hosted ThreadX interrupt-wake image");
    let output = proc
        .wait_for_all_output(Duration::from_secs(60))
        .expect("the hosted ThreadX interrupt-wake image produced no output in time");
    let not_in_isr: u64 = output
        .lines()
        .find_map(|l| l.split(ISR_WAKE_NOT_IN_ISR).nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("the image never finished:\n{output}"));
    assert_eq!(
        not_in_isr, 0,
        "the hook ran outside the port's interrupt bracket, so this did not test an interrupt wake"
    );
    check_isr_wake_output(&output, "ThreadX Linux", Latency::Hosted);
}

/// How late a handle park may end after its interrupt. The byte reaches the
/// pipe through NuttX's low-priority work queue, a thread hop the semaphore
/// park does not take, so its bound is its own; a park that missed the
/// interrupt sleeps to its 1 s deadline and is far past it.
const PARK_BOUND_US: u64 = 10_000;
/// How many handle-park rounds the NuttX image makes.
const PARK_ROUNDS: u64 = 10;

/// Issue 2706 -- the NuttX twin, on the QEMU ARM `virt` machine. A watchdog,
/// whose function runs from the timer interrupt, triggers a guard condition
/// while the executor parks on its semaphore. Then the HANDLE park, which
/// wakes on the wake object's pipe: an interrupt cannot `write` on NuttX, so
/// the port queues the pipe byte on the work queue, and the watchdog signals
/// the raw wake object while the main thread parks on it with no handles and
/// a 1 s deadline. Every round must end on the signal, within
/// [`PARK_BOUND_US`]. The board counts watchdog fires outside interrupt
/// context, which must be none.
///
/// Mutations, measured in issue 2706: the port refusing, and the port posting
/// the semaphore without queueing the pipe byte.
#[test]
fn a_timer_interrupt_wakes_the_parked_executor_on_nuttx_qemu_arm() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::NuttxArm],
        "the NuttX interrupt-wake image",
    );
    if !nros_tests::fixtures::is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not available");
    }
    let binary = build_isr_wake_nuttx_qemu_arm()
        .require("isr-wake-nuttx-qemu-arm — run `just nuttx build-fixtures`");
    let port = nros_tests::alloc::NUTTX_ISR_WAKE_PORT;
    let _zenohd = or_unmet(ZenohRouter::start_slirp(port));
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "zenohd not reachable on the interrupt-wake port"
    );
    let mut qemu = QemuProcess::start_nuttx_virt(binary, true)
        .unwrap_or_else(|e| panic!("start QEMU for the NuttX interrupt-wake image: {e:?}"));
    let output = qemu.collect_until(ISR_WAKE_NOT_IN_ISR, Duration::from_secs(120));
    qemu.kill();

    // (latency from the interrupt in us, the park's return, how long it
    // parked in us, interrupts that signalled during it).
    let parks: Vec<(i64, i64, u64, u64)> = output
        .lines()
        .filter_map(|l| l.split(ISR_PARK_SAMPLE).nth(1))
        .filter_map(|v| {
            let mut it = v.split_whitespace();
            Some((
                it.next()?.parse().ok()?,
                it.next()?.parse().ok()?,
                it.next()?.parse().ok()?,
                it.next()?.parse().ok()?,
            ))
        })
        .collect();
    eprintln!("NuttX handle park: {parks:?}");
    // Issue 2748: every round's latency is printed before any assertion, so a
    // red run shows both phases.
    let samples: Vec<&str> = output
        .lines()
        .filter_map(|l| l.split(ISR_WAKE_SAMPLE).nth(1))
        .map(str::trim)
        .collect();
    eprintln!("NuttX executor wake latencies (us): {samples:?}");
    check_isr_wake_output(&output, "NuttX", Latency::CapOnly);
    let done = output
        .lines()
        .find_map(|l| l.split(ISR_PARK_DONE).nth(1))
        .unwrap_or_else(|| panic!("the handle-park phase never finished:\n{output}"));
    let mut it = done
        .split_whitespace()
        .map(|v| v.parse::<u64>().unwrap_or(u64::MAX));
    let (woken, refused) = (it.next().unwrap_or(0), it.next().unwrap_or(u64::MAX));
    assert_eq!(
        refused, 0,
        "a handle park's interrupt signal was refused: {parks:?}"
    );
    assert_eq!(
        woken, PARK_ROUNDS,
        "only {woken} of {PARK_ROUNDS} handle parks ended on the interrupt's signal: {parks:?}"
    );
    assert_eq!(
        parks.len() as u64,
        PARK_ROUNDS,
        "handle-park rounds printed: {parks:?}"
    );
    // Lower bounds: a woken park had an interrupt signal during it, and was
    // not observed before that interrupt's stamp.
    let unprompted: Vec<_> = parks.iter().filter(|p| p.1 == 0 && p.3 == 0).collect();
    assert!(
        unprompted.is_empty(),
        "handle parks woken with no interrupt during them: {unprompted:?}"
    );
    let early: Vec<_> = parks.iter().filter(|p| p.0 < 0).collect();
    assert!(
        early.is_empty(),
        "handle parks observed before their interrupt: {early:?}"
    );
    let late: Vec<_> = parks
        .iter()
        .filter(|p| p.0 as u64 > PARK_BOUND_US)
        .collect();
    assert!(
        late.is_empty(),
        "handle parks later than {PARK_BOUND_US} us: {late:?}"
    );
    let not_in_isr: u64 = output
        .lines()
        .find_map(|l| l.split(ISR_WAKE_NOT_IN_ISR).nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(u64::MAX);
    assert_eq!(
        not_in_isr, 0,
        "the watchdog ran outside interrupt context, so this did not test an interrupt wake"
    );
}

/// Which latency bound an image is held to.
#[derive(Clone, Copy, PartialEq)]
enum Latency {
    /// [`ON_TIME_SHARE`] of the samples within [`BOUND_US`], and every one
    /// under [`CAP_US`].
    Bounded,
    /// Every sample under [`CAP_US`] and none early. Used for EVERY QEMU
    /// image, because on QEMU a microsecond bound across an idle stretch
    /// measures the host, not the wake path. There are two mechanisms, each
    /// measured:
    ///
    /// - `-icount shift=auto` (issue 2748, NuttX). The networked launchers
    ///   keep it, and QEMU raises the shift mid-run, so guest time per
    ///   instruction doubles and one fixed wake path read 39, 78, 159, 317,
    ///   then 645 us in ONE run.
    /// - Host idle-wake latency, even at a FIXED shift (the mps2 images, at
    ///   `shift=5`). Every sample here spans a `wfi`, and with `sleep=on`
    ///   (the network needs it) an idle guest's clock runs at wall speed. The
    ///   interrupt is then as late as the host is slow to wake QEMU's timer
    ///   thread. With nothing overlapping in the run, the bare-metal image
    ///   put 19 and 18 of 50 samples over [`BOUND_US`] (up to 876 us) on a
    ///   near-idle 48-thread runner (tier2-nightly, 2026-11-25), and at most 2
    ///   of 50 on the same build at load 48 of 24 cores. The RTIC levels cell
    ///   saw the same thing: 52 % on time against 93 to 99 %. A quiet host
    ///   wakes slower than a busy one, so no on-time share is a property of
    ///   the image. The other two QEMU images show the same tail over their
    ///   own fast paths (five runs each, load ~25 of 24 cores, 2026-11-25):
    ///   FreeRTOS mps2 at 83 us every run's minimum and 572 us to 3.1 ms
    ///   its maximum; ThreadX riscv64 at 6 to 14 us and 390 us to 1.3 ms.
    ///
    /// What still proves an interrupt wake: the image has nothing else that
    /// can end the park. Its only fallback is the 1 s `spin_once` budget, and
    /// a park the interrupt failed to end folds the interrupts into one
    /// callback, which `fires <= callbacks + 1` refuses. [`CAP_US`] (10 ms)
    /// sits below the 20 ms period and so far below that 1 s fallback, and a
    /// late wake cannot read as an early one. Mutation, measured in issue
    /// 2696 on FreeRTOS: a handler that skips the port's give served 50
    /// callbacks for 2462 interrupts, worst 19.7 ms. Both checks refuse it
    /// (`isr_wake_check::a_lost_wake_is_refused_by_count_and_cap`). Do not
    /// "tighten" a `CapOnly` image back to `Bounded` without a guest clock
    /// that stops when the host does. The teeth stay: the served and refused
    /// counts, the lower bounds, and the cap.
    CapOnly,
    /// HOSTED: every sample before the next interrupt only, the period the
    /// image states on its `ISR_WAKE_READY period_us=` line.
    /// On a hosted port the interrupt is a host thread and so is the waiter,
    /// so the latency is the host scheduler's: issue 2745 measured a median
    /// of 250 to 350 us with tails of 8 to 11 ms on a host at load 16 of 24
    /// cores, the load a gate runs it at. Neither [`BOUND_US`] nor [`CAP_US`]
    /// is a property of the wake path there. The wake is still checked
    /// exactly: one wake per interrupt (`fires <= callbacks + 1`), none early,
    /// and each before the next interrupt, so a missed wake is never read as
    /// a late one.
    Hosted,
}

/// The bounds every interrupt-wake image is held to, over its `ISR_WAKE*`
/// lines. Latencies are parsed SIGNED: a wake observed before its interrupt's
/// stamp is a defect of the measurement or the wake, never a fast wake.
fn check_isr_wake_output(output: &str, port_name: &str, latency: Latency) {
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
    let signed: Vec<i64> = output
        .lines()
        .filter_map(|l| l.split(ISR_WAKE_SAMPLE).nth(1))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    eprintln!(
        "{port_name} interrupt wake: {fires} interrupts, {callbacks} callbacks, latency min {:?} max {:?} us",
        signed.iter().min(),
        signed.iter().max()
    );
    // Lower bounds: no callback observed before its interrupt, and no
    // callback without one (a guard here is triggered only from the
    // interrupt).
    let early: Vec<&i64> = signed.iter().filter(|&&us| us < 0).collect();
    assert!(
        early.is_empty(),
        "callbacks observed BEFORE their interrupt's stamp: {early:?}"
    );
    assert!(
        callbacks <= fires,
        "{callbacks} callbacks for only {fires} interrupts: something other than the interrupt woke the guard"
    );
    let samples: Vec<u64> = signed.iter().map(|&us| us as u64).collect();
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
    if latency == Latency::Bounded {
        let on_time = samples.iter().filter(|&&us| us <= BOUND_US).count();
        assert!(
            on_time as f64 >= ON_TIME_SHARE * samples.len() as f64,
            "only {on_time} of {} callbacks within {BOUND_US} us of their interrupt: {samples:?}",
            samples.len()
        );
    }
    let cap = if latency == Latency::Hosted {
        output
            .lines()
            .find_map(|l| l.split(ISR_WAKE_READY_PERIOD).nth(1))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("the hosted image stated no period:\n{output}"))
    } else {
        CAP_US
    };
    let over_cap: Vec<&u64> = samples.iter().filter(|&&us| us > cap).collect();
    assert!(
        over_cap.is_empty(),
        "callbacks later than {cap} us after their interrupt: {over_cap:?}"
    );
}

/// The bounds [`check_isr_wake_output`] holds an image to, over synthetic
/// consoles: what [`Latency::CapOnly`] still refuses, and what it no longer
/// does.
mod isr_wake_check {
    use super::{Latency, check_isr_wake_output};

    /// A console of `samples` (us after their interrupt) and the image's
    /// final `fires callbacks refused` counts.
    fn console(samples: &[i64], fires: u64, callbacks: u64) -> String {
        let mut c = String::new();
        for us in samples {
            c.push_str(&format!("ISR_WAKE {us}\n"));
        }
        c.push_str(&format!("ISR_WAKE_DONE {fires} {callbacks} 0\n"));
        c
    }

    fn refused(c: &str, latency: Latency) -> String {
        let c = c.to_string();
        let err = std::panic::catch_unwind(move || check_isr_wake_output(&c, "test", latency))
            .expect_err("the check must refuse this console");
        err.downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// Tier2-nightly 2026-11-25, bare-metal mps2: 19 of 50 over 500 us on a
    /// near-idle runner. The host's, not the wake path's, so a QEMU image
    /// passes it and native_sim's bound would not.
    const QUIET_RUNNER: [i64; 50] = [
        343, 876, 511, 448, 231, 509, 578, 571, 441, 244, 812, 471, 573, 256, 617, 654, 540, 256,
        502, 322, 473, 322, 251, 328, 335, 444, 506, 575, 444, 442, 658, 358, 792, 441, 329, 560,
        260, 803, 444, 272, 280, 212, 627, 446, 464, 658, 474, 444, 430, 481,
    ];

    #[test]
    fn idle_wake_jitter_passes_cap_only_and_fails_the_bound() {
        let c = console(&QUIET_RUNNER, 50, 50);
        check_isr_wake_output(&c, "test", Latency::CapOnly);
        let err = refused(&c, Latency::Bounded);
        assert!(err.contains("only 31 of 50"), "{err}");
    }

    /// Issue 2696's mutation: a handler that skips the port's give. The
    /// park ends only on its 1 s budget, so 2462 interrupts fold into 50
    /// callbacks, each read up to a period (19.7 ms) after the last fire.
    #[test]
    fn a_lost_wake_is_refused_by_count_and_cap() {
        let samples: Vec<i64> = (0..50).map(|i| 5_000 + i * 300).collect();
        let err = refused(&console(&samples, 2462, 50), Latency::CapOnly);
        assert!(err.contains("served by only 50 callbacks"), "{err}");
        // Even with every interrupt served, a wake that lands a whole
        // fallback later than its interrupt is past the cap.
        let late = vec![19_700i64; 50];
        let err = refused(&console(&late, 50, 50), Latency::CapOnly);
        assert!(err.contains("later than 10000 us"), "{err}");
    }

    #[test]
    fn an_early_wake_is_refused_on_cap_only() {
        let mut samples = vec![300i64; 50];
        samples[7] = -5;
        let err = refused(&console(&samples, 50, 50), Latency::CapOnly);
        assert!(err.contains("BEFORE their interrupt"), "{err}");
    }
}
