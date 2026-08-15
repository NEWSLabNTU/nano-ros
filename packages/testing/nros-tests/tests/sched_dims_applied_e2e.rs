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
//! - **AcceptOrFallback** — the kernel-accept marker OR the honest fallback note
//!   (core-pin on zephyr/nuttx/threadx/freertos; nuttx sporadic).
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
        build_zephyr_workspace_cpp_realtime_entry, build_zephyr_workspace_rust_realtime_entry,
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
        ZEPHYR_CORE_PIN_FALLBACK_MARKER, ZEPHYR_CORE_PIN_MARKER, ZEPHYR_EDF_DEADLINE_MARKER,
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

/// The per-cell assertion shape.
enum Shape {
    /// Accept marker OR fallback note present (fail-loud two-mode).
    AcceptOrFallback,
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
            shape: AcceptOrFallback,
            note: "k_thread_cpu_pin honored, or uniprocessor/SMP fallback",
        },
        (SD::CorePin, MP::NuttxArm, ML::Rust) => Exec {
            resolver: || build_nuttx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: NuttxQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: core pin",
            accept: NUTTX_CORE_PIN_MARKER,
            fallback: Some(NUTTX_CORE_PIN_FALLBACK_MARKER),
            shape: AcceptOrFallback,
            note: "NuttX SMP affinity applied, or the image lacks CONFIG_SMP",
        },
        (SD::CorePin, MP::ThreadxLinux, ML::Rust) => Exec {
            resolver: || build_threadx_workspace_rust_realtime_entry().map(|p| p.to_path_buf()),
            boot: Native(NativeEnv::RustLogOnly),
            router: Router::Baked("127.0.0.1"),
            timeout_secs: 30,
            stem: "nros: core pin",
            accept: THREADX_CORE_PIN_MARKER,
            fallback: Some(THREADX_CORE_PIN_FALLBACK_MARKER),
            shape: AcceptOrFallback,
            note: "ThreadX SMP core exclusion applied, or the image lacks SMP",
        },
        (SD::CorePin, MP::FreertosMps2, ML::Cpp) => Exec {
            resolver: || build_freertos_workspace_cpp_realtime_entry().map(|p| p.to_path_buf()),
            boot: FreertosQemu,
            router: Router::Baked("0.0.0.0"),
            timeout_secs: 90,
            stem: "nros: core pin",
            accept: FREERTOS_CORE_PIN_MARKER,
            fallback: Some(FREERTOS_CORE_PIN_FALLBACK_MARKER),
            shape: AcceptOrFallback,
            note: "configUSE_CORE_AFFINITY build, or the uniprocessor fallback (W5.11)",
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
            shape: AcceptOrFallback,
            note: "W5.9: SCHED_SPORADIC applied, or the honest CONFIG_SCHED_SPORADIC-absent note",
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
fn lang_str(l: ML) -> &'static str {
    match l {
        ML::Rust => "rust",
        ML::C => "c",
        ML::Cpp => "cpp",
        ML::Mixed => "mixed",
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
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!("{:?}/{}/{}", c.dim, plat_str(c.platform), lang_str(c.lang));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_cell(c)));
        if let Err(p) = res {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            if msg.contains("[SKIPPED]") {
                skipped.push(format!("{label}: {msg}"));
            } else {
                failed.push(format!("{label}: {msg}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    assert!(
        failed.is_empty(),
        "sched_dims: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        cells.len(),
        failed.join("\n  ")
    );
    if skipped.len() == cells.len() {
        nros_tests::skip!(
            "all {} sched-dim cell(s) skipped:\n  {}",
            skipped.len(),
            skipped.join("\n  ")
        );
    }
}

/// Boot one sched-dim cell and assert its dim is honored per the [`Shape`].
/// Panics with `[SKIPPED] …` on an unmet precondition; the caller classifies.
fn run_cell(cell: &SchedCell) {
    let platform = plat_str(cell.platform);
    let lang = lang_str(cell.lang);
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
        Shape::AcceptOrFallback => {
            let fb = ex.fallback.map(|f| log.contains(f)).unwrap_or(false);
            assert!(accepted || fb, "{silence}{}\nlog:\n{log}", fail_loud());
        }
        Shape::AcceptOnly => {
            assert!(
                accepted,
                "{silence}[{platform} {lang} {:?}] expected the ACCEPT marker (`{}`); saw \
                 fallback? {}\nlog:\n{log}",
                cell.dim, ex.accept, ex.note
            );
        }
        Shape::StrictCountOne => {
            let hits = nros_tests::count_pattern(&log, ex.accept);
            assert_eq!(
                hits, 1,
                "{silence}[{platform} {lang} {:?}] expected exactly 1 `{}` (the single \
                 declaring tier), saw {hits}. {}\nlog:\n{log}",
                cell.dim, ex.accept, ex.note
            );
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
        }
    }
}
