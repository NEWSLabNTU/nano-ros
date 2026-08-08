//! Issue 0482 — a lane's fixture BUILD must cover the tests that lane RUNS.
//!
//! # The defect
//!
//! `just build-test-fixtures lane=tier2` succeeded, wrote its stamp, and
//! satisfied `_require-fixtures`. `just ci-matrix` then produced ~231 failures,
//! nearly all "Test fixture is STALE" / not-found, because `ci-matrix` invokes
//! `test-all` with no `NROS_TEST_SCOPE` — the whole suite runs, and 34 of the 47
//! fixture coordinates had never been built.
//!
//! Two questions were being answered from one lane name:
//!
//! * which fixtures must be **FRESH** — the lane's cell cover, legitimately
//!   narrow; that narrowing is tier 2's entire saving;
//! * which fixtures must **EXIST** — a property of the RUN.
//!
//! `nros_lane_build_lane` is the mapping from the first to the second.
//! `CiLane::run_scope` is its declaration, next to the cell selection. This file
//! is the gate that keeps them equal AND pins the concrete regression, because a
//! consistency check alone would pass just as happily if both sides said
//! "tier 2 builds tier 2".
//!
//! # Why it drives the shell rather than re-implementing it
//!
//! `_require-fixtures` runs `scripts/build/fixture-lane.sh`, so that is the
//! thing whose behaviour matters. Every assertion below execs the real function
//! against a temporary stamp (`NROS_FIXTURE_STAMP` is overridable for exactly
//! this). Pure bash on both sides — nothing here compiles a fixture or a binary.

use std::{collections::BTreeSet, path::PathBuf, process::Command};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root")
        .to_path_buf()
}

/// Run a snippet with `fixture-lane.sh` sourced. Returns (exit code, stdout+stderr).
fn lane_sh(snippet: &str, stamp: Option<&str>) -> (i32, String) {
    lane_sh_env(snippet, stamp, &[])
}

/// [`lane_sh`] with extra environment. phase-340 W3 made
/// `nros_fixtures_stamp_require` also check that a narrow build is paired with a
/// narrowed RUN (`NROS_TEST_COORDS`), so tests that exercise the coverage logic
/// have to supply it — and the tests that exercise the PAIRING requirement
/// deliberately do not.
fn lane_sh_env(snippet: &str, stamp: Option<&str>, env: &[(&str, &str)]) -> (i32, String) {
    let root = project_root();
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(format!(
            "set -u; source scripts/build/fixture-lane.sh; {snippet}"
        ))
        .current_dir(&root);
    if let Some(s) = stamp {
        cmd.env("NROS_FIXTURE_STAMP", s);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run fixture-lane.sh");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The coordinate file `nros_lane_coords_file <lane>` produces, as an absolute
/// path — i.e. exactly what `just ci-matrix` exports as `NROS_TEST_COORDS`.
///
/// Read from the shell rather than recomputed, so a test cannot pass against a
/// coordinate set the recipe would never produce.
fn lane_coords_file(lane: &str) -> String {
    let (code, out) = lane_sh(&format!("nros_lane_coords_file {lane}"), None);
    assert_eq!(code, 0, "nros_lane_coords_file {lane} failed:\n{out}");
    let rel = out.trim();
    assert!(!rel.is_empty(), "{lane} has no coordinate file");
    project_root().join(rel).display().to_string()
}

/// Write a stamp in the format `nros_fixtures_stamp_write` produces.
fn write_stamp(dir: &std::path::Path, lane: &str, coords: &[&str]) -> PathBuf {
    let path = dir.join(format!(".fixtures-built-{lane}"));
    let mut body = String::from("# nano-ros fixture build stamp (test)\n");
    body.push_str("built_at=2026-08-07T00:00:00Z\n");
    body.push_str(&format!("lane={lane}\n"));
    for c in coords {
        body.push_str(&format!("coord={c}\n"));
    }
    std::fs::write(&path, body).expect("write stamp");
    path
}

fn tmpdir() -> PathBuf {
    // `$project/tmp/` (gitignored), not /tmp — CLAUDE.md.
    let d = project_root()
        .join("tmp")
        .join(format!("lane-build-covers-run-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("create tmp dir");
    d
}

/// The two spellings of the mapping must agree, for every lane, in both
/// directions of the vocabulary.
///
/// The shell is the runtime implementation (a preflight must not compile
/// anything to answer this); `CiLane::run_scope` is the declaration that sits
/// next to the cell cover it has to stay consistent with. Nothing else binds
/// them, and an unbound pair is how `fixtures-manifest.py` and
/// `matrix_fixture_coverage.rs` came to hold two different coordinates for the
/// same fixture row.
#[test]
fn shell_build_lane_matches_the_rust_declaration() {
    use nros_tests::ci_lane::{ALL, CiLane};

    for lane in ALL {
        let token = lane.lane_token();
        let (code, out) = lane_sh(&format!("nros_lane_build_lane {token}"), None);
        assert_eq!(code, 0, "nros_lane_build_lane {token} failed:\n{out}");
        assert_eq!(
            out.trim(),
            lane.build_lane(),
            "fixture-lane.sh and CiLane::build_lane disagree about what a {token} RUN \
             needs built. They are two spellings of ONE fact; fix both."
        );
        // The token itself is part of the contract: `_NROS_LANES` must know the
        // name Rust spells the lane with, or every lookup above answers about a
        // different lane.
        let (code, out) = lane_sh(&format!("nros_lane_validate {token}"), None);
        assert_eq!(
            code, 0,
            "fixture-lane.sh does not know the lane token CiLane::lane_token \
             emits ({token}):\n{out}"
        );
    }
    // Keep the type in use so a removal of `CiLane` here fails to compile
    // rather than silently dropping the binding.
    let _: CiLane = CiLane::Tier2;

    // The module-level lanes are their own build lane, in both places by
    // construction — asserted so a future edit cannot quietly make `native`
    // require `all` and re-price tier 1.
    for token in ["all", "native"] {
        let (code, out) = lane_sh(&format!("nros_lane_build_lane {token}"), None);
        assert_eq!(code, 0, "nros_lane_build_lane {token} failed:\n{out}");
        assert_eq!(out.trim(), token, "{token} must be its own build lane");
    }
}

/// An unknown lane must be REFUSED, not defaulted.
///
/// The two silent readings are both wrong in dangerous ways: an empty answer
/// makes the coverage check compare against a nameless file (`sort` reads stdin
/// — the preflight HANGS), and defaulting to `all` launders a requirement nobody
/// declared.
#[test]
fn an_undeclared_lane_is_refused() {
    let (code, out) = lane_sh("nros_lane_build_lane not-a-lane", None);
    assert_ne!(code, 0, "an unknown lane must fail, got:\n{out}");
}

/// **The regression, in the only form it can still take.**
///
/// Issue 0482's defect was that a `lane=tier2` build satisfied a tier-2
/// preflight while `ci-matrix` ran the WHOLE suite — preflight green, then ~231
/// STALE failures on coordinates the lane never built. 0482 closed that by
/// refusing the narrow build. phase-340 W3 closes it the other way instead, by
/// narrowing the RUN, so the narrow build is now ACCEPTED.
///
/// Which means the acceptance on its own is no longer evidence of anything: it
/// is correct only while the run really is narrowed. So this pins the two
/// together — the preflight accepts `lane=tier2`, AND tier 2 declares a
/// coordinate-scoped run whose build lane is itself. Assert only the first and
/// a future edit reverting `run_scope` to `All` would leave this test green over
/// exactly the original bug.
#[test]
fn a_tier2_build_satisfies_the_tier2_run_because_that_run_is_narrowed() {
    use nros_tests::ci_lane::{CiLane, RunScope};

    assert_eq!(
        CiLane::Tier2.run_scope(),
        RunScope::LaneCoords,
        "tier 2 must narrow its RUN to its own coordinates; if it stops doing \
         that, its build lane must go back to `all` (issue 0482) — accepting a \
         `lane=tier2` build for an unnarrowed run is the original defect"
    );
    assert_eq!(CiLane::Tier2.build_lane(), "tier2");

    let dir = tmpdir();
    let coords = lane_coords_file("tier2");
    // The stamp must carry the lane's REAL coordinates now that the preflight
    // diffs them against the run's — a hand-picked pair would fail on coverage
    // for reasons unrelated to what this test is about.
    let coord_lines = std::fs::read_to_string(&coords).expect("read tier2 coords");
    let coord_refs: Vec<&str> = coord_lines.lines().filter(|l| !l.is_empty()).collect();
    let stamp = write_stamp(&dir, "tier2", &coord_refs);
    let (code, out) = lane_sh_env(
        "nros_fixtures_stamp_require tier2",
        Some(stamp.to_str().unwrap()),
        &[("NROS_TEST_COORDS", coords.as_str())],
    );
    assert_eq!(
        code, 0,
        "a lane=tier2 fixture build must now satisfy the tier-2 preflight — \
         phase-340 W3 narrows the tier-2 RUN to the same coordinates, which is \
         what makes the middle rung of the ladder affordable:\n{out}"
    );
    let _ = std::fs::remove_file(&stamp);
}

/// A `lane=tier1` build must still NOT satisfy a tier-2 run.
///
/// The affordability fix must not degenerate into "any stamp will do". Tier 1's
/// cover (10 of 47 coordinates) is a strict subset of tier 2's (13), so the
/// coordinate diff has to report the difference and refuse.
#[test]
fn a_narrower_lane_build_still_does_not_satisfy_a_wider_run() {
    let dir = tmpdir();
    let stamp = write_stamp(&dir, "tier1", &["linux,rust,zenoh"]);
    let (code, out) = lane_sh_env(
        "nros_fixtures_stamp_require tier2 < /dev/null",
        Some(stamp.to_str().unwrap()),
        // Correctly narrowed run — so this exercises the COVERAGE diff, not the
        // pairing check.
        &[("NROS_TEST_COORDS", lane_coords_file("tier2").as_str())],
    );
    assert_ne!(
        code, 0,
        "a lane=tier1 build satisfied a tier-2 run; the coverage diff has \
         stopped diffing:\n{out}"
    );
    assert!(
        out.contains("build-test-fixtures"),
        "the refusal must name the build that would fix it:\n{out}"
    );
    let _ = std::fs::remove_file(&stamp);
}

/// The other direction: a full build satisfies every lane. Without this the
/// previous test could be "passed" by refusing everything, which would make the
/// preflight unusable rather than correct.
#[test]
fn an_all_build_satisfies_every_lane() {
    let dir = tmpdir();
    let stamp = write_stamp(&dir, "all", &[]);
    for lane in ["all", "native", "tier1", "tier2", "tier2-nightly"] {
        let (code, out) = lane_sh(
            &format!("nros_fixtures_stamp_require {lane}"),
            Some(stamp.to_str().unwrap()),
        );
        assert_eq!(
            code, 0,
            "a full fixture build must satisfy lane {lane}:\n{out}"
        );
    }
    let _ = std::fs::remove_file(&stamp);
}

/// Tier 1 keeps its saving: a `native` build still satisfies the tier-1 run.
///
/// The fix must not re-price the one lane that was already honest — if
/// `run_scope` ever said tier 1 runs everything, `just ci` would start demanding
/// a tier-3 build and the ladder would collapse to one rung.
#[test]
fn a_native_build_satisfies_the_tier1_run() {
    let dir = tmpdir();
    let stamp = write_stamp(&dir, "native", &[]);
    for lane in ["native", "tier1"] {
        let (code, out) = lane_sh(
            &format!("nros_fixtures_stamp_require {lane}"),
            Some(stamp.to_str().unwrap()),
        );
        assert_eq!(
            code, 0,
            "a lane=native build must satisfy the {lane} run — tier 1 narrows its \
             run to host binaries, which is exactly what that build produces:\n{out}"
        );
    }
    let _ = std::fs::remove_file(&stamp);
}

/// Every buildable fixture row must be REACHABLE through the coordinate filter
/// a lane build uses.
///
/// This is the other half of issue 0482 and the half that had actually rotted.
/// `rmw` is optional on `[[fixture]]`, and `fixtures-manifest.py` compared the
/// raw key against the lane's triples while `matrix_fixture_coverage.rs` applied
/// a `zenoh` default. 67 of 240 buildable rows therefore sat at the coordinate
/// `(platform, lang, None)` — a triple no `lane-coords` file can even spell — so
/// no coordinate-scoped lane selected them, and the STALENESS GATE, which runs
/// through the same filter, could not report the omission either. Tier 2
/// selected 46 rows where it should have selected 109.
///
/// The check is a round trip: hand the filter the coordinates the manifest
/// itself reports, and every row must come back. Nothing about it depends on
/// which lanes exist, so it keeps holding as the matrix moves — and it fails
/// loudly the moment a row's coordinate stops being expressible, which is the
/// only shape this defect has.
#[test]
fn every_fixture_row_is_reachable_through_the_coordinate_filter() {
    let root = project_root();
    let manifest = root.join("scripts/build/fixtures-manifest.py");

    let coords_out = Command::new("python3")
        .arg(&manifest)
        .arg("coords")
        .current_dir(&root)
        .output()
        .expect("run fixtures-manifest.py coords");
    assert!(
        coords_out.status.success(),
        "fixtures-manifest.py coords failed: {}",
        String::from_utf8_lossy(&coords_out.stderr)
    );
    let coords_text = String::from_utf8(coords_out.stdout).expect("utf-8");

    // Distinct coordinates, and how many `[[fixture]]` rows sit on them.
    let mut triples: BTreeSet<String> = BTreeSet::new();
    let mut plain_rows = 0usize;
    for line in coords_text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\x1f').collect();
        assert_eq!(f.len(), 7, "unexpected coords record: {line:?}");
        triples.insert(format!("{},{},{}", f[1], f[2], f[3]));
        if f[0] == "fixture" {
            plain_rows += 1;
        }
    }
    assert!(
        plain_rows > 0,
        "manifest reported no buildable fixture rows"
    );

    let dir = tmpdir();
    let coord_file = dir.join("all-coords.txt");
    let mut body = String::new();
    for t in &triples {
        body.push_str(t);
        body.push('\n');
    }
    std::fs::write(&coord_file, body).expect("write coord file");

    let listed = Command::new("python3")
        .arg(&manifest)
        .arg("list")
        .arg("--coords-from")
        .arg(&coord_file)
        .current_dir(&root)
        .output()
        .expect("run fixtures-manifest.py list --coords-from");
    assert!(
        listed.status.success(),
        "list --coords-from failed: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let selected = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .count();

    assert_eq!(
        selected, plain_rows,
        "the coordinate filter selected {selected} of {plain_rows} buildable \
         [[fixture]] rows when handed EVERY coordinate the manifest reports. The \
         missing rows sit at a coordinate no lane can express, so no \
         coordinate-scoped build will ever produce them and the staleness gate — \
         which uses this same filter — cannot report them missing (issue 0482)."
    );
    let _ = std::fs::remove_file(&coord_file);
}

/// A coordinate-scoped build must be REFUSED for a module-level requirement —
/// and must refuse it by failing, not by hanging.
///
/// `nros_lane_build_lane tier1` is `native`, which has no coordinate file. The
/// coverage diff below it does `comm -23 <(sort -u "$want_file") …`; handed an
/// empty `$want_file`, `sort` reads STDIN and the preflight blocks forever. A
/// preflight that hangs is worse than one that is wrong, because nothing reports
/// it.
#[test]
fn a_coordinate_scoped_build_is_refused_for_a_module_lane() {
    let dir = tmpdir();
    let stamp = write_stamp(&dir, "tier2", &["linux,rust,zenoh"]);
    let (code, out) = lane_sh(
        // `< /dev/null` so a regression HANGS nothing: if the guard is removed,
        // `sort` gets EOF immediately and the test fails on the exit code
        // instead of blocking the suite.
        "nros_fixtures_stamp_require tier1 < /dev/null",
        Some(stamp.to_str().unwrap()),
    );
    assert_ne!(
        code, 0,
        "a coordinate-scoped build must not satisfy the module-level `native` \
         requirement — a coordinate cover is a strict subset of a module's \
         rows:\n{out}"
    );
    let _ = std::fs::remove_file(&stamp);
}

/// **phase-340 W3's own 0482 guard.** A narrow build must be refused when the
/// RUN is not narrowed to match.
///
/// The recipes export `NROS_TEST_COORDS`, and `ci_lane::tests::
/// recipes_run_the_scope_their_lane_declares` gates that they do. But
/// `NROS_FIXTURE_LANE=tier2 just test-all` typed by hand reaches the SAME
/// acceptance with no narrowing — a narrow stamp accepted for a run that
/// resolves all 333 rows, which is issue 0482 verbatim. Gated where the
/// acceptance is granted, not only where it is configured.
#[test]
fn a_narrow_build_is_refused_when_the_run_is_not_narrowed() {
    let dir = tmpdir();
    let coords = lane_coords_file("tier2");
    let coord_lines = std::fs::read_to_string(&coords).expect("read tier2 coords");
    let coord_refs: Vec<&str> = coord_lines.lines().filter(|l| !l.is_empty()).collect();
    let stamp = write_stamp(&dir, "tier2", &coord_refs);

    let (code, out) = lane_sh(
        "nros_fixtures_stamp_require tier2 < /dev/null",
        Some(stamp.to_str().unwrap()),
    );
    assert_ne!(
        code, 0,
        "a lane=tier2 stamp was accepted with NROS_TEST_COORDS unset — the run \
         would resolve every coordinate against a build of 13 of them:\n{out}"
    );
    assert!(
        out.contains("NROS_TEST_COORDS"),
        "the refusal must name the missing narrowing:\n{out}"
    );

    // A coordinate file that is not this lane's is refused too: accepting any
    // file would let the build's acceptance and the run's narrowing come from
    // two different places, which is issue 0443's shape with new names.
    let other = dir.join("wrong-coords.txt");
    std::fs::write(&other, "linux,rust,zenoh\n").expect("write coords");
    let (code, out) = lane_sh_env(
        "nros_fixtures_stamp_require tier2 < /dev/null",
        Some(stamp.to_str().unwrap()),
        &[("NROS_TEST_COORDS", other.to_str().unwrap())],
    );
    assert_ne!(code, 0, "a foreign coordinate file was accepted:\n{out}");

    // …and a full build still needs no narrowing at all, or scoping only the
    // freshness gate on top of `lane=all` would stop working.
    let full = write_stamp(&dir, "all", &[]);
    let (code, out) = lane_sh(
        "nros_fixtures_stamp_require tier2",
        Some(full.to_str().unwrap()),
    );
    assert_eq!(
        code, 0,
        "`NROS_FIXTURE_LANE=tier2` on top of a full build must still be allowed \
         — every fixture exists, so an unnarrowed run is fine:\n{out}"
    );

    let _ = std::fs::remove_file(&stamp);
    let _ = std::fs::remove_file(&other);
    let _ = std::fs::remove_file(&full);
}
