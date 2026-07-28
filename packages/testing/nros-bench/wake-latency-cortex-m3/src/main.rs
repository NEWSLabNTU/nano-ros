//! Phase 141.D — wake-latency P99 microbench, SUBSCRIBER (measured) image.
//!
//! Issue #0317 — the bench is now TWO images. This one subscribes to
//! [`wake_latency_cortex_m3::TOPIC`] and, on each **transport-arrival** wake
//! from the separate publisher image (routed through the host `zenohd`), the
//! probe's `on_wake` / `on_dispatch` hooks capture the executor
//! wake→dispatch delta. After `TARGET_SAMPLES` it drains the ring into a
//! histogram and dumps CSV (v1 format) over semihosting for the host runner
//! `nros-tests::wake_latency_cortex_m3`, then exits.
//!
//! The wake-cb path only fires on real transport arrival, so a same-image
//! pub→sub (in-process loopback) would NOT exercise it — hence the two-image
//! split (the publisher lives in `src/publisher.rs`).

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Mps2An385, println};
use nros_node::executor::wake_probe;
use nros_platform_mps2_an385::timing::{CycleCounter, clock_cycles, cycles_to_ns};
use panic_semihosting as _;
use std_msgs::msg::Int32;
use wake_latency_cortex_m3::{
    FANOUT_IDLE_SUBS, SCENARIO_NAME, SYSTEM_CORE_CLOCK_HZ, TARGET_SAMPLES, TOPIC, subscriber_config,
};

/// DWT-backed cycle reader exposed via `extern "C"` so the probe's
/// `set_cycle_reader` install accepts it; widened to `u64` for uniform storage.
unsafe extern "C" fn dwt_cycle_reader() -> u64 {
    clock_cycles() as u64
}

// Issue #0273/#0317 — the C startup (`board_mps2.c` `Reset_Handler`) jumps to
// the Rust `main` symbol; the retired `_start` shape left `undefined symbol: main`.
#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    // Phase 141.B.1 — DWT must be enabled before any cycle read.
    CycleCounter::enable();
    // Phase 141.B.2 — install the cycle reader so the probe's on_wake /
    // on_dispatch hooks have a time source.
    wake_probe::set_cycle_reader(Some(dwt_cycle_reader));

    // Phase 313 (#0243/#0317) — no-session boot wrapper (scheduler + bringup, no
    // board `Executor::open`); this image opens its OWN executor below.
    let _ = Mps2An385::run_bare(subscriber_config(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("wake-latency-sub");
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("wake-latency-sub").build()?;

        // Fanout scenario: register the idle subs BEFORE the active one so the
        // dispatch loop walks past them per wake. The probe only counts ACTIVE
        // (`TOPIC`) dispatches, so the idle subs add fan-out cost without
        // polluting the latency distribution.
        for i in 0..FANOUT_IDLE_SUBS {
            let topic: heapless::String<32> = {
                let mut s = heapless::String::new();
                let _ = core::fmt::write(&mut s, format_args!("/idle-{}", i));
                s
            };
            let _ = executor
                .node_mut(nid)
                .create_subscription::<Int32, _>(topic.as_str(), |_: &Int32| {});
        }

        use core::sync::atomic::{AtomicU32, Ordering};
        // Count DELIVERED messages so the loop can terminate + emit the CSV even
        // when the probe records 0 samples (QEMU does not emulate the DWT CYCCNT,
        // so `on_dispatch`'s `t1 - t0` is 0 and no probe sample lands — the host
        // runner then takes its "0 samples / CYCCNT not emulated" skip). On real
        // hardware the probe fills first and the P99 is meaningful.
        static RX: AtomicU32 = AtomicU32::new(0);
        executor
            .node_mut(nid)
            .create_subscription::<Int32, _>(TOPIC, |_msg: &Int32| {
                // The probe's `on_dispatch` hook fires before this runs and
                // captures `T1 - T0`; we also count deliveries for loop exit.
                RX.fetch_add(1, Ordering::Relaxed);
            })?;

        println!("scenario={}", SCENARIO_NAME);
        println!("system_core_clock_hz={}", SYSTEM_CORE_CLOCK_HZ);
        println!("target_samples={}", TARGET_SAMPLES);
        println!("subscribed on {} — waiting for the publisher image", TOPIC);

        // Spin until we have enough samples. Each `spin_once` services pending
        // wake-cb dispatches from the transport; the probe ring fills via the
        // dispatch hook. Exit once the ring's monotonic write counter clears
        // `TARGET_SAMPLES`.
        // NB (issue #0317): the probe's `on_wake` T0 fires from the wake callback,
        // which — on the multi-threaded (FreeRTOS) zpico backend — is currently
        // only invoked from the main-thread `drive_io` poll path, not from the
        // async read task that actually receives the sample. So delivery works
        // (the callback runs) but the transport-wake path the probe measures does
        // not trigger, and this loop collects 0 samples until that executor/zpico
        // wake-signal gap is fixed. See the #0317 issue.
        // Exit on whichever fills first: the probe ring (real hardware) OR the
        // delivered-message count (QEMU, where CYCCNT is 0 so the probe never
        // fills but delivery still works). A long spin timeout lets the executor
        // cv-wait so the transport arrival WAKES it (firing the probe's `on_wake`
        // T0 via the read-task wake callback — issue #0317) rather than polling.
        loop {
            executor.spin_once(core::time::Duration::from_millis(1000));
            let mut scratch = [0u64; 1];
            let (_, probe_total) = wake_probe::drain(&mut scratch);
            let rx = RX.load(Ordering::Relaxed);
            if probe_total >= TARGET_SAMPLES || rx >= TARGET_SAMPLES {
                break;
            }
        }

        // Bucketize the full ring into a histogram + dump CSV in the v1 format
        // the host harness parses. `cycles_to_ns` partial-applied to the board's
        // SYSCLK gives the probe deltas in ns.
        let mut hist = wake_probe::Histogram::new();
        let _ = wake_probe::drain_into::<{ wake_probe::PROBE_SAMPLE_CAP }>(&mut hist, |c| {
            cycles_to_ns(c as u32, SYSTEM_CORE_CLOCK_HZ)
        });

        // Buffer the whole CSV, then emit one line per board `println!`.
        // `write_csv` does PARTIAL writes (edge, `,`, count as separate
        // `write_str` calls), so a per-`write_str` println would split each record
        // across lines; accumulate into a heapless buffer and re-split on `\n`.
        let mut buf = heapless::String::<1024>::new();
        let _ = wake_probe::write_csv(&mut buf, &hist);
        for line in buf.lines() {
            println!("{}", line);
        }

        // Best-effort exit via `panic-semihosting`'s `EXIT_SUCCESS` route; QEMU
        // sees SYS_EXIT_EXTENDED and drops back to the harness.
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    });
    // `run_bare` diverges (scheduler start → the app task's semihosting exit);
    // satisfies the `-> !` entry.
    unreachable!()
}
