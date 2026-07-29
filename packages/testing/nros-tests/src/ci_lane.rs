//! CI lane selection — RFC-0061 / phase-318 W3.
//!
//! Which cells a given CI lane runs, **computed from [`crate::matrix::CELLS`]**
//! rather than listed. A hand-maintained list is the thing this replaces: adding
//! a platform to the matrix must extend the lanes without a second edit, or the
//! lanes silently skip it (audit E5, issue 0341).
//!
//! Naming: [`crate::matrix::Tier`] already means Runtime / BuildOnly / CarveOut —
//! a property of a CELL. These are lanes a RUN selects, so they are [`CiLane`].
//!
//! # Strength per axis, not per lane
//!
//! Uniform t-wise is the wrong frame, because the axes select different kinds of
//! thing and so fail in different shapes (RFC-0061 §Selection strategy):
//!
//! | axis | selects | strength |
//! | --- | --- | --- |
//! | `workload` | which core CODE PATH runs | 1-wise — pairing it is waste |
//! | `lang` × `rmw` | which ABI SEAM PAIR meets | pairwise |
//! | `platform` | toolchain + libc + linker | pairwise with `lang` |
//! | `kind` | entry vs carrier WIRING | pairwise with `platform` |
//!
//! Derived from defects, not theory: the sizes/`_opaque` class (0268, 0245) and
//! the freestanding-header class (0332) are platform × lang; the vtable/transport
//! ABI class (0331) is rmw × lang; entry/carrier wiring (0097, 0263) is
//! platform × kind. Nothing in that catalogue needs workload × platform — an
//! action-path bug fails on every platform, which is why [`CiLane::Tier1`] covers
//! every workload once on native and [`CiLane::Tier2`] does not re-cover it.
//!
//! # Why a set cover and not a covering array
//!
//! The axes are not independent — an RMW is not available on every platform — so
//! "pairwise" here means: choose a minimum subset of DECLARED cells such that
//! every (axis_i = a, axis_j = b) pair occurring in any declared cell occurs in
//! the chosen subset. Greedy is adequate (within a `ln n` factor; the input is a
//! couple of hundred cells), and ties break on the cell's debug rendering so the
//! chosen set is deterministic for a fixed table.

use crate::matrix::{Cell, PlatformId, runtime_cells};
use std::collections::BTreeSet;

/// A CI lane. See RFC-0061 for the ladder (tier 0 runs no cells at all, tier 3
/// runs everything, so neither needs a computed selection).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CiLane {
    /// Native only: every core code path once, and every language against every
    /// RMW. What `just ci` should mean — minutes, on the host, where a failure is
    /// cheap to debug.
    Tier1,
    /// Pairwise over platform × lang × rmw × kind across the whole matrix. The
    /// build/ABI/link/wiring interactions, which is what a platform changes.
    Tier2,
}

/// The axes, addressed positionally so requirements can be built generically.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Axis {
    Platform,
    Lang,
    Rmw,
    Workload,
    Kind,
}

fn value(c: &Cell, a: Axis) -> String {
    match a {
        Axis::Platform => format!("{:?}", c.platform),
        Axis::Lang => format!("{:?}", c.lang),
        Axis::Rmw => format!("{:?}", c.rmw),
        Axis::Workload => format!("{:?}", c.workload),
        Axis::Kind => format!("{:?}", c.kind),
    }
}

/// One thing a lane must cover: a single axis value, or a pair of them.
type Req = String;

fn singles(c: &Cell, axes: &[Axis]) -> Vec<Req> {
    axes.iter()
        .map(|&a| format!("1|{a:?}={}", value(c, a)))
        .collect()
}

fn pairs(c: &Cell, axes: &[Axis]) -> Vec<Req> {
    let mut out = Vec::new();
    for (i, &a) in axes.iter().enumerate() {
        for &b in &axes[i + 1..] {
            out.push(format!("2|{a:?}={}|{b:?}={}", value(c, a), value(c, b)));
        }
    }
    out
}

fn spec(lane: CiLane) -> (Vec<Axis>, Vec<Axis>) {
    match lane {
        // 1-wise(workload, kind) + pairwise(lang × rmw)
        CiLane::Tier1 => (
            vec![Axis::Workload, Axis::Kind],
            vec![Axis::Lang, Axis::Rmw],
        ),
        // pairwise(platform × lang × rmw × kind); workload deliberately absent —
        // tier 1 already ran every workload, and workload selects
        // platform-independent logic, so repeating it costs cells and buys nothing.
        CiLane::Tier2 => (
            vec![],
            vec![Axis::Platform, Axis::Lang, Axis::Rmw, Axis::Kind],
        ),
    }
}

fn pool(lane: CiLane) -> Vec<&'static Cell> {
    match lane {
        CiLane::Tier1 => runtime_cells()
            .filter(|c| matches!(c.platform, PlatformId::Native))
            .collect(),
        CiLane::Tier2 => runtime_cells().collect(),
    }
}

fn reqs_of(c: &Cell, lane: CiLane) -> BTreeSet<Req> {
    let (s, p) = spec(lane);
    singles(c, &s).into_iter().chain(pairs(c, &p)).collect()
}

/// The cells this lane runs.
///
/// Deterministic for a fixed [`crate::matrix::CELLS`]: greedy set cover with a
/// lexicographic tie-break on the cell's debug rendering. Adding a cell can still
/// reshuffle the chosen set — that is inherent to greedy cover, and why the
/// selection is recomputed rather than committed.
pub fn cells(lane: CiLane) -> Vec<&'static Cell> {
    let candidates = pool(lane);
    let universe: BTreeSet<Req> = candidates.iter().flat_map(|c| reqs_of(c, lane)).collect();

    let mut covered: BTreeSet<Req> = BTreeSet::new();
    let mut chosen: Vec<&'static Cell> = Vec::new();
    let mut remaining: Vec<&'static Cell> = candidates;

    while covered != universe {
        // Most new requirements wins; ties break on the debug rendering so the
        // result does not depend on iteration order or hash seeds.
        let best = remaining
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| {
                let gain = reqs_of(c, lane).difference(&covered).count();
                (gain, std::cmp::Reverse(format!("{c:?}")))
            })
            .map(|(i, _)| i);

        let Some(i) = best else { break };
        let cell = remaining.remove(i);
        if reqs_of(cell, lane).is_subset(&covered) {
            break; // nothing left can add coverage
        }
        covered.extend(reqs_of(cell, lane));
        chosen.push(cell);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn axis_values(cells: &[&'static Cell], a: Axis) -> BTreeSet<String> {
        cells.iter().map(|c| value(c, a)).collect()
    }

    #[test]
    fn lanes_are_deterministic() {
        for lane in [CiLane::Tier1, CiLane::Tier2] {
            let a: Vec<_> = cells(lane).iter().map(|c| format!("{c:?}")).collect();
            let b: Vec<_> = cells(lane).iter().map(|c| format!("{c:?}")).collect();
            assert_eq!(a, b, "{lane:?} selection must not vary between calls");
        }
    }

    #[test]
    fn tier1_is_native_only() {
        for c in cells(CiLane::Tier1) {
            assert!(
                matches!(c.platform, PlatformId::Native),
                "tier 1 runs on the host only, got {c:?}"
            );
        }
    }

    /// The regression this whole module exists for: adding a platform (or RMW, or
    /// language) to the matrix must extend the lane, not be silently skipped.
    #[test]
    fn lanes_touch_every_declared_value_of_every_axis_they_cover() {
        for lane in [CiLane::Tier1, CiLane::Tier2] {
            let chosen = cells(lane);
            let available = pool(lane);
            let (s, p) = spec(lane);
            for a in s.into_iter().chain(p) {
                let want = axis_values(&available, a);
                let got = axis_values(&chosen, a);
                assert_eq!(
                    want,
                    got,
                    "{lane:?} misses {a:?} values {:?}",
                    want.difference(&got).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn tier2_covers_every_declared_pair_it_claims_to() {
        let chosen = cells(CiLane::Tier2);
        let (_, p) = spec(CiLane::Tier2);
        let want: BTreeSet<Req> = pool(CiLane::Tier2)
            .iter()
            .flat_map(|c| pairs(c, &p))
            .collect();
        let got: BTreeSet<Req> = chosen.iter().flat_map(|c| pairs(c, &p)).collect();
        assert_eq!(
            want,
            got,
            "uncovered pairs: {:?}",
            want.difference(&got).take(5).collect::<Vec<_>>()
        );
    }

    /// A lane that selected everything would pass every other test here while
    /// defeating the point, so bound it: tier 2 must be a real reduction, and
    /// tier 1 must be smaller still.
    #[test]
    fn lanes_are_a_real_reduction() {
        let all = runtime_cells().count();
        let t1 = cells(CiLane::Tier1).len();
        let t2 = cells(CiLane::Tier2).len();
        assert!(t1 > 0 && t2 > 0, "empty lane: t1={t1} t2={t2}");
        assert!(
            t2 * 2 < all,
            "tier 2 ({t2}) should be well under half the matrix ({all})"
        );
        assert!(t1 < t2, "tier 1 ({t1}) must be cheaper than tier 2 ({t2})");
    }
    /// `scripts/test/lane-filter.sh` derives its exclusion tokens from
    /// `PlatformId`. If a platform is added whose family name the script cannot
    /// produce, the native lane would silently RUN that platform's binaries — the
    /// rot this whole module exists to prevent (audit E5 / issue 0341).
    #[test]
    fn lane_filter_tokens_cover_every_non_native_platform() {
        let out = std::process::Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../scripts/test/lane-filter.sh"
            ))
            .arg("native")
            .output();
        let Ok(out) = out else { return }; // script unavailable (packaged crate) — not a failure
        if !out.status.success() {
            panic!(
                "lane-filter.sh native failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let filter = String::from_utf8_lossy(&out.stdout).to_lowercase();
        for c in runtime_cells() {
            if matches!(c.platform, PlatformId::Native) {
                continue;
            }
            // Same rule the script applies: the leading CamelCase word.
            // `Esp32Qemu` -> `esp32` (NOT `esp` — stopping at the digit would
            // look for a token the script never emits).
            let debug = format!("{:?}", c.platform);
            let mut family = String::new();
            for (i, ch) in debug.chars().enumerate() {
                if i > 0 && ch.is_ascii_uppercase() {
                    break;
                }
                family.push(ch.to_ascii_lowercase());
            }
            let covered = filter.contains(&format!("binary(~{family})"));
            assert!(
                covered,
                "native lane would still run {:?} binaries — lane-filter.sh emitted:\n{filter}",
                c.platform
            );
        }
        assert!(
            !filter.contains("binary(~native)"),
            "the native lane must not exclude itself:\n{filter}"
        );
    }
}
