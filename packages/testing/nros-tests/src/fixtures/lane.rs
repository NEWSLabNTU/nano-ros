//! Coordinate-scoped test RUN narrowing — phase-340 W3, closing issue 0482.
//!
//! # The problem this solves
//!
//! A CI lane answers two questions with different answers (issue 0482): which
//! fixtures must be **FRESH** (its cell cover — narrow, and the whole saving)
//! and which must **EXIST** (a property of the RUN). Tier 2 was honest about
//! that only by giving up: `ci-matrix` executed the entire suite, so every
//! coordinate's fixtures had to exist, so the middle rung of the ladder cost
//! the top rung's fixture build. CLAUDE.md's own argument says an unaffordable
//! instruction gets followed selectively, "which is worse than a smaller
//! instruction followed honestly".
//!
//! Making the run narrow needs a test→coordinate mapping. Issue 0482 recorded
//! two candidate designs and why neither worked:
//!
//! * **`scripts/test/lane-filter.sh`** selects by platform-family TOKEN. Tier 2
//!   is 1-wise over platform, so every platform appears in it and platform-level
//!   filtering excludes nothing; the saving is in lang × rmw *within* a
//!   platform, which test names do not encode (issue 0357 settled that binary /
//!   test-name filtering cannot express this).
//! * **the fixture resolver** is where the test↔fixture binding physically
//!   exists — but it "identifies fixtures by PATH across ~30 hand-written
//!   functions, with no link back to the `fixtures.toml` row".
//!
//! # What changed
//!
//! The second objection is about the ~30 functions, and it dissolves once the
//! link is DERIVED instead of hand-written. Every one of those functions
//! computes a path under the manifest row's own artifact root — which is not a
//! coincidence but a requirement, because that is where the build writes. So
//! `fixtures-manifest.py` exports `row_artifact_root()` beside `row_coord()`
//! and this module inverts it: **resolved path → manifest row → coordinate**.
//! No per-resolver edit, no second list to drift (the #328 shape), and the
//! coordinate is `row_coord`'s — the same function `matches_filters` uses to
//! decide whether to BUILD the row.
//!
//! Measured over the manifest as shipped: all 240 buildable `[[fixture]]` rows
//! have DISTINCT artifact roots, so the inversion is exact, not heuristic.
//! [`crate::fixtures::lane`]'s round-trip gate (`tests/lane_run_narrowing.rs`)
//! asserts it stays that way.
//!
//! Workspace rows are attributed by `id` instead: both workspace resolvers take
//! the `fixture_id` already, and several rows legitimately share one `dir`.
//!
//! # One computation, stated precisely
//!
//! ```text
//! BUILD  skips row R  ⟺  row_coord(R) ∉ lane_coords     (fixtures-manifest.py --coords-from)
//! RUN    skips row R  ⟺  row_coord(R) ∉ lane_coords     (this module)
//! ```
//!
//! Same predicate, same `row_coord`, same coordinate file. That is the
//! non-negotiable issue 0482 leaves behind: build-set and run-set must derive
//! from ONE computation, because two derivations of one coordinate drift
//! silently — which is exactly how 67 of 240 rows ended up in no lane at all.
//!
//! # Why this cannot launder "never built" into "skipped" (issue 0445)
//!
//! The skip is keyed on **"this row's coordinate is outside the lane"** and
//! never on "the artifact is missing":
//!
//! * A coordinate INSIDE the lane never skips. A missing or stale in-lane
//!   fixture fails exactly as hard as it did before — [`require_in_lane`] runs
//!   before the existence check and returns `Ok` for it.
//! * A path that attributes to NO row never skips (fail closed). Families built
//!   module-level rather than by coordinate — the Zephyr west leaves, the
//!   compile-check lane — have no manifest row and keep today's hard failure,
//!   which is correct: their build is not narrowed either, so nothing is
//!   missing.
//! * With `NROS_TEST_COORDS` unset there is no narrowing at all and this module
//!   does no work (it does not even read the manifest).
//! * A `NROS_TEST_COORDS` that names a missing or empty file is a HARD ERROR,
//!   not "no narrowing" — an empty selection would silently skip the whole
//!   suite and report green. Same refusal `_coords_for` makes in
//!   `fixtures-manifest.py` and `nros_lane_coords_file` makes in
//!   `fixture-lane.sh`.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{TestResult, project_root};

/// The env var naming the coordinate file that scopes this RUN.
///
/// Set by `just ci-matrix` / `ci-matrix-nightly` to the SAME
/// `nros_lane_coords_file` output that scoped the lane's fixture build and its
/// staleness gate. Deliberately a distinct name from the build-side
/// `NROS_FIXTURE_COORDS`: that one is exported into fixture BUILD scripts and a
/// test process must not inherit a build's narrowing by accident. They are
/// bound by `CiLane::run_scope`, not by sharing a spelling.
pub const RUN_COORDS_ENV: &str = "NROS_TEST_COORDS";

/// A fixture coordinate, in the spelling `examples/fixtures.toml` uses.
pub type Coord = (String, String, String);

/// One buildable row of a coordinate-bearing manifest table.
#[derive(Debug, Clone)]
pub struct Row {
    /// `fixture` or `workspace_fixture`.
    pub kind: String,
    pub coord: Coord,
    pub dir: String,
    pub id: String,
    /// Repo-relative dir the row's artifacts land in (`row_artifact_root`).
    pub artifact_root: String,
}

impl Row {
    /// `platform,lang,rmw` — the line format of a lane-coords file.
    pub fn coord_str(&self) -> String {
        format!("{},{},{}", self.coord.0, self.coord.1, self.coord.2)
    }

    /// The most useful name for a human: workspace rows are known by `id`,
    /// plain rows by `dir`.
    pub fn label(&self) -> &str {
        if self.id.is_empty() {
            &self.dir
        } else {
            &self.id
        }
    }
}

static ROWS: OnceLock<Vec<Row>> = OnceLock::new();
static RUN_COORDS: OnceLock<Option<BTreeSet<Coord>>> = OnceLock::new();

/// Every buildable row of both coordinate-bearing tables, from
/// `fixtures-manifest.py coords`.
///
/// Shelling into the manifest reader rather than parsing `fixtures.toml` here
/// is the point: `row_coord` and `row_artifact_root` stay the single
/// computations. `nros-tests` already shells into this script for
/// `current_workspace_fixture_record`, so the precedent and the cost are known.
/// Read at most once per test process, and only when a lane actually narrows
/// the run.
pub fn manifest_rows() -> &'static [Row] {
    ROWS.get_or_init(|| {
        let root = project_root();
        let out = std::process::Command::new("python3")
            .arg(root.join("scripts/build/fixtures-manifest.py"))
            .arg("coords")
            .current_dir(&root)
            .output();
        let out = match out {
            Ok(o) if o.status.success() => o,
            // A manifest we cannot read must not silently mean "attribute
            // nothing", because that reads as "nothing is out of lane" and the
            // run then hard-fails on fixtures the lane deliberately skipped —
            // the confusing half of 0482, back again. Panic naming the cause.
            Ok(o) => panic!(
                "fixtures-manifest.py coords failed ({}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            ),
            Err(e) => panic!("could not run fixtures-manifest.py coords: {e}"),
        };
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut rows = Vec::new();
        for line in text.lines().filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split('\x1f').collect();
            assert_eq!(
                f.len(),
                7,
                "unexpected `coords` record shape (expected 7 \\x1f-separated \
                 fields: kind, platform, lang, rmw, dir, id, artifact_root): {line:?}"
            );
            rows.push(Row {
                kind: f[0].to_string(),
                coord: (f[1].to_string(), f[2].to_string(), f[3].to_string()),
                dir: f[4].to_string(),
                id: f[5].to_string(),
                artifact_root: f[6].to_string(),
            });
        }
        rows
    })
}

/// Parse a lane-coords file body. Same format and same refusals as
/// `_coords_for` in `fixtures-manifest.py` — one `platform,lang,rmw` per line,
/// `#` comments allowed, and an EMPTY selection is refused rather than treated
/// as "select nothing", which would report a green suite that ran no fixture at
/// all.
///
/// Split out of [`run_coords`] so the refusals are testable without an
/// environment variable and a `OnceLock` that has already latched.
pub fn parse_coords(text: &str, label: &str) -> BTreeSet<Coord> {
    let coords: BTreeSet<Coord> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let p: Vec<&str> = l.split(',').map(str::trim).collect();
            assert_eq!(
                p.len(),
                3,
                "{label}: expected `platform,lang,rmw`, got {l:?}"
            );
            (p[0].to_string(), p[1].to_string(), p[2].to_string())
        })
        .collect();
    assert!(
        !coords.is_empty(),
        "{label}: no coordinates — refusing to narrow the run to nothing (every \
         fixture would report as out-of-lane and the suite would report green \
         having run nothing)"
    );
    coords
}

/// The coordinate set this RUN is scoped to, or `None` when it is not scoped.
///
/// Panics on a `NROS_TEST_COORDS` that names nothing readable or selects
/// nothing — see the module docs.
pub fn run_coords() -> Option<&'static BTreeSet<Coord>> {
    RUN_COORDS
        .get_or_init(|| {
            let raw = std::env::var_os(RUN_COORDS_ENV)?;
            let raw = PathBuf::from(raw);
            if raw.as_os_str().is_empty() {
                return None;
            }
            let path = if raw.is_absolute() {
                raw
            } else {
                project_root().join(raw)
            };
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "{RUN_COORDS_ENV}={} is unreadable ({e}). It must name the \
                     lane-coords file that scoped this run's fixture build; an \
                     unreadable one would silently un-narrow the run.",
                    path.display()
                )
            });
            Some(parse_coords(&text, &path.display().to_string()))
        })
        .as_ref()
}

/// The manifest row a resolved fixture artifact belongs to, or `None`.
///
/// Matches the LONGEST `artifact_root` that is a path-component prefix of
/// `path`. Component-wise on purpose: a textual prefix would let
/// `…/talker/target` claim `…/talker/target-xrce`, which is a DIFFERENT row at
/// a different coordinate.
///
/// `None` for anything outside the manifest's artifact roots — the shared
/// `build/fixtures-cargo/<platform>` dirs (phase-226.D), the Zephyr west build
/// roots, the compile-check lane. Callers must treat `None` as "do not skip".
pub fn attribute_path(path: &Path) -> Option<&'static Row> {
    let root = project_root();
    let rel = path.strip_prefix(&root).unwrap_or(path);
    let mut best: Option<&'static Row> = None;
    for row in manifest_rows() {
        if row.kind != "fixture" || row.artifact_root.is_empty() {
            continue;
        }
        if !path_under(rel, Path::new(&row.artifact_root)) {
            continue;
        }
        let longer = best
            .map(|b| row.artifact_root.len() > b.artifact_root.len())
            .unwrap_or(true);
        if longer {
            best = Some(row);
        }
    }
    best
}

/// Is `path` inside `dir`, comparing whole path COMPONENTS?
///
/// A textual `starts_with` would say `examples/x/target` contains
/// `examples/x/target-zenoh/bin` — a different row at a different coordinate, or
/// (when the variant row is `skip_build` and therefore absent from the manifest)
/// no row at all. Either way a textual rule attributes an artifact to a row that
/// did not produce it, and the run then skips or runs the wrong cell.
///
/// Split out and given synthetic inputs in the tests below on purpose: against
/// the real manifest the longest-match rule happens to hide the difference, so a
/// test that only exercised `attribute_path` would pass under both rules and
/// gate nothing.
///
/// `pub(crate)` for [`crate::fixtures::groups`], which inverts `artifact_root`
/// for a different question (which shared cargo group did the build redirect
/// this row into?) and must answer containment identically — the two
/// inversions disagreeing is the same defect in two places.
pub(crate) fn path_under(path: &Path, dir: &Path) -> bool {
    let p: Vec<_> = path.components().collect();
    let d: Vec<_> = dir.components().collect();
    p.len() >= d.len() && p[..d.len()] == d[..]
}

/// The `[[workspace_fixture]]` row with this `id`, or `None`.
pub fn attribute_workspace_id(fixture_id: &str) -> Option<&'static Row> {
    manifest_rows()
        .iter()
        .find(|r| r.kind == "workspace_fixture" && r.id == fixture_id)
}

/// Whether `row` is inside `coords` — THE run-side selection predicate.
///
/// Exposed (and taking the coordinate set explicitly) so a gate can compare it
/// against `fixtures-manifest.py list --coords-from`, which is the build-side
/// selection, without an environment variable or a fixture. That comparison is
/// the invariant this module exists for; see `tests/lane_run_narrowing.rs`.
pub fn is_in_lane(row: &Row, coords: &BTreeSet<Coord>) -> bool {
    coords.contains(&row.coord)
}

fn out_of_lane(row: &Row) -> String {
    // Deliberately NOT phrased as (or routed through) a staleness verdict.
    // Issue 0445's lesson is that an absorbing message must account for itself:
    // this one names the row, its coordinate, and the fact that the build was
    // told to omit it — so a reader can tell "the lane excluded this" from "the
    // build was supposed to produce this and did not".
    format!(
        "out of lane: {} is at coordinate {}, which this run's lane does not \
         select, so `just build-test-fixtures lane=<this lane>` deliberately did \
         not build it.\n  \
         This is NOT a staleness or missing-fixture verdict: an in-lane fixture \
         that is absent or stale still fails hard.\n  \
         Run the full ladder (`just ci-full`) or unset {RUN_COORDS_ENV} to \
         execute this coordinate.",
        row.label(),
        row.coord_str(),
    )
}

/// Skip when `binary_path` belongs to a manifest row outside this run's lane.
///
/// Called at the fixture-resolution chokepoint, BEFORE the existence check, so
/// a coordinate the lane never built reports as skipped rather than as a
/// missing binary. Returns `Ok(())` — i.e. carry on and apply every existing
/// check — when the run is not narrowed, or the path attributes to no row, or
/// the row is in lane.
pub fn require_in_lane(binary_path: &Path) -> TestResult<()> {
    let Some(coords) = run_coords() else {
        return Ok(());
    };
    if let Some(reason) = skip_reason_for_path(binary_path, coords) {
        crate::skip!("{reason}");
    }
    Ok(())
}

/// [`require_in_lane`] for a `[[workspace_fixture]]`, attributed by `id`.
pub fn require_workspace_in_lane(fixture_id: &str) -> TestResult<()> {
    let Some(coords) = run_coords() else {
        return Ok(());
    };
    if let Some(reason) = skip_reason_for_workspace_id(fixture_id, coords) {
        crate::skip!("{reason}");
    }
    Ok(())
}

/// The whole decision, minus the environment: `Some(reason)` means skip.
///
/// Taking `coords` as an argument (rather than reading [`run_coords`]) is what
/// makes the decision testable in BOTH directions without an env var and a
/// `OnceLock` that latches for the life of the process. The `require_*`
/// wrappers add nothing but the lookup and the panic.
pub fn skip_reason_for_path(binary_path: &Path, coords: &BTreeSet<Coord>) -> Option<String> {
    let row = attribute_path(binary_path)?;
    (!is_in_lane(row, coords)).then(|| out_of_lane(row))
}

/// [`skip_reason_for_path`] for a `[[workspace_fixture]]` id.
pub fn skip_reason_for_workspace_id(fixture_id: &str, coords: &BTreeSet<Coord>) -> Option<String> {
    let row = attribute_workspace_id(fixture_id)?;
    (!is_in_lane(row, coords)).then(|| out_of_lane(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_row_attributes_to_itself() {
        // The round trip that makes the inversion trustworthy: feed each row's
        // OWN artifact root back through the path attribution and it must come
        // back. If two rows ever share an artifact root the longest-match rule
        // cannot separate them, and this fails — which is the moment to give
        // one of them its own `target_dir`, not the moment to weaken the rule.
        let root = project_root();
        let mut wrong = Vec::new();
        for row in manifest_rows().iter().filter(|r| r.kind == "fixture") {
            let probe = root.join(&row.artifact_root).join("some/binary");
            match attribute_path(&probe) {
                Some(got) if got.artifact_root == row.artifact_root => {}
                Some(got) => wrong.push(format!(
                    "{} -> {} (artifact roots {} vs {})",
                    row.label(),
                    got.label(),
                    row.artifact_root,
                    got.artifact_root
                )),
                None => wrong.push(format!("{} -> <unattributable>", row.label())),
            }
        }
        assert!(
            wrong.is_empty(),
            "{} row(s) do not attribute back to themselves:\n  {}",
            wrong.len(),
            wrong.join("\n  ")
        );
    }

    #[test]
    fn a_path_outside_every_artifact_root_is_not_attributable() {
        // Fail closed. These are the families built module-level rather than by
        // coordinate; attributing them would be guessing, and guessing here
        // turns "never built" into "skipped" (issue 0445).
        for p in [
            "build/zephyr-workspace-builds/build-ws-c-entry-zenoh/zephyr/zephyr.exe",
            "build/fixtures-cargo/qemu-arm-baremetal/thumbv7m-none-eabi/x/y",
            "build/cmake-fixtures/some-id/bin",
            "packages/testing/nros-smoke/target/x/y",
        ] {
            assert!(
                attribute_path(&project_root().join(p)).is_none(),
                "{p} must not attribute to a manifest row"
            );
        }
    }

    #[test]
    fn containment_is_component_wise_not_textual() {
        // The falsifiable form: synthetic paths where the two rules DISAGREE.
        // Against the real manifest they do not, because longest-match happens
        // to pick the right row anyway — so a test driven by `attribute_path`
        // alone would pass under a textual rule and gate nothing.
        let t = Path::new("examples/x/target");
        assert!(path_under(Path::new("examples/x/target/rel/bin"), t));
        assert!(
            !path_under(Path::new("examples/x/target-zenoh/rel/bin"), t),
            "`target` must not swallow `target-zenoh`: that is a different row at \
             a different coordinate (and if its row is skip_build, no row at all)"
        );
        assert!(!path_under(Path::new("examples/xy/target/bin"), t));
        assert!(path_under(t, t), "a dir contains itself");
        assert!(!path_under(Path::new("examples/x"), t));
    }

    #[test]
    fn artifact_roots_are_pairwise_unnested() {
        // The precondition that makes longest-match UNAMBIGUOUS: no row's
        // artifact root may sit inside another's. Two rows sharing (or nesting)
        // a root cannot be told apart from a binary path, and the run would then
        // decide a cell's fate from the wrong row's coordinate. Fix by giving
        // one of them its own `target_dir` / `build_subdir` — not by loosening
        // the rule.
        let roots: Vec<&Row> = manifest_rows()
            .iter()
            .filter(|r| r.kind == "fixture")
            .collect();
        let mut bad = Vec::new();
        for a in &roots {
            for b in &roots {
                if std::ptr::eq(*a, *b) {
                    continue;
                }
                if path_under(Path::new(&a.artifact_root), Path::new(&b.artifact_root)) {
                    bad.push(format!(
                        "{} ({}) is inside {} ({})",
                        a.label(),
                        a.artifact_root,
                        b.label(),
                        b.artifact_root
                    ));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} nested artifact root(s):\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    #[test]
    #[should_panic(expected = "refusing to narrow the run to nothing")]
    fn an_empty_coordinate_file_is_refused_not_read_as_no_narrowing() {
        // The dangerous silent reading: an empty file means "nothing is in
        // lane", so EVERY fixture skips and the suite reports green having run
        // nothing. `_coords_for` in fixtures-manifest.py refuses the same thing
        // for the same reason.
        parse_coords("\n# only a comment\n\n", "<test>");
    }

    #[test]
    fn a_coordinate_file_parses_to_row_coord_spelling() {
        let coords = parse_coords("linux,rust,zenoh\n# c\n nuttx,c,zenoh \n", "<test>");
        assert!(coords.contains(&("linux".to_string(), "rust".to_string(), "zenoh".to_string())));
        assert!(coords.contains(&("nuttx".to_string(), "c".to_string(), "zenoh".to_string())));
        assert_eq!(coords.len(), 2);
    }

    #[test]
    fn an_unnarrowed_run_reads_no_manifest_and_skips_nothing() {
        // The zero-cost property: with no lane coords the resolver must not
        // even consult the manifest, and must never skip.
        if std::env::var_os(RUN_COORDS_ENV).is_some() {
            return; // this process IS narrowed; the property is untestable here
        }
        assert!(run_coords().is_none());
        assert!(
            require_in_lane(&project_root().join("examples/native/rust/talker/target/x/talker"))
                .is_ok()
        );
        assert!(require_workspace_in_lane("workspace-rust-native").is_ok());
    }
}
