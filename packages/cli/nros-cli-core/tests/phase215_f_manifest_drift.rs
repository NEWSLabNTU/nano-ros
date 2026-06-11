//! Phase 215.F.1 — board-crate manifest drift audit.
//!
//! Every `packages/boards/nros-board-*` crate that carries BOTH a
//! `board.cmake` sidecar AND a `[package.metadata.nros.board]` table
//! MUST keep the two faces in lock-step. This test walks the workspace,
//! parses both, and asserts byte-equal values for each overlapping
//! field. Boards with only one face (bare boards w/o `board.cmake`, or
//! boards where Phase 215.A hasn't landed yet) are skipped — there is
//! nothing to drift against.
//!
//! The test runs against an external workspace pointed at by
//! `NROS_WORKSPACE_ROOT`. When the env var is unset the test self-
//! skips (it lives in the standalone `nros-cli` repo; there is no
//! in-tree board crate to audit). For local maintainers running the
//! nano-ros tree:
//!
//! ```sh
//! NROS_WORKSPACE_ROOT=~/repos/nano-ros \
//!   cargo test -p nros-cli-core --test phase215_f_manifest_drift
//! ```

use std::{
    fs,
    path::{Path, PathBuf},
};

use nros_cli_core::orchestration::board_metadata::{
    compute_drift, parse_board_cmake, parse_board_metadata,
};

/// Walk `<workspace>/packages/boards/` and run the drift audit on every
/// `nros-board-*` crate carrying BOTH manifest faces.
#[test]
fn no_drift_between_cargo_metadata_and_board_cmake() {
    let Some(root) = workspace_root() else {
        eprintln!(
            "[SKIPPED] NROS_WORKSPACE_ROOT not set — point it at a nano-ros \
             tree to enable the Phase 215.F.1 drift audit"
        );
        return;
    };
    let boards_dir = root.join("packages").join("boards");
    if !boards_dir.is_dir() {
        eprintln!(
            "[SKIPPED] no `packages/boards/` under {} — workspace root is \
             not a nano-ros tree",
            root.display()
        );
        return;
    }

    let mut audited: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    let entries =
        fs::read_dir(&boards_dir).unwrap_or_else(|e| panic!("read {}: {e}", boards_dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !dir_name.starts_with("nros-board-") {
            continue;
        }
        let cargo_toml = path.join("Cargo.toml");
        let board_cmake = path.join("board.cmake");

        // Skip bare boards (no board.cmake) — nothing to drift against.
        if !board_cmake.is_file() {
            continue;
        }
        if !cargo_toml.is_file() {
            continue;
        }
        // Skip boards where the Cargo.toml doesn't (yet) carry the
        // `[package.metadata.nros.board]` table — Phase 215.C.1 is
        // ratcheting boards in one at a time. The mirror is mandatory
        // only for boards that have OPTED IN.
        let cargo_meta = match parse_board_metadata(&cargo_toml) {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("no `[package.metadata.nros.board]`") {
                    continue;
                }
                failures.push(format!(
                    "{dir_name}: failed to parse Cargo.toml metadata: {msg}"
                ));
                continue;
            }
        };

        let cmake_src = match fs::read_to_string(&board_cmake) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{dir_name}: read {}: {e}", board_cmake.display()));
                continue;
            }
        };
        let cmake_map = parse_board_cmake(&cmake_src);

        let drift = compute_drift(&cargo_meta, &cmake_map);
        if !drift.is_empty() {
            let detail: Vec<String> = drift
                .iter()
                .map(|d| {
                    format!(
                        "  {}: Cargo.toml={:?} board.cmake={:?}",
                        d.field, d.cargo_metadata, d.board_cmake
                    )
                })
                .collect();
            failures.push(format!(
                "{dir_name}: {} field(s) drift between Cargo.toml and \
                 board.cmake\n{}",
                drift.len(),
                detail.join("\n")
            ));
        }
        audited.push(dir_name.to_string());
    }

    if audited.is_empty() {
        eprintln!(
            "[SKIPPED] no board crates carry BOTH board.cmake AND \
             `[package.metadata.nros.board]` under {} — Phase 215.A / \
             215.C.1 may not have landed yet for any board",
            boards_dir.display()
        );
        return;
    }

    assert!(
        failures.is_empty(),
        "Phase 215.F.1 drift detected ({} board(s) audited, {} failed):\n{}",
        audited.len(),
        failures.len(),
        failures.join("\n\n")
    );

    eprintln!(
        "Phase 215.F.1 drift audit: {} board crate(s) audited cleanly: {}",
        audited.len(),
        audited.join(", ")
    );
}

fn workspace_root() -> Option<PathBuf> {
    let v = std::env::var_os("NROS_WORKSPACE_ROOT")?;
    let p = PathBuf::from(v);
    if !p.is_dir() {
        eprintln!(
            "[WARN] NROS_WORKSPACE_ROOT=`{}` is not a directory",
            p.display()
        );
        return None;
    }
    canonicalise(&p)
}

fn canonicalise(p: &Path) -> Option<PathBuf> {
    p.canonicalize().ok().or_else(|| Some(p.to_path_buf()))
}
