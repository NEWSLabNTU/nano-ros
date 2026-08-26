//! Stage 5b — the `dist/<image>/` output layout and its manifest
//! (phase-383 W6.a/W6.b, RFC-0065 D8).
//!
//! ## An image is a SET, and the singleton is not a special case
//!
//! ```text
//! dist/zephyr-nrf52840/          dist/native/
//!   manifest.toml                  manifest.toml
//!   app.signed.hex                 demo          # a set of one — same shape
//!   mcuboot.hex
//!   merged.hex
//! ```
//!
//! This falls out of Zephyr **sysbuild**, whose most common configuration is
//! "build my app and an MCUboot bootloader, same signing key, same partition
//! table". On any board with a bootloader the product is ≥2 files whose config
//! must stay in sync, so a `dist/` layout that assumed one file would have to
//! be re-cut the first time anyone shipped a signed image. Modelling the host
//! image as a set of one costs a `[[artifact]]` table and buys one code path:
//! nothing downstream needs to ask "is this the singleton case?".
//!
//! ## The manifest is CONSUMED, never globbed — and it must be COMPLETE
//!
//! ESP-IDF's `flasher_args.json` is the precedent: project flash information in
//! a machine-readable file, consumed directly by `idf.py` and by `esptool
//! @build/flash_project_args`. It also supplies the cautionary tale. A filed
//! bug against it reads:
//!
//! > *"flasher_args.json is missing entry for `bootloader` when built with
//! > secure boot v2"*
//!
//! Read that failure carefully, because it is the one this module exists to
//! prevent. Nothing was broken at the moment the feature landed: secure boot
//! v2 produced a new artifact, the manifest generator was not taught about it,
//! and **the manifest silently fell behind the artifacts**. Every tool that
//! consumed the manifest kept working, on an incomplete answer. The bootloader
//! was on disk and simply never flashed.
//!
//! A glob would have "fixed" that instance and made the class permanent: a
//! flasher that globs `*.hex` cannot tell an artifact from a leftover, cannot
//! know a load address, and cannot fail when the build produced something
//! unexpected — it just flashes whatever the wildcard swept up.
//!
//! So the invariant is a **gate**, [`check_complete`]: every file in
//! `dist/<image>/` must be named by that image's manifest, and an unnamed file
//! fails the build, naming the file. The generator falling behind its artifacts
//! is then a hard error at the moment it happens, in the build that introduced
//! it, rather than a silent omission discovered by a device that will not boot.
//!
//! ## Why `dist/` and not `install/`
//!
//! RFC-0065 D8 amends RFC-0070 R1 here: `install/` promises an environment to
//! source, which nano-ros will never have, and a ROS user's first move would be
//! a `source dist/setup.bash` that cannot exist.
//!
//! ## Load addresses are hex, deliberately
//!
//! A flash address is read against a partition table, a linker script and a
//! datasheet, all of which are written in hex. TOML *accepts* `0xc000`, but the
//! `toml` serializer emits integers in decimal, so a naive round-trip would
//! rewrite a hand-checkable `0xc000` as `49152` — technically equal, and
//! useless to the person diffing it against `partitions.csv`. We therefore
//! store the address as a canonical lowercase `0x…` **string** and accept
//! either spelling on the way in (see [`hex_addr`]).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// The manifest's file name inside `dist/<image>/`.
///
/// Load-bearing in two places: it is the one file [`check_complete`] exempts
/// from the completeness gate, and it is what a downstream flasher opens. Both
/// spell it through this constant so they cannot drift.
pub const MANIFEST_FILE_NAME: &str = "manifest.toml";

/// What an artifact IS, which decides what a tool does with it.
///
/// This is a closed vocabulary on purpose. A flasher that must guess from a
/// file extension is back to globbing; `app.signed.hex` and `mcuboot.hex` are
/// indistinguishable by name and could not be more different in what happens if
/// you swap them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The application image — the thing `nros flash` puts on the device.
    /// Exactly one per image; see [`DistManifest::flashable`].
    Flashable,
    /// A bootloader (MCUboot and friends) built alongside the application.
    /// Flashed once per device rather than per iteration, which is precisely
    /// why it is easy to forget and why ESP-IDF's bug was about this role.
    Bootloader,
    /// A single file combining several of the others, produced for
    /// one-shot factory programming. Never flashed *together with* its parts.
    Merged,
    /// Symbols / ELF / map files — consumed by a debugger, never flashed.
    Debug,
    /// Everything else the build legitimately produced. Present so that
    /// "I do not have a role for this" is still a manifest entry rather than a
    /// reason to leave the file unnamed — an unnamed file fails the gate, and
    /// the gate must never be something a build wants to route around.
    Other,
}

/// One member of an image's artifact set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Path RELATIVE to the image's `dist/<image>/` directory, `/`-separated.
    ///
    /// Relative because the manifest ships next to the artifacts and must stay
    /// valid when the directory is copied to a build server or a colleague —
    /// the same reason W3.c forbids absolute paths in the generated cargo root.
    pub file: String,
    /// What this file is; see [`Role`].
    pub role: Role,
    /// Where it is programmed, when that is a property of the artifact rather
    /// than of the flashing tool.
    ///
    /// `None` is the common and correct case for a host binary, for debug
    /// output, and for any image whose address the runner derives from the
    /// board. Written and read as hex — see the module docs.
    #[serde(default, with = "hex_addr", skip_serializing_if = "Option::is_none")]
    pub load_address: Option<u64>,
}

impl Artifact {
    /// An artifact with no load address.
    pub fn new(file: impl Into<String>, role: Role) -> Self {
        Self {
            file: file.into(),
            role,
            load_address: None,
        }
    }

    /// This artifact, programmed at `addr`.
    #[must_use]
    pub fn at(mut self, addr: u64) -> Self {
        self.load_address = Some(addr);
        self
    }
}

/// `dist/<image>/manifest.toml` — the complete, authoritative description of
/// one image's output.
///
/// "Complete" is not a wish: [`check_complete`] enforces it against the
/// directory on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistManifest {
    /// The image id — the `<image>` in `dist/<image>/`, echoed here so a
    /// manifest that has been copied out of its directory still says what it
    /// describes.
    pub image: String,
    /// The board this was built for, in the RFC-0065 D9 registry's vocabulary.
    /// A hex file is meaningless without it, and "which board was this?" is the
    /// first question asked of any artifact found on a shelf.
    pub board: String,
    /// Version of the `nros` that wrote this file.
    ///
    /// Not decoration: when a consumer meets a manifest it cannot parse, the
    /// actionable message is "written by nros X, you are running Y", and that
    /// requires the producer to have recorded X.
    pub generator: String,
    /// The artifact set. Serialized as `[[artifact]]` tables, sorted by
    /// [`Artifact::file`] so the file does not churn on unrelated changes.
    #[serde(rename = "artifact", default)]
    pub artifacts: Vec<Artifact>,
}

impl DistManifest {
    /// An empty manifest for `image` on `board`, stamped with this crate's
    /// version.
    pub fn new(image: impl Into<String>, board: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            board: board.into(),
            generator: env!("CARGO_PKG_VERSION").to_string(),
            artifacts: Vec::new(),
        }
    }

    /// Add an artifact, builder-style.
    #[must_use]
    pub fn with(mut self, artifact: Artifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// The one artifact a flasher writes to the device.
    ///
    /// Errors — rather than picking one — when there is not exactly one.
    ///
    /// * **zero** is a build that produced no flashable output. Silently
    ///   flashing nothing is the worst available answer, because the device
    ///   keeps running the OLD image and looks like a code bug.
    /// * **more than one** is an ambiguity only the build system can resolve,
    ///   so the error names every candidate: whoever reads it is the person who
    ///   knows which of `app.signed.hex` and `merged.hex` they meant, and
    ///   guessing wrong bricks a bootloader slot.
    pub fn flashable(&self) -> Result<&Artifact, String> {
        let hits: Vec<&Artifact> = self
            .artifacts
            .iter()
            .filter(|a| a.role == Role::Flashable)
            .collect();
        match hits.as_slice() {
            [one] => Ok(one),
            [] => Err(format!(
                "image `{}` has no artifact with role `flashable` — nothing to \
                 flash. Every image, host included, names exactly one \
                 (RFC-0065 D8); {} artifact(s) were declared: [{}]",
                self.image,
                self.artifacts.len(),
                self.artifacts
                    .iter()
                    .map(|a| a.file.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            many => Err(format!(
                "image `{}` declares {} flashable artifacts, and only the build \
                 knows which one you meant: {}. Mark the others `merged`, \
                 `bootloader` or `other` (RFC-0065 D8)",
                self.image,
                many.len(),
                many.iter()
                    .map(|a| a.file.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// The file names this manifest claims, `/`-separated and relative to the
    /// image directory.
    #[must_use]
    pub fn declared_files(&self) -> BTreeSet<String> {
        self.artifacts.iter().map(|a| a.file.clone()).collect()
    }
}

/// The manifest path for an image directory.
#[must_use]
pub fn manifest_path(dist_dir: &Path) -> PathBuf {
    dist_dir.join(MANIFEST_FILE_NAME)
}

/// Render `manifest` as the exact bytes written to `manifest.toml`.
///
/// Artifacts are **sorted by file name** and nothing carries a timestamp, so
/// two runs of the same build produce byte-identical output — the same
/// determinism rule W3.c applies to the generated cargo root.
pub fn render(manifest: &DistManifest) -> Result<String, String> {
    let mut sorted = manifest.clone();
    sorted.artifacts.sort_by(|a, b| a.file.cmp(&b.file));

    let body = toml::to_string_pretty(&sorted).map_err(|e| {
        format!(
            "serializing the dist manifest for `{}`: {e}",
            manifest.image
        )
    })?;

    Ok(format!(
        "# GENERATED by `nros build` (phase-383 W6.a) — DO NOT EDIT.\n\
         #\n\
         # The complete artifact set for this image. Every file in this\n\
         # directory is named below, and `nros build` FAILS when one is not\n\
         # (RFC-0065 D8): a manifest that silently falls behind its artifacts\n\
         # is ESP-IDF's `flasher_args.json` secure-boot bug, where a bootloader\n\
         # existed on disk and was never flashed.\n\
         #\n\
         # Consume this file. Do not glob this directory.\n\n{body}"
    ))
}

/// Parse a `manifest.toml` body.
pub fn parse(body: &str) -> Result<DistManifest, String> {
    toml::from_str(body).map_err(|e| format!("parsing a dist manifest: {e}"))
}

/// Write `manifest` into `dist_dir`, creating the directory if needed, and
/// return the manifest path.
///
/// Only rewrites when the content changed. Same reason as the generated cargo
/// root: a gratuitous touch re-stales every fixture keyed on this tree
/// (CLAUDE.md's mtime treadmill), and an artifact directory whose manifest
/// bumps its mtime on every no-op build is exactly the kind of thing that makes
/// a downstream freshness probe useless.
pub fn write(dist_dir: &Path, manifest: &DistManifest) -> Result<PathBuf, String> {
    let body = render(manifest)?;
    std::fs::create_dir_all(dist_dir)
        .map_err(|e| format!("creating {}: {e}", dist_dir.display()))?;
    let path = manifest_path(dist_dir);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(&path, &body).map_err(|e| format!("writing {}: {e}", path.display()))?;
    }
    Ok(path)
}

/// Read the manifest an image directory carries.
pub fn read(dist_dir: &Path) -> Result<DistManifest, String> {
    let path = manifest_path(dist_dir);
    let body = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "reading {}: {e} — an image directory without a manifest cannot be \
             consumed, and must not be globbed (RFC-0065 D8)",
            path.display()
        )
    })?;
    parse(&body)
}

/// **The completeness gate (W6.b).** Every file in `dist_dir` must be named by
/// `manifest`.
///
/// This is the check ESP-IDF did not have. Its `flasher_args.json` bug —
/// *"missing entry for `bootloader` when built with secure boot v2"* — was a
/// generator that stayed silent while a new feature added an artifact it did
/// not know about. Nothing failed; the manifest was merely incomplete, and the
/// bootloader was never flashed. Here that is a build failure that names the
/// file.
///
/// The walk is **recursive**, because a gate that stopped at the top level
/// would leave `dist/<image>/sub/app.hex` unnamed and unnoticed — a hole in the
/// exact place a build system likes to put per-image subdirectories.
///
/// [`MANIFEST_FILE_NAME`] is the one exemption, and there is deliberately no
/// mechanism for a second: an exemption list is how a gate becomes advisory.
///
/// The reverse direction — a manifest naming a file that is not on disk — is
/// reported too. It is not the ESP-IDF failure, but it is the same lie told the
/// other way round, it is free to check here, and a flasher discovering it does
/// so with the device already open.
///
/// Reports EVERY offender, not the first: a build that added three artifacts
/// should learn about three, not be fixed and re-run three times (preflight's
/// rule, one stage over).
pub fn check_complete(dist_dir: &Path, manifest: &DistManifest) -> Result<(), String> {
    let present = list_files(dist_dir, dist_dir)?;
    let declared = manifest.declared_files();

    let unnamed: Vec<&String> = present
        .iter()
        .filter(|f| f.as_str() != MANIFEST_FILE_NAME)
        .filter(|f| !declared.contains(f.as_str()))
        .collect();
    let missing: Vec<&String> = declared.iter().filter(|f| !present.contains(*f)).collect();

    if unnamed.is_empty() && missing.is_empty() {
        return Ok(());
    }

    let mut msg = format!(
        "dist manifest for image `{}` does not describe {}:\n",
        manifest.image,
        dist_dir.display()
    );
    for f in &unnamed {
        msg.push_str(&format!(
            "  - `{f}` is present but NOT named by {MANIFEST_FILE_NAME}\n"
        ));
    }
    for f in &missing {
        msg.push_str(&format!(
            "  - `{f}` is named by {MANIFEST_FILE_NAME} but is NOT present\n"
        ));
    }
    msg.push_str(
        "\nEvery file in an image directory must be named by its manifest \
         (RFC-0065 D8). An unnamed artifact is how ESP-IDF's flasher_args.json \
         shipped without a bootloader entry under secure boot v2: the manifest \
         fell behind the build and nothing said so. Add the artifact to the \
         manifest (role `other` if it has no better one), or stop producing it.",
    );
    Err(msg)
}

/// Every file under `dir`, as `/`-separated paths relative to `base`.
///
/// Symlinks are followed only insofar as `is_file`/`is_dir` do; a dangling one
/// is neither, and is reported as a file so the gate complains about it rather
/// than ignoring it.
fn list_files(base: &Path, dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    let entries = std::fs::read_dir(dir).map_err(|e| {
        format!(
            "reading the image directory {}: {e} — the completeness gate cannot \
             pass a directory it cannot list",
            dir.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading an entry of {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(list_files(base, &path)?);
            continue;
        }
        let rel = path.strip_prefix(base).map_err(|_| {
            format!(
                "{} is not below {} — refusing to guess its name",
                path.display(),
                base.display()
            )
        })?;
        out.insert(
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
        );
    }
    Ok(out)
}

/// Hex-string serialization for [`Artifact::load_address`].
///
/// Writes the canonical lowercase `0x…` form; accepts that, a bare hex string,
/// a decimal string, or a plain TOML integer (which is how a hand-written
/// `load_address = 0xc000` arrives). Lenient in, canonical out — the usual
/// shape, and it means a hand-edited manifest is normalised by the next build
/// rather than rejected.
mod hex_addr {
    use serde::{Deserializer, Serializer, de};
    use std::fmt;

    pub fn serialize<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(a) => s.serialize_str(&format!("{a:#x}")),
            // `skip_serializing_if` means we never get here in practice; a
            // `None` that did reach a serializer must still be representable.
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        d.deserialize_option(OptVisitor)
    }

    struct OptVisitor;

    impl<'de> de::Visitor<'de> for OptVisitor {
        type Value = Option<u64>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a load address as a hex string (\"0xc000\") or an integer")
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_any(AddrVisitor).map(Some)
        }
    }

    struct AddrVisitor;

    impl de::Visitor<'_> for AddrVisitor {
        type Value = u64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a load address as a hex string (\"0xc000\") or an integer")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            u64::try_from(v).map_err(|_| de::Error::custom(format!("negative load address {v}")))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            let t = v.trim();
            let parsed = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => t.parse::<u64>(),
            };
            parsed.map_err(|e| de::Error::custom(format!("load address `{v}`: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"artifact").unwrap();
    }

    #[test]
    fn a_host_image_is_a_set_of_one_and_round_trips() {
        // RFC-0065 D8: the singleton is not a special case. It uses the same
        // manifest shape as a signed Zephyr image, which is the whole reason
        // downstream tools need only one code path.
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));

        let body = render(&m).expect("renders");
        let back = parse(&body).expect("parses");

        assert_eq!(back, m, "a set of one must survive a round trip\n{body}");
        assert_eq!(back.flashable().expect("one flashable").file, "demo");
        // No address on a host binary — and `skip_serializing_if` keeps the key
        // out of the file rather than writing a meaningless `load_address = 0`.
        assert!(!body.contains("load_address"), "{body}");
    }

    #[test]
    fn a_multi_artifact_image_round_trips_with_addresses() {
        // The Zephyr sysbuild case D8 is designed around: app + MCUboot +
        // merged, two of them programmed at addresses that come from the
        // partition table.
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("app.signed.hex", Role::Flashable).at(0xc000))
            .with(Artifact::new("mcuboot.hex", Role::Bootloader).at(0x0))
            .with(Artifact::new("merged.hex", Role::Merged));

        let body = render(&m).expect("renders");
        let back = parse(&body).expect("parses");

        assert_eq!(back.artifacts.len(), 3, "{body}");
        assert_eq!(back.image, "zephyr-nrf52840");
        assert_eq!(back.board, "nrf52840dk");
        assert_eq!(back.generator, env!("CARGO_PKG_VERSION"));
        for a in &m.artifacts {
            assert!(
                back.artifacts.contains(a),
                "{a:?} lost in the round trip\n{body}"
            );
        }
        // Hex, not decimal: an address is read against a partition table and a
        // linker script, both written in hex. `49152` would be correct and
        // undiffable.
        assert!(body.contains("\"0xc000\""), "addresses stay hex: {body}");
        assert!(!body.contains("49152"), "never decimal: {body}");
    }

    #[test]
    fn a_hand_written_hex_integer_is_accepted_and_normalised() {
        // TOML's own `0xc000` literal is what a person editing this file by
        // hand will write. Lenient in, canonical out.
        let body = "image = \"z\"\nboard = \"b\"\ngenerator = \"0.0.0\"\n\n\
                    [[artifact]]\nfile = \"app.hex\"\nrole = \"flashable\"\n\
                    load_address = 0xc000\n";
        let m = parse(body).expect("parses a TOML hex literal");
        assert_eq!(m.artifacts[0].load_address, Some(0xc000));
        assert!(render(&m).unwrap().contains("\"0xc000\""));
    }

    #[test]
    fn the_completeness_gate_fails_on_a_file_the_manifest_does_not_name() {
        // THE test for W6.b. This is ESP-IDF's flasher_args.json secure-boot
        // bug reproduced deliberately: the build gained an artifact, the
        // manifest generator did not learn about it, and the file sat on disk
        // never flashed. Here it is a build failure that names the file.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("app.signed.hex", Role::Flashable).at(0xc000));
        write(dir, &m).expect("writes");
        touch(dir, "app.signed.hex");
        // The artifact secure boot v2 added, that nobody taught the generator:
        touch(dir, "mcuboot.hex");

        let e = check_complete(dir, &m).expect_err("an unnamed artifact must fail the build");
        assert!(
            e.contains("mcuboot.hex"),
            "the error must NAME the file: {e}"
        );
        assert!(
            !e.contains("`app.signed.hex` is present but"),
            "a named artifact must not be reported: {e}"
        );
    }

    #[test]
    fn the_completeness_gate_reaches_into_subdirectories() {
        // A gate that stopped at the top level would leave exactly the place a
        // build system likes to nest per-image output unguarded.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));
        write(dir, &m).expect("writes");
        touch(dir, "demo");
        touch(dir, "zephyr/zephyr.elf");

        let e = check_complete(dir, &m).expect_err("a nested artifact must fail too");
        assert!(e.contains("zephyr/zephyr.elf"), "{e}");
    }

    #[test]
    fn the_completeness_gate_passes_when_the_manifest_and_the_directory_agree() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("app.signed.hex", Role::Flashable).at(0xc000))
            .with(Artifact::new("mcuboot.hex", Role::Bootloader).at(0x0))
            .with(Artifact::new("merged.hex", Role::Merged));
        write(dir, &m).expect("writes");
        touch(dir, "app.signed.hex");
        touch(dir, "mcuboot.hex");
        touch(dir, "merged.hex");

        // manifest.toml itself is the one exemption, and it is written above —
        // so this also proves the exemption works.
        check_complete(dir, &m).expect("an agreeing directory must pass");
    }

    #[test]
    fn a_manifest_naming_an_absent_file_also_fails() {
        // Not the ESP-IDF failure, but the same lie the other way round: a
        // flasher discovers it with the device already open.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));
        write(dir, &m).expect("writes");
        // `demo` is never produced.

        let e = check_complete(dir, &m).expect_err("a missing artifact must fail");
        assert!(e.contains("demo"), "{e}");
        assert!(e.contains("NOT present"), "{e}");
    }

    #[test]
    fn every_offender_is_reported_not_just_the_first() {
        // preflight's rule one stage over: three problems fixed one build at a
        // time is three round trips.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));
        write(dir, &m).expect("writes");
        touch(dir, "demo");
        touch(dir, "stray_a.bin");
        touch(dir, "stray_b.bin");

        let e = check_complete(dir, &m).expect_err("strays must fail");
        assert!(
            e.contains("stray_a.bin") && e.contains("stray_b.bin"),
            "{e}"
        );
    }

    #[test]
    fn writing_twice_does_not_touch_the_file() {
        // CLAUDE.md's mtime treadmill: a manifest that bumps its mtime on every
        // no-op build makes any downstream freshness probe useless.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dist/native");
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));

        let p1 = write(&dir, &m).expect("first");
        let t1 = std::fs::metadata(&p1).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let p2 = write(&dir, &m).expect("second");
        let t2 = std::fs::metadata(&p2).unwrap().modified().unwrap();

        assert_eq!(p1, p2);
        assert_eq!(t1, t2, "unchanged content must not rewrite the manifest");
    }

    #[test]
    fn zero_flashable_artifacts_is_an_error() {
        // Silently flashing nothing leaves the OLD image running and reads as a
        // code bug for as long as it takes someone to suspect the build.
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("mcuboot.hex", Role::Bootloader).at(0x0))
            .with(Artifact::new("zephyr.elf", Role::Debug));

        let e = m
            .flashable()
            .expect_err("nothing to flash must be an error");
        assert!(e.contains("no artifact with role `flashable`"), "{e}");
        assert!(e.contains("mcuboot.hex"), "says what WAS declared: {e}");
    }

    #[test]
    fn two_flashable_artifacts_is_an_error_naming_both() {
        // Only the build knows which was meant, and guessing wrong writes an
        // application image over a bootloader slot.
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("app.signed.hex", Role::Flashable).at(0xc000))
            .with(Artifact::new("merged.hex", Role::Flashable).at(0x0));

        let e = m
            .flashable()
            .expect_err("an ambiguity must not be resolved by guessing");
        assert!(e.contains("app.signed.hex"), "{e}");
        assert!(e.contains("merged.hex"), "{e}");
    }

    #[test]
    fn output_is_byte_identical_and_artifact_order_does_not_matter() {
        // W3.c's determinism rule, applied to the manifest: two runs of the
        // same build must produce the same bytes, and the order the build
        // happened to append artifacts in is not information.
        let a = DistManifest::new("z", "b")
            .with(Artifact::new("merged.hex", Role::Merged))
            .with(Artifact::new("app.hex", Role::Flashable));
        let b = DistManifest::new("z", "b")
            .with(Artifact::new("app.hex", Role::Flashable))
            .with(Artifact::new("merged.hex", Role::Merged));

        assert_eq!(render(&a).unwrap(), render(&b).unwrap());
        assert_eq!(render(&a).unwrap(), render(&a).unwrap());
    }

    #[test]
    fn the_manifest_says_it_is_generated_and_must_not_be_globbed() {
        let m = DistManifest::new("native", "linux-x86_64")
            .with(Artifact::new("demo", Role::Flashable));
        let body = render(&m).expect("renders");
        assert!(body.starts_with("# GENERATED"), "{body}");
        assert!(body.contains("DO NOT EDIT"), "{body}");
        assert!(body.contains("Do not glob"), "{body}");
    }

    #[test]
    fn read_round_trips_what_write_wrote() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dist/zephyr-nrf52840");
        let m = DistManifest::new("zephyr-nrf52840", "nrf52840dk")
            .with(Artifact::new("app.signed.hex", Role::Flashable).at(0xc000));
        write(&dir, &m).expect("writes");
        assert_eq!(read(&dir).expect("reads"), m);
    }

    #[test]
    fn an_image_directory_without_a_manifest_cannot_be_consumed() {
        // The alternative to a manifest is a glob, which is the thing D8 rules
        // out — so the error says so rather than falling back.
        let tmp = tempfile::tempdir().unwrap();
        let e = read(tmp.path()).expect_err("no manifest");
        assert!(e.contains("must not be globbed"), "{e}");
    }
}
