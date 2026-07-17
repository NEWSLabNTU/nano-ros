//! RFC-0051 / phase-295 W1.c — matrix ⊆⊇ fixtures.toml cross-check.
//!
//! Forward (ASSERTED): every `Tier::Runtime` cell of the declared matrix
//! whose platform bakes fixtures has at least one matching
//! `[[fixture]]` / `[[workspace_fixture]]` row in `examples/fixtures.toml`
//! for its (platform, lang, rmw) coordinate — a Runtime cell nothing
//! builds is a lie in the table.
//!
//! Reverse (REPORTED, flips to an assert at phase-295 W3-end): every
//! fixture row's (platform, lang, rmw) maps onto SOME cell coordinate —
//! rows outside the matrix are either debt the table must model or
//! orphans to delete. Reported (not asserted) while W3 migrates the
//! long tail; the report keeps the count visible in every run's output.
//!
//! Sibling of `examples_fixture_coverage.rs` (which checks example DIRS
//! have fixture rows); this file checks the MATRIX against the rows.

use std::{collections::BTreeSet, path::PathBuf};

use nros_tests::matrix::{CELLS, Kind, Lang, PlatformId, Rmw, Tier};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root")
        .to_path_buf()
}

/// fixtures.toml `platform` strings → matrix platform.
fn platform_from_str(s: &str) -> Option<PlatformId> {
    Some(match s {
        "native" => PlatformId::Native,
        "zephyr" => PlatformId::ZephyrNativeSim,
        "freertos" => PlatformId::FreertosMps2,
        "nuttx" => PlatformId::NuttxArm,
        "nuttx-riscv" => PlatformId::NuttxRiscv,
        "threadx-linux" => PlatformId::ThreadxLinux,
        "threadx-riscv64" => PlatformId::ThreadxRiscv64,
        "esp32" | "qemu-esp32-baremetal" => PlatformId::Esp32Qemu,
        "qemu-arm-baremetal" => PlatformId::QemuBaremetal,
        "stm32f4" => PlatformId::Stm32F4,
        "fvp" => PlatformId::Fvp,
        _ => return None,
    })
}

fn lang_from_str(s: &str) -> Option<Lang> {
    Some(match s {
        "rust" => Lang::Rust,
        "c" => Lang::C,
        "cpp" => Lang::Cpp,
        "mixed" => Lang::Mixed,
        _ => return None,
    })
}

fn rmw_from_str(s: &str) -> Option<Rmw> {
    Some(match s {
        "zenoh" => Rmw::Zenoh,
        "cyclonedds" => Rmw::Cyclonedds,
        "xrce" => Rmw::Xrce,
        _ => return None,
    })
}

/// (platform, lang, rmw, is_workspace) triples present in fixtures.toml.
fn fixture_coords() -> (BTreeSet<(u16, u16, u16, bool)>, Vec<String>) {
    let text = std::fs::read_to_string(project_root().join("examples/fixtures.toml"))
        .expect("read examples/fixtures.toml");
    let doc: toml::Table = toml::from_str(&text).expect("parse fixtures.toml");
    let mut coords = BTreeSet::new();
    let mut unmapped = Vec::new();
    for (table, is_ws) in [("fixture", false), ("workspace_fixture", true)] {
        let Some(rows) = doc.get(table).and_then(|v| v.as_array()) else {
            continue;
        };
        for row in rows {
            let get = |k: &str| row.get(k).and_then(|v| v.as_str());
            let (Some(p), Some(l)) = (get("platform"), get("lang")) else {
                continue;
            };
            // rmw defaults to zenoh when omitted (fixtures.toml convention).
            let r = get("rmw").unwrap_or("zenoh");
            match (platform_from_str(p), lang_from_str(l), rmw_from_str(r)) {
                (Some(p), Some(l), Some(r)) => {
                    coords.insert((p.index(), l.port_index(), r.index(), is_ws));
                }
                _ => unmapped.push(format!("{table}: platform={p} lang={l} rmw={r}")),
            }
        }
    }
    (coords, unmapped)
}

/// Forward: every baked Runtime cell has a fixture row at its coordinate.
#[test]
fn every_runtime_cell_has_a_fixture_row() {
    let (coords, _) = fixture_coords();
    let mut missing = Vec::new();
    for c in CELLS {
        if !matches!(c.tier, Tier::Runtime) {
            continue;
        }
        // Cells built OUTSIDE fixtures.toml — each exemption names its
        // real build channel (the W4 goal is shrinking this list by
        // folding the lanes into fixtures.toml or a sibling manifest):
        // - Native: `just build-test-fixtures` native family + ephemeral
        //   isolation; Interop/Bridge: ros2 peers / bridge harness.
        // - ZephyrNativeSim examples + non-rust workspaces: the west
        //   leaves lane (scripts/build/zephyr-fixture-leaves.sh — its own
        //   staleness sig, fixtures.toml `skip_probe` note).
        // - Fvp: `just zephyr build-fvp-*` recipes.
        // - NuttxRiscv examples: `just nuttx build-riscv-*` recipes.
        // - ThreadxRiscv64 cyclone: `just threadx_riscv64 build-fixtures`
        //   deploy-overlay lane (#214).
        let west_lane_zephyr = matches!(c.platform, PlatformId::ZephyrNativeSim)
            && (matches!(c.kind, Kind::Example)
                || (matches!(c.kind, Kind::Workspace) && !matches!(c.lang, Lang::Rust)));
        if matches!(c.platform, PlatformId::Native)
            || matches!(c.kind, Kind::Interop | Kind::Bridge)
            || west_lane_zephyr
            || matches!(c.platform, PlatformId::Fvp)
            || (matches!(c.platform, PlatformId::NuttxRiscv) && matches!(c.kind, Kind::Example))
            || (matches!(c.platform, PlatformId::ThreadxRiscv64)
                && matches!(c.rmw, Rmw::Cyclonedds))
        {
            continue;
        }
        let is_ws = matches!(c.kind, Kind::Workspace);
        let key = (
            c.platform.index(),
            c.lang.port_index(),
            c.rmw.index(),
            is_ws,
        );
        if !coords.contains(&key) {
            missing.push(format!("{c:?}"));
        }
    }
    assert!(
        missing.is_empty(),
        "Runtime cells with NO fixtures.toml row at their (platform, lang, rmw) \
         coordinate — either the table lies or the fixture is missing:\n{}",
        missing.join("\n")
    );
}

/// Reverse: report fixture coordinates the matrix doesn't model yet.
/// Flips to an assert when phase-295 W3 lands (tracked in the phase doc).
#[test]
fn report_fixture_rows_outside_matrix() {
    let (coords, unmapped) = fixture_coords();
    let cell_keys: BTreeSet<_> = CELLS
        .iter()
        .map(|c| {
            (
                c.platform.index(),
                c.lang.port_index(),
                c.rmw.index(),
                matches!(c.kind, Kind::Workspace),
            )
        })
        .collect();
    let orphans: Vec<_> = coords.difference(&cell_keys).collect();
    eprintln!(
        "[matrix-coverage] fixture coordinates not yet modeled by the matrix: {} \
         (W3 flips this to an assert); unmapped platform strings: {}",
        orphans.len(),
        unmapped.len()
    );
    for o in &orphans {
        eprintln!("  orphan coord (platform_idx, lang_idx, rmw_idx, is_ws): {o:?}");
    }
    for u in &unmapped {
        eprintln!("  unmapped row: {u}");
    }
}
