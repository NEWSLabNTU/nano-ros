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
//! `profile` — and never leave generation. This module is for what nobody
//! foresaw.
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
