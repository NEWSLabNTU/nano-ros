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
//! every workload once on native and the tier-2 lanes do not re-cover it.
//!
//! # Why tier 2 splits (2026-07-30)
//!
//! Cost is [`coords`], not cell count: cells share fixtures, and it is FIXTURES
//! that take hours to build. Measured 2026-07-30 against 182 runtime cells and
//! 47 coordinates:
//!
//! | lane | selection | cells | coords | cost |
//! | --- | --- | --- | --- | --- |
//! | [`CiLane::Tier1`] | native, 1-wise w,k + pairwise l × r | 16 | 10 | 21 % |
//! | [`CiLane::Tier2`] | 1-wise p, l, r, k | 11 | 12 | 26 % |
//! | [`CiLane::Tier2Nightly`] | pairwise p × l × r × k | 37 | 33 | 70 % |
//! | tier 3 | everything | 182 | 47 | 100 % |
//!
//! The pairwise cover reduces cells by 80 % and fixtures by only 30 %, and the
//! floor is structural: pairwise(platform × lang) needs one fixture per pair and
//! there are 29 declared pairs, so it cannot be tuned away. A middle tier costing
//! 70 % of the sweep is one nobody runs — the failure mode RFC-0061 exists to fix
//! — and pairwise(platform × lang) is also exactly the interaction the
//! 0268 / 0245 / 0332 class lives in, so it cannot simply be dropped either.
//!
//! Hence two lanes: [`CiLane::Tier2`] is 1-wise and affordable per change,
//! [`CiLane::Tier2Nightly`] is the pairwise cover on a nightly cadence. Cheap
//! enough to run, thorough enough to catch the class — a day later rather than
//! pre-merge. The nightly lane keeps rmw and kind in the pairing: over
//! pairwise(platform × lang) alone it costs ~4 more coordinates, and a lane that
//! is not on the critical path should not trade coverage that cheap.
//!
//! Note the ladder is not monotone in CELLS — tier 1 picks 16 and tier 2 picks 11
//! — because tier 1's cells are all native and a native fixture is nearly free.
//! Cell count is the wrong unit; `the_ladder_is_monotone_in_fixture_cost` asserts
//! the ordering in coordinates so that confusion cannot come back.
//!
//! # Why a set cover and not a covering array
//!
//! The axes are not independent — an RMW is not available on every platform — so
//! "pairwise" here means: choose a minimum subset of DECLARED cells such that
//! every (axis_i = a, axis_j = b) pair occurring in any declared cell occurs in
//! the chosen subset. Greedy is adequate (within a `ln n` factor; the input is a
//! couple of hundred cells), and ties break on the cell's debug rendering so the
//! chosen set is deterministic for a fixed table.

use crate::matrix::{Cell, PlatformId};

/// Every Runtime cell the tiers select over — baked cells (`matrix::CELLS`) AND
/// interop/bridge cells (`crate::interop::CELLS`), issue 0352 / phase-324.
///
/// `Kind::Interop` and `Kind::Bridge` are values of the `kind` axis the tiers
/// cover (tier 1 is 1-wise over kind), so moving the interop/bridge cells into
/// their own list must not remove them from the selection pool — or tier 1 would
/// silently stop covering the interop/bridge wiring it always did.
fn all_runtime_cells() -> impl Iterator<Item = &'static Cell> {
    crate::matrix::runtime_cells().chain(crate::interop::runtime_cells().map(|ic| &ic.cell))
}
use std::collections::BTreeSet;

/// A CI lane. See RFC-0061 for the ladder (tier 0 runs no cells at all, tier 3
/// runs everything, so neither needs a computed selection).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CiLane {
    /// Native only: every core code path once, and every language against every
    /// RMW. What `just ci` should mean — minutes, on the host, where a failure is
    /// cheap to debug.
    Tier1,
    /// 1-wise over platform × lang × rmw × kind: every declared value of each
    /// axis appears at least once, no pairing. What `just ci-matrix` runs — the
    /// per-change gate that has to be affordable enough to actually get run.
    Tier2,
    /// Pairwise over platform × lang × rmw × kind. What `just ci-matrix-nightly`
    /// runs — the interaction coverage tier 2 gives up in exchange for being
    /// cheap. See [`ALL`] for why the split exists.
    Tier2Nightly,
}

/// Every computed lane, so tests and tooling iterate rather than enumerate.
pub const ALL: [CiLane; 3] = [CiLane::Tier1, CiLane::Tier2, CiLane::Tier2Nightly];

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
        // 1-wise(platform, lang, rmw, kind) — every declared value once, no
        // pairing. 11 of 46 coordinates.
        CiLane::Tier2 => (
            vec![Axis::Platform, Axis::Lang, Axis::Rmw, Axis::Kind],
            vec![],
        ),
        // pairwise(platform × lang × rmw × kind); workload deliberately absent —
        // tier 1 already ran every workload, and workload selects
        // platform-independent logic, so repeating it costs cells and buys nothing.
        CiLane::Tier2Nightly => (
            vec![],
            vec![Axis::Platform, Axis::Lang, Axis::Rmw, Axis::Kind],
        ),
    }
}

fn pool(lane: CiLane) -> Vec<&'static Cell> {
    match lane {
        CiLane::Tier1 => all_runtime_cells()
            .filter(|c| matches!(c.platform, PlatformId::Linux))
            .collect(),
        CiLane::Tier2 | CiLane::Tier2Nightly => all_runtime_cells().collect(),
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

/// The distinct `platform,lang,rmw` FIXTURE coordinates this lane needs, in the
/// spelling `examples/fixtures.toml` uses.
///
/// This — not [`cells`]`.len()` — is what a lane costs, because cells share
/// fixtures and a fixture build is the expensive part. Consumed by the
/// `lane-coords` binary, `fixtures-manifest.py --coords-from` and
/// `NROS_FIXTURE_COORDS`, so a lane's build, its staleness gate and its test
/// selection all derive from one computation.
pub fn coords(lane: CiLane) -> BTreeSet<String> {
    cells(lane)
        .iter()
        .flat_map(|c| {
            let (lang, rmw) = (
                format!("{:?}", c.lang).to_lowercase(),
                format!("{:?}", c.rmw).to_lowercase(),
            );
            c.platform
                .fixture_tokens()
                .iter()
                .map(move |p| format!("{p},{lang},{rmw}"))
        })
        .collect()
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
        for lane in ALL {
            let a: Vec<_> = cells(lane).iter().map(|c| format!("{c:?}")).collect();
            let b: Vec<_> = cells(lane).iter().map(|c| format!("{c:?}")).collect();
            assert_eq!(a, b, "{lane:?} selection must not vary between calls");
        }
    }

    #[test]
    fn tier1_is_native_only() {
        for c in cells(CiLane::Tier1) {
            assert!(
                matches!(c.platform, PlatformId::Linux),
                "tier 1 runs on the host only, got {c:?}"
            );
        }
    }

    /// Issue 0393 — `build-test-fixtures-leaves` narrows its fan-out by FILTERING
    /// a hardcoded, deliberately-ordered platform list (zephyr first and solo,
    /// because it wants the whole job budget). Order is a scheduling property, so
    /// the list stays; but a module that a lane can select and the list does not
    /// name would be filtered out silently — the lane would build less than it
    /// gates, and the run would fail "Binary not found" with no clue why.
    ///
    /// So: the justfile's list must be a SUPERSET of every module the matrix can
    /// produce. Adding a platform to `matrix::CELLS` without adding it there
    /// fails here instead of in a sweep three hours later.
    #[test]
    fn build_fanout_names_every_module_the_matrix_can_select() {
        let justfile = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../justfile");
        let Ok(text) = std::fs::read_to_string(justfile) else {
            return; // out-of-tree checkout; nothing to gate
        };
        // The canonical ordered list — the one the make graph filters.
        let line = text
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("for platform in zephyr native qemu"))
            .unwrap_or_else(|| {
                panic!(
                    "justfile: canonical fixture platform list not found — if it was \
                     renamed or reordered, update this gate rather than deleting it"
                )
            });
        let listed: BTreeSet<&str> = line
            .trim_start_matches("for platform in")
            .trim_end_matches("; do")
            .split_whitespace()
            .collect();

        let needed: BTreeSet<&str> = PlatformId::ALL.iter().map(|p| p.just_module()).collect();
        let missing: Vec<&&str> = needed.difference(&listed).collect();
        assert!(
            missing.is_empty(),
            "justfile `build-test-fixtures-leaves` does not name {missing:?}; a lane \
             selecting that module would build nothing for it.\n  listed:  {listed:?}\n  \
             matrix:  {needed:?}"
        );
    }

    /// The regression this whole module exists for: adding a platform (or RMW, or
    /// language) to the matrix must extend the lane, not be silently skipped.
    #[test]
    fn lanes_touch_every_declared_value_of_every_axis_they_cover() {
        for lane in ALL {
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
    fn nightly_covers_every_declared_pair_it_claims_to() {
        let lane = CiLane::Tier2Nightly;
        let (_, p) = spec(lane);
        let want: BTreeSet<Req> = pool(lane).iter().flat_map(|c| pairs(c, &p)).collect();
        let got: BTreeSet<Req> = cells(lane).iter().flat_map(|c| pairs(c, &p)).collect();
        assert_eq!(
            want,
            got,
            "uncovered pairs: {:?}",
            want.difference(&got).take(5).collect::<Vec<_>>()
        );
    }

    /// A lane that selected everything would pass every other test here while
    /// defeating the point, so bound the ladder.
    ///
    /// Measured in COORDINATES, not cells. In cells the ladder is not even
    /// monotone — tier 1 picks 16 cells and 1-wise tier 2 picks 11 — because tier
    /// 1's cells are all native and a native fixture is nearly free. Cell count is
    /// the wrong unit for cost; asserting on it would encode the very confusion
    /// that put "tier 2 is 20 % of the sweep" in RFC-0061.
    #[test]
    fn the_ladder_is_monotone_in_fixture_cost() {
        let all: BTreeSet<String> = all_runtime_cells()
            .flat_map(|c| {
                let (lang, rmw) = (
                    format!("{:?}", c.lang).to_lowercase(),
                    format!("{:?}", c.rmw).to_lowercase(),
                );
                c.platform
                    .fixture_tokens()
                    .iter()
                    .map(move |p| format!("{p},{lang},{rmw}"))
            })
            .collect();
        let t1 = coords(CiLane::Tier1).len();
        let t2 = coords(CiLane::Tier2).len();
        let tn = coords(CiLane::Tier2Nightly).len();

        assert!(t1 > 0 && t2 > 0 && tn > 0, "empty lane: {t1}/{t2}/{tn}");
        assert!(
            t1 <= t2 && t2 < tn && tn < all.len(),
            "ladder must cost strictly more at each rung: \
             tier1={t1} tier2={t2} nightly={tn} all={}",
            all.len()
        );
        // The point of splitting tier 2: the per-change gate has to be cheap
        // enough that it gets run. If it ever creeps past a third of the sweep it
        // has stopped being a middle tier.
        assert!(
            t2 * 3 <= all.len(),
            "tier 2 ({t2}) must stay under a third of the sweep ({}) — \
             it is the lane that runs on every core change",
            all.len()
        );
    }

    /// Tier 2 gives up pairing to stay cheap; the nightly lane is where that
    /// coverage comes back. If they ever computed the same set, the split would be
    /// pure cost with no benefit.
    #[test]
    fn nightly_is_strictly_more_than_the_gate() {
        let gate = coords(CiLane::Tier2);
        let nightly = coords(CiLane::Tier2Nightly);
        assert!(
            gate.len() < nightly.len(),
            "nightly ({}) must cover more than the gate ({})",
            nightly.len(),
            gate.len()
        );
    }

    // Whether every coordinate a lane selects actually HAS a fixture row is not
    // asserted here on purpose: `tests/matrix_fixture_coverage.rs` already owns
    // that question in both directions, together with the exemption list naming
    // each cell built outside the manifest (Fvp's `just zephyr build-fvp-*`, the
    // zephyr west leaves, NuttxRiscv examples…). A second gate here would need a
    // second copy of that list, and a copy that drifts narrower is the "gates
    // narrower than the rule they enforce" defect (issue 0196).

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
        // Two views on purpose: the family/binary checks are spelling-agnostic,
        // but the CAPITALISED assertion below is precisely about case — matching
        // it against a lowercased copy would make it unfalsifiable.
        let raw = String::from_utf8_lossy(&out.stdout).to_string();
        let filter = raw.to_lowercase();
        for c in all_runtime_cells() {
            if matches!(c.platform, PlatformId::Linux) {
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
            assert!(
                filter.contains(&format!("binary(~{family})")),
                "native lane would still run {:?} binaries — lane-filter.sh emitted:\n{filter}",
                c.platform
            );

            // Issue 0357 — binary exclusion alone is not coverage. The matrix
            // consumers put EVERY platform's cases in one generically-named
            // binary (`rtos_e2e` is entirely cross-platform and matches no token
            // at all), so a lane filtered only by binary name still ran 53
            // cross-platform tests on a host with none of their fixtures.
            assert!(
                filter.contains(&format!("test(~{family})")),
                "no TEST-level exclusion for {:?}; a cross-platform case inside a \
                 generically-named binary would still run — lane-filter.sh emitted:\n{filter}",
                c.platform
            );

            // nextest's `~` is a case-SENSITIVE substring match and the harnesses
            // disagree on spelling: rstest emits `platform_1_Platform__Freertos`,
            // hand-rolled matrices emit `case_05_zephyr_rust`. One spelling
            // silently covers half the suite.
            let mut cap = family.clone();
            cap[..1].make_ascii_uppercase();
            assert!(
                raw.contains(&format!("test(~{cap})")),
                "no CAPITALISED test exclusion for {:?} — rstest case names would \
                 survive (nextest `~` is case-sensitive):\n{raw}",
                c.platform
            );
        }
        // The host lane must not exclude ITSELF. This assertion used to read
        // `!filter.contains("binary(~native)")` — a hardcoded literal, and a
        // gate narrower than the rule it enforces (the issue-0196 class) in two
        // ways at once:
        //
        //   1. It named `native`. phase-337 W8.b renamed the host variant to
        //      `Linux`, `lane-filter.sh`'s `grep -v '^Native$'` stopped matching,
        //      and `Linux` survived into the exclusion set — while this assertion
        //      kept passing, because the string it looked for was gone too.
        //   2. It checked only `binary(...)`. The damage is done by the TEST
        //      form: `not test(~Linux)` filters out every
        //      `platform_N_Platform__Linux` rstest case, i.e. the entire host
        //      matrix, and the run reports a fast green.
        //
        // Both fixed by deriving the host family from `PlatformId` the same way
        // the script does, and by checking every spelling the script emits.
        let host_family = {
            let debug = format!("{:?}", PlatformId::Linux);
            let mut f = String::new();
            for (i, ch) in debug.chars().enumerate() {
                if i > 0 && ch.is_ascii_uppercase() {
                    break;
                }
                f.push(ch.to_ascii_lowercase());
            }
            f
        };
        let mut host_cap = host_family.clone();
        host_cap[..1].make_ascii_uppercase();
        for form in [host_family.as_str(), host_cap.as_str()] {
            for kind in ["binary", "test"] {
                assert!(
                    !raw.contains(&format!("{kind}(~{form})")),
                    "the host lane excludes ITSELF via {kind}(~{form}) — every host \
                     case would be filtered out and the lane would report a fast \
                     green. `lane-filter.sh` must skip the `{:?}` variant:\n{raw}",
                    PlatformId::Linux
                );
            }
        }

        // The unit-test exemption. Without it the lane drops host-only tests
        // whose names merely mention a platform — `board::tier::tests::
        // threadx_inverts_scale`, `qemu::tests::test_parse_results`, and
        // `zephyr::tests::content_aware_staleness_ignores_mtime_only_bumps`, a
        // phase-318 test. Those need no fixture and no toolchain, so excluding
        // them trades one coverage hole for another.
        assert!(
            filter.contains("test(~tests::)"),
            "the unit-test exemption is missing; host-only `mod tests` cases that \
             mention a platform would be excluded from tier 1:\n{filter}"
        );
    }

    /// The filter's lines are ANDed by the caller. Emitting them as separate
    /// nextest `-E` flags instead UNIONS them, and `not A or not B` is a
    /// tautology that selects everything — a filter that silently does nothing.
    /// `just test-all` joins with " and " into a single `-E`; this pins the
    /// grouped line's shape so a future edit cannot produce an expression that
    /// only composes correctly under OR.
    #[test]
    fn lane_filter_test_exclusions_are_one_grouped_conjunction() {
        let out = std::process::Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../scripts/test/lane-filter.sh"
            ))
            .arg("native")
            .output();
        let Ok(out) = out else { return };
        if !out.status.success() {
            panic!(
                "lane-filter.sh native failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let grouped: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("(test(~tests::)"))
            .collect();
        assert_eq!(
            grouped.len(),
            1,
            "expected exactly one grouped test-exclusion line, got {}:\n{text}",
            grouped.len()
        );
        let g = grouped[0];
        assert!(
            g.ends_with(')') && g.contains(" or (") && g.contains(" and not test(~"),
            "the grouped line must be `(exemption or (not … and not …))`; got:\n{g}"
        );
        // Every other line is a standalone binary exclusion.
        for l in text.lines().filter(|l| !l.starts_with("(test(")) {
            assert!(
                l.starts_with("not binary(~"),
                "unexpected filter line (the caller ANDs these): {l}"
            );
        }
    }
}
