//! `nros-toolchain.toml` — the per-project pin (phase-440 W7, RFC-0095 D7/D9).
//!
//! The user-audience twin of `rust-toolchain.toml`: a file beside a project
//! that names which nano-ros builds it. Its shape mirrors Rust's deliberately,
//! because the question it answers is the same one and a reader who knows the
//! other should not have to learn a second idiom:
//!
//! ```toml
//! [toolchain]
//! version = "0.5.0-nros1"
//! ```
//!
//! ## Which version string this is, and why it matters
//!
//! The STORE prefix (`0.5.0-nros1`), not the crate version `nros --version`
//! prints (`0.5.0`). They differ by the `-nrosN` repackaging counter, and
//! `scripts/install.sh` already had to state the same distinction for the same
//! reason: the pin's whole job is to name a directory — `toolchains/<version>`
//! (RFC-0095 D2) or today's `sdk/nros/<version>` — so a pin in the other
//! spelling names nothing.
//!
//! [`running_version`] therefore reads the version off the running binary's own
//! store PREFIX, or out of the `share/nros/VERSION` the release asset carries,
//! and never off the crate version. A binary that is in neither shape — a
//! contributor's `packages/cli/target/**` build — has no store version, and
//! this module says so rather than inventing one. Pinning a version that is not
//! in any store is worse than not pinning: the next build dispatches to a
//! directory nobody can create.
//!
//! ## Pin on first build (D9)
//!
//! > A project with no `nros-toolchain.toml` floats, which for embedded output
//! > is a reproducibility bug waiting to be discovered on someone else's
//! > machine.
//!
//! So the first `nros build` WRITES the pin it used and says so — the
//! `Cargo.lock` rule of issues 0359/0378, one layer up. [`write`] refuses to
//! overwrite, which is the other half of that rule: a pin moves only when a dev
//! means it. Nothing in this crate calls [`write`] on an existing file, so
//! `nros self update` — which moves the fronted binary and nothing else
//! (RFC-0095 D7) — cannot move a pin even by accident.
//!
//! ## What W6 already assumed about this file
//!
//! `store::PIN_FILE_NAMES` names `nros-toolchain.toml` and reads any file it
//! does not have a schema for as generic TOML, taking every string value at any
//! depth as a version rule (see `store::load_pin_file`). That was written
//! before this schema existed, deliberately, so that W7 could not disarm the
//! delete guard by picking a key name W6 did not guess. The schema above stays
//! inside that envelope: `version` is a string value, so
//! `nros toolchain uninstall` sees it without being taught anything.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr, bail};
use serde::Deserialize;

/// The pin file's name. `store::PIN_FILE_NAMES` names the same constant, so
/// there is one spelling of it in the crate.
pub const FILE_NAME: &str = "nros-toolchain.toml";

/// The `[toolchain]` table.
#[derive(Debug, Deserialize)]
struct PinFile {
    toolchain: ToolchainTable,
}

#[derive(Debug, Deserialize)]
struct ToolchainTable {
    version: String,
}

/// A pin that was found and read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    /// The file it came from — printed in every message, because "which pin"
    /// is the first question when two projects disagree.
    pub path: PathBuf,
    /// The store version it names.
    pub version: String,
}

/// The nearest `nros-toolchain.toml` at or above `start`.
///
/// Ancestors, not just `start`: RFC-0095's own open question — *"one
/// `nros-toolchain.toml` at the workspace root is the obvious shape, but it
/// must survive `nros build` invoked from a subdirectory"* — and this is the
/// answer. Same walk `cargo` does for a workspace root, and the same one
/// `store::discover_pin_files` already does for the reclaim verbs.
#[must_use]
pub fn find(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Read one pin file.
///
/// A file that exists and cannot be read as this schema is an ERROR, never an
/// absent pin — the same asymmetry `store::load_pin_file` states: "no pin" and
/// "I could not tell" must not reach a caller as one value.
pub fn load(path: &Path) -> Result<Pin> {
    let raw = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("read the toolchain pin {}", path.display()))?;
    let parsed: PinFile = toml::from_str(&raw).wrap_err_with(|| {
        format!(
            "parse the toolchain pin {} — it must carry\n\n    [toolchain]\n    version = \"<store version>\"\n",
            path.display()
        )
    })?;
    let version = parsed.toolchain.version.trim().to_string();
    if version.is_empty() {
        bail!(
            "{} names an empty `[toolchain] version`. A pin with no version is \
             not a floating project, it is an unreadable one — delete the file \
             to float, or name a version.",
            path.display()
        );
    }
    Ok(Pin {
        path: path.to_path_buf(),
        version,
    })
}

/// [`find`] then [`load`]. `Ok(None)` means "no pin here"; `Err` means "there
/// is one and it is broken".
pub fn find_and_load(start: &Path) -> Result<Option<Pin>> {
    match find(start) {
        Some(p) => load(&p).map(Some),
        None => Ok(None),
    }
}

/// The file this writes, byte for byte. Split out so a test can assert the
/// schema without a filesystem, and so the header lives in one place.
#[must_use]
pub fn render(version: &str) -> String {
    format!(
        "# nano-ros toolchain pin — written by `nros build` on the first build\n\
         # of this project (RFC-0095 D9). It names the nano-ros that builds it,\n\
         # the way `rust-toolchain.toml` names a Rust.\n\
         #\n\
         # It is SOURCE: commit it. Without it the build floats, and firmware\n\
         # built from a floating toolchain is only reproducible by accident.\n\
         #\n\
         # To move it, edit this line. Nothing else does — `nros self update`\n\
         # moves the launcher and no pin (RFC-0095 D7), so an update never\n\
         # silently rebuilds your image with a different codegen.\n\
         [toolchain]\n\
         version = \"{version}\"\n"
    )
}

/// Write a pin into `dir`. REFUSES if one is already there.
///
/// The refusal is the D9 half that is easy to leave out: writing "the version
/// it used" every build would move a pin on every `nros self update`, which is
/// exactly the silent rebuild D7 forbids.
pub fn write(dir: &Path, version: &str) -> Result<PathBuf> {
    let path = dir.join(FILE_NAME);
    if path.exists() {
        bail!(
            "{} already exists — refusing to overwrite a pin. A pin moves only \
             when a dev means it (issues 0359/0378, one layer up); edit the \
             file to change it.",
            path.display()
        );
    }
    std::fs::write(&path, render(version))
        .wrap_err_with(|| format!("write the toolchain pin {}", path.display()))?;
    Ok(path)
}

/// How [`running_version`] learned the version — printed beside it, because a
/// version with no provenance is a number a reader has to trust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionOrigin {
    /// The binary sits at `<store>/{toolchains,sdk/nros}/<version>/bin/nros`,
    /// so the store itself named the version.
    StorePrefix,
    /// `<prefix>/share/nros/VERSION`, written into the release asset by
    /// `.github/workflows/release-nros.yml` from `nros-sdk-index.toml`.
    VersionFile,
}

impl VersionOrigin {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            VersionOrigin::StorePrefix => "store prefix",
            VersionOrigin::VersionFile => "share/nros/VERSION",
        }
    }
}

/// The STORE version of the running binary, if it has one.
///
/// `None` for a checkout build (`packages/cli/target/**`) and for any other
/// layout we do not model — which is the honest answer, not a gap: such a
/// binary corresponds to no store entry, so there is nothing a pin naming it
/// could dispatch to.
///
/// The store prefix wins over the VERSION file when both are available. They
/// agree in every asset `install.sh` writes (it reads the file to CHOOSE the
/// prefix), and where they could disagree the directory is the one that
/// resolves.
#[must_use]
pub fn running_version(exe: &Path) -> Option<(String, VersionOrigin)> {
    if let Some(v) = version_from_store_prefix(exe) {
        return Some((v, VersionOrigin::StorePrefix));
    }
    let prefix = exe.parent()?.parent()?;
    let file = prefix.join("share").join("nros").join("VERSION");
    let raw = std::fs::read_to_string(file).ok()?;
    let v = raw.trim().to_string();
    if v.is_empty() {
        return None;
    }
    Some((v, VersionOrigin::VersionFile))
}

/// `<version>` when `exe` is `<...>/toolchains/<version>/bin/nros` or
/// `<...>/sdk/nros/<version>/bin/nros`.
///
/// Both spellings, because RFC-0095 D2's `toolchains/<version>` is a RENAME of
/// the `sdk/nros/<version>` `scripts/install.sh` writes today and W7 is not the
/// wave that performs it. A reader who only taught this the new name would have
/// written a dispatcher that cannot recognise a single installed host.
fn version_from_store_prefix(exe: &Path) -> Option<String> {
    // .../<version>/bin/<exe>
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let version_dir = bin.parent()?;
    let version = version_dir.file_name()?.to_str()?.to_string();
    let category = version_dir.parent()?;
    match category.file_name()?.to_str()? {
        "toolchains" => Some(version),
        "nros" if category.parent().and_then(Path::file_name)? == "sdk" => Some(version),
        _ => None,
    }
}

/// Where a pinned toolchain's binary would live under `root`, newest layout
/// first. CONSTRUCTED from the version, never searched for — the
/// `sdk_store::tool_dir` rule (issue 0625).
#[must_use]
pub fn candidate_bins(root: &Path, version: &str) -> Vec<PathBuf> {
    vec![
        root.join("toolchains")
            .join(version)
            .join("bin")
            .join("nros"),
        root.join("sdk")
            .join("nros")
            .join(version)
            .join("bin")
            .join("nros"),
    ]
}

/// The first candidate that is actually there.
#[must_use]
pub fn installed_bin(root: &Path, version: &str) -> Option<PathBuf> {
    candidate_bins(root, version)
        .into_iter()
        .find(|p| p.is_file())
}

/// What [`pin_on_first_build`] did, so the caller prints it and a test asserts
/// it without reading stdout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinOutcome {
    /// A pin was already there. Never touched.
    Already(Pin),
    /// D9's case: none existed, so one was written.
    Wrote { path: PathBuf, version: String },
    /// The workspace is inside a nano-ros checkout — the contributor audience
    /// (RFC-0095 D0/D5), whose nano-ros is that clone at whatever HEAD is.
    /// There is no version to name and nothing a pin would change.
    InCheckout(PathBuf),
    /// The running binary has no store version, so there is no honest version
    /// to write. Reported, not silent: an unpinned project is D9's bug, and the
    /// user needs to know they still have it.
    NoRunningVersion,
}

/// RFC-0095 D9, performed.
///
/// `exe` is passed in rather than read from `current_exe()` so the decision is
/// a pure function of two paths and its tests need no installed store on the
/// host running them — the same argument `stale_guard::refuse_if_stale` makes
/// for `workspace`, and `store_reclaim.rs` for the pin search directory.
pub fn pin_on_first_build(workspace: &Path, exe: &Path) -> Result<PinOutcome> {
    if let Some(root) = crate::abi_guard::find_monorepo_root(workspace) {
        return Ok(PinOutcome::InCheckout(root));
    }
    if let Some(existing) = find_and_load(workspace)? {
        return Ok(PinOutcome::Already(existing));
    }
    let Some((version, _origin)) = running_version(exe) else {
        return Ok(PinOutcome::NoRunningVersion);
    };
    let path = write(workspace, &version)?;
    Ok(PinOutcome::Wrote { path, version })
}

/// The line `nros build` prints for an outcome — one place, so the message and
/// the decision cannot drift.
#[must_use]
pub fn describe(outcome: &PinOutcome) -> Option<String> {
    match outcome {
        PinOutcome::Already(p) => Some(format!(
            "nros build: toolchain {} (pinned by {})",
            p.version,
            p.path.display()
        )),
        PinOutcome::Wrote { path, version } => Some(format!(
            "nros build: pinned this project to nano-ros {version}\n\
             \x20   wrote {}\n\
             \x20   Commit it. It is what makes this build reproducible on \
             another machine (RFC-0095 D9).",
            path.display()
        )),
        // A contributor is told nothing: their nano-ros is the clone they are
        // standing in, they know it, and a line on every build would be noise.
        PinOutcome::InCheckout(_) => None,
        PinOutcome::NoRunningVersion => Some(
            "nros build: warning: this project is UNPINNED and this `nros` has no \
             store version to pin it to.\n\
             \x20   A floating toolchain builds different firmware on a different \
             machine.\n\
             \x20   Install a released nros (scripts/install.sh) and build again, \
             or write nros-toolchain.toml yourself."
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_round_trips_through_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "0.5.0-nros1").unwrap();
        let pin = load(&path).unwrap();
        assert_eq!(pin.version, "0.5.0-nros1");
    }

    #[test]
    fn a_pin_with_no_version_is_an_error_not_an_absent_pin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "[toolchain]\nversion = \"\"\n").unwrap();
        assert!(load(&path).is_err());
        // And a file that is not this schema at all.
        std::fs::write(&path, "channel = \"stable\"\n").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn store_prefix_beats_a_version_file_and_a_checkout_has_neither() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path();
        for rel in ["toolchains/1.2.3-nros4", "sdk/nros/9.9.9-nros7"] {
            let bin = store.join(rel).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("nros"), b"x").unwrap();
        }
        assert_eq!(
            running_version(&store.join("toolchains/1.2.3-nros4/bin/nros")),
            Some(("1.2.3-nros4".to_string(), VersionOrigin::StorePrefix))
        );
        assert_eq!(
            running_version(&store.join("sdk/nros/9.9.9-nros7/bin/nros")),
            Some(("9.9.9-nros7".to_string(), VersionOrigin::StorePrefix))
        );
        // A checkout build corresponds to no store entry.
        let checkout = store.join("packages/cli/target/debug");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::write(checkout.join("nros"), b"x").unwrap();
        assert_eq!(running_version(&checkout.join("nros")), None);
    }
}
