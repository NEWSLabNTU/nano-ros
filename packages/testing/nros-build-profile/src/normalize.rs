//! Normalizer — fold collector outputs into one [`BuildProfile`].
//!
//! Durations are real seconds. `total_s` is the wall-clock **span** (last end
//! minus first start) summed per backend. Each `Stage::dur_s` is that stage's
//! **wall** time — the merged length of its units' time intervals, not a sum of
//! overlapping parallel edges — so a stage never exceeds the build's wall span
//! (a naive sum would report compile > total on any `-j` build). `Stage::pct` is
//! that wall share of `total_s`; with concurrent stages the percentages can sum
//! to slightly over 100 (stages that genuinely overlapped in time).

use crate::{
    collect::Collected,
    model::{Backend, BuildProfile, Kind, Stage, Unit},
};

/// Combine one or more collector outputs into a normalized profile.
pub fn normalize(collected: Vec<Collected>) -> BuildProfile {
    let mut raw: Vec<crate::model::RawUnit> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut total_s = 0.0f64;
    let mut deep = false;
    let mut backends: Vec<Backend> = Vec::new();

    for c in &collected {
        total_s += span(c);
        deep |= c.deep;
        if let Some(b) = c.backend {
            backends.push(b);
        }
        notes.extend(c.notes.iter().cloned());
    }
    for c in collected {
        raw.extend(c.units);
    }

    let stages = build_stages(&raw, total_s);
    let units: Vec<Unit> = raw
        .into_iter()
        .map(|u| Unit {
            name: u.name,
            kind: u.kind,
            dur_s: u.dur_s,
            is_native: u.is_native,
        })
        .collect();
    let backend = resolve_backend(&backends, &units);

    BuildProfile {
        backend,
        total_s,
        stages,
        units,
        captured_deep: deep,
        notes,
    }
}

/// Wall-clock span of one collector's units (max end − min start).
fn span(c: &Collected) -> f64 {
    let mut min_start = f64::INFINITY;
    let mut max_end = 0.0f64;
    for u in &c.units {
        min_start = min_start.min(u.start_s);
        max_end = max_end.max(u.start_s + u.dur_s);
    }
    if min_start.is_finite() {
        (max_end - min_start).max(0.0)
    } else {
        0.0
    }
}

/// Pick the backend label: distinct ninja + cargo → Mixed; otherwise the single
/// identified backend, falling back to a guess from the unit shape.
fn resolve_backend(backends: &[Backend], units: &[Unit]) -> Backend {
    let has_cargo = backends.contains(&Backend::Cargo);
    let has_ninja = backends.iter().any(|b| {
        matches!(
            b,
            Backend::Ninja | Backend::NinjaWest | Backend::NinjaCmake | Backend::NinjaIdf
        )
    });
    match (has_cargo, has_ninja) {
        (true, true) => Backend::Mixed,
        _ => backends
            .iter()
            .copied()
            .next()
            .unwrap_or(if units.is_empty() {
                Backend::Ninja
            } else {
                Backend::Cargo
            }),
    }
}

/// Aggregate units into per-stage **wall** durations (merged-interval length,
/// not a sum of overlapping parallel edges) + percentage of the total wall span.
///
/// Summing raw durations would report a stage longer than the whole build on any
/// parallel (`-j`) build — e.g. esp-idf compile = 134 s of CPU across a 9 s wall.
/// Merging each stage's `[start, start+dur]` intervals gives the real wall time
/// that stage occupied, so stage durations stay within the build's wall span.
fn build_stages(units: &[crate::model::RawUnit], total_s: f64) -> Vec<Stage> {
    let mut stages = Vec::new();
    for kind in Kind::ORDER {
        let mut intervals: Vec<(f64, f64)> = units
            .iter()
            .filter(|u| u.kind == kind)
            .map(|u| (u.start_s, u.start_s + u.dur_s))
            .collect();
        if intervals.is_empty() {
            continue;
        }
        let dur = merged_len(&mut intervals);
        if dur <= 0.0 {
            continue;
        }
        let pct = if total_s > 0.0 {
            dur / total_s * 100.0
        } else {
            0.0
        };
        stages.push(Stage {
            name: kind.name(),
            dur_s: dur,
            pct,
        });
    }
    stages
}

/// Total length covered by a set of intervals after merging overlaps.
fn merged_len(intervals: &mut [(f64, f64)]) -> f64 {
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut total = 0.0;
    let mut cur: Option<(f64, f64)> = None;
    for &(s, e) in intervals.iter() {
        match cur {
            None => cur = Some((s, e)),
            Some((cs, ce)) => {
                if s <= ce {
                    cur = Some((cs, ce.max(e)));
                } else {
                    total += ce - cs;
                    cur = Some((s, e));
                }
            }
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RawUnit;

    fn raw(name: &str, kind: Kind, start_s: f64, dur_s: f64) -> RawUnit {
        RawUnit {
            name: name.to_string(),
            kind,
            dur_s,
            start_s,
            is_native: false,
        }
    }

    #[test]
    fn stages_aggregate_and_total_is_wall_span() {
        let c = Collected {
            units: vec![
                raw("a.o", Kind::Compile, 0.0, 18.0),
                raw("b.o", Kind::Compile, 18.0, 2.0),
                raw("img.elf", Kind::Link, 20.0, 1.0),
            ],
            backend: Some(Backend::NinjaWest),
            deep: true,
            notes: vec![],
        };
        let p = normalize(vec![c]);

        assert_eq!(p.backend, Backend::NinjaWest);
        assert!((p.total_s - 21.0).abs() < 1e-6, "wall span {}", p.total_s);
        assert!(p.captured_deep);

        let compile = p.stages.iter().find(|s| s.name == "compile").unwrap();
        assert!((compile.dur_s - 20.0).abs() < 1e-6);
        // share of work = 20 / 21 ≈ 95.2%
        assert!((compile.pct - 20.0 / 21.0 * 100.0).abs() < 1e-6);

        let link = p.stages.iter().find(|s| s.name == "link").unwrap();
        assert!((link.dur_s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parallel_stage_uses_merged_wall_not_sum() {
        // Three compiles overlapping in [0,5] on a parallel build: raw sum = 12s
        // but wall = 5s. The stage must report the 5s wall, not 12s.
        let c = Collected {
            units: vec![
                raw("a.o", Kind::Compile, 0.0, 5.0),
                raw("b.o", Kind::Compile, 1.0, 4.0),
                raw("c.o", Kind::Compile, 2.0, 3.0),
            ],
            backend: Some(Backend::NinjaIdf),
            deep: true,
            notes: vec![],
        };
        let p = normalize(vec![c]);
        let compile = p.stages.iter().find(|s| s.name == "compile").unwrap();
        assert!(
            (compile.dur_s - 5.0).abs() < 1e-6,
            "wall not sum: {}",
            compile.dur_s
        );
        assert!(compile.dur_s <= p.total_s + 1e-9, "stage within wall");
    }

    #[test]
    fn distinct_ninja_and_cargo_is_mixed() {
        let ninja = Collected {
            units: vec![raw("a.o", Kind::Compile, 0.0, 1.0)],
            backend: Some(Backend::NinjaCmake),
            deep: true,
            notes: vec![],
        };
        let cargo = Collected {
            units: vec![raw("crate", Kind::Compile, 0.0, 1.0)],
            backend: Some(Backend::Cargo),
            deep: true,
            notes: vec![],
        };
        let p = normalize(vec![ninja, cargo]);
        assert_eq!(p.backend, Backend::Mixed);
        // total = sum of both spans (1.0 + 1.0)
        assert!((p.total_s - 2.0).abs() < 1e-6, "{}", p.total_s);
    }

    #[test]
    fn empty_collectors_yield_empty_profile() {
        let p = normalize(vec![Collected::default()]);
        assert_eq!(p.total_s, 0.0);
        assert!(p.stages.is_empty());
        assert!(!p.captured_deep);
    }
}
