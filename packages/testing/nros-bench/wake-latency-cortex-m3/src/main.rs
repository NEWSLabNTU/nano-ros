//! Phase 141.D — wake-latency P99 microbench on FreeRTOS +
//! Cortex-M3 (MPS2-AN385) + zenoh-pico.
//!
//! Topology: one Executor in this binary; a publisher publishes
//! `std_msgs/Int32` at the scenario's rate; a subscription on
//! the same topic receives via the host-side `zenohd` loop-back
//! (so the wake-cb path fires on actual transport-arrival, not
//! on a local short-circuit).
//!
//! Probe wiring:
//!
//! - `wake_probe::set_cycle_reader(clock_cycles)` installs the
//!   DWT cycle-counter reader as the probe's time source
//!   (Phase 141.B.1 + .B.2).
//! - `nros_rmw_runtime_wake_cb` (entry) +
//!   `dispatch_one(EntryKind::Subscription)` (entry) auto-fire
//!   the probe's `on_wake` / `on_dispatch` hooks under
//!   `feature = "wake-latency-probe"` (Phase 141.B.2).
//! - After `N` samples this binary drains the probe ring,
//!   bucketizes into a `Histogram`, and dumps CSV over the
//!   board's UART via `nros_board_mps2_an385_freertos::println!`
//!   in the v1 format the host harness (Phase 141.C.2 helpers)
//!   parses.
//!
//! Scenario is selected at compile time via the
//! `scenario-{single,fanout,burst}` features (mutually
//! exclusive). Defaults to `scenario-single` when none is
//! given — matches the spec's 141.D.1 baseline.
//!
//! Acceptance assertion lives host-side
//! (`nros-tests::wake_latency_cortex_m3`); this binary only
//! collects + reports.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use nros_node::executor::wake_probe;
use nros_platform_mps2_an385::timing::{CycleCounter, clock_cycles, cycles_to_ns};
use panic_semihosting as _;
use std_msgs::msg::Int32;

// MPS2-AN385's nominal SYSCLK is 25 MHz. The CMSDK Timer0
// (Phase 132) drives the 1 ms SysTick at that rate; the DWT
// counter increments at the same rate so `cycles_to_ns` uses
// the same constant.
const SYSTEM_CORE_CLOCK_HZ: u32 = 25_000_000;

/// DWT-backed cycle reader exposed via `extern "C"` so the
/// probe's `set_cycle_reader` install accepts it. `clock_cycles`
/// returns `u32`; widen to `u64` so the probe's storage stays
/// uniform across reader implementations.
unsafe extern "C" fn dwt_cycle_reader() -> u64 {
    clock_cycles() as u64
}

/// Compile-time selection of the active scenario. Exactly one
/// `scenario-*` feature must be active.
#[cfg(all(
    feature = "scenario-single",
    any(feature = "scenario-fanout", feature = "scenario-burst")
))]
compile_error!("wake-latency-cortex-m3: pick exactly one `scenario-*` feature");
#[cfg(all(feature = "scenario-fanout", feature = "scenario-burst"))]
compile_error!("wake-latency-cortex-m3: pick exactly one `scenario-*` feature");

/// Default to single-sub when no feature is set; matches
/// 141.D.1 baseline.
#[cfg(not(any(
    feature = "scenario-single",
    feature = "scenario-fanout",
    feature = "scenario-burst"
)))]
const SCENARIO_NAME: &str = "scenario-single";
#[cfg(feature = "scenario-single")]
const SCENARIO_NAME: &str = "scenario-single";
#[cfg(feature = "scenario-fanout")]
const SCENARIO_NAME: &str = "scenario-fanout";
#[cfg(feature = "scenario-burst")]
const SCENARIO_NAME: &str = "scenario-burst";

/// Number of pub→sub round-trips per scenario before the
/// binary dumps the histogram + exits. 200 keeps the run under
/// ~3 s at 100 Hz (D.1) and stays within the probe ring's 256
/// capacity so the host parser sees a non-wrapped snapshot.
const TARGET_SAMPLES: u32 = 200;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // Phase 141.B.1 — DWT must be enabled before any cycle read.
    CycleCounter::enable();
    // Phase 141.B.2 — install the cycle reader so the probe's
    // `on_wake` / `on_dispatch` hooks have a time source.
    wake_probe::set_cycle_reader(Some(dwt_cycle_reader));

    // Phase 212.M-F.18 — build-time `Config` literal supersedes
    // the pre-212 `Config::from_toml(include_str!(...))` sidecar.
    // Transcribed verbatim from the retired `nros.toml`: ip
    // 10.0.2.21/24 (netmask derived from CIDR /24 → 255.255.255.0),
    // mac 02:00:00:00:00:01, gateway 10.0.2.2, locator
    // tcp/10.0.2.2:7451, domain_id 0. The `[node.rt]` block in
    // the retired toml set every scheduling field to its board-
    // crate `Default::default()` value (app_priority=12,
    // app_stack_bytes=262144, zenoh_read/lease_priority=16,
    // zenoh_read/lease_stack_bytes=5120, poll_priority=16,
    // poll_interval_ms=5) so `..Config::default()` is exact.
    // Bench fixtures are static — no runtime parameter sweep —
    // so the literal matches the §M.10 native example pattern
    // (ref: commit `e6f4cb346`).
    let config = Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        ip: [10, 0, 2, 21],
        netmask: [255, 255, 255, 0],
        gateway: [10, 0, 2, 2],
        zenoh_locator: "tcp/10.0.2.2:7451",
        domain_id: 0,
        ..Config::default()
    };
    run(config, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("wake-latency");
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("wake-latency").build()?;
        let publisher = {
            let mut node = executor.create_node("wake-latency")?;
            node.create_publisher::<Int32>("/wake-latency")?
        };

        // Fan-out scenarios: register the extra idle subs
        // BEFORE the active one so the dispatch loop has to
        // walk past them per wake. The probe only counts
        // ACTIVE subscription dispatches (the
        // `/wake-latency` topic), so the idle subs add
        // fan-out cost without polluting the latency
        // distribution.
        #[cfg(feature = "scenario-fanout")]
        for i in 0..4 {
            let topic: heapless::String<32> = {
                let mut s = heapless::String::new();
                let _ = core::fmt::write(&mut s, format_args!("/idle-{}", i));
                s
            };
            let _ = executor
                .node_mut(nid)
                .create_subscription::<Int32, _>(topic.as_str(), |_: &Int32| {});
        }

        executor.node_mut(nid).create_subscription::<Int32, _>(
            "/wake-latency",
            |_msg: &Int32| {
                // No-op cb body. The probe's `on_dispatch`
                // hook fires before this runs and captures
                // `T1 - T0` automatically.
            },
        )?;

        println!("scenario={}", SCENARIO_NAME);
        println!("system_core_clock_hz={}", SYSTEM_CORE_CLOCK_HZ);
        println!("target_samples={}", TARGET_SAMPLES);
        println!("publishing on /wake-latency");

        // Burst scenario: emit 10 messages back-to-back per
        // "tick" so multiple wakes pile into one cv-wait
        // cycle. Per the 141.D.3 spec this is the worst-case
        // path the executor must handle.
        #[cfg(feature = "scenario-burst")]
        const BURST: u32 = 10;
        #[cfg(not(feature = "scenario-burst"))]
        const BURST: u32 = 1;

        let mut emitted: i32 = 0;
        executor.register_timer(
            nros::TimerDuration::from_millis(10), // 100 Hz
            move || {
                for _ in 0..BURST {
                    let _ = publisher.publish(&Int32 { data: emitted });
                    emitted = emitted.wrapping_add(1);
                }
            },
        )?;

        // Spin until we have enough samples. Each
        // `spin_once` advances both the publisher timer +
        // any pending wake-cb dispatches; the probe ring
        // fills via the dispatch hook. Exit once the ring's
        // monotonic write counter clears `TARGET_SAMPLES`.
        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
            let mut scratch = [0u64; 1];
            let (_, total) = wake_probe::drain(&mut scratch);
            if total >= TARGET_SAMPLES {
                break;
            }
        }

        // Bucketize the full ring into a histogram + dump
        // CSV in the v1 format the host harness parses.
        // `cycles_to_ns` partial-applied to the board's
        // SYSCLK gives the probe deltas in ns.
        let mut hist = wake_probe::Histogram::new();
        let _ = wake_probe::drain_into::<{ wake_probe::PROBE_SAMPLE_CAP }>(&mut hist, |c| {
            cycles_to_ns(c as u32, SYSTEM_CORE_CLOCK_HZ)
        });

        // The board's `println!` writes through the
        // semihosting UART. Wrap that as a
        // `core::fmt::Write` adapter so `write_csv` can
        // emit through it without pulling `std`.
        struct UartWriter;
        impl core::fmt::Write for UartWriter {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                // `println!` adds a trailing newline. To
                // preserve the on-the-wire CSV format
                // (one record per line, no double-newlines)
                // strip a trailing `\n` if present and use
                // `println!` for each pre-split chunk.
                for chunk in s.split_inclusive('\n') {
                    let bare = chunk.strip_suffix('\n').unwrap_or(chunk);
                    println!("{}", bare);
                }
                Ok(())
            }
        }
        let _ = wake_probe::write_csv(&mut UartWriter, &hist);

        // Best-effort exit. `panic-semihosting`'s exit
        // feature routes a controlled "exit 0" through
        // ARMSemihosting. QEMU sees the
        // SYS_EXIT_EXTENDED and drops back to the harness.
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
