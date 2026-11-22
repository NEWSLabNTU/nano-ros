//! phase-582 W2 (RFC-0141 D6, issue 2691) -- FreeRTOS's libc clocks answer
//! from the nano-ros clock symbols, on the MPS2-AN385 QEMU image.
//!
//! The image (workspace `fixtures/freertos_libc_clock`, its owned entry
//! `libc_clock`) is a `ROS2_MAIN` C++ program, so
//! it links the toolchain's libstdc++ the way a ported node does. It prints
//! every clock in two phases:
//!
//! - **unsynced**: no wall-clock source. `gettimeofday`, `clock_gettime(
//!   CLOCK_REALTIME)`, `time()` and `system_clock::now()` all read 0, the
//!   value `nros_platform_time_now_ns` gives for "no wall clock". Before W2
//!   the first and second answered ticks since the scheduler started, which
//!   after the probe's sleep is at least one second: this half fails on the
//!   old shims.
//! - **stub**: a stub board clock answers `nros_platform_time_now_ns` with a
//!   fixed instant (a link-time `--wrap`, in the fixture's CMakeLists), and
//!   every wall path reads it.
//!
//! In both phases `CLOCK_MONOTONIC` and `steady_clock` ADVANCE across the
//! probe's sleep (a lower bound, not only "did not go backwards"), stay far
//! from the stub instant, and agree with `nros_platform_clock_ns`: they never
//! follow the wall clock.
//!
//! Fixture must be prebuilt (`just build freertos`).

use nros_tests::{
    fixtures::{
        self, QemuProcess, RequireFixture, Rmw, is_qemu_available, migrated_leaf::LeafVariant,
    },
    matrix::PlatformId,
};
use std::{collections::HashMap, time::Duration};

const WORKSPACE: &str = "packages/testing/nros-tests/fixtures/freertos_libc_clock";
const DONE: &str = "CLOCK_PROBE done";
const NS_PER_MS: u64 = 1_000_000;

/// The probe's lines, `CLOCK_PROBE <phase> <clock>=<value>`, keyed by
/// `<phase> <clock>`.
fn parse(output: &str) -> HashMap<String, i128> {
    output
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("CLOCK_PROBE ")?;
            let (key, value) = rest.split_once('=')?;
            Some((key.to_string(), value.trim().parse().ok()?))
        })
        .collect()
}

struct Probe {
    values: HashMap<String, i128>,
    output: String,
}

impl Probe {
    fn get(&self, phase: &str, clock: &str) -> i128 {
        let key = format!("{phase} {clock}");
        *self
            .values
            .get(&key)
            .unwrap_or_else(|| panic!("the probe printed no `{key}` line:\n{}", self.output))
    }

    /// The monotonic clocks of `phase`: each advances across the sleep by at
    /// least the sleep and by less than the whole run's budget, and never
    /// comes near the stub instant.
    fn assert_monotonic(&self, phase: &str, sleep_ns: i128, stub_ns: i128) {
        for clock in ["monotonic", "steady_clock", "platform_clock"] {
            if clock == "monotonic" {
                for s in ["a", "b"] {
                    assert_eq!(
                        self.get(phase, &format!("monotonic_rc_{s}")),
                        0,
                        "{phase}: clock_gettime(CLOCK_MONOTONIC) failed:\n{}",
                        self.output
                    );
                }
            }
            let a = self.get(phase, &format!("{clock}_ns_a"));
            let b = self.get(phase, &format!("{clock}_ns_b"));
            let step = b - a;
            assert!(
                step >= sleep_ns,
                "{phase}: {clock} advanced {step} ns across a {sleep_ns} ns sleep; a \
                 monotonic clock must advance at least the time slept:\n{}",
                self.output
            );
            assert!(
                step < 60_000 * NS_PER_MS as i128,
                "{phase}: {clock} advanced {step} ns across a {sleep_ns} ns sleep:\n{}",
                self.output
            );
            assert!(
                b < stub_ns / 2,
                "{phase}: {clock} read {b} ns, near the stub wall instant {stub_ns}: the \
                 monotonic clock follows the wall clock:\n{}",
                self.output
            );
        }
        // The libc and libstdc++ monotonic clocks read the port's clock. Each
        // set is read in the order monotonic, steady_clock, platform, so a
        // reading lies at or before its own set's platform reading, and within
        // a second (far less than the sleep's slack) of the first one.
        let pa = self.get(phase, "platform_clock_ns_a");
        let pb = self.get(phase, "platform_clock_ns_b");
        for clock in ["monotonic", "steady_clock"] {
            for s in ["a", "b"] {
                let v = self.get(phase, &format!("{clock}_ns_{s}"));
                assert!(
                    v + 1_000 * NS_PER_MS as i128 >= pa && v <= pb,
                    "{phase}: {clock}_ns_{s} = {v} is not the port's monotonic clock \
                     (platform clock {pa}..{pb}):\n{}",
                    self.output
                );
            }
        }
    }
}

#[test]
fn freertos_libc_clocks_answer_from_the_clock_symbols_on_mps2() {
    fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS libc clock probe",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not available");
    }

    let binary = fixtures::migrated_leaf_binary_named(
        WORKSPACE,
        Rmw::Zenoh,
        LeafVariant::Plain,
        "libc_clock",
    )
    .unwrap_or_else(|| panic!("{WORKSPACE} is no workspace fixture"))
    .require("freertos-cpp-zenoh-libc-clock -- run `just build freertos`");
    let mut qemu = QemuProcess::start_mps2_an385_networked(&binary)
        .expect("start QEMU for the FreeRTOS libc clock probe");
    let output = qemu.collect_until(DONE, Duration::from_secs(120));
    qemu.kill();
    assert!(
        output.contains(DONE),
        "the libc clock probe never finished:\n{output}"
    );
    let p = Probe {
        values: parse(&output),
        output,
    };

    let sleep_ns = p.get("start", "sleep_ms") * NS_PER_MS as i128;
    let stub_ns = p.get("start", "stub_ns");

    // ---- unsynced: every wall path reads 0, "no wall clock" -----------------
    assert_eq!(p.get("unsynced", "gettimeofday_rc"), 0, "{}", p.output);
    assert_eq!(
        p.get("unsynced", "gettimeofday_us"),
        0,
        "unsynced: gettimeofday did not read 0 s; it answers something other than \
         nros_platform_time_now_ns (ticks since boot, issue 2691):\n{}",
        p.output
    );
    assert_eq!(p.get("unsynced", "realtime_rc"), 0, "{}", p.output);
    assert_eq!(
        p.get("unsynced", "realtime_ns"),
        0,
        "unsynced: clock_gettime(CLOCK_REALTIME) did not read 0; it answers \
         something other than nros_platform_time_now_ns:\n{}",
        p.output
    );
    assert_eq!(
        p.get("unsynced", "time_s"),
        0,
        "unsynced: time():\n{}",
        p.output
    );
    assert_eq!(
        p.get("unsynced", "system_clock_ns"),
        0,
        "unsynced: std::chrono::system_clock::now():\n{}",
        p.output
    );
    assert_eq!(
        p.get("unsynced", "cputime_rc"),
        -1,
        "an unknown clock id must be refused, not answered:\n{}",
        p.output
    );
    p.assert_monotonic("unsynced", sleep_ns, stub_ns);

    // ---- stub board clock: every wall path reads the stub's instant ---------
    assert_eq!(
        p.get("stub", "gettimeofday_us"),
        stub_ns / 1_000,
        "stub: gettimeofday does not read the board clock:\n{}",
        p.output
    );
    assert_eq!(
        p.get("stub", "realtime_ns"),
        stub_ns,
        "stub: clock_gettime(CLOCK_REALTIME) does not read the board clock:\n{}",
        p.output
    );
    assert_eq!(
        p.get("stub", "time_s"),
        stub_ns / 1_000_000_000,
        "stub: time():\n{}",
        p.output
    );
    assert_eq!(
        p.get("stub", "system_clock_ns"),
        stub_ns,
        "stub: std::chrono::system_clock::now():\n{}",
        p.output
    );
    p.assert_monotonic("stub", sleep_ns, stub_ns);

    // The monotonic clocks run on through the wall clock's step to the stub.
    for clock in ["monotonic", "steady_clock", "platform_clock"] {
        let before = p.get("unsynced", &format!("{clock}_ns_b"));
        let after = p.get("stub", &format!("{clock}_ns_a"));
        assert!(
            after >= before && after - before < 60_000 * NS_PER_MS as i128,
            "{clock} stepped from {before} to {after} when the wall clock did:\n{}",
            p.output
        );
    }
}
