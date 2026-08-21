//! phase-329 W2 — THE realtime-dim matrix consumer (RFC-0051 §6a).
//!
//! Consolidates the ten hand-written `*_applied.rs` files — `{zephyr,nuttx,
//! threadx,freertos,posix}_core_pin_applied`, `zephyr_edf_deadline_applied`
//! (×3 langs), `nuttx_{sporadic_budget,tier_priority}_applied`,
//! `threadx_{preempt_threshold,time_slice}_applied` — into ONE parametrized test
//! over `matrix::SCHED_CELLS`. Each cell boots a `ws-realtime-*` fixture (the
//! SAME images `realtime_tiers_e2e` uses) and asserts its scheduler dim is
//! HONORED at boot, per the RFC-0052 fail-loud contract: silence is the failure.
//!
//! The per-cell markers, boot mechanism, router, and assert SHAPE are the
//! execution data ([`exec_for`]); the neutral `(dim, platform, lang)` coordinate
//! lives in `matrix::SCHED_CELLS`. Four assert shapes, preserved 1:1 from the
//! per-file originals:
//! - **AcceptOrFallback { expect }** — the kernel-accept marker OR the honest
//!   fallback note (core-pin on zephyr/nuttx/threadx/freertos; nuttx sporadic),
//!   AND it is the arm that fixture is known to take. Issue 0260 / phase-356 W3
//!   added `expect`: asserting only "one of the two happened" makes the cell
//!   pass identically whichever arm runs, so an accept path that regresses to
//!   the fallback stays green — which is how #260 stayed invisible. The arm is
//!   a property of the IMAGE (SMP? `CONFIG_SCHED_SPORADIC`?), so it is knowable
//!   up front and belongs beside the markers. Every cell also PRINTS its arm
//!   (`sched-dim arm: …`), so a sweep answers "is the accept path exercised
//!   anywhere?" without reading defconfigs.
//! - **AcceptOnly** — the accept marker must be present (posix core-pin, the #260
//!   runtime proof: `sched_setaffinity(cpu 0)` succeeds on any host).
//! - **StrictCountOne** — exactly one accept marker (zephyr EDF; threadx
//!   preempt-threshold / time-slice — the fixture bakes exactly one such tier).
//! - **EachTierOrFailNote** — accept marker OR loud failure note, per DECLARING
//!   TIER and per DECLARED VALUE (nuttx tier-priority). Phase-358 W4 replaced
//!   the whole-log `AcceptOrFailNote` with this: one tier's line satisfied it
//!   for the entire image, so the cell stayed green through issue 0579 while
//!   the boot tier's declared priority was dropped. A per-tier dim needs a
//!   per-tier assert.
//!
//! Run with: `cargo nextest run -p nros-tests --test sched_dims_applied_e2e`.

use nros_tests::{
    TestResult,
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_freertos_workspace_cpp_realtime_entry, build_native_workspace_rust_realtime_entry,
        build_nuttx_workspace_cpp_realtime_entry, build_nuttx_workspace_rust_realtime_entry,
        build_threadx_workspace_rust_realtime_entry, build_zephyr_workspace_c_realtime_entry,
        build_zephyr_workspace_c_realtime_entry_smp, build_zephyr_workspace_cpp_realtime_entry,
        build_zephyr_workspace_rust_realtime_entry,
    },
    matrix::{
        Lang as ML, PlatformId as MP, SchedCell, SchedDim as SD, Workload as MW,
        sched_runtime_cells,
    },
    output::{
        FREERTOS_CORE_PIN_FALLBACK_MARKER, FREERTOS_CORE_PIN_MARKER,
        NUTTX_CORE_PIN_FALLBACK_MARKER, NUTTX_CORE_PIN_MARKER, NUTTX_SPORADIC_FALLBACK_MARKER,
        NUTTX_SPORADIC_MARKER, NUTTX_TIER_PRIORITY_FAILED_MARKER, NUTTX_TIER_PRIORITY_MARKER,
        POSIX_CORE_PIN_FALLBACK_MARKER, POSIX_CORE_PIN_MARKER, THREADX_CORE_PIN_FALLBACK_MARKER,
        THREADX_CORE_PIN_MARKER, THREADX_PREEMPT_MARKER, THREADX_TIME_SLICE_MARKER,
        ZEPHYR_CORE_PIN_FALLBACK_MARKER, ZEPHYR_CORE_PIN_MARKER, ZEPHYR_CORE_PIN_OBSERVED_CPU1,
        ZEPHYR_EDF_DEADLINE_MARKER,
    },
};
use std::{path::PathBuf, process::Command, time::Duration};

type Resolver = fn() -> TestResult<PathBuf>;

/// Env for a native (host-process) boot.
enum NativeEnv {
    /// Hosted-spin client (posix): full session env on an ephemeral router.
    SpinClient { spin_ms: u32, step_ms: u32 },
    /// ThreadX host sim: the locator is COMPILE-baked, so only `RUST_LOG`.
    RustLogOnly,
}

/// How the cell's guest boots.
enum Boot {
    Native(NativeEnv),
    Zephyr,
    /// issue 0260 — the multi-core Zephyr target. A separate variant from
    /// `Zephyr` because that one direct-execs a host `zephyr.exe`; this is an
    /// aarch64 ELF under qemu-system-aarch64 with two cpus.
    ZephyrQemuA53Smp,
    NuttxQemu,
    FreertosQemu,
}

/// Where the zenoh router runs.
enum Router {
    /// Ephemeral (posix core-pin — the entry dials `router.locator()`).
    Ephemeral,
    /// The allocator's baked port on `host` (`0.0.0.0` for slirp guests,
    /// `127.0.0.1` for native_sim / host-sim).
    Baked(&'static str),
}

/// Which arm of a two-mode (fail-loud) dim a fixture is EXPECTED to take on
/// TODAY's images — issue 0260 / phase-356 W3.
///
/// RFC-0052 consumers are two-mode by design: honor the declaration, or say
/// loudly that the kernel could not. `AcceptOrFallback` used to assert only
/// that ONE of the two happened, which means the cell passed identically
/// whichever arm ran — and that is precisely why #260 went unnoticed for as
/// long as it did. The arm a fixture takes is a property of the IMAGE (is it
/// SMP? does the defconfig carry `CONFIG_SCHED_SPORADIC`?), so it is knowable
/// ahead of the run and belongs in the table beside the markers.
///
/// Declaring it buys two things the old shape could not:
///   * a silent REGRESSION on an accept arm (the kernel stops honoring a dim
///     and the consumer dutifully falls back) now FAILS instead of passing;
///   * a fixture that silently GAINS the capability is caught too, so the
///     accept path cannot start being exercised without anyone noticing —
///     which is the state #260 wants to reach deliberately, via its own cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// The kernel honored the declaration.
    Accept,
    /// The kernel could not, and the consumer said so loudly.
    Fallback,
}

impl Arm {
    fn as_str(self) -> &'static str {
        match self {
            Arm::Accept => "ACCEPT",
            Arm::Fallback => "FALLBACK",
        }
    }
}

/// The per-cell assertion shape.
enum Shape {
    /// Accept marker OR fallback note present (fail-loud two-mode), AND it is
    /// the arm this fixture is known to take (`expect`) — see [`Arm`].
    AcceptOrFallback { expect: Arm },
    /// Accept marker present (the fallback is a real degrade we don't expect).
    AcceptOnly,
    /// Exactly one accept marker (the fixture bakes exactly one such tier).
    StrictCountOne,
    /// issue 0579 / phase-358 W4 — EVERY listed `(tier, declared priority)`
    /// adopted its OWN declared value, or said loudly why not.
    ///
    /// Replaces `AcceptOrFailNote`, which asked only whether the accept marker
    /// or the failure note appeared ANYWHERE in the log, so on a fixture with
    /// several declaring tiers it only ever proved that SOME tier adopted
    /// SOMETHING. That is how #579 lived: the spawned `low`
    /// tier printed the marker while the boot tier's declared 110 was parsed,
    /// carried to the board and dropped, with this cell green throughout. A
    /// per-tier dim needs a per-tier assert (the issue-0196 class — gate
    /// coverage narrower than the rule it enforces).
    EachTierOrFailNote {
        tiers: &'static [(&'static str, u32)],
        fail_marker: &'static str,
    },
}

struct Exec {
    resolver: Resolver,
    boot: Boot,
    router: Router,
    timeout_secs: u64,
    /// The pattern to wait on: the common accept/fallback STEM, or (for
    /// `StrictCountOne`) the accept marker itself.
    stem: &'static str,
    accept: &'static str,
    fallback: Option<&'static str>,
    shape: Shape,
    note: &'static str,
}

/// Map a `(dim, platform, lang)` coordinate to its execution data. An unmapped
/// coordinate is a HARD panic (phase-329 W2: a new `SCHED_CELLS` row must wire
/// its boot here).
fn exec_for(dim: SD, platform: MP, lang: ML) -> Exec {
    use Boot::*;
    use Shape::*;
    match (dim, platform, lang) {
        (SD::CorePin, MP::ZephyrNativeSim, ML::Rust) => Exec {
            resolver: || build_zephyr_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Zephyr,
            router: Router::Baked("127.0.0.1"),
            timeout_secs: 30,
            stem: "nros: core pin",
            accept: ZEPHYR_CORE_PIN_MARKER,
            fallback: Some(ZEPHYR_CORE_PIN_FALLBACK_MARKER),
            // issue 0655 — flipped Fallback -> Accept, which is exactly the
            // deliberate edit this field exists to force. The arm changed
            // because the BUG was fixed, not because the assert was loosened:
            // the board now pins a spawned tier in its `k_thread_create` ->
            // `k_thread_start` window (Zephyr refuses a cpu mask on a started
            // thread), the image sets CONFIG_SCHED_CPU_MASK, and the fixture
            // declares the `core` on the SPAWNED `high` tier rather than the
            // boot tier, which has no such window. Still uniprocessor: this
            // proves the API is used CORRECTLY, not multi-core placement,
            // which is #260's remaining SMP-fixture item.
            shape: AcceptOrFallback {
                expect: Arm::Accept,
            },
            note: "spawned tier pinned before start (#655); uniprocessor, so this proves the \
                   call is correct, not SMP placement",
        },
        (SD::CorePinPlacement, MP::ZephyrNativeSim, ML::C) => Exec {
            resolver: || build_zephyr_workspace_c_realtime_entry_smp(),
            boot: ZephyrQemuA53Smp,
            // "0.0.0.0" is the BIND address for the router on the HOST, which
            // is why every slirp-guest cell uses it — 10.0.2.2 is the guest's
            // view of the host and cannot be bound here (`zenohd exited before
            // listening on tcp/10.0.2.2:7591`). The guest still DIALS 10.0.2.2;
            // that half lives in the fixture's baked locator.
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            // The OBSERVED marker, not the acceptance one. That is the whole
            // difference between this cell and the CorePin cells: they assert
            // the kernel took the pin, this asserts the tier RAN where it asked.
            stem: "nros: core pin observed",
            // The EXACT line, not the marker prefix: the prefix matches
            // `running_on=0` too, and 0 is where an unpinned tier lands anyway,
            // so asserting the prefix would assert nothing.
            accept: ZEPHYR_CORE_PIN_OBSERVED_CPU1,
            fallback: None,
            // No fallback arm: on an image that cannot answer, the board prints
            // NOTHING rather than a fabricated cpu 0, so "absent" is a failure
            // here rather than a second arm to tolerate.
            shape: AcceptOnly,
            note: "core = 1 on a 2-cpu image; asserts PLACEMENT (running_on=1), not acceptance",
        },
        (SD::CorePin, MP::NuttxArm, ML::Rust) => Exec {
            resolver: || build_nuttx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: NuttxQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: core pin",
            accept: NUTTX_CORE_PIN_MARKER,
            fallback: Some(NUTTX_CORE_PIN_FALLBACK_MARKER),
            // #260: qemu-arm-virt is single-core and the defconfig has no
            // CONFIG_SMP, so the affinity call is not compiled in.
            shape: AcceptOrFallback {
                expect: Arm::Fallback,
            },
            note: "uniprocessor image (no CONFIG_SMP): expect the loud fallback",
        },
        (SD::CorePin, MP::ThreadxLinux, ML::Rust) => Exec {
            resolver: || build_threadx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Native(NativeEnv::RustLogOnly),
            router: Router::Baked("127.0.0.1"),
            timeout_secs: 30,
            stem: "nros: core pin",
            accept: THREADX_CORE_PIN_MARKER,
            fallback: Some(THREADX_CORE_PIN_FALLBACK_MARKER),
            // #260: the ThreadX POSIX-sim build carries no TX_THREAD_SMP, so
            // tx_thread_smp_core_exclude is absent.
            shape: AcceptOrFallback {
                expect: Arm::Fallback,
            },
            note: "no TX_THREAD_SMP in the POSIX-sim build: expect the loud fallback",
        },
        (SD::CorePin, MP::FreertosMps2, ML::Cpp) => Exec {
            resolver: || build_freertos_workspace_cpp_realtime_entry().map(|p| p.to_path_buf()),
            boot: FreertosQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: core pin",
            accept: FREERTOS_CORE_PIN_MARKER,
            fallback: Some(FREERTOS_CORE_PIN_FALLBACK_MARKER),
            // #260: mps2-an385 is uniprocessor, so configUSE_CORE_AFFINITY is
            // off and vTaskCoreAffinitySet is not built (W5.11 made this loud
            // where it had been a silent `(void)task`).
            shape: AcceptOrFallback {
                expect: Arm::Fallback,
            },
            note: "uniprocessor mps2-an385: expect the loud fallback (W5.11)",
        },
        (SD::CorePin, MP::Linux, ML::Rust) => Exec {
            resolver: || build_native_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Native(NativeEnv::SpinClient {
                spin_ms: 8000,
                step_ms: 5,
            }),
            router: Router::Ephemeral,
            timeout_secs: 20,
            stem: "nros: core pin",
            accept: POSIX_CORE_PIN_MARKER,
            fallback: Some(POSIX_CORE_PIN_FALLBACK_MARKER),
            shape: AcceptOnly,
            note: "#260: sched_setaffinity(cpu 0) succeeds on any multi-core host",
        },
        (SD::EdfDeadline, MP::ZephyrNativeSim, ML::Rust) => {
            edf(build_zephyr_workspace_rust_realtime_entry)
        }
        (SD::EdfDeadline, MP::ZephyrNativeSim, ML::Cpp) => {
            edf(build_zephyr_workspace_cpp_realtime_entry)
        }
        (SD::EdfDeadline, MP::ZephyrNativeSim, ML::C) => {
            edf(build_zephyr_workspace_c_realtime_entry)
        }
        (SD::SporadicBudget, MP::NuttxArm, ML::Cpp) => Exec {
            resolver: || build_nuttx_workspace_cpp_realtime_entry().map(|p| p.to_path_buf()),
            boot: NuttxQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: sporadic budget",
            accept: NUTTX_SPORADIC_MARKER,
            fallback: Some(NUTTX_SPORADIC_FALLBACK_MARKER),
            // #260, and the reason declaring the arm is worth doing: the
            // arm/riscv defconfigs gained CONFIG_SCHED_SPORADIC=y in W5.9b, so
            // this cell measures a KERNEL-ACCEPTED budget. Asserting only
            // "accept OR fallback" let a regression to the fallback arm pass
            // green while #260 recorded the dim as covered.
            shape: AcceptOrFallback {
                expect: Arm::Accept,
            },
            note: "W5.9b defconfigs carry CONFIG_SCHED_SPORADIC=y: the kernel must ACCEPT",
        },
        (SD::TierPriority, MP::NuttxArm, ML::Rust) => Exec {
            resolver: || build_nuttx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: NuttxQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: tier priority",
            accept: NUTTX_TIER_PRIORITY_MARKER,
            fallback: None,
            // issue 0579 — BOTH tiers, named, with the priority each one
            // declares in `realtime-rust/src/demo_bringup/system.toml`:
            // `high` is tiers[0] (the boot tier, 110) and `low` is spawned
            // (100). Before W4 only `low` ever printed.
            shape: EachTierOrFailNote {
                tiers: &[("high", 110), ("low", 100)],
                fail_marker: NUTTX_TIER_PRIORITY_FAILED_MARKER,
            },
            note: "per-tier SCHED_FIFO priority applied for EVERY declaring tier, \
                   boot tier included (#579)",
        },
        (SD::TierPriority, MP::NuttxArm, ML::Cpp) => Exec {
            // Same bringup and the same two tiers as the Rust cell above — the
            // C/C++ arm of one board, so the expectations are identical by
            // construction rather than by a second table. If these ever have to
            // differ, the arms have diverged and that is the finding.
            resolver: || build_nuttx_workspace_cpp_realtime_entry().map(|p| p.to_path_buf()),
            boot: NuttxQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: tier priority",
            accept: NUTTX_TIER_PRIORITY_MARKER,
            fallback: None,
            shape: EachTierOrFailNote {
                tiers: &[("high", 110), ("low", 100)],
                fail_marker: NUTTX_TIER_PRIORITY_FAILED_MARKER,
            },
            note: "issue 0636 — the C arm reports per-tier priority too, boot tier \
                   included; it applied them at pthread_create and printed nothing",
        },
        (SD::PreemptThreshold, MP::ThreadxLinux, ML::Rust) => Exec {
            resolver: || build_threadx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Native(NativeEnv::RustLogOnly),
            router: Router::Baked("127.0.0.1"),
            timeout_secs: 30,
            stem: THREADX_PREEMPT_MARKER,
            accept: THREADX_PREEMPT_MARKER,
            fallback: None,
            shape: StrictCountOne,
            note: "tx_thread_preemption_change applied for the one declaring tier",
        },
        (SD::TimeSlice, MP::ThreadxLinux, ML::Rust) => Exec {
            resolver: || build_threadx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Native(NativeEnv::RustLogOnly),
            router: Router::Baked("127.0.0.1"),
            timeout_secs: 30,
            stem: THREADX_TIME_SLICE_MARKER,
            accept: THREADX_TIME_SLICE_MARKER,
            fallback: None,
            shape: StrictCountOne,
            note: "tx_thread_time_slice_change applied for the one declaring tier",
        },
        (d, p, l) => panic!(
            "sched_dims_applied_e2e: no execution mapping for {d:?}/{p:?}/{l:?} — add an \
             `exec_for` arm (phase-329 W2)"
        ),
    }
}

/// The three Zephyr EDF language arms share everything but the resolver.
fn edf(resolver: Resolver) -> Exec {
    Exec {
        resolver,
        boot: Boot::Zephyr,
        router: Router::Baked("127.0.0.1"),
        timeout_secs: 30,
        stem: ZEPHYR_EDF_DEADLINE_MARKER,
        accept: ZEPHYR_EDF_DEADLINE_MARKER,
        fallback: None,
        shape: Shape::StrictCountOne,
        note: "k_thread_deadline_set for exactly the one real_time deadline tier (W5.5/W5.8)",
    }
}

fn plat_str(p: MP) -> &'static str {
    match p {
        MP::ZephyrNativeSim => "zephyr",
        MP::NuttxArm => "nuttx",
        MP::ThreadxLinux => "threadx-linux",
        MP::FreertosMps2 => "freertos",
        MP::Linux => "posix",
        _ => "?",
    }
}
/// THE realtime-dim matrix consumer. Iterates every Runtime `SchedCell` and runs
/// each in one process, catching per-cell skips/failures so one missing fixture
/// never aborts the rest.
#[test]
fn sched_dims_applied() {
    let cells: Vec<&SchedCell> = sched_runtime_cells().collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no Runtime sched-dim cells"
    );

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut skipped: Vec<String> = Vec::new();
    let mut out_of_lane: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!("{:?}/{}/{}", c.dim, plat_str(c.platform), c.lang.as_str());
        // issue 0630 — narrow by LANE here, because no name filter can reach
        // inside one test. This is issue 0571's fix at its fifth site: that
        // issue found four consolidated matrix consumers that escape both
        // halves of `lane-filter.sh native`, and phase-329 W2 had already
        // folded ten `*_applied.rs` files into this one, making it a fifth
        // nobody listed. Without this, a tier-1 host reaches the zephyr cells,
        // finds no west-built image, and — because `NROS_TEST_SCOPE` is set —
        // takes the gated-run branch and PANICS, so `just ci` cannot go green
        // on a host with no Zephyr workspace at all.
        //
        // The skip is keyed on the CELL'S PLATFORM, never on "the artifact is
        // missing" (issue 0445): an admitted platform whose fixture is absent
        // still fails exactly as hard.
        if !nros_tests::lane_scope::admits(c.platform) {
            out_of_lane.push(nros_tests::lane_scope::skip_note(
                c.platform,
                c.lang.as_str(),
            ));
            continue;
        }
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_cell(c)));
        if let Err(p) = res {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            if nros_tests::skip_marker::is_skip(&msg) {
                skipped.push(format!("{label}: {msg}"));
            } else {
                failed.push(format!("{label}: {msg}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    // issue 0571's other half, and it is the half that matters more: say what
    // did NOT run, always. A cell dropped for lane or fixture reasons used to
    // vanish into a green verdict unless EVERY cell went, so "1 of 9 ran" and
    // "9 of 9 passed" printed the same thing.
    let ran = cells.len() - out_of_lane.len();
    println!(
        "sched_dims: {ran} cell(s) ran, {} skipped, {} out of lane",
        skipped.len(),
        out_of_lane.len()
    );
    for note in out_of_lane.iter().chain(skipped.iter()) {
        println!("  - {note}");
    }

    assert!(
        failed.is_empty(),
        "sched_dims: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        ran,
        failed.join("\n  ")
    );
    if ran == 0 || skipped.len() == ran {
        nros_tests::skip!(
            "no sched-dim cell RAN ({} skipped, {} out of lane):\n  {}",
            skipped.len(),
            out_of_lane.len(),
            skipped
                .iter()
                .chain(out_of_lane.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }
}

/// issue 0260 / phase-356 W3 — say which arm the dim actually took.
///
/// The assertions above make the arm LOAD-BEARING; this makes it READABLE. A
/// reviewer asking "is the accept path exercised anywhere?" could previously
/// only answer by reading each consumer's `#ifdef`s and each fixture's
/// defconfig. One grep over a sweep now answers it:
///
/// ```text
/// cargo nextest run --success-output final -E 'test(sched_dims_applied)' \
///     | grep 'sched-dim arm:'
/// ```
///
/// Deliberately a plain `println!` on the SUCCESS path: nextest captures it and
/// shows it on failure always, on success with `--success-output`. The arm is
/// context for a human, not a second channel a gate should parse — the gate is
/// the `expect:` field.
fn report_arm(platform: &str, lang: &str, dim: SD, arm: &str) {
    println!("sched-dim arm: [{platform} {lang} {dim:?}] {arm}");
}

/// Boot one sched-dim cell and assert its dim is honored per the [`Shape`].
/// Panics with `[SKIPPED] …` on an unmet precondition; the caller classifies.
fn run_cell(cell: &SchedCell) {
    let platform = plat_str(cell.platform);
    let lang = cell.lang.as_str();
    let ex = exec_for(cell.dim, cell.platform, cell.lang);

    let entry = (ex.resolver)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{platform} {lang} {:?} realtime fixture unavailable: {e}",
            cell.dim
        )
    });

    // Router: ephemeral (posix) or the allocator's baked RealtimeTiers port.
    let router = match ex.router {
        Router::Ephemeral => ZenohRouter::start_unique()
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}")),
        Router::Baked(host) => {
            let port = port_of(cell.platform, cell.lang, MW::RealtimeTiers);
            ZenohRouter::start_on(host, port)
                .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"))
        }
    };
    let timeout = Duration::from_secs(ex.timeout_secs);

    let fail_loud = || -> String {
        match ex.fallback {
            Some(fb) => format!(
                "[{platform} {lang} {:?}] boot produced NEITHER the accept marker (`{}`) NOR the \
                 fallback note (`{fb}`) — the dim was silently dropped (RFC-0052 fail-loud). {}",
                cell.dim, ex.accept, ex.note
            ),
            None => format!(
                "[{platform} {lang} {:?}] boot produced no `{}` marker — the dim was silently \
                 dropped (RFC-0052 fail-loud). {}",
                cell.dim, ex.accept, ex.note
            ),
        }
    };

    // Boot the guest and collect the log up to the wait target (`ex.stem`).
    let log: String = match ex.boot {
        Boot::ZephyrQemuA53Smp => {
            let mut z = ZephyrProcess::start(&entry, ZephyrPlatform::QemuCortexA53Smp)
                .unwrap_or_else(|e| panic!("[{platform} {lang}] boot zephyr a53 smp: {e}"));
            let l = z.wait_for_pattern(ex.stem, timeout);
            z.kill();
            l
        }
        Boot::Zephyr => {
            let mut z = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
                .unwrap_or_else(|e| panic!("[{platform} {lang}] boot zephyr native_sim: {e}"));
            let l = z.wait_for_pattern(ex.stem, timeout);
            z.kill();
            l
        }
        Boot::Native(ref env) => {
            let mut cmd = Command::new(&entry);
            match env {
                NativeEnv::SpinClient { spin_ms, step_ms } => {
                    cmd.env("RUST_LOG", "info")
                        .env("NROS_LOCATOR", router.locator())
                        .env("NROS_SESSION_MODE", "client")
                        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
                        .env("NROS_ENTRY_SPIN_STEP_MS", step_ms.to_string());
                }
                NativeEnv::RustLogOnly => {
                    cmd.env("RUST_LOG", "info");
                }
            }
            let mut g = ManagedProcess::spawn_command(cmd, "sched-dim-entry")
                .unwrap_or_else(|e| panic!("[{platform} {lang}] spawn entry: {e}"));
            let l = g.wait_for_output_pattern(ex.stem, timeout);
            g.kill();
            l.unwrap_or_else(|_| panic!("{}", fail_loud()))
        }
        Boot::NuttxQemu => {
            let mut q = QemuProcess::start_nuttx_virt(&entry, true)
                .unwrap_or_else(|e| panic!("[{platform} {lang}] boot NuttX QEMU: {e}"));
            let l = q.wait_for_output_pattern(ex.stem, timeout);
            q.kill();
            l.unwrap_or_else(|_| panic!("{}", fail_loud()))
        }
        Boot::FreertosQemu => {
            let mut q = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
                .unwrap_or_else(|e| panic!("[{platform} {lang}] boot FreeRTOS QEMU: {e}"));
            let l = q.wait_for_output_pattern(ex.stem, timeout);
            q.kill();
            l.unwrap_or_else(|_| panic!("{}", fail_loud()))
        }
    };

    // issues 0459/0460 — if the image never reached application code, say THAT
    // first. Every shape below names a missing marker, and a missing marker is
    // only evidence when something was running to emit it. Issue 0459 was
    // reported as a missing EDF marker for an image that produced four lines
    // total, which sent the investigation to the scheduler.
    let silence = nros_tests::output::runtime_silence_note(&log)
        .map(|n| format!("{n}\n  "))
        .unwrap_or_default();

    // Classify per the cell's assert shape.
    let accepted = log.contains(ex.accept);
    match ex.shape {
        Shape::AcceptOrFallback { expect } => {
            let fb = ex.fallback.map(|f| log.contains(f)).unwrap_or(false);

            // The RFC-0052 fail-loud contract first: SOMETHING must be said.
            // Kept as its own assert because "silently dropped" and "took the
            // wrong arm" are different bugs and want different messages.
            assert!(accepted || fb, "{silence}{}\nlog:\n{log}", fail_loud());

            // issue 0260 / phase-356 W3 — then WHICH arm. A cell that passes
            // identically under both arms cannot notice the accept path
            // regressing to the fallback, which is the hole this closes.
            let observed = match (accepted, fb) {
                (true, false) => Arm::Accept,
                (false, true) => Arm::Fallback,
                // Both arms in one log: some declaring tier was honored and
                // another was not. Every fixture here is uniformly capable or
                // uniformly not, so this is not a state any current image can
                // reach — and if one does, it is a finding, not a pass.
                (true, true) => panic!(
                    "{silence}[{platform} {lang} {:?}] BOTH the accept marker (`{}`) and the \
                     fallback note (`{}`) appear — this image honored the dim for some tiers \
                     and not others, which no current fixture should do. Expected a uniform \
                     {}. {}\nlog:\n{log}",
                    cell.dim,
                    ex.accept,
                    ex.fallback.unwrap_or("<none>"),
                    expect.as_str(),
                    ex.note
                ),
                (false, false) => unreachable!("guarded by the fail-loud assert above"),
            };

            assert_eq!(
                observed,
                expect,
                "{silence}[{platform} {lang} {:?}] took the {} arm, expected {}. {}\n\
                 If this is a DELIBERATE capability change to the image (e.g. it gained SMP, \
                 or a defconfig knob moved), update this cell's `expect:` — do not widen the \
                 assert back to \"either arm\", which is what issue 0260 is about.\nlog:\n{log}",
                cell.dim,
                observed.as_str(),
                expect.as_str(),
                ex.note
            );

            report_arm(platform, lang, cell.dim, observed.as_str());
        }
        Shape::AcceptOnly => {
            assert!(
                accepted,
                "{silence}[{platform} {lang} {:?}] expected the ACCEPT marker (`{}`); saw \
                 fallback? {}\nlog:\n{log}",
                cell.dim, ex.accept, ex.note
            );
            report_arm(platform, lang, cell.dim, Arm::Accept.as_str());
        }
        Shape::StrictCountOne => {
            let hits = nros_tests::count_pattern(&log, ex.accept);
            assert_eq!(
                hits, 1,
                "{silence}[{platform} {lang} {:?}] expected exactly 1 `{}` (the single \
                 declaring tier), saw {hits}. {}\nlog:\n{log}",
                cell.dim, ex.accept, ex.note
            );
            report_arm(platform, lang, cell.dim, Arm::Accept.as_str());
        }
        Shape::EachTierOrFailNote { tiers, fail_marker } => {
            // issue 0579 — assert per DECLARING TIER. Each tier must produce
            // its own line naming its own declared priority, so neither a
            // sibling tier's line nor a right-tier/wrong-value line satisfies
            // it.
            let missing: Vec<String> = tiers
                .iter()
                .filter(|(tier, prio)| {
                    let ok = nros_tests::output::nuttx_tier_priority_line(ex.accept, tier, *prio);
                    let loud =
                        nros_tests::output::nuttx_tier_priority_line(fail_marker, tier, *prio);
                    !log.contains(&ok) && !log.contains(&loud)
                })
                .map(|(tier, prio)| format!("`{tier}` prio={prio}"))
                .collect();
            assert!(
                missing.is_empty(),
                "{silence}[{platform} {lang} {:?}] {} of {} declaring tiers produced NEITHER \
                 `{}` NOR `{fail_marker}` with their own declared priority: {} — accepted and \
                 dropped (RFC-0052 fail-loud). {}\nlog:\n{log}",
                cell.dim,
                missing.len(),
                tiers.len(),
                ex.accept,
                missing.join(", "),
                ex.note
            );

            // Per-TIER dim, so there is no single arm: report the split. A
            // sweep where every tier silently moved to the fail-note arm is a
            // real degrade that the assert above tolerates by design (it is
            // the fail-loud contract), and this is what makes it visible.
            let honored = tiers
                .iter()
                .filter(|(tier, prio)| {
                    log.contains(&nros_tests::output::nuttx_tier_priority_line(
                        ex.accept, tier, *prio,
                    ))
                })
                .count();
            report_arm(
                platform,
                lang,
                cell.dim,
                &format!(
                    "{honored}/{} tiers ACCEPT, {} FALLBACK",
                    tiers.len(),
                    tiers.len() - honored
                ),
            );
        }
    }
}
