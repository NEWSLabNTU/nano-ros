//! Diagnostics — data-driven rules over a [`BuildProfile`] that emit at most one
//! actionable hint each. Each rule is independent and individually suppressible;
//! the reporter renders the collected hints. Profile-only rules are unit-testable
//! without touching the filesystem; environment rules read a [`Context`].

use std::collections::BTreeMap;

use crate::model::{BuildProfile, Kind};

/// Host context for environment-aware rules (job count, RAM). Detected from the
/// environment in production; constructed directly in tests.
#[derive(Debug, Default, Clone)]
pub struct Context {
    /// `NROS_BUILD_JOBS` (or an equivalent) if set.
    pub jobs: Option<u64>,
    /// Total system RAM in KiB (from `/proc/meminfo`) if available.
    pub mem_total_kb: Option<u64>,
}

impl Context {
    /// Read job count + RAM from the environment. Best-effort; missing values
    /// just disable the corresponding rule.
    pub fn detect() -> Self {
        let jobs = std::env::var("NROS_BUILD_JOBS")
            .ok()
            .and_then(|v| v.trim().parse().ok());
        let mem_total_kb = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| parse_memtotal_kb(&s));
        Context { jobs, mem_total_kb }
    }
}

fn parse_memtotal_kb(meminfo: &str) -> Option<u64> {
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

/// Run all rules and return the hints (in a stable order).
pub fn run(profile: &BuildProfile, ctx: &Context) -> Vec<String> {
    [
        dominant_unit(profile),
        cold_c_build(profile),
        shared_crate_recompiled(profile),
        job_count_vs_ram(ctx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Headline: the single slowest unit and its share of its stage.
fn dominant_unit(p: &BuildProfile) -> Option<String> {
    let slowest = p
        .units
        .iter()
        .max_by(|a, b| a.dur_s.total_cmp(&b.dur_s))?;
    if slowest.dur_s <= 0.0 {
        return None;
    }
    let stage_dur: f64 = p
        .units
        .iter()
        .filter(|u| u.kind == slowest.kind)
        .map(|u| u.dur_s)
        .sum();
    if stage_dur <= 0.0 {
        return None;
    }
    let pct = (slowest.dur_s / stage_dur * 100.0).round() as u32;
    // Only worth flagging when it actually dominates its stage.
    if pct < 40 {
        return None;
    }
    Some(format!(
        "1 unit = {pct}% of {} ({}, {:.1}s)",
        slowest.kind.name(),
        slowest.name,
        slowest.dur_s
    ))
}

/// A large native (C/C++/-sys) unit with no incremental → suggest a compiler cache.
fn cold_c_build(p: &BuildProfile) -> Option<String> {
    let compile: f64 = p
        .units
        .iter()
        .filter(|u| u.kind == Kind::Compile)
        .map(|u| u.dur_s)
        .sum();
    if compile <= 0.0 {
        return None;
    }
    let biggest_native = p
        .units
        .iter()
        .filter(|u| u.is_native && u.kind == Kind::Compile)
        .max_by(|a, b| a.dur_s.total_cmp(&b.dur_s))?;
    if biggest_native.dur_s / compile < 0.5 {
        return None;
    }
    Some(format!(
        "{} ({:.1}s, native, no incremental) dominates compile \u{2014} enable a compiler cache (sccache/ccache) for warm rebuilds",
        biggest_native.name, biggest_native.dur_s
    ))
}

/// The same unit name timed more than once → likely a shared crate rebuilt per
/// isolated `target/`; suggest pooling `target_dir` (phase-226 pattern).
fn shared_crate_recompiled(p: &BuildProfile) -> Option<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for u in &p.units {
        *counts.entry(u.name.as_str()).or_default() += 1;
    }
    let (name, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < 2 {
        return None;
    }
    Some(format!(
        "{name} compiled {n}\u{00d7} \u{2014} examples use isolated target/; pool target_dir to reuse the build"
    ))
}

/// Warn when the configured parallelism risks RAM exhaustion (issue #57), or
/// confirm it is within budget. Heuristic: ~1.5 GiB headroom per concurrent job.
fn job_count_vs_ram(ctx: &Context) -> Option<String> {
    let jobs = ctx.jobs?;
    let mem_kb = ctx.mem_total_kb?;
    if jobs == 0 {
        return None;
    }
    let gib = mem_kb as f64 / 1024.0 / 1024.0;
    let per_job = gib / jobs as f64;
    if per_job < 1.5 {
        Some(format!(
            "jobs={jobs} on {gib:.1} GiB = {per_job:.1} GiB/job \u{2014} OOM risk (issue #57); lower NROS_BUILD_JOBS"
        ))
    } else {
        Some(format!("jobs={jobs} within RAM budget ({per_job:.1} GiB/job)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Backend, Stage, Unit};

    fn unit(name: &str, kind: Kind, dur_s: f64, is_native: bool) -> Unit {
        Unit {
            name: name.to_string(),
            kind,
            dur_s,
            is_native,
        }
    }

    fn profile(units: Vec<Unit>) -> BuildProfile {
        BuildProfile {
            backend: Backend::Cargo,
            total_s: units.iter().map(|u| u.dur_s).sum(),
            stages: Vec::<Stage>::new(),
            units,
            captured_deep: true,
            notes: Vec::new(),
        }
    }

    #[test]
    fn cold_c_build_fires_on_dominant_native_unit() {
        let p = profile(vec![
            unit("zenoh-pico-sys", Kind::Compile, 18.0, true),
            unit("nros-node", Kind::Compile, 2.0, false),
        ]);
        let h = cold_c_build(&p).expect("fires");
        assert!(h.contains("zenoh-pico-sys"));
        assert!(h.contains("compiler cache"));
    }

    #[test]
    fn cold_c_build_silent_when_native_is_minor() {
        let p = profile(vec![
            unit("small-sys", Kind::Compile, 1.0, true),
            unit("nros-node", Kind::Compile, 19.0, false),
        ]);
        assert!(cold_c_build(&p).is_none());
    }

    #[test]
    fn shared_crate_recompiled_counts_duplicates() {
        let p = profile(vec![
            unit("nros-c", Kind::Compile, 2.0, false),
            unit("nros-c", Kind::Compile, 2.0, false),
            unit("nros-c", Kind::Compile, 2.0, false),
        ]);
        let h = shared_crate_recompiled(&p).expect("fires");
        assert!(h.contains("nros-c compiled 3\u{00d7}"), "{h}");
    }

    #[test]
    fn shared_crate_silent_when_unique() {
        let p = profile(vec![
            unit("a", Kind::Compile, 1.0, false),
            unit("b", Kind::Compile, 1.0, false),
        ]);
        assert!(shared_crate_recompiled(&p).is_none());
    }

    #[test]
    fn dominant_unit_reports_stage_share() {
        let p = profile(vec![
            unit("big", Kind::Compile, 18.0, false),
            unit("small", Kind::Compile, 2.0, false),
        ]);
        let h = dominant_unit(&p).expect("fires");
        assert!(h.contains("90% of compile"), "{h}");
    }

    #[test]
    fn job_ram_warns_when_starved_and_confirms_when_ample() {
        let warn = job_count_vs_ram(&Context {
            jobs: Some(8),
            mem_total_kb: Some(7 * 1024 * 1024),
        })
        .unwrap();
        assert!(warn.contains("OOM risk"), "{warn}");

        let ok = job_count_vs_ram(&Context {
            jobs: Some(2),
            mem_total_kb: Some(32 * 1024 * 1024),
        })
        .unwrap();
        assert!(ok.contains("within RAM budget"), "{ok}");
    }
}
