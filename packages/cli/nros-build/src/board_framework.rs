//! Issue 0415 — tell `nros::main!` which framework the BOARD wants.
//!
//! `nros::main!` picks its entry shape (`#[rtic::app]`,
//! `#[embassy_executor::main]`, a plain `fn main()`, …) at macro-expansion
//! time. For in-tree boards it can key that off the deploy string, but an
//! out-of-tree board is not in any in-tree table, and expansion cannot resolve
//! one either: a proc-macro's `std::env::var` and file reads are invisible to
//! cargo's fingerprint, so anything it discovers can go stale silently, and a
//! cargo-config value is not even visible when cargo runs from a workspace
//! root.
//!
//! A build script has neither problem: it can declare `rerun-if-changed` on
//! every file it reads, and `cargo::rustc-env` reaches the rustc invocation the
//! macro expands inside. So the Entry package's `build.rs` resolves the board's
//! framework and hands the answer to expansion:
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     nros_build::emit_board_framework();
//! }
//! ```
//!
//! Silent no-op when the board cannot be resolved — the in-tree table still
//! covers in-tree boards, and a build script that failed here would break every
//! leaf that never needed it.

use std::path::{Path, PathBuf};

/// Resolve the board crate's declared framework and emit it for
/// `nros::main!`, plus the `rerun-if-changed` edges that keep it honest.
///
/// Returns the framework name when one was found, for callers that want to
/// log it. Emits nothing when the board declares none (that means
/// `owned-spin`, which is also the macro's default).
pub fn emit_board_framework() -> Option<String> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    let entry_manifest = manifest_dir.join("Cargo.toml");
    println!("cargo::rerun-if-changed={}", entry_manifest.display());

    let (board_name, board_dir) = resolve_board_crate(&manifest_dir)?;
    let board_manifest = board_dir.join("Cargo.toml");
    // The edge cargo CAN see: re-run when the board's descriptor changes.
    println!("cargo::rerun-if-changed={}", board_manifest.display());

    let framework = read_board_framework(&board_manifest)?;
    println!("cargo::rustc-env=NROS_BOARD_FRAMEWORK={framework}");
    println!("cargo::rerun-if-env-changed=NROS_BOARD_FRAMEWORK");
    let _ = board_name;
    Some(framework)
}

/// Find the Entry package's board dependency and where its source lives.
///
/// The board must be a DIRECT dependency: `cargo::rustc-env` and the manifest
/// walk both stop at one hop, and every in-tree Entry package declares it that
/// way. A board reached transitively is not resolved here — deliberately, since
/// silently resolving the wrong one is worse than falling back to the in-tree
/// table.
fn resolve_board_crate(manifest_dir: &Path) -> Option<(String, PathBuf)> {
    let raw = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).ok()?;
    let manifest: toml::Value = toml::from_str(&raw).ok()?;

    // An explicit override wins, for a board crate not named `nros-board-*`.
    let declared = manifest
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .and_then(|e| e.get("board_crate"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let deps = manifest.get("dependencies")?.as_table()?;
    let (name, spec) = deps
        .iter()
        .find(|(name, _)| match &declared {
            Some(d) => *name == d,
            None => name.starts_with("nros-board-"),
        })
        .map(|(n, s)| (n.clone(), s.clone()))?;

    // Shape 1: a path dependency — resolve it relative to the Entry package.
    if let Some(rel) = spec.get("path").and_then(|p| p.as_str()) {
        return Some((name, normalize(&manifest_dir.join(rel))));
    }

    // Shape 2: a bare version plus a `[patch.crates-io]` row in the leaf's own
    // cargo config, which is how in-repo leaves name their board. Only the
    // leaf's own config is read: walking the cargo config hierarchy would mean
    // reimplementing cargo's merge rules, and an out-of-tree consumer keeps
    // everything inline in its own leaf anyway.
    let cfg_path = manifest_dir.join(".cargo").join("config.toml");
    let cfg_raw = std::fs::read_to_string(&cfg_path).ok()?;
    println!("cargo::rerun-if-changed={}", cfg_path.display());
    let cfg: toml::Value = toml::from_str(&cfg_raw).ok()?;
    let rel = cfg
        .get("patch")?
        .get("crates-io")?
        .get(&name)?
        .get("path")?
        .as_str()?;
    Some((name, normalize(&manifest_dir.join(rel))))
}

fn read_board_framework(board_manifest: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(board_manifest).ok()?;
    let value: toml::Value = toml::from_str(&raw).ok()?;
    value
        .get("package")?
        .get("metadata")?
        .get("nros")?
        .get("board")?
        .get("framework")?
        .as_str()
        .map(str::to_string)
}

/// Collapse `..` lexically — the board directory may not exist yet when a build
/// script runs in a fresh checkout, so `canonicalize` is not usable here.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn board(dir: &Path, framework: Option<&str>) {
        let fw = framework
            .map(|f| format!("\n[package.metadata.nros.board]\nframework = \"{f}\"\n"))
            .unwrap_or_default();
        write(
            &dir.join("Cargo.toml"),
            &format!("[package]\nname = \"nros-board-x\"\nversion = \"0.1.0\"\n{fw}"),
        );
    }

    #[test]
    fn resolves_a_path_dependency() {
        let tmp = std::env::temp_dir().join("nros_bf_path");
        let _ = std::fs::remove_dir_all(&tmp);
        let entry = tmp.join("entry");
        write(
            &entry.join("Cargo.toml"),
            "[package]\nname = \"e\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             nros-board-x = { path = \"../board\" }\n",
        );
        board(&tmp.join("board"), Some("embassy"));
        let (name, dir) = resolve_board_crate(&entry).expect("board resolves");
        assert_eq!(name, "nros-board-x");
        assert_eq!(
            read_board_framework(&dir.join("Cargo.toml")).as_deref(),
            Some("embassy")
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The majority in-repo shape: a bare version plus a `[patch.crates-io]`
    /// row. A resolver that only understood path deps would silently skip
    /// these and hand the macro nothing.
    #[test]
    fn resolves_a_patched_version_dependency() {
        let tmp = std::env::temp_dir().join("nros_bf_patch");
        let _ = std::fs::remove_dir_all(&tmp);
        let entry = tmp.join("entry");
        write(
            &entry.join("Cargo.toml"),
            "[package]\nname = \"e\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             nros-board-x = { version = \"*\", features = [\"rmw-zenoh\"] }\n",
        );
        write(
            &entry.join(".cargo").join("config.toml"),
            "[patch.crates-io]\nnros-board-x = { path = \"../board\" }\n",
        );
        board(&tmp.join("board"), Some("rtic"));
        let (_, dir) = resolve_board_crate(&entry).expect("board resolves");
        assert_eq!(
            read_board_framework(&dir.join("Cargo.toml")).as_deref(),
            Some("rtic")
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A board with no framework key means `owned-spin`; the helper must emit
    /// nothing rather than guess, so the macro keeps its own default.
    #[test]
    fn a_board_without_a_framework_key_yields_none() {
        let tmp = std::env::temp_dir().join("nros_bf_none");
        let _ = std::fs::remove_dir_all(&tmp);
        let entry = tmp.join("entry");
        write(
            &entry.join("Cargo.toml"),
            "[package]\nname = \"e\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             nros-board-x = { path = \"../board\" }\n",
        );
        board(&tmp.join("board"), None);
        let (_, dir) = resolve_board_crate(&entry).unwrap();
        assert_eq!(read_board_framework(&dir.join("Cargo.toml")), None);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A transitive board is NOT resolved — the constraint the spike measured,
    /// locked in so it stays a documented limit rather than a surprise.
    #[test]
    fn a_non_direct_board_dependency_is_not_resolved() {
        let tmp = std::env::temp_dir().join("nros_bf_indirect");
        let _ = std::fs::remove_dir_all(&tmp);
        let entry = tmp.join("entry");
        write(
            &entry.join("Cargo.toml"),
            "[package]\nname = \"e\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             some-lib = { path = \"../lib\" }\n",
        );
        assert!(resolve_board_crate(&entry).is_none());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
