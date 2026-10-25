//! Real-time verification of executor levels on a Linux host (phase-162).
//!
//! Every test here needs the kernel to honour a level's scheduling: its
//! `SCHED_FIFO` priority (W1, W2) or its `SCHED_DEADLINE` budget (W1). Both need
//! `CAP_SYS_NICE`, which no lane has, so each test asks the lane's scope FIRST
//! (`rt-sched`, [`nros_tests::lane_scope::require_rt_sched_claimed`]) and is a
//! `[SKIPPED:lane]` everywhere that does not claim it. A run that claims it
//! and then finds the kernel refusing fails with `[UNMET PRECONDITION]`: on a
//! host without the capability the levels run at the default policy, and
//! these assertions would measure that instead.
//!
//! To run them, grant the capability to the three fixture binaries (it is
//! bound to the file, so again after every rebuild) and claim the scope:
//!
//! ```sh
//! sudo setcap cap_sys_nice+ep examples/workspaces/levels-cpp/build/posix-zenoh-linux/cmake/linux_entry
//! # ... and linux_s1_entry, linux_s1_flat_entry, linux_budget_entry
//! NROS_TEST_UNCLAIMED= cargo nextest run -p nros-tests --test rt_sched_levels_e2e
//! ```
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
    levels::{attitude_run, level_order_verdict, load_ticks, throttle_verdict},
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

/// Run `entry` against a fresh router for `spin_ms`, optionally pinned to
/// CPU 0, and return its whole console.
fn run_levels_image(entry: &Path, spin_ms: u64, pin_to_one_cpu: bool, label: &str) -> String {
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));
    let mut cmd = if pin_to_one_cpu {
        let mut c = Command::new("taskset");
        c.args(["-c", "0"]).arg(entry);
        c
    } else {
        Command::new(entry)
    };
    cmd.env("NROS_LOCATOR", router.locator())
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string());
    let mut guest = ManagedProcess::spawn_command(cmd, label)
        .unwrap_or_else(|e| nros_tests::unmet!("spawn {label} (taskset is util-linux): {e}"));
    let console = guest
        .wait_for_all_output(Duration::from_millis(spin_ms + 20_000))
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
             scheduling ({l}). Grant it: sudo setcap cap_sys_nice+ep {}",
            entry.display()
        );
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
             reservation ({l}). Grant it: sudo setcap cap_sys_nice+ep {}",
            entry.display()
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
    if let Err(e) = throttle_verdict(&ticks, CAPPED_BUDGET_US, CAPPED_PERIOD_US) {
        panic!("{e}\n--- console ---\n{console}");
    }
}

/// W2 -- RFC-0002 S1 on levels: the 1 kHz critical loop on its own level
/// misses no deadline in 60 s while S1's 5 ms telemetry runs on a less urgent
/// level of the same CPU. The negative control runs first: the same two nodes
/// on ONE level must miss, which shows the count can see a miss.
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
    assert!(
        run.windows >= 55,
        "S1 reported {} one-second windows in a 60 s run: the critical loop did \
         not keep its rate ({run:?})\n--- console ---\n{console}",
        run.windows
    );
    assert_eq!(
        run.misses, 0,
        "S1's critical loop missed deadlines on its own level ({run:?}); the \
         negative control missed {}\n--- console ---\n{console}",
        control.misses
    );
    assert!(
        !load_ticks(&console, "load").is_empty(),
        "the telemetry level never ran, so nothing competed for the CPU\n\
         --- console ---\n{console}"
    );
}
