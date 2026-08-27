//! A content-addressed fixture cache key, in SHADOW MODE — phase-395 W10.
//!
//! # Nothing here may serve a hit, and that is the whole design
//!
//! For a staleness PROBE an incomplete input set is survivable. It errs toward
//! rebuilding, and [`super::staleness::candidates_changed_content_policy`]
//! returns `None` rather than "fresh" when it examined nothing, deferring to
//! the stricter mtime verdict. That fallback is what makes the current design
//! safe.
//!
//! For a CACHE there is no fallback. A hit SKIPS the build, so an incomplete
//! key does not cause a redundant rebuild — it silently serves a wrong
//! artifact. That is the museum-binary failure mode (issues 0391, 0475) with
//! its one safeguard removed.
//!
//! So this module computes the key, records what the build actually produced,
//! and compares. It has no lookup path, no restore path and no way to skip
//! work; the only thing it can do to a build is observe it. Whether the key is
//! complete is then a MEASUREMENT rather than an argument — which is the
//! discipline phase-395 has already been corrected by three times.
//!
//! # The four inputs the compiler cannot see
//!
//! The measured input set comes from the toolchain's own record — cargo's `.d`
//! dep-info or ninja's `.ninja_deps`. That covers sources and headers. It does
//! not cover the inputs this tree's issue history is largely *about*, and
//! [`INVISIBLE_CLASSES`] names each one, says whether the key covers it, and
//! says how an observation WITNESSES it so a mismatch can be attributed rather
//! than merely counted. An uncovered class that is not written down is the
//! defect this design exists to avoid, so "not covered" is a value in the
//! table, never an omission from it.
//!
//! # What a witness is, and why it is not in the key
//!
//! A witness is an input the key does NOT hash but the record DOES. It buys
//! exactly one thing: when two observations share a key and disagree about the
//! artifact, the report can name which witness moved. Putting witnesses into
//! the key would make the key wider — but two of the four cannot go in on any
//! honest account:
//!
//! * a linked `.a` under the build root is an OUTPUT of a build, and a cache
//!   key must be computable BEFORE one runs;
//! * an env var's value as seen by this process is not necessarily the value
//!   the build saw, and cargo's own env fingerprints are an internal format.
//!
//! Recording them anyway is what turns "we think the key is incomplete" into
//! "here is the input that differed on 2026-08-28".

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    groups::{self, GroupRow},
    lane::{self, Coord},
    staleness::{self, Exemption},
};
use crate::project_root;

/// Turn the shadow recorder on inside the fixture resolvers. Off by default:
/// an observation shells out to `ninja`/`python3`/`nros`, which no test run
/// should pay for unless it is measuring.
pub const SHADOW_ENV: &str = "NROS_FIXTURE_CACHE_SHADOW";

/// Override the record store (tests, and a runner with a persistent disk
/// elsewhere).
pub const STORE_ENV: &str = "NROS_FIXTURE_CACHE_SHADOW_DIR";

/// Also write every covered input path into each record. Off by default — a
/// ninja-measured Zephyr image lists ~3300 of them, and a key that MATCHES
/// already proves the covered sets agree (the digest is in the record either
/// way). Turn it on when you are debugging the key itself.
pub const FULL_ENV: &str = "NROS_FIXTURE_CACHE_SHADOW_FULL";

const RECORD_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// The coverage table — the load-bearing documentation of this module.
// ---------------------------------------------------------------------------

/// Whether the KEY hashes this class of input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Coverage {
    /// The key hashes it. A change to it changes the key.
    Covered,
    /// The key does NOT hash it. A change to it does not change the key, so a
    /// real cache would serve a stale artifact — which is precisely what the
    /// shadow record is here to detect.
    Uncovered,
    /// The key would hash it, but this artifact exposed nothing to hash (no
    /// `.config` under the build root, no in-tree `nros` binary). Recorded
    /// per-observation, because "covered in principle" and "covered for this
    /// artifact" are different facts and only the second one is evidence.
    NotObservable,
}

impl Coverage {
    fn tag(self) -> &'static str {
        match self {
            Coverage::Covered => "covered",
            Coverage::Uncovered => "uncovered",
            Coverage::NotObservable => "not-observable",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "covered" => Some(Coverage::Covered),
            "uncovered" => Some(Coverage::Uncovered),
            "not-observable" => Some(Coverage::NotObservable),
            _ => None,
        }
    }
}

/// One class of build input the compiler's dep record cannot see.
pub struct InvisibleClass {
    /// Stable identifier, used in records and in the report.
    pub name: &'static str,
    /// The issue that measured this class.
    pub issue: &'static str,
    /// What is invisible, in one line.
    pub what: &'static str,
    /// The DESIGNED coverage. An observation may downgrade it to
    /// [`Coverage::NotObservable`]; it may never upgrade it.
    pub designed: Coverage,
    /// Why it is (or is not) in the key — the sentence a reader needs before
    /// believing a green report.
    pub rationale: &'static str,
}

/// The spec's table, as data. Every entry is either covered by the key or
/// explicitly recorded as not covered, with the reason.
pub const INVISIBLE_CLASSES: &[InvisibleClass] = &[
    InvisibleClass {
        name: "link-archives",
        issue: "0475",
        what: "a whole-archived `.a` reached through a raw `-Wl,…` link flag",
        designed: Coverage::Uncovered,
        rationale: "an archive under the build root is an OUTPUT; a cache key must be \
                    computable before the build that produces it. Covering this class needs \
                    the archive's own inputs resolved TRANSITIVELY (the producing project's \
                    dep record), which is a second measurement this shadow pass does not \
                    make. Witnessed by hashing every `.a` under the artifact's build root, \
                    so a mismatch can name the archive.",
    },
    InvisibleClass {
        name: "env-vars",
        issue: "0491",
        what: "env vars declared as build inputs via `rerun-if-env-changed`",
        designed: Coverage::Uncovered,
        rationale: "the value this process sees is not necessarily the value the BUILD saw — \
                    that is issue 0491's whole point, one directory with three spellings — and \
                    cargo's per-unit env fingerprints are an internal format. The witness \
                    records the declared names' values AS SEEN BY THE RECORDER and is labelled \
                    so; it attributes a mismatch, it does not certify a match.",
    },
    InvisibleClass {
        name: "kconfig",
        issue: "0460",
        what: "Kconfig knobs, which reach the C lane and not the Rust one",
        designed: Coverage::Covered,
        rationale: "the resolved `.config` under the build root IS the knob set, it is an \
                    INPUT to both lanes, and it is one file. Hashed into the key when the \
                    artifact has one; recorded `not-observable` when it has none, which is \
                    every non-Kconfig platform and is not evidence of anything.",
    },
    InvisibleClass {
        name: "cli-closure",
        issue: "0627",
        what: "the `nros` CLI's own source closure, which stales every workspace fixture",
        designed: Coverage::Covered,
        rationale: "`nros source-stamp` reports the stamp over the GENERATED closure \
                    (`cli-source-dirs.txt`), which is 0627's fix — so this is the measured \
                    closure, not a `path =` walk. Hashed into the key wholesale, which is \
                    deliberately over-broad: a CLI edit invalidates every key, and over-broad \
                    is the safe direction for a cache. `not-observable` when no in-tree \
                    binary exists yet.",
    },
];

fn class(name: &str) -> Option<&'static InvisibleClass> {
    INVISIBLE_CLASSES.iter().find(|c| c.name == name)
}

// ---------------------------------------------------------------------------
// Observation
// ---------------------------------------------------------------------------

/// Where the measured input set came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provenance {
    /// cargo's `<artifact>.d` dep-info file.
    CargoDepInfo,
    /// ninja's `.ninja_deps` log, via `ninja -t deps`.
    NinjaDeps,
}

impl Provenance {
    fn tag(self) -> &'static str {
        match self {
            Provenance::CargoDepInfo => "cargo-dep-info",
            Provenance::NinjaDeps => "ninja-deps",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "cargo-dep-info" => Some(Provenance::CargoDepInfo),
            "ninja-deps" => Some(Provenance::NinjaDeps),
            _ => None,
        }
    }
}

/// One class as this observation saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassObservation {
    pub name: String,
    pub coverage: Coverage,
    /// `(witness name, content hash)`, sorted. Empty when nothing was found.
    pub witnesses: Vec<(String, u64)>,
}

/// One shadow observation: what the key was, and what the build produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub epoch: u64,
    /// The row's IDENTITY path — the authored leaf path a caller names, before
    /// the phase-340 shared-group redirect. Distinct from `artifact` because
    /// several rows redirect into ONE group dir: the group path says where the
    /// bytes are, this says whose they are. Records bucket on this.
    pub authored: String,
    /// Where the bytes actually are, repo-relative when inside the tree.
    pub artifact: String,
    pub artifact_hash: u64,
    pub coord: Coord,
    pub row_label: String,
    pub provenance: Provenance,
    pub key: u64,
    pub covered_count: usize,
    /// Hash of the whole covered `(hash, path)` list. Two observations with the
    /// same key have the same digest unless the key collided.
    pub covered_digest: u64,
    /// Present only under [`FULL_ENV`].
    pub covered: Vec<(u64, String)>,
    pub classes: Vec<ClassObservation>,
}

impl Observation {
    pub fn coord_str(&self) -> String {
        format!("{},{},{}", self.coord.0, self.coord.1, self.coord.2)
    }

    /// Every `(class, witness, hash)` triple, for diffing.
    fn witness_map(&self) -> BTreeMap<(String, String), u64> {
        let mut m = BTreeMap::new();
        for c in &self.classes {
            for (n, h) in &c.witnesses {
                m.insert((c.name.clone(), n.clone()), *h);
            }
        }
        m
    }
}

/// Why an artifact could not be observed. Every variant is a REFUSAL, never a
/// degraded observation: a key computed over a set we could not measure is the
/// exact object this module exists to keep out of a cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserveError {
    /// The artifact is missing or unreadable.
    Unreadable(String),
    /// No `.d` and no `.ninja_deps` — nothing measured the inputs.
    NoInputRecord(String),
    /// A dep record exists but lists no usable input.
    EmptyInputSet(String),
    /// The path attributes to no manifest row, so it has no coordinate — and
    /// the coordinate is half the key. `attribute_path` also returns `None` for
    /// an AMBIGUOUS match (issue 0517), which must not be resolved by guessing.
    NoCoordinate(String),
}

impl std::fmt::Display for ObserveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObserveError::Unreadable(p) => write!(f, "artifact unreadable: {p}"),
            ObserveError::NoInputRecord(p) => write!(
                f,
                "no dep record for {p}: neither a cargo `.d` beside it nor a \
                 `.ninja_deps` in its build dir. Refusing to key on an unmeasured \
                 input set — a key over nothing matches everything."
            ),
            ObserveError::EmptyInputSet(p) => write!(
                f,
                "the dep record for {p} listed no usable input. Refusing to key on \
                 nothing (the `files.is_empty()` refusal the staleness probe already \
                 makes, where it returns `None` rather than `Some(false)`)."
            ),
            ObserveError::NoCoordinate(p) => write!(
                f,
                "{p} attributes to no manifest row (or to an ambiguous one — issue \
                 0517), so it has no `row_coord()` coordinate. The coordinate is half \
                 the key; guessing it would key two different cells the same."
            ),
        }
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rel(path: &Path) -> String {
    let root = project_root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The measured input set for `artifact`, and where it came from.
///
/// Reads the toolchain's own record through the SHARED readers in
/// [`super::staleness`] — the same ones the freshness probe walks. There is no
/// second parser here, deliberately: a cache key derived from a different
/// reading of the dep graph than the probe uses would be two derivations of one
/// fact, which is the drift class CLAUDE.md's "fix the CLASS" rule is about.
pub fn measured_inputs(artifact: &Path) -> Result<(Provenance, Vec<PathBuf>), ObserveError> {
    let dep = artifact.with_extension("d");
    let cargo = staleness::dep_file_paths(&dep);
    if !cargo.is_empty() {
        return Ok((Provenance::CargoDepInfo, cargo));
    }
    if let Some(build_dir) = artifact.parent() {
        let ninja = staleness::ninja_dep_paths(build_dir);
        if !ninja.is_empty() {
            return Ok((Provenance::NinjaDeps, ninja));
        }
        if build_dir.join(".ninja_deps").exists() {
            return Err(ObserveError::EmptyInputSet(rel(artifact)));
        }
    }
    if dep.exists() {
        return Err(ObserveError::EmptyInputSet(rel(artifact)));
    }
    Err(ObserveError::NoInputRecord(rel(artifact)))
}

/// The covered `(hash, relpath)` list: every measured input, content-hashed,
/// sorted, deduped.
///
/// # Which exemption applies here, and which does not
///
/// [`super::staleness::exempt_probe_input`] is the ONE rule for "is this
/// candidate an edit event". This consumer acts on its two answers differently,
/// and names the choice rather than hiding it (the `first_sight_is_fresh`
/// precedent one module over):
///
/// * [`Exemption::CargoOutDir`] — SKIPPED. An `OUT_DIR` product is a build
///   output, and a key over outputs is not computable before the build.
/// * [`Exemption::RegeneratedInPlace`] — KEPT. That exemption exists because
///   cbindgen moves an mtime without changing bytes, and this consumer compares
///   BYTES, so the reason does not apply. Dropping it would be actively unsafe:
///   for a cmake fixture the Rust sources behind corrosion are invisible to
///   ninja, so `nros_generated.h` is the only place a change to that surface
///   shows up at all.
fn covered_inputs(paths: &[PathBuf]) -> (Vec<(u64, String)>, usize) {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut exempt_outdir = 0usize;
    for p in paths {
        if staleness::exempt_probe_input(p) == Some(Exemption::CargoOutDir) {
            exempt_outdir += 1;
            continue;
        }
        files.push(p.clone());
    }
    files.sort();
    files.dedup();
    let mut out: Vec<(u64, String)> = files
        .iter()
        .filter_map(|f| staleness::hash_file_content(f).map(|h| (h, rel(f))))
        .collect();
    out.sort();
    (out, exempt_outdir)
}

// ---------------------------------------------------------------------------
// Witnesses for the four invisible classes.
// ---------------------------------------------------------------------------

/// Every `.a` under `build_root`, hashed. Bounded: build trees are deep and a
/// witness that walks forever is a witness nobody turns on.
fn link_archives(build_root: &Path) -> Vec<(String, u64)> {
    const MAX_DEPTH: usize = 8;
    const MAX_FILES: usize = 512;
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut stack = vec![(build_root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH || out.len() >= MAX_FILES {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push((p, depth + 1));
            } else if ft.is_file() && p.extension().is_some_and(|x| x == "a") {
                if out.len() >= MAX_FILES {
                    break;
                }
                if let Some(h) = staleness::hash_file_content(&p) {
                    let name = p
                        .strip_prefix(build_root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .into_owned();
                    out.push((name, h));
                }
            }
        }
    }
    out.sort();
    out
}

/// The env var NAMES this repo declares as build inputs.
///
/// From `scripts/check-path-env-fingerprints.py --list-env-names`, which is the
/// tree's ONE enumerator of both producers — the `cargo:rerun-if-env-changed=`
/// literals in tracked Rust sources AND the `rerun_if_env_changed` arrays in
/// `config/*/nros-platform.toml`. Issue 0491 records what happens when only one
/// producer is consulted: the FreeRTOS rows went to zero units while every
/// ThreadX row still rebuilt six.
fn declared_env_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let root = project_root();
        let out = std::process::Command::new("python3")
            .arg(root.join("scripts/check-path-env-fingerprints.py"))
            .arg("--list-env-names")
            .current_dir(&root)
            .output();
        match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
    })
}

/// `(NAME, hash(value))` for every declared name, as visible to THIS process.
///
/// `<unset>` is hashed like any other value, because unset-vs-set is exactly
/// the distinction issue 0491's third spelling is about.
fn env_witnesses() -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = declared_env_names()
        .iter()
        .map(|n| {
            let v = std::env::var(n).unwrap_or_else(|_| "<unset>".to_string());
            (n.clone(), staleness::fnv1a(v.as_bytes()))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The resolved Kconfig `.config` under an artifact's build root, if any.
///
/// Both spellings are checked because both exist: `<build>/zephyr/.config` is
/// what `$DOTCONFIG` names for a Zephyr image, and a bare `<build>/.config` is
/// what the other Kconfig-shaped builds write.
fn kconfig_files(build_root: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for cand in [
        build_root.join("zephyr/.config"),
        build_root.join(".config"),
    ] {
        if cand.is_file()
            && let Some(h) = staleness::hash_file_content(&cand)
        {
            let name = cand
                .strip_prefix(build_root)
                .unwrap_or(&cand)
                .to_string_lossy()
                .into_owned();
            out.push((name, h));
        }
    }
    out.sort();
    out
}

/// The in-tree CLI's source-closure stamp, from the binary itself.
///
/// `nros source-stamp` prints `fresh (<hex>)` when the embedded stamp matches
/// and EXITS NON-ZERO with `sources are now <hex>` when it does not. Both carry
/// the number we want — the stamp over the sources AS THEY ARE NOW, which is
/// what a key must hash. A stale CLI is a legitimate state to observe; it is
/// not a reason to refuse.
fn cli_source_stamp() -> Option<u64> {
    let root = project_root();
    let bin = root.join("packages/cli/target/release/nros");
    if !bin.is_file() {
        return None;
    }
    let out = std::process::Command::new(&bin)
        .arg("source-stamp")
        .current_dir(&root)
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if let Some(rest) = text.split("sources are now ").nth(1) {
        return parse_leading_hex(rest);
    }
    let rest = text.split("fresh (").nth(1)?;
    parse_leading_hex(rest)
}

fn parse_leading_hex(s: &str) -> Option<u64> {
    let hex: String = s.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() {
        return None;
    }
    u64::from_str_radix(&hex, 16).ok()
}

// ---------------------------------------------------------------------------
// The key.
// ---------------------------------------------------------------------------

/// The canonical text a key is the hash of. Exposed so a disagreement can be
/// DIFFED rather than argued about.
pub fn key_preimage(
    coord: &Coord,
    provenance: Provenance,
    covered: &[(u64, String)],
    classes: &[ClassObservation],
) -> String {
    let mut s = format!("nros-fixture-cache-key v{RECORD_VERSION}\n");
    s.push_str(&format!("coord {},{},{}\n", coord.0, coord.1, coord.2));
    s.push_str(&format!("provenance {}\n", provenance.tag()));
    // Only COVERED classes enter the preimage. An uncovered class is recorded
    // in the observation and deliberately absent here — that absence IS the
    // incompleteness the shadow pass measures.
    for c in classes {
        if c.coverage != Coverage::Covered {
            continue;
        }
        for (n, h) in &c.witnesses {
            s.push_str(&format!("class {} {n} {h:016x}\n", c.name));
        }
    }
    for (h, p) in covered {
        s.push_str(&format!("input {h:016x} {p}\n"));
    }
    s
}

fn digest_of(covered: &[(u64, String)]) -> u64 {
    let mut s = String::new();
    for (h, p) in covered {
        s.push_str(&format!("{h:016x} {p}\n"));
    }
    staleness::fnv1a(s.as_bytes())
}

/// Compute one shadow observation for a built artifact.
///
/// Every failure is a refusal ([`ObserveError`]); there is no path that returns
/// a key over an input set it could not measure.
pub fn observe(authored: &Path) -> Result<Observation, ObserveError> {
    // The COORDINATE comes from the authored path, because that is the one the
    // manifest can attribute: since phase-340 many rows redirect into ONE
    // shared `build/cargo-fixtures/<platform>` group dir, and `attribute_path`
    // deliberately returns `None` there — a group path names a platform, not a
    // row. The BYTES come from wherever `groups::resolved` says the build put
    // them. Doing it the other way round (attributing the resolved path) is how
    // every migrated cargo fixture would refuse.
    let row =
        lane::attribute_path(authored).ok_or_else(|| ObserveError::NoCoordinate(rel(authored)))?;
    let artifact = groups::resolved(authored);
    observe_at(&row.coord, row.label(), authored, &artifact)
}

/// [`observe`] with the coordinate SUPPLIED rather than attributed.
///
/// Necessary, not a convenience: measured on the manifest as shipped, 8 of 221
/// artifact roots carry rows at MORE THAN ONE coordinate (33 of 256 rows —
/// every native rust talker/listener/service/action leaf, whose zenoh, xrce and
/// cyclonedds rows all land in `<leaf>/target` since the `target_dir` column
/// was dropped). [`lane::attribute_path`] fails CLOSED on those, per issue
/// 0517, and it is right to: picking one would key three different builds the
/// same, which for a cache is the wrong-artifact bug it is trying to prevent.
/// So the caller names the coordinate instead of the tool guessing it.
pub fn observe_with_coord(
    coord: &Coord,
    label: &str,
    authored: &Path,
) -> Result<Observation, ObserveError> {
    let artifact = groups::resolved(authored);
    observe_at(coord, label, authored, &artifact)
}

/// [`observe`] for a caller that ALREADY selected its manifest row (issue 0517
/// step 1) and so needs no path attribution. `GroupRow` carries `row_coord`'s
/// coordinate, which is the same computation `attribute_path` would recover —
/// asking the row directly just skips the round trip.
pub fn observe_row(row: &GroupRow, artifact: &Path) -> Result<Observation, ObserveError> {
    observe_at(&row.coord, &row.dir, artifact, artifact)
}

/// The observation itself, with the coordinate already decided.
pub fn observe_at(
    coord: &Coord,
    label: &str,
    authored: &Path,
    artifact: &Path,
) -> Result<Observation, ObserveError> {
    let artifact_hash = staleness::hash_file_content(artifact)
        .ok_or_else(|| ObserveError::Unreadable(rel(artifact)))?;
    let (provenance, inputs) = measured_inputs(artifact)?;
    let (covered, _exempt_outdir) = covered_inputs(&inputs);
    if covered.is_empty() {
        return Err(ObserveError::EmptyInputSet(rel(artifact)));
    }

    let build_root = artifact.parent().unwrap_or(artifact);
    let mut classes = Vec::new();
    for c in INVISIBLE_CLASSES {
        let witnesses = match c.name {
            "link-archives" => link_archives(build_root),
            "env-vars" => env_witnesses(),
            "kconfig" => kconfig_files(build_root),
            "cli-closure" => cli_source_stamp()
                .map(|h| vec![("nros-source-stamp".to_string(), h)])
                .unwrap_or_default(),
            other => unreachable!("unhandled invisible-input class {other}"),
        };
        // A class DESIGNED as covered but with nothing to hash is recorded
        // `not-observable`, never silently "covered". "Covered in principle"
        // and "covered for this artifact" are different facts, and only the
        // second one is evidence.
        let coverage = match (c.designed, witnesses.is_empty()) {
            (Coverage::Covered, true) => Coverage::NotObservable,
            (designed, _) => designed,
        };
        classes.push(ClassObservation {
            name: c.name.to_string(),
            coverage,
            witnesses,
        });
    }

    let key = staleness::fnv1a(key_preimage(coord, provenance, &covered, &classes).as_bytes());
    let full = std::env::var_os(FULL_ENV).is_some();

    Ok(Observation {
        epoch: now_epoch(),
        authored: rel(authored),
        artifact: rel(artifact),
        artifact_hash,
        coord: coord.clone(),
        row_label: label.to_string(),
        provenance,
        key,
        covered_count: covered.len(),
        covered_digest: digest_of(&covered),
        covered: if full { covered } else { Vec::new() },
        classes,
    })
}

// ---------------------------------------------------------------------------
// The store.
// ---------------------------------------------------------------------------

/// Where records live: under `target/` (gitignored by `**/target/`), like the
/// staleness ledger. Overridable with [`STORE_ENV`].
pub fn store_dir() -> PathBuf {
    match std::env::var_os(STORE_ENV) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => project_root().join("target/nros-fixture-cache-shadow"),
    }
}

fn encode(obs: &Observation) -> String {
    let mut s = String::new();
    s.push_str(&format!("version {RECORD_VERSION}\n"));
    s.push_str(&format!("epoch {}\n", obs.epoch));
    s.push_str(&format!("authored {}\n", obs.authored));
    s.push_str(&format!("artifact {}\n", obs.artifact));
    s.push_str(&format!("artifact-hash {:016x}\n", obs.artifact_hash));
    s.push_str(&format!(
        "coord {},{},{}\n",
        obs.coord.0, obs.coord.1, obs.coord.2
    ));
    s.push_str(&format!("row {}\n", obs.row_label));
    s.push_str(&format!("provenance {}\n", obs.provenance.tag()));
    s.push_str(&format!("key {:016x}\n", obs.key));
    s.push_str(&format!("covered-count {}\n", obs.covered_count));
    s.push_str(&format!("covered-digest {:016x}\n", obs.covered_digest));
    for c in &obs.classes {
        s.push_str(&format!("class {} {}\n", c.name, c.coverage.tag()));
        for (n, h) in &c.witnesses {
            s.push_str(&format!("witness {} {h:016x} {n}\n", c.name));
        }
    }
    for (h, p) in &obs.covered {
        s.push_str(&format!("covered {h:016x} {p}\n"));
    }
    s
}

/// Parse one record. `None` for anything this build cannot read — a record
/// written by a different `RECORD_VERSION` included, because comparing two
/// different key definitions would manufacture mismatches.
pub fn decode(text: &str) -> Option<Observation> {
    let mut version = None;
    let mut epoch = 0u64;
    let mut authored = String::new();
    let mut artifact = String::new();
    let mut artifact_hash = 0u64;
    let mut coord: Option<Coord> = None;
    let mut row_label = String::new();
    let mut provenance = None;
    let mut key = None;
    let mut covered_count = 0usize;
    let mut covered_digest = 0u64;
    let mut covered: Vec<(u64, String)> = Vec::new();
    let mut classes: Vec<ClassObservation> = Vec::new();

    for line in text.lines() {
        let (k, v) = line.split_once(' ')?;
        match k {
            "version" => version = v.parse::<u32>().ok(),
            "epoch" => epoch = v.parse().ok()?,
            "authored" => authored = v.to_string(),
            "artifact" => artifact = v.to_string(),
            "artifact-hash" => artifact_hash = u64::from_str_radix(v, 16).ok()?,
            "coord" => {
                let p: Vec<&str> = v.split(',').collect();
                if p.len() != 3 {
                    return None;
                }
                coord = Some((p[0].to_string(), p[1].to_string(), p[2].to_string()));
            }
            "row" => row_label = v.to_string(),
            "provenance" => provenance = Provenance::parse(v),
            "key" => key = u64::from_str_radix(v, 16).ok(),
            "covered-count" => covered_count = v.parse().ok()?,
            "covered-digest" => covered_digest = u64::from_str_radix(v, 16).ok()?,
            "class" => {
                let (name, cov) = v.split_once(' ')?;
                classes.push(ClassObservation {
                    name: name.to_string(),
                    coverage: Coverage::parse(cov)?,
                    witnesses: Vec::new(),
                });
            }
            "witness" => {
                let mut it = v.splitn(3, ' ');
                let (cname, h, wname) = (it.next()?, it.next()?, it.next()?);
                let h = u64::from_str_radix(h, 16).ok()?;
                classes
                    .iter_mut()
                    .find(|c| c.name == cname)?
                    .witnesses
                    .push((wname.to_string(), h));
            }
            "covered" => {
                let (h, p) = v.split_once(' ')?;
                covered.push((u64::from_str_radix(h, 16).ok()?, p.to_string()));
            }
            _ => return None,
        }
    }
    if version != Some(RECORD_VERSION) {
        return None;
    }
    Some(Observation {
        epoch,
        authored,
        artifact,
        artifact_hash,
        coord: coord?,
        row_label,
        provenance: provenance?,
        key: key?,
        covered_count,
        covered_digest,
        covered,
        classes,
    })
}

/// Append one observation to the store. Atomic (temp + rename), one file per
/// observation, so parallel recorders never interleave.
pub fn record(obs: &Observation) -> std::io::Result<PathBuf> {
    let dir = store_dir().join(staleness::flatten_path_key(Path::new(&obs.authored)));
    fs::create_dir_all(&dir)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{nanos:039}-{}.rec", std::process::id()));
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, encode(obs).as_bytes())?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Every record in the store, oldest first.
pub fn load_records() -> Vec<Observation> {
    let mut out = Vec::new();
    let Ok(dirs) = fs::read_dir(store_dir()) else {
        return out;
    };
    for d in dirs.flatten() {
        let Ok(files) = fs::read_dir(d.path()) else {
            continue;
        };
        for f in files.flatten() {
            if f.path().extension().is_none_or(|e| e != "rec") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(f.path())
                && let Some(obs) = decode(&text)
            {
                out.push(obs);
            }
        }
    }
    out.sort_by_key(|o| (o.authored.clone(), o.epoch));
    out
}

/// Observe and record, best-effort, only when [`SHADOW_ENV`] is set.
///
/// Called from the fixture resolvers so a normal shadow-mode test run seeds the
/// store without any extra wiring. A refusal is PRINTED, never propagated: this
/// is measurement riding along with a build, and it must not be able to fail
/// one. It also cannot change what the caller does — there is no return value a
/// caller could branch on.
pub fn observe_and_record_if_enabled(authored: &Path) {
    if std::env::var_os(SHADOW_ENV).is_none() {
        return;
    }
    record_outcome(observe(authored));
}

/// [`observe_and_record_if_enabled`] for a resolver that already has its row.
pub fn observe_row_and_record_if_enabled(row: &GroupRow, artifact: &Path) {
    if std::env::var_os(SHADOW_ENV).is_none() {
        return;
    }
    record_outcome(observe_row(row, artifact));
}

fn record_outcome(observed: Result<Observation, ObserveError>) {
    match observed {
        Ok(obs) => {
            if let Err(e) = record(&obs) {
                eprintln!("cache-shadow: could not record {}: {e}", obs.artifact);
            }
        }
        Err(e) => eprintln!("cache-shadow: not observed — {e}"),
    }
}

// ---------------------------------------------------------------------------
// The report.
// ---------------------------------------------------------------------------

/// One key that predicted two different artifacts — the thing shadow mode
/// exists to find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub artifact: String,
    pub coord: Coord,
    pub key: u64,
    pub first_epoch: u64,
    pub first_artifact_hash: u64,
    pub second_epoch: u64,
    pub second_artifact_hash: u64,
    /// `(class, witness, before, after)` for every witness that moved.
    pub differing: Vec<(String, String, u64, u64)>,
    /// True when the covered digest ALSO differs, i.e. the key hash collided
    /// rather than the key being incomplete.
    pub covered_digest_differs: bool,
}

impl Mismatch {
    /// The line a reader needs: which input differed, or that none did.
    pub fn attribution(&self) -> String {
        if self.covered_digest_differs {
            return "KEY COLLISION — the covered input sets differ, so two different \
                    inputs hashed to one key. This is not an incompleteness; it is the \
                    hash."
                .to_string();
        }
        if self.differing.is_empty() {
            return "UNATTRIBUTED — the covered inputs are identical and NO witness \
                    moved. The differing input is outside both the key and every \
                    witness, so a fifth invisible class exists and is not yet named."
                .to_string();
        }
        let mut s = String::from("differing inputs, all OUTSIDE the key:");
        for (cls, name, before, after) in &self.differing {
            let issue = class(cls).map(|c| c.issue).unwrap_or("?");
            s.push_str(&format!(
                "\n      [{cls} / issue {issue}] {name}: {before:016x} -> {after:016x}"
            ));
        }
        s
    }
}

/// Per-coordinate tallies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoordStats {
    pub observations: usize,
    /// Observations whose key had been seen before — a real cache would have
    /// HIT here, so these are the only ones the key makes a prediction for.
    pub predictions: usize,
    /// Predictions where the artifact matched what the key had served before.
    pub correct: usize,
    /// Predictions where it did not.
    pub mismatches: usize,
}

impl CoordStats {
    /// Observations whose key was new — a cache would have MISSED, so the key
    /// made no prediction and nothing is proved either way.
    pub fn novel(&self) -> usize {
        self.observations - self.predictions
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub per_coord: BTreeMap<String, CoordStats>,
    pub mismatches: Vec<Mismatch>,
}

impl Report {
    pub fn total_observations(&self) -> usize {
        self.per_coord.values().map(|s| s.observations).sum()
    }
    pub fn total_predictions(&self) -> usize {
        self.per_coord.values().map(|s| s.predictions).sum()
    }
    pub fn total_correct(&self) -> usize {
        self.per_coord.values().map(|s| s.correct).sum()
    }
}

/// Fold a record set into a report.
///
/// A "prediction" is an observation whose `(artifact, key)` pair was already in
/// the store: that is exactly when a cache would have skipped the build. It is
/// correct when the artifact the build produced equals the one the key was last
/// seen with, and a mismatch otherwise. Keys are scoped per artifact because
/// two artifacts legitimately share neither key nor bytes.
pub fn report_from(records: &[Observation]) -> Report {
    let mut sorted: Vec<&Observation> = records.iter().collect();
    sorted.sort_by_key(|o| (o.authored.clone(), o.epoch));

    let mut report = Report::default();
    // (artifact, key) -> the observation that first established it.
    let mut seen: BTreeMap<(String, u64), &Observation> = BTreeMap::new();

    for obs in sorted {
        let stats = report.per_coord.entry(obs.coord_str()).or_default();
        stats.observations += 1;
        let slot = (obs.authored.clone(), obs.key);
        match seen.get(&slot) {
            None => {
                seen.insert(slot, obs);
            }
            Some(prev) => {
                stats.predictions += 1;
                if prev.artifact_hash == obs.artifact_hash {
                    stats.correct += 1;
                } else {
                    stats.mismatches += 1;
                    let before = prev.witness_map();
                    let after = obs.witness_map();
                    let mut differing = Vec::new();
                    for (k, b) in &before {
                        match after.get(k) {
                            Some(a) if a == b => {}
                            Some(a) => differing.push((k.0.clone(), k.1.clone(), *b, *a)),
                            // A witness that disappeared is still a difference.
                            None => differing.push((k.0.clone(), k.1.clone(), *b, 0)),
                        }
                    }
                    for (k, a) in &after {
                        if !before.contains_key(k) {
                            differing.push((k.0.clone(), k.1.clone(), 0, *a));
                        }
                    }
                    differing.sort();
                    report.mismatches.push(Mismatch {
                        artifact: obs.authored.clone(),
                        coord: obs.coord.clone(),
                        key: obs.key,
                        first_epoch: prev.epoch,
                        first_artifact_hash: prev.artifact_hash,
                        second_epoch: obs.epoch,
                        second_artifact_hash: obs.artifact_hash,
                        differing,
                        covered_digest_differs: prev.covered_digest != obs.covered_digest,
                    });
                }
            }
        }
    }
    report
}

/// Render a report, coverage table included.
///
/// The coverage table is printed on EVERY report, not on request: a tally of
/// "0 mismatches" is only meaningful next to the list of input classes the key
/// does not hash, and separating the two is how a partial key comes to read as
/// a complete one.
pub fn render(report: &Report, records: &[Observation]) -> String {
    let mut s = String::new();
    s.push_str("nros fixture cache — SHADOW MODE (no hit may skip a build)\n\n");

    s.push_str("INPUT CLASSES THE COMPILER CANNOT SEE\n");
    for c in INVISIBLE_CLASSES {
        // What the RECORDS actually show, which can be weaker than the design.
        let observed: Vec<Coverage> = records
            .iter()
            .filter_map(|o| o.classes.iter().find(|x| x.name == c.name))
            .map(|x| x.coverage)
            .collect();
        let not_obs = observed
            .iter()
            .filter(|c| **c == Coverage::NotObservable)
            .count();
        s.push_str(&format!(
            "  {:<14} issue {}  designed: {:<14} {}\n",
            c.name,
            c.issue,
            c.designed.tag(),
            if observed.is_empty() {
                "(no observations)".to_string()
            } else {
                format!(
                    "observed not-observable in {not_obs}/{} records",
                    observed.len()
                )
            }
        ));
        s.push_str(&format!("    {}\n", c.what));
        s.push_str(&format!("    {}\n", c.rationale));
    }

    s.push_str("\nPER COORDINATE\n");
    s.push_str(&format!(
        "  {:<34} {:>4} {:>6} {:>8} {:>9} {:>6}\n",
        "coordinate", "obs", "novel", "predicted", "correct", "MISM"
    ));
    for (coord, st) in &report.per_coord {
        s.push_str(&format!(
            "  {:<34} {:>4} {:>6} {:>8} {:>9} {:>6}\n",
            coord,
            st.observations,
            st.novel(),
            st.predictions,
            st.correct,
            st.mismatches
        ));
    }
    s.push_str(&format!(
        "  {:<34} {:>4} {:>6} {:>8} {:>9} {:>6}\n",
        "TOTAL",
        report.total_observations(),
        report.total_observations() - report.total_predictions(),
        report.total_predictions(),
        report.total_correct(),
        report.mismatches.len()
    ));

    if report.mismatches.is_empty() {
        s.push_str(
            "\nNo mismatch recorded. That is NOT yet a licence to serve hits: read the\n\
             `predicted` column — a key that has never been re-seen has never been\n\
             tested, and an uncovered class above is untested until a change to it has\n\
             actually been observed.\n",
        );
    } else {
        s.push_str("\nMISMATCHES — a key that predicted two different artifacts\n");
        for m in &report.mismatches {
            s.push_str(&format!(
                "\n  {} [{},{},{}]\n    key {:016x}\n    artifact {:016x} @ {} -> {:016x} @ {}\n    {}\n",
                m.artifact,
                m.coord.0,
                m.coord.1,
                m.coord.2,
                m.key,
                m.first_artifact_hash,
                m.first_epoch,
                m.second_artifact_hash,
                m.second_epoch,
                m.attribution(),
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coord() -> Coord {
        ("native".into(), "rust".into(), "zenoh".into())
    }

    fn obs(epoch: u64, key: u64, artifact_hash: u64, witness: u64) -> Observation {
        Observation {
            epoch,
            authored: "examples/native/rust/talker/target/x/talker".into(),
            artifact: "build/cargo-fixtures/linux/nros-relwithdebinfo/talker".into(),
            artifact_hash,
            coord: coord(),
            row_label: "examples/native/rust/talker".into(),
            provenance: Provenance::CargoDepInfo,
            key,
            covered_count: 3,
            covered_digest: 0xdead_beef,
            covered: Vec::new(),
            classes: vec![ClassObservation {
                name: "link-archives".into(),
                coverage: Coverage::Uncovered,
                witnesses: vec![("libnros_rmw_cyclonedds.a".into(), witness)],
            }],
        }
    }

    #[test]
    fn the_key_changes_when_a_covered_input_changes() {
        let classes: Vec<ClassObservation> = Vec::new();
        let a = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &[(1, "src/lib.rs".into())],
            &classes,
        );
        let b = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &[(2, "src/lib.rs".into())],
            &classes,
        );
        assert_ne!(
            staleness::fnv1a(a.as_bytes()),
            staleness::fnv1a(b.as_bytes()),
            "a covered input's content hash must reach the key, or the key is \
             keyed on filenames"
        );
    }

    #[test]
    fn the_key_changes_with_the_coordinate() {
        let classes: Vec<ClassObservation> = Vec::new();
        let inputs = [(1u64, "src/lib.rs".to_string())];
        let a = key_preimage(&coord(), Provenance::CargoDepInfo, &inputs, &classes);
        let b = key_preimage(
            &("native".into(), "rust".into(), "cyclonedds".into()),
            Provenance::CargoDepInfo,
            &inputs,
            &classes,
        );
        assert_ne!(
            staleness::fnv1a(a.as_bytes()),
            staleness::fnv1a(b.as_bytes()),
            "two coordinates sharing a source tree must not share a key — the \
             coordinate is half the key (issue 0482's `row_coord`)"
        );
    }

    #[test]
    fn a_covered_class_reaches_the_key_and_an_uncovered_one_does_not() {
        let inputs = [(1u64, "src/lib.rs".to_string())];
        let mk = |name: &str, coverage, h| {
            vec![ClassObservation {
                name: name.into(),
                coverage,
                witnesses: vec![("w".into(), h)],
            }]
        };
        let covered_a = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &inputs,
            &mk("kconfig", Coverage::Covered, 1),
        );
        let covered_b = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &inputs,
            &mk("kconfig", Coverage::Covered, 2),
        );
        assert_ne!(covered_a, covered_b, "a COVERED class must move the key");

        let unc_a = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &inputs,
            &mk("link-archives", Coverage::Uncovered, 1),
        );
        let unc_b = key_preimage(
            &coord(),
            Provenance::CargoDepInfo,
            &inputs,
            &mk("link-archives", Coverage::Uncovered, 2),
        );
        assert_eq!(
            unc_a, unc_b,
            "an UNCOVERED class must NOT move the key — if it did, the report's \
             mismatch count would be measuring something else and the coverage \
             table would be a lie"
        );
    }

    #[test]
    fn a_repeated_key_with_the_same_artifact_is_a_correct_prediction() {
        let r = report_from(&[obs(1, 7, 100, 1), obs(2, 7, 100, 1)]);
        let st = &r.per_coord["native,rust,zenoh"];
        assert_eq!((st.observations, st.predictions, st.correct), (2, 1, 1));
        assert_eq!(st.novel(), 1);
        assert!(r.mismatches.is_empty());
    }

    #[test]
    fn a_repeated_key_with_a_different_artifact_names_the_witness_that_moved() {
        let r = report_from(&[obs(1, 7, 100, 0xaaaa), obs(2, 7, 200, 0xbbbb)]);
        let st = &r.per_coord["native,rust,zenoh"];
        assert_eq!(
            st.mismatches, 1,
            "same key, different artifact IS a mismatch"
        );
        let m = &r.mismatches[0];
        assert_eq!(
            m.differing,
            vec![(
                "link-archives".to_string(),
                "libnros_rmw_cyclonedds.a".to_string(),
                0xaaaa,
                0xbbbb
            )]
        );
        let attribution = m.attribution();
        assert!(
            attribution.contains("libnros_rmw_cyclonedds.a") && attribution.contains("0475"),
            "a mismatch must NAME the input that differed and the issue that \
             predicted it: {attribution}"
        );
    }

    #[test]
    fn a_mismatch_no_witness_explains_says_so_loudly() {
        let r = report_from(&[obs(1, 7, 100, 0xaaaa), obs(2, 7, 200, 0xaaaa)]);
        let m = &r.mismatches[0];
        assert!(
            m.attribution().contains("UNATTRIBUTED"),
            "a mismatch no witness explains means a FIFTH invisible class exists; \
             reporting it as an ordinary mismatch would bury the only evidence \
             that the class list is short: {}",
            m.attribution()
        );
    }

    #[test]
    fn a_different_key_is_never_a_prediction() {
        let r = report_from(&[obs(1, 7, 100, 1), obs(2, 8, 200, 1)]);
        let st = &r.per_coord["native,rust,zenoh"];
        assert_eq!(
            (st.predictions, st.mismatches),
            (0, 0),
            "a cache would have MISSED and rebuilt; that is the safe direction \
             and must not be counted as a correct prediction"
        );
        assert_eq!(st.novel(), 2);
    }

    #[test]
    fn a_record_round_trips_through_the_store_format() {
        let o = obs(11, 22, 33, 44);
        let back = decode(&encode(&o)).expect("a record this build wrote must decode");
        assert_eq!(back, o);
    }

    #[test]
    fn a_record_from_a_different_key_definition_is_dropped_not_compared() {
        let text = encode(&obs(1, 2, 3, 4)).replace("version 1", "version 99");
        assert!(
            decode(&text).is_none(),
            "comparing records written under two key definitions would manufacture \
             mismatches out of a definition change"
        );
    }

    #[test]
    fn an_unmeasured_input_set_is_refused_rather_than_keyed() {
        let dir = project_root().join("target/nros-cache-shadow-selftest");
        std::fs::create_dir_all(&dir).unwrap();
        let artifact = dir.join("no-dep-record");
        std::fs::write(&artifact, b"artifact bytes").unwrap();
        let err = observe(&artifact).expect_err(
            "an artifact with no `.d` and no `.ninja_deps` has an UNMEASURED input \
             set; keying it would produce a key over nothing, which matches \
             everything",
        );
        // No manifest row either, so the coordinate refusal fires first — both
        // are refusals, and asserting the union is what matters here.
        assert!(
            matches!(
                err,
                ObserveError::NoInputRecord(_) | ObserveError::NoCoordinate(_)
            ),
            "unexpected refusal: {err}"
        );
        let _ = std::fs::remove_file(&artifact);
    }

    /// The exact entry point the fixture resolvers call
    /// (`observe_row_and_record_if_enabled` → `observe_row`), on a real
    /// artifact with a real cargo `.d` beside it.
    #[test]
    fn a_resolver_row_observation_reads_the_real_dep_file() {
        let dir = project_root().join("target/nros-cache-shadow-rowtest");
        std::fs::create_dir_all(&dir).unwrap();
        let artifact = dir.join("app");
        // A source this test OWNS, under `target/`. Emphatically NOT a tracked
        // file: the mutation below would move a real source's mtime, and per
        // CLAUDE.md that re-stales every prebuilt fixture in the tree — a unit
        // test that costs a fixture rebuild is a unit test nobody runs.
        let source = dir.join("owned-source.rs");
        std::fs::write(&source, b"fn main() {}\n").unwrap();
        std::fs::write(&artifact, b"artifact bytes").unwrap();
        std::fs::write(
            dir.join("app.d"),
            format!("{}: {}\n", artifact.display(), source.display()).as_bytes(),
        )
        .unwrap();

        let row = GroupRow::for_test("linux");
        let obs = observe_row(&row, &artifact).expect(
            "an artifact with a cargo `.d` naming a real source must observe — if \
             this refuses, the resolver hook records nothing and the whole shadow \
             pass measures zero",
        );
        assert_eq!(obs.provenance, Provenance::CargoDepInfo);
        assert_eq!(obs.covered_count, 1, "the `.d` names exactly one source");
        assert_eq!(obs.coord, row.coord);
        assert_eq!(
            decode(&encode(&obs)).as_ref(),
            Some(&obs),
            "a live observation must survive the store format"
        );

        // The key must move when a covered input's CONTENT moves, which is the
        // one property the whole cache rests on. Proved by mutation rather than
        // asserted.
        let before = obs.key;
        std::fs::write(&source, b"fn main() { /* edited */ }\n").unwrap();
        let after = observe_row(&row, &artifact).unwrap().key;
        assert_ne!(after, before, "editing a COVERED input must move the key");
        std::fs::write(&source, b"fn main() {}\n").unwrap();
        assert_eq!(
            observe_row(&row, &artifact).unwrap().key,
            before,
            "restoring the source must restore the key, or the key is reading \
             something other than content"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_class_in_the_table_has_a_witness_arm() {
        // `observe` matches on `c.name` with an `unreachable!` default, so a
        // class added to the table without a witness arm panics at runtime on
        // the first observation. Catch it here instead.
        for c in INVISIBLE_CLASSES {
            assert!(
                matches!(
                    c.name,
                    "link-archives" | "env-vars" | "kconfig" | "cli-closure"
                ),
                "class {} has no witness arm in `observe`",
                c.name
            );
            assert!(!c.rationale.is_empty(), "class {} has no rationale", c.name);
        }
    }
}
