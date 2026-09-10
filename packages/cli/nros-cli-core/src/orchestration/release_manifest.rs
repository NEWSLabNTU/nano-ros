//! `share/nros/manifest.toml` — what a release is MADE OF (RFC-0097 D7,
//! phase-443 W2).
//!
//! ```toml
//! version  = "0.7.9"      # the toolchain: CLI + codegen + runtime
//! codegen  = 7            # the ONLY field that can invalidate existing output
//! index    = "2026-09-10" # pointers into nano-ros-sdk
//! nano_ros = "abc1234"    # the runtime commit
//! ```
//!
//! ## Why a file, and why it replaces three assertions
//!
//! Four axes carry a version and one artifact carries all four, so any of them
//! moving forces a release of the whole. Measured on `main`, 2026-09-10:
//! `packages/cli` **710** commits per 60 days, the runtime crates the CLI
//! compiles **248**, `nros-sdk-index.toml` **70**, and `NROS_CODEGEN_VERSION` —
//! the only axis that can make a user's *existing* generated code wrong
//! (RFC-0090) — **2, all time**.
//!
//! `release-nros.yml` did not merely bundle them, it **asserted they were
//! equal**: the release version against `NROS_CODEGEN_VERSION`, and against
//! `[tool.nros] version` in the index. The coupling was mechanised, which is
//! why it never drifted; it is also why nothing could move alone.
//!
//! D7 replaces asserting with RECORDING. `0.7.1` and `0.7.9` both declaring
//! `codegen = 7` is the whole feature: a user takes either and nothing is
//! re-emitted. `0.8.0` declaring `codegen = 8` is a compatibility event, and
//! [`CodegenDelta`] is how a user is told *before* they take it.
//!
//! ## One equality survives, because it is the one that is true
//!
//! The manifest's `codegen` must equal the `NROS_CODEGEN_VERSION` of the tree
//! the binary was built from. That is not a coupling between axes; it is the
//! manifest being *about* the artifact. It is asserted at release time by
//! `.github/workflows/release-nros.yml`, and the fact that it is asserted is
//! itself gated by `check-release-manifest`.
//!
//! It cannot be got wrong here, either: [`stamp`] takes the number from
//! [`crate::abi_guard::EMITTED_VERSION`], the constant this binary actually
//! emits into generated code. A manifest rendered by the binary it describes
//! cannot lie about the one field that matters.
//!
//! ## Read, never parsed out of a filename
//!
//! Nothing derives a component version from an asset name, a directory name or
//! a release tag. `nros-linux-x86_64.tar.zst` says nothing; the store prefix
//! `sdk/nros/0.5.0-nros1` says only which directory it is in. Every function
//! here reaches a FILE — the same argument `scripts/install.sh` already makes
//! for `share/nros/VERSION` ("a filename is a claim while a file inside the
//! artifact is the artifact").
//!
//! ## What this does NOT replace
//!
//! `share/nros/VERSION` stays, unchanged and still written by the release
//! workflow. `scripts/install.sh` reads it to choose the store prefix *before
//! anything is installed*, in POSIX shell, with a documented fallback for a
//! pre-W5 asset that carries neither file. Giving the installer a second file
//! to learn would buy nothing and would put the prefix decision behind a TOML
//! parse. [`Found::version`] and the VERSION file agree by construction — the
//! workflow writes both from one input.
//!
//! ## The acceptance range, and why the warning is a warning
//!
//! `nros_core::codegen_version` declares a range,
//! `[NROS_CODEGEN_VERSION_MIN, NROS_CODEGEN_VERSION]`, and RFC-0097 D6 keeps it
//! narrow deliberately. So a `codegen` bump does not *always* invalidate
//! existing output — it does when the old number falls out of the new runtime's
//! range, which is a property of the runtime being installed and not of this
//! file. [`CodegenDelta::Differs`] is therefore reported as a WARNING that names
//! the remedy, never as a refusal: a false alarm costs one `nros sync`, and the
//! silent version of this is the failure that withdrew the last release
//! (phase-288 D1/D2 — drifted generated code COMPILES).

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr, bail};
use serde::Deserialize;

/// The file's name inside `share/nros/`.
pub const FILE_NAME: &str = "manifest.toml";

/// What a release declares about itself.
///
/// `index` and `nano_ros` are optional because they are PROVENANCE: a manifest
/// that cannot name the commit it was cut from is still a manifest that can
/// answer the re-emit question. `version` and `codegen` are not optional,
/// because a manifest missing either answers nothing at all.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ReleaseManifest {
    /// The STORE version (`0.5.0-nros1`) — the string a pin names and a store
    /// prefix is called, not the crate version `nros --version` prints. Same
    /// distinction `orchestration::pin` states at length, for the same reason.
    pub version: String,
    /// `NROS_CODEGEN_VERSION`. The only field that can invalidate output a user
    /// already has.
    pub codegen: u32,
    /// When the `nros-sdk-index.toml` in this asset was last moved. A date, not
    /// a version: the index is a manifest of pointers into `nano-ros-sdk`,
    /// which publishes per-tool and has no version of its own (RFC-0097 D5).
    #[serde(default)]
    pub index: Option<String>,
    /// The `nano-ros` commit this was built from.
    #[serde(default)]
    pub nano_ros: Option<String>,
}

/// A manifest and the file it came from — printed in every message, because
/// "which release said that" is the first question when two disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub manifest: ReleaseManifest,
}

impl Found {
    #[must_use]
    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    #[must_use]
    pub fn codegen(&self) -> u32 {
        self.manifest.codegen
    }
}

/// Render a manifest, byte for byte. Split out so the round trip is a unit test
/// with no filesystem, and so the header lives in exactly one place.
///
/// Hand-written rather than `toml::to_string`: the comments are the reason a
/// human opening this file learns what `codegen` means, and a serializer drops
/// them.
#[must_use]
pub fn render(m: &ReleaseManifest) -> String {
    let mut s = String::new();
    s.push_str(
        "# What this nano-ros release is MADE OF (RFC-0097 D7).\n\
         #\n\
         # These are four INDEPENDENT axes, recorded rather than asserted equal.\n\
         # Only `codegen` carries a compatibility promise: two releases that\n\
         # declare the same `codegen` accept the same generated code, so moving\n\
         # between them re-emits nothing. A DIFFERENT `codegen` is a\n\
         # compatibility event — regenerate with `nros sync`.\n\
         #\n\
         # `nros pin <version>` reports the delta before it moves your pin.\n\n",
    );
    s.push_str(&format!("version = \"{}\"\n", m.version));
    s.push_str(&format!("codegen = {}\n", m.codegen));
    if let Some(i) = &m.index {
        s.push_str(&format!("index = \"{i}\"\n"));
    }
    if let Some(n) = &m.nano_ros {
        s.push_str(&format!("nano_ros = \"{n}\"\n"));
    }
    s
}

/// A manifest for THIS binary: `codegen` comes from
/// [`crate::abi_guard::EMITTED_VERSION`], never from an argument.
///
/// This is the whole reason the release workflow calls the binary it just built
/// instead of `printf`-ing four lines of YAML: the number a manifest claims and
/// the number the binary emits are then the same number by construction, and
/// the surviving equality check has something real to compare against.
#[must_use]
pub fn stamp(version: &str, index: Option<&str>, nano_ros: Option<&str>) -> ReleaseManifest {
    ReleaseManifest {
        version: version.to_string(),
        codegen: crate::abi_guard::EMITTED_VERSION,
        index: index.map(str::to_string),
        nano_ros: nano_ros.map(str::to_string),
    }
}

/// Read one manifest file.
///
/// A file that exists and cannot be read as this schema is an ERROR, never an
/// absent manifest — the asymmetry `pin::load` and `store::load_pin_file`
/// already state: "no manifest" and "I could not tell" must not reach a caller
/// as one value. A user acting on "no manifest" re-emits nothing.
pub fn load(path: &Path) -> Result<ReleaseManifest> {
    let raw = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("read the release manifest {}", path.display()))?;
    let m: ReleaseManifest = toml::from_str(&raw).wrap_err_with(|| {
        format!(
            "parse the release manifest {} — it must carry at least\n\n    \
             version = \"<store version>\"\n    codegen = <n>\n",
            path.display()
        )
    })?;
    if m.version.trim().is_empty() {
        bail!(
            "{} declares an empty `version`. A release that cannot name itself \
             is unreadable, not unversioned.",
            path.display()
        );
    }
    Ok(m)
}

/// `<prefix>/share/nros/manifest.toml`. CONSTRUCTED, never searched for.
#[must_use]
pub fn path_in_prefix(prefix: &Path) -> PathBuf {
    prefix.join("share").join("nros").join(FILE_NAME)
}

/// The manifest of the release installed at `prefix`, if it carries one.
///
/// `Ok(None)` is the pre-W2 asset: it shipped `share/nros/VERSION` and no
/// manifest, which is a real state on any host that installed before this
/// landed. `Err` is a manifest that is there and broken.
pub fn for_prefix(prefix: &Path) -> Result<Option<Found>> {
    let path = path_in_prefix(prefix);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(Found {
        manifest: load(&path)?,
        path,
    }))
}

/// The manifest of the release that `exe` belongs to (`<exe>/../..` is the
/// prefix — the same two-parent walk `pin::running_version` and
/// `dispatch::bundled_installer` do).
pub fn for_exe(exe: &Path) -> Result<Option<Found>> {
    let Some(prefix) = exe.parent().and_then(Path::parent) else {
        return Ok(None);
    };
    for_prefix(prefix)
}

/// Every prefix a toolchain `version` could occupy under store `root`, newest
/// layout first.
///
/// Derived from [`super::pin::candidate_bins`] rather than re-spelled, so the
/// two `toolchains/<v>` vs `sdk/nros/<v>` layouts have ONE definition. Two
/// copies is how a reader teaches only the new name and writes a lookup that
/// recognises no installed host.
#[must_use]
pub fn candidate_prefixes(root: &Path, version: &str) -> Vec<PathBuf> {
    super::pin::candidate_bins(root, version)
        .into_iter()
        .filter_map(|bin| Some(bin.parent()?.parent()?.to_path_buf()))
        .collect()
}

/// The manifest of an INSTALLED toolchain, by store version.
///
/// `Ok(None)` means either "that version is not in this store" or "it is, and
/// it predates manifests" — both of which leave the codegen question
/// unanswerable, which is what [`CodegenDelta::Unknown`] exists to say out
/// loud.
pub fn for_store_version(root: &Path, version: &str) -> Result<Option<Found>> {
    for prefix in candidate_prefixes(root, version) {
        if let Some(found) = for_prefix(&prefix)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// What moving between two toolchains does to a user's already-generated code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodegenDelta {
    /// Both declare the same number. **Nothing is re-emitted.** This is the
    /// case D7 exists to make sayable, and the reason a CLI may move 710 times
    /// in 60 days without touching a user's `generated/` tree.
    Same(u32),
    /// The number moves. A compatibility event.
    Differs { from: u32, to: u32 },
    /// At least one side declares nothing, so the question has no answer here.
    /// Treated as "assume you must re-emit": a false alarm costs one
    /// `nros sync`, and being silently wrong is the failure that withdrew the
    /// last release.
    Unknown {
        /// Which side(s) could not be read, for the message.
        from_known: bool,
        to_known: bool,
    },
}

impl CodegenDelta {
    /// Compare two optional declarations.
    #[must_use]
    pub fn between(from: Option<u32>, to: Option<u32>) -> Self {
        match (from, to) {
            (Some(a), Some(b)) if a == b => CodegenDelta::Same(a),
            (Some(a), Some(b)) => CodegenDelta::Differs { from: a, to: b },
            (a, b) => CodegenDelta::Unknown {
                from_known: a.is_some(),
                to_known: b.is_some(),
            },
        }
    }

    /// Must the user regenerate? True for [`Self::Differs`] AND for
    /// [`Self::Unknown`] — see the variant's own note on why unknown is not
    /// "probably fine".
    #[must_use]
    pub fn requires_reemit(self) -> bool {
        !matches!(self, CodegenDelta::Same(_))
    }
}

/// The report `nros pin` prints, and the thing a test asserts on instead of
/// scraping stdout.
///
/// Multi-line and deliberately ordered: the VERDICT first, the remedy second,
/// the provenance last. A user who reads one line has read the one that decides
/// whether their tree still builds.
#[must_use]
pub fn describe_delta(delta: &CodegenDelta, from_version: &str, to_version: &str) -> String {
    match delta {
        CodegenDelta::Same(n) => format!(
            "codegen {n}: UNCHANGED from {from_version} to {to_version}.\n\
             \x20   Your generated code stays valid — nothing to re-emit."
        ),
        CodegenDelta::Differs { from, to } => format!(
            "warning: codegen {from} -> {to} ({from_version} -> {to_version}).\n\
             \x20   This is a COMPATIBILITY EVENT: code generated by {from_version} \
             was emitted against codegen {from},\n\
             \x20   and {to_version} emits {to}. Regenerate before building:\n\
             \x20       nros sync\n\
             \x20   Drifted generated code COMPILES, so nothing downstream will \
             tell you (RFC-0090)."
        ),
        CodegenDelta::Unknown {
            from_known,
            to_known,
        } => {
            let which = match (from_known, to_known) {
                (false, false) => format!("neither {from_version} nor {to_version} declares one"),
                (false, true) => format!("{from_version} declares none"),
                (true, false) => format!("{to_version} declares none"),
                (true, true) => unreachable!("both known is Same or Differs"),
            };
            format!(
                "warning: codegen version UNKNOWN — {which}.\n\
                 \x20   A release that predates RFC-0097 D7 ships no \
                 share/nros/{FILE_NAME}, so this cannot be answered from the \
                 store.\n\
                 \x20   Assume you must regenerate:\n\
                 \x20       nros sync"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ReleaseManifest {
        ReleaseManifest {
            version: "0.7.9-nros1".to_string(),
            codegen: 7,
            index: Some("2026-09-10".to_string()),
            nano_ros: Some("abc1234".to_string()),
        }
    }

    #[test]
    fn render_round_trips_through_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, render(&sample())).unwrap();
        assert_eq!(load(&path).unwrap(), sample());
    }

    /// The provenance fields are optional; the two that answer the re-emit
    /// question are not.
    #[test]
    fn a_manifest_without_provenance_still_loads_and_one_without_codegen_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);

        std::fs::write(&path, "version = \"0.1.0-nros1\"\ncodegen = 3\n").unwrap();
        let m = load(&path).unwrap();
        assert_eq!(m.codegen, 3);
        assert_eq!(m.index, None);
        assert_eq!(m.nano_ros, None);
        // And it round-trips without inventing empty keys.
        assert!(!render(&m).contains("index"));

        std::fs::write(&path, "version = \"0.1.0-nros1\"\n").unwrap();
        assert!(
            load(&path).is_err(),
            "a manifest with no codegen answers nothing"
        );

        std::fs::write(&path, "version = \"\"\ncodegen = 3\n").unwrap();
        assert!(
            load(&path).is_err(),
            "an empty version is unreadable, not unversioned"
        );
    }

    /// `Ok(None)` (no manifest) and `Err` (a broken one) must not collapse:
    /// acting on the first re-emits nothing.
    #[test]
    fn absent_is_not_broken() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("sdk/nros/0.1.0-nros1");
        std::fs::create_dir_all(prefix.join("share/nros")).unwrap();
        assert_eq!(for_prefix(&prefix).unwrap(), None);
        std::fs::write(path_in_prefix(&prefix), "codegen = \"seven\"\n").unwrap();
        assert!(for_prefix(&prefix).is_err());
    }

    /// Both store layouts resolve, and the prefix list is derived from
    /// `pin::candidate_bins` so it cannot know only one of them.
    #[test]
    fn both_store_layouts_resolve_by_construction() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            candidate_prefixes(root, "1.2.3-nros4"),
            vec![
                root.join("toolchains/1.2.3-nros4"),
                root.join("sdk/nros/1.2.3-nros4"),
            ]
        );
        for (rel, codegen) in [
            ("toolchains/9.0.0-nros1", 9u32),
            ("sdk/nros/8.0.0-nros1", 8),
        ] {
            let prefix = root.join(rel);
            std::fs::create_dir_all(prefix.join("share/nros")).unwrap();
            std::fs::write(
                path_in_prefix(&prefix),
                render(&ReleaseManifest {
                    version: rel.rsplit('/').next().unwrap().to_string(),
                    codegen,
                    index: None,
                    nano_ros: None,
                }),
            )
            .unwrap();
        }
        assert_eq!(
            for_store_version(root, "9.0.0-nros1")
                .unwrap()
                .unwrap()
                .codegen(),
            9
        );
        assert_eq!(
            for_store_version(root, "8.0.0-nros1")
                .unwrap()
                .unwrap()
                .codegen(),
            8
        );
        assert_eq!(for_store_version(root, "7.0.0-nros1").unwrap(), None);
    }

    /// The feature, stated as a test: same number, no re-emit — even though
    /// every other field differs.
    #[test]
    fn two_releases_with_the_same_codegen_require_no_reemit() {
        let a = ReleaseManifest {
            version: "0.7.1-nros1".into(),
            codegen: 7,
            index: Some("2026-01-01".into()),
            nano_ros: Some("1111111".into()),
        };
        let b = ReleaseManifest {
            version: "0.7.9-nros4".into(),
            codegen: 7,
            index: Some("2026-09-10".into()),
            nano_ros: Some("9999999".into()),
        };
        let d = CodegenDelta::between(Some(a.codegen), Some(b.codegen));
        assert_eq!(d, CodegenDelta::Same(7));
        assert!(!d.requires_reemit());
        let msg = describe_delta(&d, &a.version, &b.version);
        assert!(
            !msg.contains("warning"),
            "no warning for an unchanged codegen: {msg}"
        );
        assert!(msg.contains("nothing to re-emit"), "{msg}");
    }

    #[test]
    fn a_different_codegen_warns_and_names_the_remedy() {
        let d = CodegenDelta::between(Some(7), Some(8));
        assert_eq!(d, CodegenDelta::Differs { from: 7, to: 8 });
        assert!(d.requires_reemit());
        let msg = describe_delta(&d, "0.7.9-nros1", "0.8.0-nros1");
        assert!(
            msg.starts_with("warning:"),
            "the verdict comes first: {msg}"
        );
        assert!(msg.contains("nros sync"), "the remedy is named: {msg}");
    }

    /// Unknown is not "probably fine".
    #[test]
    fn an_undeclared_codegen_is_treated_as_a_reemit() {
        for (from, to) in [(None, Some(7)), (Some(7), None), (None, None)] {
            let d = CodegenDelta::between(from, to);
            assert!(d.requires_reemit(), "{d:?}");
            let msg = describe_delta(&d, "old", "new");
            assert!(msg.starts_with("warning:"), "{msg}");
            assert!(msg.contains("nros sync"), "{msg}");
        }
    }

    /// `stamp` takes the number from the binary, not from a caller. This is the
    /// property that makes the surviving release-time equality meaningful — a
    /// caller that could pass a codegen number could pass the wrong one.
    #[test]
    fn stamp_takes_codegen_from_the_binary_never_from_an_argument() {
        let m = stamp("0.7.9-nros1", Some("2026-09-10"), Some("abc1234"));
        assert_eq!(m.codegen, crate::abi_guard::EMITTED_VERSION);
        assert_eq!(m.version, "0.7.9-nros1");
    }
}
