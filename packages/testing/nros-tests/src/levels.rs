//! Verdicts read off the console of an executor-levels image (RFC-0108).
//!
//! Each function takes the image's whole console output and returns `Err`
//! naming the first thing it did not find, so a test can print it beside the
//! console. They are pure text functions, unit-tested here, and shared by
//! `realtime_sched_tasks_e2e` (phase-509) and `rt_sched_levels_e2e` (phase-162).

/// The text between each `[<node>] begin=N` and the `[<node>] end=N` that
/// closes it, for every complete pair in `console` (phase-509 P9).
pub fn callback_runs<'a>(console: &'a str, node: &str) -> Vec<&'a str> {
    let begin = format!("[{node}] begin=");
    let mut runs = Vec::new();
    let mut at = 0;
    while let Some(b) = console[at..].find(&begin) {
        let start = at + b + begin.len();
        let n: String = console[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if n.is_empty() {
            at = start;
            continue;
        }
        let end = format!("[{node}] end={n}");
        match console[start..].find(&end) {
            Some(e) => {
                runs.push(&console[start + n.len()..start + e]);
                at = start + e + end.len();
            }
            None => break,
        }
    }
    runs
}

/// phase-509 P9 -- the preemption order of `levels-cpp`'s three levels: a more
/// urgent level's marker inside a less urgent level's `begin=`/`end=` pair,
/// and never the reverse.
pub fn level_order_verdict(console: &str) -> Result<(), String> {
    let middle = callback_runs(console, "middle");
    let lazy = callback_runs(console, "lazy");
    if middle.len() < 2 || lazy.len() < 2 {
        return Err(format!(
            "too few complete callbacks to judge ({} middle, {} lazy)",
            middle.len(),
            lazy.len()
        ));
    }
    if !middle.iter().any(|r| r.contains("[urgent] tick=")) {
        return Err("level 0 (urgent) never preempted level 1 (middle)".into());
    }
    if !lazy.iter().any(|r| r.contains("[urgent] tick=")) {
        return Err("level 0 (urgent) never preempted level 2 (lazy)".into());
    }
    if !lazy.iter().any(|r| r.contains("[middle] begin=")) {
        return Err("level 1 (middle) never preempted level 2 (lazy)".into());
    }
    if let Some(r) = middle.iter().find(|r| r.contains("[lazy] ")) {
        return Err(format!(
            "level 2 (lazy) ran inside a level-1 (middle) callback: {r:?}"
        ));
    }
    Ok(())
}

/// The value of `key=` in `line`, as an integer.
fn field(line: &str, key: &str) -> Option<u64> {
    let at = line.find(&format!("{key}="))? + key.len() + 1;
    line[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// phase-162 W2 -- the S1 attitude loop's deadline misses, summed over the
/// run. Each `[attitude] window=W ticks=T misses=M worst_gap_us=G` line is
/// one window; `misses` counts release gaps longer than period plus deadline,
/// a lower bound on deadline misses (`attitude_pkg`'s Attitude.cpp).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttitudeRun {
    /// Complete windows the console reported.
    pub windows: u64,
    /// Releases in those windows.
    pub ticks: u64,
    /// Release gaps longer than period plus deadline.
    pub misses: u64,
    /// The longest gap between two releases, in microseconds.
    pub worst_gap_us: u64,
}

/// Sum every `[<node>] window=` line in `console` into an [`AttitudeRun`].
/// `node` is the launch name (`attitude`, or `attitude_flat`): the class
/// prints the name its launch file gave it.
pub fn attitude_run(console: &str, node: &str) -> AttitudeRun {
    let tag = format!("[{node}] window=");
    let mut run = AttitudeRun::default();
    for line in console.lines().filter(|l| l.starts_with(&tag)) {
        let (Some(ticks), Some(misses), Some(gap)) = (
            field(line, "ticks"),
            field(line, "misses"),
            field(line, "worst_gap_us"),
        ) else {
            continue;
        };
        run.windows += 1;
        run.ticks += ticks;
        run.misses += misses;
        run.worst_gap_us = run.worst_gap_us.max(gap);
    }
    run
}

/// phase-162 W1 -- one `[<node>] tick=N cpu_us=C wall_us=W max_gap_us=G` line
/// of `load_pkg`: the CPU one release burned, the wall time it took, and the
/// longest stretch it was off the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadTick {
    /// The release's index.
    pub tick: u64,
    /// CPU burned, in microseconds.
    pub cpu_us: u64,
    /// Wall time the burn took, in microseconds.
    pub wall_us: u64,
    /// The longest stretch off the CPU, in microseconds.
    pub max_gap_us: u64,
}

/// Every `[<node>] tick=` line `load_pkg` printed under the launch name
/// `node`, in order.
pub fn load_ticks(console: &str, node: &str) -> Vec<LoadTick> {
    let tag = format!("[{node}] tick=");
    console
        .lines()
        .filter(|l| l.starts_with(&tag))
        .filter_map(|l| {
            Some(LoadTick {
                tick: field(l, "tick")?,
                cpu_us: field(l, "cpu_us")?,
                wall_us: field(l, "wall_us")?,
                max_gap_us: field(l, "max_gap_us")?,
            })
        })
        .collect()
}

/// phase-162 W1 -- did the kernel throttle a level burning more CPU per
/// release than its `budget_us` per `period_us` reservation? A throttled
/// release waits off the CPU for a refill, the rest of a period; a release the
/// kernel never throttles never leaves the CPU. Every release after the first
/// (whose reservation may start mid-period) must wait for at least a quarter
/// of a refill.
///
/// Nothing here judges wall time. Issue 2066 judged its MEAN; issue 2797
/// retired that, because no wall-time floor holds once the clock decides how
/// many refills a burn needs ([`deadline_burn_margin`]).
pub fn throttle_verdict(ticks: &[LoadTick], budget_us: u64, period_us: u64) -> Result<(), String> {
    let judged: Vec<&LoadTick> = ticks.iter().skip(1).collect();
    if judged.len() < 3 {
        return Err(format!(
            "too few releases to judge ({} after the first)",
            judged.len()
        ));
    }
    let wait_us = period_us - budget_us;
    let free: Vec<&&LoadTick> = judged
        .iter()
        .filter(|t| t.max_gap_us < wait_us / 4)
        .collect();
    if let Some(first) = free.first() {
        return Err(format!(
            "{} of {} releases were never off the CPU for a quarter of a replenishment \
             wait ({} us), e.g. release {} (longest {} us): it did not throttle",
            free.len(),
            judged.len(),
            wait_us,
            first.tick,
            first.max_gap_us
        ));
    }
    Ok(())
}

/// Issue 2797 -- can a burn of `burn_us` escape a `budget_us` reservation on a
/// CPU whose clock ranges from `min_khz` to `max_khz`?
///
/// Linux charges a SCHED_DEADLINE budget as WORK, not CPU time: without
/// `SCHED_FLAG_RECLAIM`, `update_curr_dl_se` scales every slice it charges by
/// `arch_scale_freq_capacity`, the CPU's current clock against its maximum. So
/// a burn is charged at least `burn_us * min_khz / max_khz`. A release reaches
/// at most two budgets without waiting: what is left of the current one, plus
/// one immediate refill when its deadline has already passed (the kernel's
/// "lagging" replenishment). A burn charged more than that throttles at any
/// clock; one charged less can run through on a clocked-down CPU.
///
/// Measured on a 24-CPU host (0.8 to 6.8 GHz, the `powersave` governor) with a
/// 10 ms burn against 2 ms per 10 ms, reading the thread's own
/// `se.sum_exec_runtime`, `dl.runtime`, `dl.deadline` and its CPU's
/// `scaling_cur_freq` around each release. Over 224 releases, the 52 that never
/// left the CPU ran at a median 2.27 GHz and were charged a median 0.26 of their
/// CPU time. The ones that throttled ran at 4.70 GHz and were charged 0.71. The
/// lowest charge was 0.13, the hardware's own floor (0.118).
///
/// `Err` names the burn the host needs; `min_khz == 0` (no cpufreq: no
/// frequency invariance either) is a clock that never moves.
pub fn deadline_burn_margin(
    burn_us: u64,
    budget_us: u64,
    min_khz: u64,
    max_khz: u64,
) -> Result<(), String> {
    let (min_khz, max_khz) = if min_khz == 0 || max_khz == 0 {
        (1, 1)
    } else {
        (min_khz, max_khz)
    };
    // burn * min / max > 2 * budget, without the division.
    if burn_us * min_khz > 2 * budget_us * max_khz {
        Ok(())
    } else {
        Err(format!(
            "a {burn_us} us burn is charged as little as {} us of a {budget_us} us budget \
             on a CPU clocked from {min_khz} to {max_khz} kHz, which two budgets cover, so \
             the kernel may never throttle it: the burn must exceed {} us",
            burn_us * min_khz / max_khz,
            2 * budget_us * max_khz / min_khz
        ))
    }
}

/// How many times the one-level negative control's miss RATE the S1 run may
/// reach on a kernel without `PREEMPT_RT`. Measured on a generic 6.8 kernel:
/// the control misses 128-181 of 7,000 releases (~2 %) and S1 0-5 of 59,000
/// (<0.01 %), so S1 fails only past ~20x what it measured, and a level that
/// let telemetry run in front of the critical loop (the control's rate) fails
/// by 50x.
pub const S1_GENERIC_KERNEL_RATE_DIVISOR: u64 = 50;

/// Whether the running kernel is `PREEMPT_RT` (`/sys/kernel/realtime` is `1`).
pub fn kernel_is_preempt_rt() -> bool {
    std::fs::read_to_string("/sys/kernel/realtime").is_ok_and(|s| s.trim() == "1")
}

/// phase-162 W2 -- RFC-0002 S1's verdict on the two-level `run` against the
/// one-level `control`.
///
/// On a `PREEMPT_RT` kernel the critical loop on its own level misses no
/// deadline. A generic kernel cannot promise that to ANY program: a bare C
/// loop at the same priority, on the same CPU, with nothing else of ours
/// running, missed 2 of 60,000 releases on the host this was written on. So
/// there the claim is the part the levels own: telemetry no longer runs in
/// front of the critical loop, which shows as a miss rate at least
/// [`S1_GENERIC_KERNEL_RATE_DIVISOR`] times below the control's.
pub fn s1_verdict(
    run: &AttitudeRun,
    control: &AttitudeRun,
    preempt_rt: bool,
) -> Result<(), String> {
    if preempt_rt {
        return if run.misses == 0 {
            Ok(())
        } else {
            Err(format!(
                "on a PREEMPT_RT kernel the critical loop missed {} deadlines on its own \
                 level ({run:?})",
                run.misses
            ))
        };
    }
    if control.ticks == 0 || control.misses == 0 {
        return Err(format!("the negative control saw no miss ({control:?})"));
    }
    // run.misses / run.ticks * divisor <= control.misses / control.ticks
    if run.misses * control.ticks * S1_GENERIC_KERNEL_RATE_DIVISOR <= control.misses * run.ticks {
        Ok(())
    } else {
        Err(format!(
            "on a generic kernel the critical loop's miss rate ({}/{}) is not {}x below \
             the one-level control's ({}/{}): telemetry still runs in front of it",
            run.misses, run.ticks, S1_GENERIC_KERNEL_RATE_DIVISOR, control.misses, control.ticks
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(ticks: u64, misses: u64) -> AttitudeRun {
        AttitudeRun {
            windows: ticks / 1000,
            ticks,
            misses,
            worst_gap_us: 0,
        }
    }

    #[test]
    fn the_s1_verdict_is_zero_misses_on_preempt_rt_and_a_rate_bound_elsewhere() {
        let control = att(7_000, 131);
        assert!(s1_verdict(&att(59_000, 0), &control, true).is_ok());
        assert!(s1_verdict(&att(59_000, 1), &control, true).is_err());
        // Measured on a generic kernel: 0-5 of 59,000.
        assert!(s1_verdict(&att(59_000, 5), &control, false).is_ok());
        // 131/7000 / 50 * 59000 = 22.08: 22 passes, 23 does not.
        assert!(s1_verdict(&att(59_000, 22), &control, false).is_ok());
        assert!(s1_verdict(&att(59_000, 23), &control, false).is_err());
        // Telemetry in front of the loop: the control's own rate.
        assert!(s1_verdict(&att(59_000, 1_100), &control, false).is_err());
        assert!(s1_verdict(&att(59_000, 0), &att(7_000, 0), false).is_err());
    }

    #[test]
    fn the_level_order_verdict_reads_preemption_off_the_console() {
        let good = "[lazy] begin=0\n[urgent] tick=1\n[middle] begin=0\n[urgent] tick=2\n\
                    [middle] end=0\n[lazy] end=0\n[middle] begin=1\n[urgent] tick=3\n\
                    [middle] end=1\n[lazy] begin=1\n[middle] begin=2\n[middle] end=2\n\
                    [lazy] end=1\n";
        assert_eq!(level_order_verdict(good), Ok(()));
        // One thread in registration order: nothing ever lands inside a run.
        let flat = "[lazy] begin=0\n[lazy] end=0\n[middle] begin=0\n[middle] end=0\n\
                    [urgent] tick=0\n[lazy] begin=1\n[lazy] end=1\n[middle] begin=1\n\
                    [middle] end=1\n";
        assert!(level_order_verdict(flat).is_err());
        // Inverted: the least urgent level runs inside a more urgent callback.
        let inverted = good.replace(
            "[middle] begin=2\n[middle] end=2",
            "[middle] begin=2\n[lazy] begin=9\n[middle] end=2",
        );
        assert!(level_order_verdict(&inverted).unwrap_err().contains("lazy"));
    }

    #[test]
    fn attitude_windows_are_summed_under_their_launch_name() {
        let console = "[attitude] window=0 ticks=1000 misses=0 worst_gap_us=1100\n\
                       [WARN] nros: noise\n\
                       [attitude_flat] window=0 ticks=1000 misses=9 worst_gap_us=5400\n\
                       [attitude] window=1 ticks=1000 misses=2 worst_gap_us=2300\n";
        assert_eq!(
            attitude_run(console, "attitude"),
            AttitudeRun {
                windows: 2,
                ticks: 2000,
                misses: 2,
                worst_gap_us: 2300
            }
        );
        assert_eq!(attitude_run(console, "attitude_flat").misses, 9);
        assert_eq!(attitude_run(console, "absent"), AttitudeRun::default());
    }

    fn tick(tick: u64, wall_us: u64, max_gap_us: u64) -> LoadTick {
        LoadTick {
            tick,
            cpu_us: 10_000,
            wall_us,
            max_gap_us,
        }
    }

    #[test]
    fn load_lines_parse_under_their_launch_name() {
        let console = "[load_capped] burn_us=10000\n\
                       [load_capped] tick=0 cpu_us=10000 wall_us=41000 max_gap_us=8000\n\
                       [load] tick=0 cpu_us=5000 wall_us=5000 max_gap_us=0\n";
        assert_eq!(
            load_ticks(console, "load_capped"),
            vec![tick(0, 41_000, 8_000)]
        );
        assert_eq!(load_ticks(console, "load").len(), 1);
    }

    #[test]
    fn a_burn_must_outrun_two_budgets_at_the_lowest_clock() {
        // The measured host: 0.8 to 6.8 GHz. 10 ms escapes a 2 ms budget (it
        // is charged as little as 1.18 ms), 80 ms cannot (9.4 ms).
        let err = deadline_burn_margin(10_000, 2_000, 800_000, 6_800_000).unwrap_err();
        assert!(err.contains("must exceed 34000 us"), "{err}");
        assert_eq!(
            deadline_burn_margin(80_000, 2_000, 800_000, 6_800_000),
            Ok(())
        );
        // No cpufreq: a fixed clock, so anything past two budgets throttles.
        assert_eq!(deadline_burn_margin(4_001, 2_000, 0, 0), Ok(()));
        assert!(deadline_burn_margin(4_000, 2_000, 0, 0).is_err());
    }

    #[test]
    fn the_throttle_verdict_tells_a_throttled_burn_from_a_free_one() {
        // 10 ms of CPU at 2 ms per 10 ms: four refills, ~40 ms of wall time,
        // and 8 ms waits between budgets.
        let throttled: Vec<LoadTick> = (0..5).map(|i| tick(i, 41_000, 8_000)).collect();
        assert_eq!(throttle_verdict(&throttled, 2_000, 10_000), Ok(()));
        // The executor fallback: the burn runs to completion on the CPU.
        let free: Vec<LoadTick> = (0..5).map(|i| tick(i, 10_000, 0)).collect();
        assert!(
            throttle_verdict(&free, 2_000, 10_000)
                .unwrap_err()
                .contains("did not throttle")
        );
        // Long enough, but never off the CPU for a refill wait.
        let preempted: Vec<LoadTick> = (0..5).map(|i| tick(i, 41_000, 600)).collect();
        assert!(throttle_verdict(&preempted, 2_000, 10_000).is_err());
        // Issue 2797 -- one free release in thirty is a release that did not
        // throttle: the burn is sized so that none can (`deadline_burn_margin`).
        let one_free: Vec<LoadTick> = (0..30)
            .map(|i| {
                if i == 13 {
                    tick(i, 11_000, 1_000)
                } else {
                    tick(i, 35_000, 8_000)
                }
            })
            .collect();
        assert!(
            throttle_verdict(&one_free, 2_000, 10_000)
                .unwrap_err()
                .contains("1 of 29 releases")
        );
        // Issue 2066 -- a tick-enforced kernel (HZ=250): one release in ten
        // overruns to 14 ms.
        let ticked: Vec<LoadTick> = (0..10)
            .map(|i| {
                if i == 6 {
                    tick(i, 14_000, 3_900)
                } else {
                    tick(i, 30_000, 16_000)
                }
            })
            .collect();
        assert_eq!(throttle_verdict(&ticked, 2_000, 10_000), Ok(()));
        // Issue 2797 -- one refill per release: a throttle, however short the
        // wall time.
        let one_refill: Vec<LoadTick> = (0..10).map(|i| tick(i, 14_000, 3_900)).collect();
        assert_eq!(throttle_verdict(&one_refill, 2_000, 10_000), Ok(()));
    }
}
