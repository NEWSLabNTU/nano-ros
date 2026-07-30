//! Print a CI lane's fixture coordinates — RFC-0061 / phase-318 W4.d.
//!
//! ```console
//! $ lane-coords tier1             # platform,lang,rmw — one per line, sorted
//! $ lane-coords tier2             # the per-change gate (1-wise)
//! $ lane-coords tier2-nightly     # the pairwise cover
//! $ lane-coords tier2 --cells     # the full cells, for inspection
//! ```
//!
//! # Why coordinates and not cells
//!
//! A lane's cost is not its cell count — it is the number of distinct
//! `(platform, lang, rmw)` FIXTURES it has to build, because cells share fixtures
//! and a fixture build is the expensive part. On the matrix as of 2026-07-30, 182
//! runtime cells collapse to 46 coordinates, and the pairwise cover's 37 cells
//! collapse to 32: an 80 % cell reduction that is only a 30 % build reduction.
//! That measurement is why tier 2 split into a 1-wise gate and a pairwise nightly
//! lane — see [`nros_tests::ci_lane`].
//!
//! `--cells` exists for reading, not for scripting.
//!
//! Consumed by `scripts/build/fixtures-manifest.py --coords-from` and by
//! `NROS_FIXTURE_COORDS` in `scripts/check-fixtures-stale.sh`, so a lane's build,
//! its gate, and its test selection all derive from one computation.

use nros_tests::ci_lane::{CiLane, cells, coords};

fn main() {
    let mut args = std::env::args().skip(1);
    let lane = match args.next().as_deref() {
        Some("tier1") => CiLane::Tier1,
        Some("tier2") => CiLane::Tier2,
        Some("tier2-nightly") => CiLane::Tier2Nightly,
        other => {
            eprintln!(
                "usage: lane-coords <tier1|tier2|tier2-nightly> [--cells]   (got {other:?})\n\
                 \n\
                 Prints `platform,lang,rmw` triples — the FIXTURE coordinates a\n\
                 lane needs, which is what its cost is measured in. Cells share\n\
                 fixtures, so cell count overstates the saving."
            );
            std::process::exit(2);
        }
    };

    if args.next().as_deref() == Some("--cells") {
        for c in cells(lane) {
            println!("{c:?}");
        }
        return;
    }

    for c in coords(lane) {
        println!("{c}");
    }
}
