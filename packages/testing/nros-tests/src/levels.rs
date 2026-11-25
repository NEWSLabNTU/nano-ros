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

/// phase-162 W1 -- one `[<node>] tick=N cpu_us=C wall_us=W max_gap_us=G
/// max_run_us=R` line of `load_pkg`: the CPU one release burned, the wall time
/// it took, the longest stretch it was off the CPU, and the longest it held the
/// CPU in one piece.
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
    /// The longest stretch ON the CPU without a step off it, in microseconds.
    pub max_run_us: u64,
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
                max_run_us: field(l, "max_run_us")?,
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

/// The critical loop's period plus its deadline (`attitude_pkg`'s
/// Attitude.cpp): a release gap longer than this is the loop's miss.
pub const S1_LOOP_MISS_GAP_US: u64 = 2_000;

/// Issue 2965 -- can telemetry's burn see every preemption the critical loop
/// makes? `load_pkg` ends an on-CPU stretch only at a clock step longer than
/// its `step_us`; a preemption shorter than that reads as CPU time, and a
/// correct S1 would read as a leak. The loop takes the CPU for its `work_us`
/// plus two context switches, so the floor must sit well below it: at most
/// half. Both values come from the images' own start lines (`[<load>]
/// burn_us=B step_us=S`, `[<attitude>] work_us=W miss_gap_us=G`), and `G`
/// must be the gap [`s1_verdict`] judges against.
pub fn s1_detection_floor(console: &str, attitude: &str, load: &str) -> Result<(), String> {
    let start = |node: &str, key: &str| {
        let tag = format!("[{node}] ");
        console
            .lines()
            .filter(|l| l.starts_with(&tag))
            .find_map(|l| field(l, key))
            .ok_or_else(|| format!("`[{node}]` printed no `{key}=` start line"))
    };
    let step_us = start(load, "step_us")?;
    let work_us = start(attitude, "work_us")?;
    let gap_us = start(attitude, "miss_gap_us")?;
    if gap_us != S1_LOOP_MISS_GAP_US {
        return Err(format!(
            "the loop's miss gap is {gap_us} us, and the verdict judges against \
             {S1_LOOP_MISS_GAP_US} us"
        ));
    }
    if 2 * step_us > work_us {
        return Err(format!(
            "telemetry's burn ends a stretch only at a step over {step_us} us, which is not \
             well below the loop's {work_us} us callback: a preemption could read as CPU time"
        ));
    }
    Ok(())
}

/// Whether the running kernel is `PREEMPT_RT` (`/sys/kernel/realtime` is `1`).
pub fn kernel_is_preempt_rt() -> bool {
    std::fs::read_to_string("/sys/kernel/realtime").is_ok_and(|s| s.trim() == "1")
}

/// phase-162 W2 -- RFC-0002 S1's verdict: the two-level run's critical loop
/// `run` and its telemetry releases `telemetry`, against the one-level
/// control's telemetry `control`.
///
/// The claim the levels own is that telemetry no longer runs in front of the
/// critical loop. Telemetry's own console line says whether it did, per
/// release: `max_run_us` is the longest stretch it held the CPU without a step
/// off it. With the loop on a more urgent level of the same CPU, the loop
/// preempts telemetry at every release, so no stretch reaches the loop's miss
/// gap ([`S1_LOOP_MISS_GAP_US`]). A stretch that does is a moment the loop was
/// overdue and telemetry was on the CPU instead: a miss telemetry caused. A
/// host stall cannot produce one, because it takes the CPU from telemetry too,
/// and that is a step off it. A step counts once it is longer than
/// telemetry's detection floor, which [`s1_detection_floor`] holds to at most
/// half the loop's callback, so every preemption the loop makes ends a stretch.
/// So:
///
/// - most control releases must show such a stretch (there telemetry runs its
///   5 ms burn in one piece; a host stall can split a release, so not every
///   one), which shows the measure can see telemetry in front;
/// - no S1 release may show one. The bound tolerates no leaked release.
///
/// The loop's own miss count judges only on a `PREEMPT_RT` kernel, where it
/// must be zero. On a generic kernel it is the host's floor: a bare C loop at
/// the same priority missed 2 of 60,000 releases on a quiet 6.8 host, and a
/// GitHub-hosted VM stalled S1's loop 55 times in 60 s in bursts, up to 7.2 ms,
/// longer than any gap telemetry causes (issue 2965). The rule before this
/// compared that count with the control's over a fixed 50x divisor; the
/// control's rate is set by telemetry and S1's by the host, so the ratio
/// measured the host, and it failed a correct image on a noisy VM.
pub fn s1_verdict(
    run: &AttitudeRun,
    telemetry: &[LoadTick],
    control: &[LoadTick],
    preempt_rt: bool,
) -> Result<(), String> {
    let in_front = |t: &&LoadTick| t.max_run_us > S1_LOOP_MISS_GAP_US;
    let seen = control.iter().filter(in_front).count();
    if control.is_empty() || 2 * seen <= control.len() {
        return Err(format!(
            "the negative control's telemetry held the CPU past the loop's miss gap \
             ({S1_LOOP_MISS_GAP_US} us) in {seen} of {} releases: with telemetry in front \
             of the loop most of them must, so this measure cannot see it",
            control.len()
        ));
    }
    if telemetry.is_empty() {
        return Err("S1's telemetry printed no release, so nothing competed for the CPU".into());
    }
    let leaked: Vec<String> = telemetry
        .iter()
        .filter(in_front)
        .map(|t| format!("tick {} ran {} us", t.tick, t.max_run_us))
        .collect();
    if !leaked.is_empty() {
        return Err(format!(
            "telemetry held the CPU past the loop's miss gap ({S1_LOOP_MISS_GAP_US} us) in {} \
             of {} releases, so it ran in front of the critical loop: {}",
            leaked.len(),
            telemetry.len(),
            leaked.join(", ")
        ));
    }
    if preempt_rt && run.misses != 0 {
        return Err(format!(
            "on a PREEMPT_RT kernel the critical loop missed {} deadlines on its own level \
             ({run:?})",
            run.misses
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(misses: u64) -> AttitudeRun {
        AttitudeRun {
            windows: 59,
            ticks: 59_000,
            misses,
            worst_gap_us: 0,
        }
    }

    fn telemetry(runs: &[u64]) -> Vec<LoadTick> {
        runs.iter()
            .enumerate()
            .map(|(i, &max_run_us)| LoadTick {
                tick: i as u64,
                cpu_us: 5_000,
                wall_us: 7_150,
                max_gap_us: 310,
                max_run_us,
            })
            .collect()
    }

    #[test]
    fn the_s1_verdict_reads_whether_telemetry_ever_held_the_cpu_past_a_miss() {
        // One level: telemetry's 5 ms burn runs in one piece.
        let control = telemetry(&[5_000; 80]);
        // Two levels: the loop preempts it every 1 ms.
        let s1 = telemetry(&[700; 600]);

        assert!(s1_verdict(&att(0), &s1, &control, true).is_ok());
        assert!(s1_verdict(&att(1), &s1, &control, true).is_err());
        // A generic kernel's host floor, bursty misses included (issue 2965:
        // 55 of 59,000 on a GitHub-hosted VM), is not telemetry's.
        assert!(s1_verdict(&att(55), &s1, &control, false).is_ok());

        // ONE leaked release fails it, on either kernel.
        let mut leak = s1.clone();
        leak[300].max_run_us = 5_000;
        let e = s1_verdict(&att(0), &leak, &control, false).unwrap_err();
        assert!(e.contains("tick 300 ran 5000 us"), "{e}");
        // The bound is the loop's miss gap: a stretch AT it is no miss.
        leak[300].max_run_us = S1_LOOP_MISS_GAP_US;
        assert!(s1_verdict(&att(0), &leak, &control, false).is_ok());

        // A host stall split some control releases: still visible.
        let mut split = control.clone();
        split[..30].iter_mut().for_each(|t| t.max_run_us = 1_900);
        assert!(s1_verdict(&att(0), &s1, &split, false).is_ok());
        // A control whose telemetry the loop preempted sees nothing.
        assert!(s1_verdict(&att(0), &s1, &s1, false).is_err());
        assert!(s1_verdict(&att(0), &s1, &[], false).is_err());
        assert!(s1_verdict(&att(0), &[], &control, false).is_err());
    }

    #[test]
    fn telemetry_sees_every_preemption_the_loop_makes() {
        let good = "[load] burn_us=5000 step_us=100\n[attitude] work_us=300 miss_gap_us=2000\n";
        assert_eq!(s1_detection_floor(good, "attitude", "load"), Ok(()));
        // The floor before issue 2965: 200 us against a 300 us callback.
        let coarse = good.replace("step_us=100", "step_us=200");
        assert!(s1_detection_floor(&coarse, "attitude", "load").is_err());
        let other_gap = good.replace("miss_gap_us=2000", "miss_gap_us=3000");
        assert!(s1_detection_floor(&other_gap, "attitude", "load").is_err());
        // An image older than the start lines.
        assert!(s1_detection_floor("[load] burn_us=5000\n", "attitude", "load").is_err());
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
            max_run_us: 2_000,
            tick,
            cpu_us: 10_000,
            wall_us,
            max_gap_us,
        }
    }

    #[test]
    fn load_lines_parse_under_their_launch_name() {
        let console = "[load_capped] burn_us=10000\n\
                       [load_capped] tick=0 cpu_us=10000 wall_us=41000 max_gap_us=8000 max_run_us=2000\n\
                       [load] tick=0 cpu_us=5000 wall_us=5000 max_gap_us=0 max_run_us=5000\n\
                       [load] tick=1 cpu_us=5000 wall_us=5000 max_gap_us=0\n";
        assert_eq!(
            load_ticks(console, "load_capped"),
            vec![tick(0, 41_000, 8_000)]
        );
        // A line without `max_run_us` (an image older than the S1 verdict) is
        // not read as a release.
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
