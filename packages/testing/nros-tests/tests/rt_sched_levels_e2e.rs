//! Real-time verification of executor levels on a Linux host (phase-162).
//!
//! Every test here needs the kernel to honour a level's scheduling: its
//! `SCHED_FIFO` priority (W1, W2) or its `SCHED_DEADLINE` budget (W1). Both
//! need `CAP_SYS_NICE`, which only host-tests' `rt-sched` job claims, so each
//! test asks the lane's scope FIRST (`rt-sched`,
//! [`nros_tests::lane_scope::require_rt_sched_claimed`]) and is a
//! `[SKIPPED:lane]` everywhere that does not claim it. A run that claims it
//! and then finds the kernel refusing fails with `[UNMET PRECONDITION]`: on a
//! host without the capability the levels run at the default policy, and
//! these assertions would measure that instead.
//!
//! The capability comes from the launcher (`NROS_RT_SCHED_LAUNCHER`), not from
//! the host: by default each image runs in a throwaway docker container granted
//! `SYS_NICE`, so a member of the `docker` group, or a CI runner, needs no
//! `sudo`. Claim the scope to run them:
//!
//! ```sh
//! NROS_TEST_UNCLAIMED= cargo nextest run -p nros-tests --test rt_sched_levels_e2e
//! ```
//!
//! `NROS_RT_SCHED_LAUNCHER=direct` runs the binaries as they are, for a host
//! that grants the capability another way (`setcap cap_sys_nice+ep` on each
//! fixture, again after every rebuild).
//!
//! The preemption and S1 runs are pinned to one CPU (`taskset -c 0`): on a
//! multi-core host the levels' threads otherwise run in parallel, and neither
//! preemption nor a priority's effect on a deadline is observable. The budget
//! run is NOT pinned: `SCHED_DEADLINE` refuses a thread whose affinity is
//! narrower than its root domain.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter,
        build_native_workspace_cpp_levels_budget_entry, build_native_workspace_cpp_levels_entry,
        build_native_workspace_cpp_levels_s1_entry,
        build_native_workspace_cpp_levels_s1_flat_entry, lane::require_workspace_in_lane,
    },
    lane_scope::require_rt_sched_claimed,
    levels::{
        attitude_run, kernel_is_preempt_rt, level_order_verdict, load_ticks, s1_verdict,
        throttle_verdict,
    },
};
use std::{path::Path, process::Command, time::Duration};

/// `levels-cpp`'s `capped` tier (`[tiers.capped.posix]`): the reservation the
/// budget image's `load_capped` level runs under.
const CAPPED_BUDGET_US: u64 = 2_000;
const CAPPED_PERIOD_US: u64 = 10_000;

/// RFC-0002 S1's acceptance window: 60 s of the critical loop.
const S1_RUN_MS: u64 = 60_000;
/// The negative control needs only enough windows to show misses.
const S1_FLAT_RUN_MS: u64 = 10_000;

/// How an image gets the `CAP_SYS_NICE` its levels need, from
/// `NROS_RT_SCHED_LAUNCHER`.
///
/// - `docker` (the default): in a throwaway container started with
///   `--cap-add SYS_NICE --ulimit rtprio=99`. Membership of the `docker` group
///   is all it needs, so it runs on a CI runner with no `sudo`. The container
///   shares the host's network (the router is on loopback) and CPUs, and
///   mounts only the fixture's directory, read-only. Its image is
///   `NROS_RT_SCHED_IMAGE`, else the host's own distribution and release
///   (`ubuntu:22.04` on a jammy host) so the binary finds the libraries it was
///   linked against.
/// - `direct`: the binary as is, for a host that already grants the
///   capability (`setcap cap_sys_nice+ep`, or a root run).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Launcher {
    Docker,
    Direct,
}

const LAUNCHER_ENV: &str = "NROS_RT_SCHED_LAUNCHER";
const IMAGE_ENV: &str = "NROS_RT_SCHED_IMAGE";

fn launcher() -> Launcher {
    match std::env::var(LAUNCHER_ENV).as_deref() {
        Err(_) | Ok("") | Ok("docker") => Launcher::Docker,
        Ok("direct") => Launcher::Direct,
        Ok(other) => panic!("{LAUNCHER_ENV}={other}: expected `docker` or `direct`"),
    }
}

/// The container image matching this host's userland, read from
/// `/etc/os-release` (`ID` + `VERSION_ID`).
fn host_matching_image() -> String {
    if let Ok(image) = std::env::var(IMAGE_ENV)
        && !image.is_empty()
    {
        return image;
    }
    let release = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_else(|e| nros_tests::unmet!("read /etc/os-release to pick {IMAGE_ENV}: {e}"));
    let field = |key: &str| {
        release.lines().find_map(|l| {
            l.strip_prefix(key)
                .and_then(|v| v.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_string())
        })
    };
    match (field("ID").as_deref(), field("VERSION_ID")) {
        (Some(id @ ("ubuntu" | "debian")), Some(version)) => format!("{id}:{version}"),
        (id, version) => nros_tests::unmet!(
            "no container image matches this host ({id:?} {version:?}); set {IMAGE_ENV} to \
             one with the same C++ runtime, or {LAUNCHER_ENV}=direct after setcap"
        ),
    }
}

/// Removes the launcher's container when the run ends, however it ends:
/// killing the `docker run` client leaves its container running.
struct Container(Option<String>);

impl Drop for Container {
    fn drop(&mut self) {
        if let Some(name) = self.0.take() {
            let _ = Command::new("docker")
                .args(["rm", "-f", &name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

/// Run `entry` against a fresh router for `spin_ms`, optionally pinned to
/// CPU 0, and return its whole console.
fn run_levels_image(entry: &Path, spin_ms: u64, pin_to_one_cpu: bool, label: &str) -> String {
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));
    let env = [
        ("NROS_LOCATOR", router.locator()),
        ("NROS_SESSION_MODE", "client".to_string()),
        ("NROS_ENTRY_SPIN_MS", spin_ms.to_string()),
    ];
    let mut program: Vec<String> = Vec::new();
    if pin_to_one_cpu {
        let cpu = std::env::var("NROS_RT_SCHED_CPU").unwrap_or_else(|_| "0".into());
        program.extend(["taskset".into(), "-c".into(), cpu]);
    }
    program.push(entry.display().to_string());

    let (cmd, _container) = match launcher() {
        Launcher::Direct => {
            let mut c = Command::new(&program[0]);
            c.args(&program[1..]);
            for (k, v) in &env {
                c.env(k, v);
            }
            (c, Container(None))
        }
        Launcher::Docker => {
            let name = format!("nros-rt-{}-{label}", std::process::id());
            let dir = entry.parent().expect("a fixture binary has a directory");
            let mut c = Command::new("docker");
            c.args(["run", "--rm", "--init", "--name", &name])
                .args(["--cap-add", "SYS_NICE", "--ulimit", "rtprio=99"])
                .args(["--network", "host"])
                .arg("--volume")
                .arg(format!("{0}:{0}:ro", dir.display()));
            for (k, v) in &env {
                c.arg("--env").arg(format!("{k}={v}"));
            }
            c.arg(host_matching_image()).args(&program);
            (c, Container(Some(name)))
        }
    };
    let mut guest = ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| {
        nros_tests::unmet!(
            "spawn {label} ({LAUNCHER_ENV}: docker needs the docker CLI and daemon, \
             taskset is util-linux): {e}"
        )
    });
    let console = guest
        .wait_for_all_output(Duration::from_millis(spin_ms + 30_000))
        .unwrap_or_else(|e| format!("<no output: {e}>"));
    guest.kill();
    console
}

/// In scope, a kernel that refused the levels' real-time policy is a missing
/// precondition, not a result: the run measured the default policy.
fn require_kernel_accepted(console: &str, entry: &Path) {
    if let Some(l) = console.lines().find(|l| l.contains("was REFUSED (EPERM)")) {
        nros_tests::unmet!(
            "this lane claims rt-sched, but the kernel refused the levels' real-time \
             scheduling ({l}). {}",
            grant_hint(entry)
        );
    }
}

/// What to change when the kernel refused: the launcher, not the test.
fn grant_hint(entry: &Path) -> String {
    match launcher() {
        Launcher::Docker => format!(
            "The docker launcher passes --cap-add SYS_NICE; a daemon that cannot grant it \
             (rootless docker, a user-namespaced daemon) needs {LAUNCHER_ENV}=direct and \
             `setcap cap_sys_nice+ep {}`",
            entry.display()
        ),
        Launcher::Direct => format!(
            "Grant it: `setcap cap_sys_nice+ep {}`, or use {LAUNCHER_ENV}=docker",
            entry.display()
        ),
    }
}

fn fail_on_boot_failure(console: &str) {
    if let Some(l) = console
        .lines()
        .find(|l| l.contains("failing the boot") || l.contains("setup failed"))
    {
        panic!("a level failed the boot: {l}\n--- console ---\n{console}");
    }
}

/// W1 -- on one CPU, under SCHED_FIFO, a more urgent level preempts a less
/// urgent one mid-callback and never the reverse: the order the levels cells
/// already prove on Zephyr (phase-509 P9), on the Linux runner, where the
/// unprivileged levels cell can only show that every level runs.
#[test]
fn linux_levels_preempt_in_rank_order_on_one_cpu() {
    require_rt_sched_claimed();
    require_workspace_in_lane("workspace-cpp-native-levels")
        .require("native levels-cpp workspace entry");
    let entry = build_native_workspace_cpp_levels_entry().require("native levels-cpp entry");
    let console = run_levels_image(entry, 4_000, true, "levels-entry");
    require_kernel_accepted(&console, entry);
    fail_on_boot_failure(&console);
    if let Err(e) = level_order_verdict(&console) {
        panic!("levels-cpp on one CPU under SCHED_FIFO: {e}\n--- console ---\n{console}");
    }
}

/// W1 -- a spawned level past its SCHED_DEADLINE budget is throttled by the
/// KERNEL: the runner hands the budget to the kernel (one enforcer per level,
/// issue 2018), and each 10 ms burn against 2 ms per 10 ms takes several
/// periods of wall time, most of it off the CPU waiting for a refill.
#[test]
fn linux_level_budget_overrun_is_throttled_by_the_kernel() {
    require_rt_sched_claimed();
    require_workspace_in_lane("workspace-cpp-native-levels-budget")
        .require("native levels-cpp budget entry");
    let entry =
        build_native_workspace_cpp_levels_budget_entry().require("native levels-cpp budget entry");
    let console = run_levels_image(entry, 3_000, false, "levels-budget-entry");
    require_kernel_accepted(&console, entry);
    if let Some(l) = console
        .lines()
        .find(|l| l.contains("deadline budget FAILED tier=`capped`"))
    {
        nros_tests::unmet!(
            "this lane claims rt-sched, but the kernel refused the SCHED_DEADLINE \
             reservation ({l}). {}",
            grant_hint(entry)
        );
    }
    fail_on_boot_failure(&console);
    for marker in [
        "nros: deadline budget set tier=`capped` 2000us/10000us",
        "nros: level budget tier=`capped` 2000us/10000us enforced by the KERNEL",
    ] {
        assert!(
            console.contains(marker),
            "`{marker}` absent: the kernel does not hold the level's budget\n\
             --- console ---\n{console}"
        );
    }
    let ticks = load_ticks(&console, "load_capped");
    eprintln!("budget: load_capped ticks {ticks:?}");
    if let Err(e) = throttle_verdict(&ticks, CAPPED_BUDGET_US, CAPPED_PERIOD_US) {
        panic!("{e}\n--- console ---\n{console}");
    }
}

/// W2 -- RFC-0002 S1 on levels: the 1 kHz critical loop on its own level
/// keeps its deadlines for 60 s while S1's 5 ms telemetry runs on a less
/// urgent level of the same CPU -- every one on a PREEMPT_RT kernel, and on a
/// generic kernel all but the host's own floor ([`s1_verdict`]). The negative
/// control runs first: the same two nodes on ONE level must miss, which shows
/// the count can see a miss.
#[test]
fn s1_critical_loop_meets_every_deadline_on_its_own_level() {
    require_rt_sched_claimed();
    require_workspace_in_lane("workspace-cpp-native-levels-s1")
        .require("native levels-cpp S1 entry");

    let flat = build_native_workspace_cpp_levels_s1_flat_entry()
        .require("native levels-cpp S1 negative-control entry");
    let console = run_levels_image(flat, S1_FLAT_RUN_MS, true, "levels-s1-flat-entry");
    require_kernel_accepted(&console, flat);
    fail_on_boot_failure(&console);
    let control = attitude_run(&console, "attitude_flat");
    eprintln!("S1 negative control (one level): {control:?}");
    assert!(
        control.windows >= 5 && control.misses >= 5,
        "negative control: one level holding both nodes should miss on most \
         telemetry releases, and reported {control:?}. A count that cannot see a \
         miss here proves nothing below.\n--- console ---\n{console}"
    );

    let s1 = build_native_workspace_cpp_levels_s1_entry().require("native levels-cpp S1 entry");
    let console = run_levels_image(s1, S1_RUN_MS, true, "levels-s1-entry");
    require_kernel_accepted(&console, s1);
    fail_on_boot_failure(&console);
    let run = attitude_run(&console, "attitude");
    eprintln!("S1 (two levels): {run:?}");
    assert!(
        run.windows >= 55,
        "S1 reported {} one-second windows in a 60 s run: the critical loop did \
         not keep its rate ({run:?})\n--- console ---\n{console}",
        run.windows
    );
    if let Err(e) = s1_verdict(&run, &control, kernel_is_preempt_rt()) {
        panic!("S1 ({run:?}, control {control:?}): {e}\n--- console ---\n{console}");
    }
    assert!(
        !load_ticks(&console, "load").is_empty(),
        "the telemetry level never ran, so nothing competed for the CPU\n\
         --- console ---\n{console}"
    );
}
