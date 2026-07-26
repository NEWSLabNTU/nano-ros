//! phase-307 W4 (second half) — the `nros::main!` bake reads source-metadata
//! sidecars.
//!
//! W4 landed the `max(model_wiring, recorded)` rule in the CLI bake only, and
//! deliberately left the macro on the model bound: a macro expansion had no
//! guarantee that anything had produced a sidecar. W2 removed that objection —
//! `nros sync` refreshes sidecars as its last step and stamps each with a
//! content-addressed provenance digest — so the macro can now read them.
//!
//! The objection that DID kill the naive issue-0257 approach still stands and
//! is respected here: nothing in this module shells out. Spawning a nested
//! `cargo build` during proc-macro expansion is the trap. Reading a JSON file
//! that a prior build step already wrote is not.
//!
//! Why the sidecar matters to the MACRO specifically: on boards that honor
//! per-entry sizing the macro's derived value IS the executor's capacity
//! (`Executor::open_sized`). A node with one modelled subscription and five
//! timers derives 3 slots from the model and dies at boot on the third timer —
//! the CLI's check cannot save it, because the CLI only refuses over-capacity
//! systems, it does not size this executor.
//!
//! No schema struct is mirrored here, and no counting rule either. The slot
//! accounting lives in `nros_orchestration_ir::sidecar_slots` — the same crate
//! that holds the merge rule — because the CLI bake reads these sidecars too
//! and the two must agree: the CLI REFUSES an over-capacity system while this
//! macro SIZES the executor, so a disagreement is an image that passes the
//! check and dies at boot anyway. This module owns discovery and file I/O only.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// `(package, executable)` → recorded callback slots.
pub type RecordedSlots = BTreeMap<(String, String), usize>;

/// What [`collect`] found: the counts, plus the files they came from.
///
/// The paths matter as much as the counts. The macro emits `include_bytes!`
/// rebuild stamps for every file that influenced its decision, and a sidecar
/// now does: edit a node's source, `nros sync` rewrites its sidecar, and the
/// Entry must re-expand or it keeps an executor sized for the old topology —
/// the museum-data failure in its most literal form.
#[derive(Debug, Default)]
pub struct Sidecars {
    pub slots: RecordedSlots,
    pub paths: Vec<PathBuf>,
}

/// Collect every sidecar reachable from an Entry package directory.
///
/// Walks up to the workspace root (the first ancestor with a `src/` directory
/// holding packages, else the ancestor holding the entry itself), then reads
/// `<pkg>/metadata/*.json` for each package. Absence is never an error: no
/// sidecars means the caller keeps the model bound, which is exactly the
/// pre-307 behaviour.
pub fn collect(entry_manifest_dir: &Path) -> Sidecars {
    let mut out = Sidecars::default();
    let Some(src_root) = workspace_src_root(entry_manifest_dir) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&src_root) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path().join("metadata");
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some((key, slots)) = read_sidecar(&path) {
                let cell = out.slots.entry(key).or_insert(0);
                *cell = (*cell).max(slots);
                out.paths.push(path);
            }
        }
    }
    out
}

/// The directory holding sibling packages. For the canonical colcon layout
/// that is `<ws>/src`; for a standalone entry it is the entry's own parent.
fn workspace_src_root(entry_manifest_dir: &Path) -> Option<PathBuf> {
    for ancestor in entry_manifest_dir.ancestors() {
        let src = ancestor.join("src");
        // `<ws>/src/<pkg>/package.xml` — the colcon shape. Guard on a package
        // actually being there so an entry's own `src/` (its Rust sources) is
        // not mistaken for the workspace's.
        if src.is_dir()
            && std::fs::read_dir(&src).ok().is_some_and(|mut d| {
                d.any(|e| e.is_ok_and(|e| e.path().join("package.xml").is_file()))
            })
        {
            return Some(src);
        }
    }
    entry_manifest_dir.parent().map(Path::to_path_buf)
}

/// Read one sidecar and hand it to the shared accounting rule.
fn read_sidecar(path: &Path) -> Option<((String, String), usize)> {
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    nros_orchestration_ir::sidecar_slots::slots_of_component(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nros-sidecars-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/talker_pkg/metadata")).unwrap();
        std::fs::create_dir_all(dir.join("src/native_entry/src")).unwrap();
        std::fs::write(dir.join("src/talker_pkg/package.xml"), "<package/>").unwrap();
        std::fs::write(dir.join("src/native_entry/package.xml"), "<package/>").unwrap();
        dir
    }

    fn sidecar(pkg: &str, exec: &str, subs: usize, timers: usize) -> String {
        let arr = |n: usize| vec!["{}"; n].join(",");
        format!(
            r#"{{"package":"{pkg}","executable":"{exec}","nodes":[{{"subscribers":[{}],
               "timers":[{}],"services":[],"actions":[],"publishers":[{{}},{{}}]}}]}}"#,
            arr(subs),
            arr(timers)
        )
    }

    /// The shape that motivates the whole phase: one subscription the model can
    /// see, plus timers it cannot.
    #[test]
    fn counts_subs_and_timers_but_not_publishers() {
        let dir = ws("counts");
        std::fs::write(
            dir.join("src/talker_pkg/metadata/talker.json"),
            sidecar("talker_pkg", "talker", 1, 5),
        )
        .unwrap();
        let found = collect(&dir.join("src/native_entry"));
        assert_eq!(found.paths.len(), 1, "the sidecar is tracked for rebuilds");
        assert_eq!(
            found
                .slots
                .get(&("talker_pkg".into(), "talker".into()))
                .copied(),
            Some(6),
            "1 sub + 5 timers = 6 slots; the 2 publishers take none"
        );
    }

    /// A workspace with no sidecars must yield nothing rather than failing —
    /// the caller falls back to the model bound (pre-307 behaviour).
    #[test]
    fn absent_sidecars_are_not_an_error() {
        let dir = ws("absent");
        assert!(collect(&dir.join("src/native_entry")).slots.is_empty());
    }

    /// Malformed JSON is skipped, not fatal: a bake that died because a stale
    /// sidecar existed would be worse than the bug this phase fixes.
    #[test]
    fn garbage_sidecar_is_skipped() {
        let dir = ws("garbage");
        std::fs::write(dir.join("src/talker_pkg/metadata/talker.json"), "not json").unwrap();
        let found = collect(&dir.join("src/native_entry"));
        assert!(found.slots.is_empty() && found.paths.is_empty());
    }

    /// An entry's own `src/` (Rust sources, no `package.xml`) must not be
    /// mistaken for the workspace's package root.
    #[test]
    fn entrys_own_src_dir_is_not_the_workspace_root() {
        let dir = ws("own_src");
        std::fs::write(
            dir.join("src/talker_pkg/metadata/talker.json"),
            sidecar("talker_pkg", "talker", 2, 0),
        )
        .unwrap();
        // Walk starts inside `<ws>/src/native_entry`, which HAS a `src/`.
        let found = collect(&dir.join("src/native_entry"));
        assert_eq!(
            found.slots.len(),
            1,
            "resolved to <ws>/src, not <entry>/src"
        );
    }
}
