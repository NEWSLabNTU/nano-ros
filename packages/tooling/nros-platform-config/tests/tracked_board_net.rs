//! RFC-0109 D2/D3 (phase-514 W1) — every in-tree board descriptor states who
//! owns its network (`[board.net] stack`) and which links it carries
//! (`[board.net] links`). Both are what nano-ros READS about a board's
//! network; a descriptor that omits either leaves the build guessing, which is
//! the shape RFC-0109 exists to end.

use std::path::{Path, PathBuf};

use nros_platform_config::platform_config::BoardNet;

/// Every `nros-board.toml` under `packages/boards`, skipping build output.
fn descriptors(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if !matches!(name.as_ref(), "target" | "build" | "third-party" | ".git") {
                descriptors(&p, out);
            }
        } else if name == "nros-board.toml" {
            out.push(p);
        }
    }
}

#[test]
fn every_tracked_board_states_stack_and_links() {
    let boards = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../boards");
    let mut files = Vec::new();
    descriptors(&boards, &mut files);
    files.sort();
    assert!(
        files.len() >= 14,
        "found only {} descriptors under {}",
        files.len(),
        boards.display()
    );

    let mut missing = Vec::new();
    for f in &files {
        let doc: toml::Value = toml::from_str(&std::fs::read_to_string(f).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let entries = doc
            .get("board")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        for b in entries {
            let name = b
                .get("names")
                .and_then(|n| n.as_array())
                .and_then(|n| n.first())
                .and_then(|n| n.as_str())
                .unwrap_or("?")
                .to_string();
            let net: BoardNet = match b.get("net") {
                Some(v) => v
                    .clone()
                    .try_into()
                    .unwrap_or_else(|e| panic!("{} [{name}] [board.net]: {e}", f.display())),
                None => BoardNet::default(),
            };
            let owner = net
                .stack_owner()
                .unwrap_or_else(|e| panic!("{} [{name}]: {e}", f.display()));
            let links = net
                .links()
                .unwrap_or_else(|e| panic!("{} [{name}]: {e}", f.display()));
            if owner.is_none() || links.is_empty() {
                missing.push(format!(
                    "{} [{name}]: stack {}, links {}",
                    f.display(),
                    if owner.is_none() { "MISSING" } else { "ok" },
                    if links.is_empty() { "MISSING" } else { "ok" },
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "every board states `[board.net] stack = \"host\" | \"os\" | \"board\"` and \
         `links = [..]` (RFC-0109 D2/D3):\n  {}",
        missing.join("\n  ")
    );
}
