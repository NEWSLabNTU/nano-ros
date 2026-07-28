//! Shared config + scenario constants for the TWO-IMAGE wake-latency bench
//! (issue #0317).
//!
//! The publisher and subscriber run as SEPARATE QEMU images (distinct zenoh
//! sessions) so the zenohd router delivers a real **transport-arrival** wake to
//! the subscriber's executor — which is what the wake-cb probe measures. A
//! single-image pub→sub would only ever loop back in-process
//! (`Z_FEATURE_LOCAL_SUBSCRIBER`), bypassing the transport path, and a vanilla
//! router does not echo a sample back to the publishing session — so the probe
//! captured 0 samples (see #0317).

#![no_std]

use nros_board_mps2_an385_freertos::Config;

/// zenohd locator. The port MUST match `nros_tests::platform::FREERTOS.zenohd_port`
/// (= 7000 + FreertosMps2 index 2 * 400 = 7800). Slirp routes the guest's
/// `10.0.2.2` to the host loopback where the test's `ZenohRouter` listens.
pub const LOCATOR: &str = "tcp/10.0.2.2:7800";
pub const DOMAIN: u32 = 0;
/// The topic the publisher image publishes and the subscriber image measures on.
pub const TOPIC: &str = "/wake-latency";
/// MPS2-AN385's nominal SYSCLK / DWT rate (Phase 132 CMSDK Timer0).
pub const SYSTEM_CORE_CLOCK_HZ: u32 = 25_000_000;
/// Samples the subscriber collects before dumping the histogram + exiting.
/// 200 keeps the run under ~3 s at 100 Hz and within the probe ring's 256 cap.
pub const TARGET_SAMPLES: u32 = 200;

// Phase 141.D scenarios — exactly one active at build time.
#[cfg(all(
    feature = "scenario-single",
    any(feature = "scenario-fanout", feature = "scenario-burst")
))]
compile_error!("wake-latency: pick exactly one `scenario-*` feature");
#[cfg(all(feature = "scenario-fanout", feature = "scenario-burst"))]
compile_error!("wake-latency: pick exactly one `scenario-*` feature");

/// Scenario name (subscriber prints it in the CSV preamble). Defaults to
/// single-sub when no feature is set (141.D.1 baseline).
#[cfg(any(
    feature = "scenario-single",
    not(any(feature = "scenario-fanout", feature = "scenario-burst"))
))]
pub const SCENARIO_NAME: &str = "scenario-single";
#[cfg(feature = "scenario-fanout")]
pub const SCENARIO_NAME: &str = "scenario-fanout";
#[cfg(feature = "scenario-burst")]
pub const SCENARIO_NAME: &str = "scenario-burst";

/// Messages the PUBLISHER emits per 100 Hz tick. The burst scenario (141.D.3)
/// emits 10 back-to-back so multiple transport wakes pile into one cv-wait
/// cycle — the worst case the subscriber's executor must handle.
#[cfg(feature = "scenario-burst")]
pub const BURST: u32 = 10;
#[cfg(not(feature = "scenario-burst"))]
pub const BURST: u32 = 1;

/// Number of idle subscriptions the SUBSCRIBER registers in the fanout scenario
/// (141.D.2) — dispatch-loop walk cost without polluting the active-topic
/// latency distribution. Zero outside fanout.
#[cfg(feature = "scenario-fanout")]
pub const FANOUT_IDLE_SUBS: u32 = 4;
#[cfg(not(feature = "scenario-fanout"))]
pub const FANOUT_IDLE_SUBS: u32 = 0;

/// Publisher image config. Distinct IP/MAC from the subscriber so the FreeRTOS
/// board's IP/MAC-seeded RNG yields a distinct zenoh session id — the two peers
/// then discover each OTHER (not themselves) through the router (the #0157
/// distinct-seed rule for a hand-run pair).
pub fn publisher_config() -> Config {
    Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x20],
        ip: [10, 0, 2, 20],
        netmask: [255, 255, 255, 0],
        gateway: [10, 0, 2, 2],
        zenoh_locator: LOCATOR,
        domain_id: DOMAIN,
        ..Config::default()
    }
}

/// Subscriber (measured) image config. IP `.21`, MAC `…:21`.
pub fn subscriber_config() -> Config {
    Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x21],
        ip: [10, 0, 2, 21],
        netmask: [255, 255, 255, 0],
        gateway: [10, 0, 2, 2],
        zenoh_locator: LOCATOR,
        domain_id: DOMAIN,
        ..Config::default()
    }
}
