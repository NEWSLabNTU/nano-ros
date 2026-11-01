//! The escape hatch, and the stamp that keeps it honest (phase-383 W7, RFC-0065 D5).
//!
//! ## Why this is the LAST resort, not the first
//!
//! An earlier draft of RFC-0065 made `nros eject` the primary escape. The
//! closest prior art ran that experiment and abandoned it: Expo shipped
//! `eject`, found it *"a one-way door for most projects"*, and replaced it with
//! always-generate plus declarative config plugins — `eject` is now
//! *"historical vocabulary"* in their docs. Their stated reason transfers
//! exactly: *"If you modify the generated directories manually then you risk
//! losing your changes the next time you run `prebuild --clean`."*
//!
//! So the escapes that are KNOWN are declarations on the image — `panic`,
//! `profile`, `before_boot` (a pre-boot step, the 2026-10-24 amendment) — and
//! never leave generation. This module is for what nobody foresaw.
//!
//! ## What materialising does and does NOT freeze
//!
//! Narrower than it looks. A generated entry is a one-line
//! `nros::main!(launch = …)`, and that macro reads the launch XML **at
//! expansion time** (its tracked inputs are "launch.xml, every `package.xml`
//! the pkg-index walked"). RMW and capability selection flow through the
//! `*_nros_selection` facade `nros sync` regenerates. So adding a node to a
//! launch file reaches a materialised entry on the next compile: **the
//! derivation stays live.**
//!
//! What freezes is the SHELL — `#![no_std]` / `#![no_main]`, the panic policy,
//! board boilerplate like `esp_app_desc!()`, `[profile.release]`, and
//! `[[bin]]`-vs-`[lib]`. If nano-ros later changes what an entry for a board
//! must look like, a materialised one silently keeps the old shape. That is
//! issue 0798's class one layer up — a hardcoded entry while the system around
//! it moved.
//!
//! ## Hence the stamp, and why it WARNS
//!
//! [`Stamp`] records the generator version and the board shape an entry was cut
//! for, and [`check`] reports drift. It **must never be an error** (W7.c):
//! `autoware-safety-island` carries `freertos_main.cpp`, `board_init.c`,
//! `cp15_arm.S` and four `.ld` fragments, and will hold a materialised entry
//! **forever, by design**. Erroring would break a legitimate downstream
//! permanently — and a tool that breaks the honest user to protect the careless
//! one has chosen wrong.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The marker file a materialised entry carries.
pub const STAMP_FILE: &str = ".nros-materialized.toml";

/// What an entry was cut from, so drift is visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The image this entry was materialised from.
    pub image: String,
    /// Board id at the time — a board change usually means a shape change.
    pub board: String,
    /// Board's platform token; the shape is a property of this, not the board.
    pub platform: String,
    /// `nros` version that generated it.
    pub generator: String,
    /// The entry-kind the board declared — `hosted-main`, `board-run`,
    /// `zephyr-staticlib`. THIS is the shape: it decides `fn main` vs
    /// `rust_main`, `[[bin]]` vs `[lib]`, `no_main` or not.
    pub entry_kind: String,
}

impl Stamp {
    /// The stamp a freshly generated entry would carry.
    #[must_use]
    pub fn current(image: &str, board: &str, platform: &str, entry_kind: &str) -> Self {
        Self {
            image: image.to_string(),
            board: board.to_string(),
            platform: platform.to_string(),
            generator: env!("CARGO_PKG_VERSION").to_string(),
            entry_kind: entry_kind.to_string(),
        }
    }

    /// Write beside a materialised entry.
    pub fn write(&self, entry_dir: &Path) -> Result<(), String> {
        let body = format!(
            "# Written by `nros materialize`. This entry is YOURS now — \
             `nros build` will not regenerate it.\n\
             #\n\
             # Recorded so drift is visible: if nano-ros later changes what an\n\
             # entry for this board must look like, `nros build` says so. It is\n\
             # a WARNING, never an error — a project that owns its entry on\n\
             # purpose must keep building.\n\n{}",
            toml::to_string_pretty(self).map_err(|e| format!("serialising stamp: {e}"))?
        );
        std::fs::write(entry_dir.join(STAMP_FILE), body)
            .map_err(|e| format!("writing {}: {e}", entry_dir.join(STAMP_FILE).display()))
    }

    /// Read from a materialised entry; `None` when there is none.
    #[must_use]
    pub fn read(entry_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(entry_dir.join(STAMP_FILE)).ok()?;
        toml::from_str(&text).ok()
    }
}

/// Drift between what an entry was cut for and what it would be cut for now.
///
/// Returns the WARNINGS to print. Empty means no drift. Never an error — see
/// the module docs.
#[must_use]
pub fn check(entry_dir: &Path, current: &Stamp) -> Vec<String> {
    let Some(old) = Stamp::read(entry_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if old.entry_kind != current.entry_kind {
        out.push(format!(
            "materialised entry {} was cut for entry-kind `{}`, but board `{}` \
             now needs `{}`. The generated shape has changed — regenerate with \
             `nros materialize {}` and re-apply your edits, or verify the entry \
             still matches the board.",
            entry_dir.display(),
            old.entry_kind,
            current.board,
            current.entry_kind,
            current.image
        ));
    }
    if old.board != current.board {
        out.push(format!(
            "materialised entry {} was cut for board `{}`, but image `{}` now \
             names `{}`.",
            entry_dir.display(),
            old.board,
            current.image,
            current.board
        ));
    }
    if old.generator != current.generator {
        // Informational, and deliberately last: a version bump alone usually
        // changes nothing about the shape. It is reported so a user chasing a
        // shape difference can see it, not because it is itself a problem.
        out.push(format!(
            "materialised entry {} was generated by nros {}; this is nros {}. \
             No shape change detected — noted only so the provenance is visible.",
            entry_dir.display(),
            old.generator,
            current.generator
        ));
    }
    out
}

/// Whether `entry_dir` is a materialised entry the builder must not regenerate.
#[must_use]
pub fn is_materialized(entry_dir: &Path) -> bool {
    entry_dir.join(STAMP_FILE).is_file()
}

/// The file, inside an image's cargo target dir, naming the cargo root that
/// last built into it (issue 2167).
pub const ENTRY_ROOT_MARKER: &str = ".nros-entry-root";

/// Which cargo root an image's entry builds from, and what to do when it
/// changes (issue 2167).
///
/// The generated entry (`build/<coord>/<entry>/`) and a materialised or
/// hand-written one (`src/<entry>/`) are two cargo roots with the same package
/// name, bin name and target dir. Each is the root of its own one-member
/// workspace, so cargo derives the SAME `-C metadata` for both and keeps one
/// `deps/<entry>-<hash>` unit, whose dep-info lists `src/main.rs` relative to
/// whichever package built it last. So after `rm -r src/<entry>` the
/// regenerated entry's older `main.rs` reads as fresh, the build exits 0, and
/// the binary is still the removed program — measured. Cargo cannot see the
/// switch, so the builder tells it: when the recorded root differs from this
/// build's, `clean` (a `cargo clean -p <entry>` against this root's settings)
/// runs before the build. It removes only that package's units, never the
/// dependencies'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRoot {
    /// `<target dir>/`[`ENTRY_ROOT_MARKER`].
    pub marker: std::path::PathBuf,
    /// The manifest directory this build uses.
    pub manifest_dir: std::path::PathBuf,
    /// Run when the root changed.
    pub clean: super::handoff::Handoff,
}

impl EntryRoot {
    /// Whether the target dir was last built from a DIFFERENT root. A target
    /// dir with no marker but some content predates the marker, so its root is
    /// unknown and it is cleaned once; an absent target dir needs nothing.
    #[must_use]
    pub fn changed(&self) -> bool {
        match std::fs::read_to_string(&self.marker) {
            Ok(prev) => Path::new(prev.trim()) != self.manifest_dir,
            Err(_) => self
                .marker
                .parent()
                .and_then(|t| std::fs::read_dir(t).ok())
                .is_some_and(|mut d| d.next().is_some()),
        }
    }

    /// Record this build's root.
    pub fn record(&self) -> Result<(), String> {
        if let Some(t) = self.marker.parent() {
            std::fs::create_dir_all(t).map_err(|e| format!("creating {}: {e}", t.display()))?;
        }
        std::fs::write(&self.marker, format!("{}\n", self.manifest_dir.display()))
            .map_err(|e| format!("writing {}: {e}", self.marker.display()))
    }
}

/// Rewrite a copied entry manifest's relative `path =` dependencies so they
/// resolve from `to_dir` as they did from `from_dir` (issue 2175).
///
/// The generator writes every path dep relative to the entry it emits
/// (phase-383 W3.c), so a byte-for-byte copy to `src/<entry>/` points each one
/// at the wrong directory, and the first build after materialising — the one
/// the verb's own message asks for — fails to load the manifest. Rebased:
/// every dependency table (`[dependencies]`, `[dev-dependencies]`,
/// `[build-dependencies]`, and each under `[target.*]`) and every
/// `[patch.<source>]` row. `[lib]` / `[[bin]]` paths are relative to the
/// package itself and travel with it, so they are left alone. Formatting and
/// comments are kept (`toml_edit`).
pub fn rebase_manifest_paths(text: &str, from_dir: &Path, to_dir: &Path) -> Result<String, String> {
    use toml_edit::{DocumentMut, Item};

    fn rebase_deps(
        table: &mut dyn toml_edit::TableLike,
        from: &Path,
        to: &Path,
    ) -> Result<(), String> {
        for (_, dep) in table.iter_mut() {
            let Some(dep) = dep.as_table_like_mut() else {
                continue;
            };
            let Some(path) = dep.get("path").and_then(Item::as_str).map(str::to_string) else {
                continue;
            };
            let rebased = super::paths::rebase_relative(from, to, &path)?;
            if let Some(v) = dep.get_mut("path").and_then(Item::as_value_mut) {
                let decor = v.decor().clone();
                *v = rebased.into();
                *v.decor_mut() = decor;
            }
        }
        Ok(())
    }

    const DEP_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("parsing the entry manifest: {e}"))?;
    for key in DEP_TABLES {
        if let Some(t) = doc.get_mut(key).and_then(Item::as_table_like_mut) {
            rebase_deps(t, from_dir, to_dir)?;
        }
    }
    if let Some(targets) = doc.get_mut("target").and_then(Item::as_table_like_mut) {
        for (_, cfg) in targets.iter_mut() {
            let Some(cfg) = cfg.as_table_like_mut() else {
                continue;
            };
            for key in DEP_TABLES {
                if let Some(t) = cfg.get_mut(key).and_then(Item::as_table_like_mut) {
                    rebase_deps(t, from_dir, to_dir)?;
                }
            }
        }
    }
    if let Some(patch) = doc.get_mut("patch").and_then(Item::as_table_like_mut) {
        for (_, source) in patch.iter_mut() {
            if let Some(t) = source.as_table_like_mut() {
                rebase_deps(t, from_dir, to_dir)?;
            }
        }
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(kind: &str, board: &str) -> Stamp {
        Stamp {
            image: "freertos".to_string(),
            board: board.to_string(),
            platform: "freertos".to_string(),
            generator: "0.5.0".to_string(),
            entry_kind: kind.to_string(),
        }
    }

    #[test]
    fn rebasing_the_manifest_moves_every_dep_path_and_nothing_else() {
        // issue 2175 — the generated entry's paths, as measured.
        let text = "\
[package]
name = \"linux_entry\"

[[bin]]
name = \"linux_entry\"
path = \"src/main.rs\"

[dependencies]
linux_entry_nros_selection = { path = \"../../../generated/nros-selection/linux_entry\" }
nros = { path = \"../../../../../../packages/api/nros\", default-features = false } # umbrella
ctrl_pkg = { path = \"../../../src/ctrl_pkg\", default-features = false }
heapless = \"0.8\"

[target.'cfg(unix)'.build-dependencies]
helper = { path = \"../../../src/helper\" }

[patch.crates-io]
nros-log = { path = \"../../../../../../packages/core/nros-log\" }
";
        let out = rebase_manifest_paths(
            text,
            Path::new("/r/examples/workspaces/rt/build/posix-zenoh/linux_entry"),
            Path::new("/r/examples/workspaces/rt/src/linux_entry"),
        )
        .expect("rebases");
        for want in [
            "path = \"../../generated/nros-selection/linux_entry\"",
            "path = \"../../../../../packages/api/nros\", default-features = false } # umbrella",
            "path = \"../ctrl_pkg\"",
            "path = \"../helper\"",
            "path = \"../../../../../packages/core/nros-log\"",
            "path = \"src/main.rs\"",
            "heapless = \"0.8\"",
        ] {
            assert!(out.contains(want), "missing `{want}` in:\n{out}");
        }
    }

    #[test]
    fn an_entry_root_change_is_seen_until_it_is_recorded() {
        // issue 2167 — the root flip cargo cannot see.
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        let root = |dir: &str| EntryRoot {
            marker: target.join(ENTRY_ROOT_MARKER),
            manifest_dir: tmp.path().join(dir),
            clean: super::super::handoff::Handoff::new("cargo", ["clean"]),
        };
        let generated = root("build/posix-zenoh/linux_entry");
        let owned = root("src/linux_entry");

        assert!(!generated.changed(), "no target dir yet: nothing to clean");
        std::fs::create_dir_all(target.join("debug")).unwrap();
        assert!(
            generated.changed(),
            "a target dir with no marker has an unknown root"
        );
        generated.record().unwrap();
        assert!(!generated.changed(), "same root: no clean");
        assert!(owned.changed(), "materialised: clean");
        owned.record().unwrap();
        assert!(
            generated.changed(),
            "materialised entry removed: clean again"
        );
    }

    #[test]
    fn a_stamp_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let s = stamp("board-run", "mps2-an385-freertos");
        s.write(tmp.path()).expect("writes");
        assert_eq!(Stamp::read(tmp.path()).expect("reads"), s);
    }

    #[test]
    fn an_unmaterialized_dir_has_no_stamp_and_no_drift() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_materialized(tmp.path()));
        assert!(check(tmp.path(), &stamp("board-run", "b")).is_empty());
    }

    #[test]
    fn a_changed_entry_kind_warns_and_says_what_to_do() {
        // The drift that matters: entry-kind decides `fn main` vs `rust_main`,
        // [[bin]] vs [lib], no_main or not. A materialised entry keeping the
        // old shape is issue 0798's class one layer up.
        let tmp = tempfile::tempdir().unwrap();
        stamp("board-run", "mps2-an385-freertos")
            .write(tmp.path())
            .unwrap();
        let now = stamp("zephyr-staticlib", "mps2-an385-freertos");
        let w = check(tmp.path(), &now);
        assert!(!w.is_empty());
        assert!(w[0].contains("board-run"), "{w:?}");
        assert!(w[0].contains("zephyr-staticlib"), "{w:?}");
        assert!(w[0].contains("nros materialize"), "names the fix: {w:?}");
    }

    #[test]
    fn a_changed_board_warns() {
        let tmp = tempfile::tempdir().unwrap();
        stamp("board-run", "mps2-an385-freertos")
            .write(tmp.path())
            .unwrap();
        let w = check(tmp.path(), &stamp("board-run", "s32z270-freertos"));
        assert!(w.iter().any(|m| m.contains("s32z270-freertos")), "{w:?}");
    }

    #[test]
    fn an_identical_stamp_produces_no_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let s = stamp("board-run", "mps2-an385-freertos");
        s.write(tmp.path()).unwrap();
        assert!(check(tmp.path(), &s).is_empty());
    }

    #[test]
    fn a_generator_bump_alone_is_reported_as_provenance_not_a_problem() {
        let tmp = tempfile::tempdir().unwrap();
        stamp("board-run", "b").write(tmp.path()).unwrap();
        let mut now = stamp("board-run", "b");
        now.generator = "0.6.0".to_string();
        let w = check(tmp.path(), &now);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains("No shape change detected"),
            "a version bump alone must not read as a defect: {w:?}"
        );
    }

    #[test]
    fn drift_is_never_an_error_only_a_warning() {
        // W7.c — autoware-safety-island carries freertos_main.cpp, cp15_arm.S
        // and four .ld fragments, and will hold a materialised entry forever by
        // design. An error would break a legitimate downstream permanently.
        //
        // The signature IS the guarantee: `check` returns Vec<String>, so there
        // is no error variant to return. This test exists so that changing the
        // signature to Result requires deleting it.
        let tmp = tempfile::tempdir().unwrap();
        stamp("board-run", "old").write(tmp.path()).unwrap();
        let warnings: Vec<String> = check(tmp.path(), &stamp("zephyr-staticlib", "new"));
        assert!(!warnings.is_empty(), "drift IS detected");
    }

    #[test]
    fn the_stamp_file_says_the_entry_is_owned_by_the_user() {
        let tmp = tempfile::tempdir().unwrap();
        stamp("board-run", "b").write(tmp.path()).unwrap();
        let body = std::fs::read_to_string(tmp.path().join(STAMP_FILE)).unwrap();
        assert!(body.contains("YOURS now"), "{body}");
        assert!(body.contains("will not regenerate"), "{body}");
        assert!(body.contains("never an error"), "{body}");
    }
}
