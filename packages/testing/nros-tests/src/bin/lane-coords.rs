//! Print a CI lane's fixture coordinates — RFC-0061 / phase-318 W4.d.
//!
//! ```console
//! $ lane-coords tier2-nightly                    # platform,lang,rmw — sorted
//! $ lane-coords tier2-nightly --platform nuttx   # …only that fixture platform
//! $ lane-coords tier2-nightly --module nuttx     # …every token that module owns
//! $ lane-coords tier2-nightly --modules          # the `just <mod>` set, deduped
//! $ lane-coords tier2 --cells                    # full cells, for inspection
//! $ lane-coords tier2 --run-scope                # NROS_TEST_SCOPE for this lane
//! $ lane-coords tier2 --build-lane               # build-test-fixtures lane= it needs
//! $ lane-coords tier2 --unclaimed                # NROS_TEST_UNCLAIMED for this lane
//! $ lane-coords --scope nuttx                    # every row a `just nuttx` run owns
//! ```
//!
//! # `--scope <module>` is a PLATFORM scope's coordinates (issue 1758)
//!
//! `just test <platform>` runs that platform module's suite, and before issue
//! 1758 it ran it UNSCOPED: a test of another platform reached by the module's
//! filter found its fixture unbuilt and skipped, which read as a pass. Now an
//! unscoped run claims every coordinate, so the platform run needs a scope of
//! its own — every manifest row (west leaves included) whose platform the
//! module owns, which is exactly what `just <module> build-fixtures` builds.
//!
//! # Why coordinates and not cells
//!
//! A lane's cost is not its cell count — it is the number of distinct
//! `(platform, lang, rmw)` FIXTURES it has to build, because cells share fixtures
//! and a fixture build is the expensive part. On the matrix as of 2026-07-30, 182
//! runtime cells collapse to 47 coordinates, and the pairwise cover's 37 cells
//! collapse to 33: an 80 % cell reduction that is only a 30 % build reduction.
//! That measurement is why tier 2 split into a 1-wise gate and a pairwise nightly
//! lane — see [`nros_tests::ci_lane`].
//!
//! `--cells` exists for reading, not for scripting.
//!
//! # The three output modes exist for the three consumers
//!
//! * bare — `fixtures-manifest.py --coords-from` and `NROS_FIXTURE_COORDS`, so a
//!   lane's build and its staleness gate derive from one computation;
//! * `--platform` / `--module` — one CI job builds one platform (or one `just`
//!   module, which can own several: `nuttx` covers arm AND riscv), and needs only
//!   its slice;
//! * `--modules` — the CI job MATRIX itself, so the platform list in a workflow
//!   yml is computed from `matrix::CELLS` instead of hand-written (it was
//!   hand-written in `nightly.yml`, where nothing notices it going stale).
//!
//! # `--run-scope` / `--build-lane` answer a DIFFERENT question (issue 0482)
//!
//! The coordinate modes above say which fixtures a lane must have FRESH. These
//! two say which fixtures must EXIST, which is a property of the RUN, not of the
//! cell selection. Answering both from the lane name alone is what let `just
//! build-test-fixtures lane=tier2` satisfy the preflight for a run that then
//! failed on 34 unbuilt coordinates. Both are consumed by
//! `scripts/build/fixture-lane.sh`.
//!
//! They still differ for tier 1, whose run is filtered by NAME
//! (`NROS_TEST_SCOPE=native`) and so needs the broader `native` build. Since
//! phase-340 W3 tier 2 and the nightly lane narrow their run at fixture
//! RESOLUTION time instead, to exactly `coords(lane)` — so for those two the
//! answers coincide, and `--build-lane` prints the lane itself.

use nros_tests::{
    ci_lane::{CiLane, cells, coords},
    fixtures::lane::{manifest_rows, west_leaves},
    matrix::PlatformId,
};
use std::collections::BTreeSet;

fn usage(got: Option<&str>) -> ! {
    eprintln!(
        "usage: lane-coords <tier1|tier2|tier2-nightly> \
[--cells | --modules | --platform <token> | --module <name> | --run-scope | \
--build-lane | --unclaimed]\n       lane-coords --scope <just module>   (got {got:?})\n\
         \n\
         Prints `platform,lang,rmw` triples — the FIXTURE coordinates a lane\n\
         needs, which is what its cost is measured in. Cells share fixtures, so\n\
         cell count overstates the saving."
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--scope") {
        let Some(module) = args.get(1) else {
            usage(Some("--scope without a module"))
        };
        print_scope(module);
        return;
    }
    let lane = match args.first().map(String::as_str) {
        Some("tier1") => CiLane::Tier1,
        Some("tier2") => CiLane::Tier2,
        Some("tier2-nightly") => CiLane::Tier2Nightly,
        other => usage(other),
    };

    match args.get(1).map(String::as_str) {
        None => {
            for c in coords(lane) {
                println!("{c}");
            }
        }
        Some("--cells") => {
            for c in cells(lane) {
                println!("{c:?}");
            }
        }
        // Issue 0482 — one line, no trailing anything, so a shell can `$( )` it.
        Some("--run-scope") => println!("{}", lane.run_scope().test_scope()),
        Some("--build-lane") => println!("{}", lane.build_lane()),
        // Issue 1758 — may be EMPTY (a lane that claims everything), so the
        // line is printed either way and a shell `$( )` reads "".
        Some("--unclaimed") => println!("{}", lane.unclaimed_env_value()),
        Some("--modules") => {
            // Deduped: `nuttx` owns both NuttxArm and NuttxRiscv, `zephyr` owns
            // both ZephyrNativeSim and Fvp — one job each, not two.
            let mods: BTreeSet<&str> = cells(lane)
                .iter()
                .map(|c| c.platform.just_module())
                .collect();
            for m in mods {
                println!("{m}");
            }
        }
        Some("--platform") => {
            let Some(want) = args.get(2) else {
                usage(Some("--platform without a token"))
            };
            // Reject an unknown token rather than printing nothing: a typo would
            // otherwise make a CI job build zero fixtures and pass in seconds.
            if PlatformId::from_fixture_token(want).is_none() {
                eprintln!(
                    "unknown fixture platform token {want:?} — expected one of: {}",
                    PlatformId::ALL
                        .iter()
                        .flat_map(|p| p.fixture_tokens())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                std::process::exit(2);
            }
            print_prefixed(lane, &[want.as_str()]);
        }
        Some("--module") => {
            let Some(want) = args.get(2) else {
                usage(Some("--module without a name"))
            };
            let tokens = module_tokens(want);
            print_prefixed(lane, &tokens);
        }
        other => usage(other),
    }
}

/// The fixture-platform tokens a `just` module owns, or exit 2 naming the
/// known modules — an unknown name must not scope a run to nothing.
fn module_tokens(want: &str) -> Vec<&'static str> {
    let tokens: Vec<&str> = PlatformId::ALL
        .iter()
        .filter(|p| p.just_module() == want)
        .flat_map(|p| p.fixture_tokens())
        .copied()
        .collect();
    if tokens.is_empty() {
        eprintln!(
            "unknown just module {want:?} — expected one of: {}",
            PlatformId::ALL
                .iter()
                .map(|p| p.just_module())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::process::exit(2);
    }
    tokens
}

/// Print every manifest coordinate (west leaves included) on a platform the
/// module owns. A module with no rows is REFUSED: `NROS_TEST_COORDS` over an
/// empty file is itself refused by the resolver, and printing nothing would
/// hand the caller a file that fails every test for a reason about the file.
fn print_scope(module: &str) {
    let tokens = module_tokens(module);
    let set: BTreeSet<String> = manifest_rows()
        .iter()
        .map(|r| &r.coord)
        .chain(west_leaves().iter().map(|l| &l.coord))
        .filter(|(p, _, _)| tokens.contains(&p.as_str()))
        .map(|(p, l, r)| format!("{p},{l},{r}"))
        .collect();
    if set.is_empty() {
        eprintln!("module {module:?} owns no fixture row — refusing an empty scope");
        std::process::exit(1);
    }
    for c in set {
        println!("{c}");
    }
}

/// Print the lane's coordinates whose platform is one of `tokens`.
///
/// A lane slice that comes out EMPTY is printed as nothing and exits 0, which a
/// caller cannot distinguish from "this platform needs no fixtures". That is a
/// real state (a platform can be entirely absent from a 1-wise cover), so it is
/// not an error here — but callers that dispatch a CI job per module must treat
/// an empty slice as "skip", never as "built everything".
fn print_prefixed(lane: CiLane, tokens: &[&str]) {
    for c in coords(lane) {
        if tokens.iter().any(|t| c.starts_with(&format!("{t},"))) {
            println!("{c}");
        }
    }
}
