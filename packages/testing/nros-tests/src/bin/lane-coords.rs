//! Print a CI lane's fixture coordinates — RFC-0061 / phase-318 W4.d.
//!
//! ```console
//! $ lane-coords tier1        # platform,lang,rmw — one per line, sorted
//! $ lane-coords tier2
//! $ lane-coords tier2 --cells   # the full cells, for inspection
//! ```
//!
//! # Why coordinates and not cells
//!
//! A lane's cost is not its cell count — it is the number of distinct
//! `(platform, lang, rmw)` FIXTURES it has to build, because cells share
//! fixtures. On the matrix as of 2026-07-30: 182 runtime cells collapse to 46
//! coordinates, and the tier-2 pairwise cover's 37 cells collapse to 32. So the
//! selection reduces cells by 80 % and fixtures by only 30 %.
//!
//! That is the unit the build loop and the staleness gate consume, which is why
//! this prints coordinates. `--cells` exists for reading, not for scripting.
//!
//! Consumed by `scripts/build/fixtures-manifest.py --coords-from` and by
//! `NROS_FIXTURE_COORDS` in `scripts/check-fixtures-stale.sh`, so a lane's build,
//! its gate, and its test selection all derive from one computation.

use nros_tests::ci_lane::{CiLane, cells};
use std::collections::BTreeSet;

fn main() {
    let mut args = std::env::args().skip(1);
    let lane = match args.next().as_deref() {
        Some("tier1") => CiLane::Tier1,
        Some("tier2") => CiLane::Tier2,
        other => {
            eprintln!(
                "usage: lane-coords <tier1|tier2> [--cells]   (got {other:?})\n\
                 \n\
                 Prints `platform,lang,rmw` triples — the FIXTURE coordinates a\n\
                 lane needs, which is what its cost is measured in. Cells share\n\
                 fixtures, so cell count overstates the saving."
            );
            std::process::exit(2);
        }
    };
    let want_cells = args.next().as_deref() == Some("--cells");

    let chosen = cells(lane);
    if want_cells {
        for c in &chosen {
            println!("{c:?}");
        }
        return;
    }

    // Lower-cased to match the fixtures.toml spelling the manifest reads.
    let coords: BTreeSet<String> = chosen
        .iter()
        .map(|c| {
            format!(
                "{},{},{}",
                fixture_token(&format!("{:?}", c.platform)),
                format!("{:?}", c.lang).to_lowercase(),
                format!("{:?}", c.rmw).to_lowercase(),
            )
        })
        .collect();
    for c in coords {
        println!("{c}");
    }
}

/// Matrix platform → the string `examples/fixtures.toml` uses.
///
/// The two vocabularies differ (`FreertosMps2` vs `freertos`,
/// `ZephyrNativeSim` vs `zephyr`), and `matrix_fixture_coverage.rs` already owns
/// the authoritative mapping in the other direction. Keep this in step with it —
/// a silent mismatch here means a lane builds nothing for that platform and looks
/// fast rather than broken.
fn fixture_token(debug: &str) -> String {
    match debug {
        "Native" => "native",
        "ZephyrNativeSim" => "zephyr",
        "FreertosMps2" => "freertos",
        "NuttxArm" => "nuttx",
        "NuttxRiscv" => "nuttx-riscv",
        "ThreadxLinux" => "threadx-linux",
        "ThreadxRiscv64" => "threadx-riscv64",
        "Esp32Qemu" => "esp32",
        "QemuBaremetal" => "qemu-baremetal",
        "Stm32F4" => "stm32f4",
        "Fvp" => "fvp",
        other => other,
    }
    .to_string()
}
