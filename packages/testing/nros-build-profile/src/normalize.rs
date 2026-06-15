//! Normalizer — fold collector outputs into one [`BuildProfile`].
//!
//! Durations are real seconds. `total_s` is the wall-clock **span** (last end
//! minus first start) summed per backend; `Stage::pct` is each stage's share of
//! total *work* (sum of unit durations), so the percentages sum to 100 even when
//! the build ran units in parallel (where work-sum exceeds the wall span).

use crate::collect::Collected;
use crate::model::{Backend, BuildProfile, Kind, Stage, Unit};

/// Combine one or more collector outputs into a normalized profile.
pub fn normalize(collected: Vec<Collected>) -> BuildProfile {
    let mut units: Vec<Unit> = Vec::new();
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
        for u in c.units {
            units.push(Unit {
                name: u.name,
                kind: u.kind,
                dur_s: u.dur_s,
                is_native: u.is_native,
            });
        }
    }

    let backend = resolve_backend(&backends, &units);
    let stages = build_stages(&units);

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
    let has_ninja = backends
        .iter()
        .any(|b| matches!(b, Backend::Ninja | Backend::NinjaWest | Backend::NinjaCmake | Backend::NinjaIdf));
    match (has_cargo, has_ninja) {
        (true, true) => Backend::Mixed,
        _ => backends
            .iter()
            .copied()
            .next()
            .unwrap_or(if units.is_empty() { Backend::Ninja } else { Backend::Cargo }),
    }
}

/// Aggregate units into per-stage durations + share-of-work percentages.
fn build_stages(units: &[Unit]) -> Vec<Stage> {
    let work: f64 = units.iter().map(|u| u.dur_s).sum();
    let mut stages = Vec::new();
    for kind in Kind::ORDER {
        let dur: f64 = units
            .iter()
            .filter(|u| u.kind == kind)
            .map(|u| u.dur_s)
            .sum();
        if dur <= 0.0 {
            continue;
        }
        let pct = if work > 0.0 { dur / work * 100.0 } else { 0.0 };
        stages.push(Stage {
            name: kind.name(),
            dur_s: dur,
            pct,
        });
    }
    stages
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
