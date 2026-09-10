//! phase-440 W6 — the store's own inventory: what is in it, how big it is, when
//! it was last read, and which pins forbid removing it (RFC-0095 D2 + D11).
//!
//! The store is **additive by design** — that is what makes rollback free
//! (RFC-0095 D7) — so nothing in it is ever overwritten and nothing shrinks it.
//! D11 states the consequence and the shape of the remedy:
//!
//! > Reference-counting against pins scattered across a filesystem is not
//! > reliable, so the honest shape is explicit and inspectable rather than
//! > clever.
//!
//! Everything here is therefore a *report a human acts on*, not an authority.
//! Two rules follow from that, and both are load-bearing:
//!
//! * **`gc` removes only what this module can attribute to an install.** An
//!   entry with no `.nros-provenance` marker was written by something we do not
//!   model — a source build tree (`corrosion/0.6.src`), the legacy flat prefix
//!   issue 0628 still resolves through, a partial unpack — and deleting one is
//!   how a reclaim verb becomes the thing that broke a host. They are LISTED,
//!   loudly, so the human can decide; they are never removed for them.
//! * **A pin protects, and an absent pin does not permit.** The predicate below
//!   is deliberately conservative in both directions: a pin file we cannot parse
//!   is an error rather than an empty pin set, and a generic pin file protects
//!   any entry whose version string appears in it *anywhere*. A false "pinned"
//!   costs a leftover directory; a false "unpinned" costs someone's toolchain.
//!
//! Nothing here resolves a store path by SEARCHING for a version — that rule
//! (issue 0625, phase-365) governs *resolution*, and this module asks the
//! opposite question. `sdk_store::installed_versions` already carries the same
//! distinction in its own doc comment: "where does the version I pinned live"
//! has one right answer to construct; "what is in here" is only knowable by
//! enumerating.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use eyre::{Result, WrapErr, bail};

use super::{
    sdk_index::SdkIndex,
    sdk_store::{LOCK_FILE, Provenance, SdkLock},
};

/// The store ROOT — `~/.nros`, the parent of `sdk/`, `bin/`, `toolchains/`.
///
/// RFC-0095 D2: "resolved via `NROS_HOME`/`NROS_STORE`, never an absolute
/// literal". `NROS_STORE` is the name the RFC gives the root itself and takes
/// precedence; `NROS_HOME` is what every existing consumer already sets and
/// keeps working unchanged.
///
/// [`super::sdk_store::store_root`] (`<root>/sdk`) and
/// [`super::sdk_store::front_dir`] (`<root>/bin`) are its two established
/// children and now derive from it, so the resolution order has one spelling
/// rather than three copies that could disagree about `NROS_STORE`.
///
/// MOVED to `nros_launcher::store_root` by phase-443 W3 and re-exported here:
/// the launcher is a separate binary now (RFC-0097 D4) and constructs every
/// path it touches under this root, so it cannot ask this crate where the root
/// is — and two implementations is how a launcher comes to look in a different
/// store from the `nros toolchain` verbs that fill it.
pub fn root() -> PathBuf {
    nros_launcher::store_root::root()
}

/// Which environment variable answered [`root`] — for the header line, so a
/// reader never has to guess whose store they are about to shrink.
pub fn root_origin() -> &'static str {
    nros_launcher::store_root::root_origin()
}

/// A top-level directory of the store, and how deep its entries sit.
///
/// The depths are the layout D2 fixes. `Toolchains` and `Workspaces` do not
/// exist on any host yet (W7 and W4 create them); they are modelled here
/// because an absent directory contributes nothing and a listing that learns
/// about them later would be a second enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// `sdk/<tool>/<version>` — provisioned tools (`nros setup`).
    Sdk,
    /// `toolchains/<version>` — nano-ros itself (RFC-0095 D2; phase-440 W7).
    Toolchains,
    /// `workspaces/<name>/<version>` — provisioned SOURCE (phase-440 W4).
    Workspaces,
    /// `fetch/<file-or-dir>` — the download cache (today's `external/`, W3).
    Fetch,
    /// Anything else at the store root (`bin/`, `presets/`). Listed so the
    /// total reconciles with `du`; never a gc candidate.
    Other,
}

impl Category {
    fn dir_name(self) -> &'static str {
        match self {
            Category::Sdk => "sdk",
            Category::Toolchains => "toolchains",
            Category::Workspaces => "workspaces",
            Category::Fetch => "fetch",
            Category::Other => "",
        }
    }

    /// How many path components below the category dir an entry sits.
    fn depth(self) -> usize {
        match self {
            Category::Sdk | Category::Workspaces => 2,
            Category::Toolchains | Category::Fetch => 1,
            Category::Other => 0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Other => "root",
            other => other.dir_name(),
        }
    }

    /// The categories a scan visits, in listing order.
    pub const ALL: [Category; 5] = [
        Category::Toolchains,
        Category::Sdk,
        Category::Workspaces,
        Category::Fetch,
        Category::Other,
    ];
}

/// What we know about who wrote an entry — and therefore whether `gc` may
/// remove it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    /// Carries `.nros-provenance`: written by [`super::sdk_store::execute`].
    /// The only state `gc` will remove.
    Installed,
    /// Part of the legacy FLAT prefix (`<tool>/{lib,share}` beside a
    /// `<tool>/.installed-version`). `tool_dir_usable` still resolves through
    /// it on hosts provisioned before phase-365 — issue 0628 — so removing one
    /// silently un-provisions a working host.
    LegacyFlat,
    /// `sdk/<tool>/<version>.src` — the tree a source install was BUILT from,
    /// written by `provision_source` as `prefix.with_extension("src")`.
    ///
    /// Named separately from `Unmanaged` because it is the largest reclaimable
    /// thing in a real store (1.5 G of 4.4 G on the host this landed on) and
    /// "unmanaged" would tell a human nothing about whether it is safe to
    /// remove. It is still not a gc target: `execute` deletes and re-clones it
    /// on the next source install, so nothing NEEDS it — but a source tree is
    /// also where somebody debugging a build put their local patches, and a
    /// reclaim verb that quietly eats those is worse than one that reclaims
    /// less. It is reported with its size; the human deletes it.
    SourceTree,
    /// Everything else: partial unpacks, hand-made directories, the fetch
    /// cache. Real disk, unknown owner, never removed.
    Unmanaged,
}

impl EntryState {
    pub fn label(self) -> &'static str {
        match self {
            EntryState::Installed => "installed",
            EntryState::LegacyFlat => "legacy-flat",
            EntryState::SourceTree => "source-tree",
            EntryState::Unmanaged => "unmanaged",
        }
    }
}

/// Whether the timestamp in [`Entry::last_used`] is evidence of a READ, or only
/// of the install.
///
/// The distinction exists because it is the difference between a number a human
/// can act on and one that will get their 4 G workspace deleted. `atime` is only
/// a usage signal when the filesystem records reads: under `relatime` (the
/// default) it is, at ~24 h granularity; under `noatime` it is not, and every
/// entry then reports its install time forever.
///
/// Detected rather than assumed: if no file in the entry has an `atime` later
/// than its own `mtime`, nothing has been read since it was written, and the
/// timestamp is the install. Only REGULAR FILES are consulted — walking the tree
/// updates every DIRECTORY's atime, so a scan that counted those would report
/// "used just now" for everything the moment you ran it once.
///
/// The same trap has a second door, and it was open on the first run of this
/// code: deciding an entry's STATE reads its `.nros-provenance`, which updates
/// that file's atime — so every installed entry in the store reported "last used
/// 1s ago (read)" and gc would have found nothing to collect, forever. The
/// marker files this tool reads for its own bookkeeping are therefore excluded
/// from the signal ([`SELF_READ_MARKERS`]). A measurement that its own act of
/// measuring destroys is not a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseEvidence {
    /// A file was read after it was written.
    Read,
    /// No read recorded — the timestamp is when the entry was written.
    InstallOnly,
}

impl UseEvidence {
    pub fn label(self) -> &'static str {
        match self {
            UseEvidence::Read => "read",
            UseEvidence::InstallOnly => "install",
        }
    }
}

/// One thing in the store that occupies disk.
#[derive(Clone, Debug)]
pub struct Entry {
    pub category: Category,
    /// The tool / workspace name, for the two-deep categories.
    pub name: Option<String>,
    /// The last path component — a version for `sdk`/`toolchains`/`workspaces`.
    pub version: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub last_used: SystemTime,
    pub evidence: UseEvidence,
    pub state: EntryState,
    /// The recorded version from `.nros-provenance`, when there is one. It can
    /// differ from the directory name (nothing enforces agreement), and the
    /// pin predicate honours BOTH so a rename cannot orphan a protection.
    pub provenance_version: Option<String>,
}

impl Entry {
    /// `sdk/corrosion/0.6.1-nros1` — the identity a human types back at us.
    pub fn display_id(&self) -> String {
        let mut s = String::from(self.category.label());
        if let Some(name) = &self.name {
            s.push('/');
            s.push_str(name);
        }
        if !self.version.is_empty() {
            s.push('/');
            s.push_str(&self.version);
        }
        s
    }

    /// Every version string this entry answers to.
    fn versions(&self) -> Vec<&str> {
        let mut v = vec![self.version.as_str()];
        if let Some(p) = self.provenance_version.as_deref() {
            if p != self.version {
                v.push(p);
            }
        }
        v
    }
}

/// Size + timestamps for one directory tree, measured in a single walk.
struct Measured {
    size_bytes: u64,
    /// Newest mtime over regular files (or the entry itself when it has none).
    newest_mtime: SystemTime,
    /// Newest atime over regular files, when any exceeded its own mtime.
    newest_read: Option<SystemTime>,
}

/// Files this tool itself opens while inventorying the store. Their `atime` is
/// evidence of a `nros store list`, not of a build using the entry.
const SELF_READ_MARKERS: [&str; 2] = [".nros-provenance", ".installed-version"];

fn measure(path: &Path) -> Measured {
    use std::os::unix::fs::MetadataExt;

    let mut size_bytes = 0u64;
    let mut newest_mtime = std::fs::symlink_metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut newest_read: Option<SystemTime> = None;

    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let Ok(md) = entry.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        size_bytes = size_bytes.saturating_add(md.len());
        // Counted for SIZE, ignored for the timestamps: see SELF_READ_MARKERS.
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| SELF_READ_MARKERS.contains(&n))
        {
            continue;
        }
        // NANOSECOND precision, not `atime()`/`mtime()` seconds. At one-second
        // resolution a file written and read in the same second compares EQUAL,
        // so the read is invisible — which makes a test of this predicate pass
        // whatever the code does, and it did.
        let mtime = (md.mtime(), md.mtime_nsec());
        let atime = (md.atime(), md.atime_nsec());
        if let Some(t) = unix_time(mtime) {
            newest_mtime = newest_mtime.max(t);
        }
        // Strictly LATER, not `>=`: a file written and never read has
        // `atime == mtime` under every mount option, so equality is exactly the
        // no-evidence case.
        if atime > mtime {
            if let Some(t) = unix_time(atime) {
                newest_read = Some(newest_read.map_or(t, |prev: SystemTime| prev.max(t)));
            }
        }
    }

    Measured {
        size_bytes,
        newest_mtime,
        newest_read,
    }
}

fn unix_time((secs, nsecs): (i64, i64)) -> Option<SystemTime> {
    let secs = u64::try_from(secs).ok()?;
    let nsecs = u32::try_from(nsecs).unwrap_or(0);
    Some(SystemTime::UNIX_EPOCH + Duration::new(secs, nsecs))
}

/// Every entry in the store under `root`, in category order then by path.
///
/// A missing category directory contributes nothing — that is how this reads a
/// host with no `toolchains/` yet without special-casing W7.
pub fn scan(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    for category in Category::ALL {
        out.extend(scan_category(root, category));
    }
    out
}

fn scan_category(root: &Path, category: Category) -> Vec<Entry> {
    let mut out = Vec::new();
    match category {
        Category::Other => {
            // Whatever sits at the root that is not a known category: `bin/`,
            // `presets/`. One entry each, so `list`'s total reconciles with
            // `du -sh` on the root instead of quietly under-reporting.
            let known: BTreeSet<&str> = Category::ALL
                .iter()
                .filter(|c| **c != Category::Other)
                .map(|c| c.dir_name())
                .collect();
            for child in children(root) {
                let name = file_name(&child);
                if known.contains(name.as_str()) {
                    continue;
                }
                out.push(build_entry(
                    category,
                    None,
                    name,
                    child,
                    EntryState::Unmanaged,
                ));
            }
        }
        _ if category.depth() == 1 => {
            for child in children(&root.join(category.dir_name())) {
                let name = file_name(&child);
                let state = state_of(&child, None);
                out.push(build_entry(category, None, name, child, state));
            }
        }
        _ => {
            for group in children(&root.join(category.dir_name())) {
                let group_name = file_name(&group);
                let flat = flat_marker(&group);
                for child in children(&group) {
                    let version = file_name(&child);
                    let state = state_of(&child, flat.then_some(EntryState::LegacyFlat));
                    out.push(build_entry(
                        category,
                        Some(group_name.clone()),
                        version,
                        child,
                        state,
                    ));
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Directory children only — a stray file at a category level is not an entry
/// this module models, and reporting it as one would invite a `gc` that removes
/// it. `fetch/` is the exception in principle (a download cache holds files),
/// but its entries are `Unmanaged` either way, so nothing is lost by the simpler
/// rule and a mistake there cannot delete anything.
fn children(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The marker the legacy flat prefix carries — the same pair
/// [`super::sdk_store::tool_dir_candidates`] tests, because it is the same
/// question: is this tool directory itself an install?
fn flat_marker(tool_dir: &Path) -> bool {
    tool_dir.join(".installed-version").is_file() || tool_dir.join(".nros-provenance").is_file()
}

fn state_of(path: &Path, legacy: Option<EntryState>) -> EntryState {
    if Provenance::read(path).is_some() {
        return EntryState::Installed;
    }
    // `<version>.src` is `provision_source`'s build tree — checked BEFORE the
    // legacy-flat fallback, because a tool with a flat marker (corrosion here)
    // has both shapes side by side and calling a source tree "part of the
    // legacy prefix" would be a wrong label on the largest row in the table.
    if path.extension().is_some_and(|e| e == "src") {
        return EntryState::SourceTree;
    }
    legacy.unwrap_or(EntryState::Unmanaged)
}

fn build_entry(
    category: Category,
    name: Option<String>,
    version: String,
    path: PathBuf,
    state: EntryState,
) -> Entry {
    let m = measure(&path);
    let (last_used, evidence) = match m.newest_read {
        Some(read) if read > m.newest_mtime => (read, UseEvidence::Read),
        _ => (m.newest_mtime, UseEvidence::InstallOnly),
    };
    let provenance_version = Provenance::read(&path).map(|p| p.version);
    Entry {
        category,
        name,
        version,
        path,
        size_bytes: m.size_bytes,
        last_used,
        evidence,
        state,
        provenance_version,
    }
}

/// `<root>/toolchains` — where RFC-0095 D2 puts nano-ros itself.
///
/// Constructed, never searched: phase-440 W7 writes `toolchains/<version>`
/// because a pin named that version, so a consumer builds the path from the same
/// input rather than enumerating (the `sdk_store::tool_dir` rule, issue 0625).
pub fn toolchains_dir(root: &Path) -> PathBuf {
    root.join(Category::Toolchains.dir_name())
}

/// The `toolchains/<version>` entry, measured, if that directory is there.
pub fn toolchain_entry(root: &Path, version: &str) -> Option<Entry> {
    let path = toolchains_dir(root).join(version);
    if !path.is_dir() {
        return None;
    }
    let state = state_of(&path, None);
    Some(build_entry(
        Category::Toolchains,
        None,
        version.to_string(),
        path,
        state,
    ))
}

// ---------------------------------------------------------------------------
// Pins
// ---------------------------------------------------------------------------

/// One thing a pin file says is in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinRule {
    /// A structured source that names both halves: `[tool.qemu] version = …`.
    Tool { tool: String, version: String },
    /// A version string from a source whose schema we do not model. Matches any
    /// entry with that version, in any category — deliberately over-broad, see
    /// [`load_pin_file`].
    AnyVersion(String),
}

/// A file consulted for pins, and what it said.
#[derive(Clone, Debug)]
pub struct PinSource {
    pub path: PathBuf,
    pub rules: Vec<PinRule>,
}

/// The pin files this looks for, walking up from the working directory.
///
/// `nros-toolchain.toml` was named here before phase-440 W7 wrote it, because
/// W7 must not have to also teach the reclaim verbs about itself — a delete
/// guard that learns about a new pin file one release LATE is a delete guard
/// that deleted something. W7 landed and this now takes the name from
/// [`super::pin::FILE_NAME`], so there is one spelling of it in the crate.
pub const PIN_FILE_NAMES: [&str; 3] = [super::pin::FILE_NAME, "nros-sdk-index.toml", LOCK_FILE];

/// Load one pin file, choosing the reader by NAME.
///
/// The two structured readers are the schemas this crate already owns, so a pin
/// there names a `(tool, version)` pair and matches exactly. Everything else —
/// including W7's `nros-toolchain.toml`, whose schema W7 gets to choose — is
/// read as generic TOML and every STRING VALUE at any depth becomes a version
/// rule.
///
/// That is a deliberate over-approximation, and the asymmetry is the point:
/// this predicate guards a delete, so being wrong in the "pinned" direction
/// costs a directory that stays, and being wrong in the "unpinned" direction
/// costs a toolchain somebody was using. It also means W7 cannot break this by
/// picking a key name we did not guess.
///
/// An unreadable or unparseable pin file is an ERROR rather than an empty pin
/// set: "no pins" and "I could not tell" must not reach the caller as the same
/// value.
pub fn load_pin_file(path: &Path) -> Result<PinSource> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let rules = match name.as_deref() {
        Some("nros-sdk-index.toml") => {
            let index = SdkIndex::load(path)?;
            index
                .tool
                .iter()
                .map(|(tool, t)| PinRule::Tool {
                    tool: tool.clone(),
                    version: t.version.clone(),
                })
                .collect()
        }
        Some(n) if n == LOCK_FILE => {
            let lock = SdkLock::load(path)?;
            lock.tool
                .iter()
                .map(|(tool, e)| PinRule::Tool {
                    tool: tool.clone(),
                    version: e.version.clone(),
                })
                .collect()
        }
        _ => {
            let raw = std::fs::read_to_string(path)
                .wrap_err_with(|| format!("read pin file {}", path.display()))?;
            let value: toml::Value = toml::from_str(&raw)
                .wrap_err_with(|| format!("parse pin file {}", path.display()))?;
            let mut out = Vec::new();
            collect_strings(&value, &mut out);
            out.into_iter().map(PinRule::AnyVersion).collect()
        }
    };
    Ok(PinSource {
        path: path.to_path_buf(),
        rules,
    })
}

fn collect_strings(value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::String(s) => out.push(s.clone()),
        toml::Value::Array(a) => a.iter().for_each(|v| collect_strings(v, out)),
        toml::Value::Table(t) => t.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

/// The pin files reachable from `start`, nearest first — `start` and its
/// ancestors, taking the closest occurrence of each name.
///
/// Ancestors, not just the cwd, because a user runs a store verb from wherever
/// they happen to be inside their project, the way cargo finds a workspace root.
pub fn discover_pin_files(start: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for dir in start.ancestors() {
        for name in PIN_FILE_NAMES {
            if seen.contains(name) {
                continue;
            }
            let candidate = dir.join(name);
            if candidate.is_file() {
                seen.insert(name);
                out.push(candidate);
            }
        }
    }
    out
}

/// The pin sources a reclaim verb should consult: the files named explicitly,
/// then whatever [`discover_pin_files`] finds from `search_from`.
///
/// `search_from` is a PARAMETER rather than a `current_dir()` read inside, so
/// the decision is a pure function of its inputs and a test can exercise the
/// "nothing to consult" case without `set_current_dir` — a process-global that
/// leaks between parallel tests (issue 1101), and the same refactor
/// `stale_guard::refuse_if_stale` already made for the workspace guard. In-tree
/// it matters twice over: every ancestor of a test's working directory is inside
/// this checkout, which HAS an `nros-sdk-index.toml`, so a discovery keyed on the
/// process cwd can never observe an empty pin set here.
pub fn collect_pins(explicit: &[PathBuf], search_from: Option<&Path>) -> Result<Vec<PinSource>> {
    let mut paths = explicit.to_vec();
    if let Some(dir) = search_from {
        paths.extend(discover_pin_files(dir));
    }
    paths.iter().map(|p| load_pin_file(p)).collect()
}

/// Which of `sources` name this entry. Empty means no pin protects it —
/// which is NOT the same as "safe to delete"; see [`plan_gc`].
pub fn pins_naming<'a>(entry: &Entry, sources: &'a [PinSource]) -> Vec<&'a PinSource> {
    let versions = entry.versions();
    sources
        .iter()
        .filter(|src| {
            src.rules.iter().any(|rule| match rule {
                PinRule::Tool { tool, version } => {
                    entry.category == Category::Sdk
                        && entry.name.as_deref() == Some(tool.as_str())
                        && versions.contains(&version.as_str())
                }
                PinRule::AnyVersion(v) => versions.contains(&v.as_str()),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// gc
// ---------------------------------------------------------------------------

/// Why an entry survived a gc plan.
#[derive(Clone, Debug)]
pub enum KeepReason {
    /// A pin names it. Carries the files that did.
    Pinned(Vec<PathBuf>),
    /// Younger than `--older-than`.
    TooNew,
    /// Not attributable to an install — `gc` never removes these.
    NotInstalled(EntryState),
}

/// A gc decision over every entry. Nothing here touches the filesystem: the
/// plan is computed, printed, and only then — on an explicit opt-in — applied.
#[derive(Debug, Default)]
pub struct GcPlan {
    pub remove: Vec<Entry>,
    pub keep: Vec<(Entry, KeepReason)>,
}

impl GcPlan {
    pub fn bytes_to_remove(&self) -> u64 {
        self.remove.iter().map(|e| e.size_bytes).sum()
    }

    pub fn pinned(&self) -> impl Iterator<Item = (&Entry, &[PathBuf])> {
        self.keep.iter().filter_map(|(e, r)| match r {
            KeepReason::Pinned(files) => Some((e, files.as_slice())),
            _ => None,
        })
    }
}

/// Decide, for every entry, whether gc would remove it.
///
/// Pure — `now` is a parameter, so the age boundary is testable without
/// sleeping and without a process-global clock. The order of the tests is the
/// safety argument, cheapest refusal last:
///
/// 1. **not an install** → keep. Unknown owner (see the module header).
/// 2. **a pin names it** → keep. This is checked BEFORE age on purpose: a
///    pinned entry is protected however old it is, and an implementation that
///    filtered by age first would only be *accidentally* correct — the pin test
///    would never run for the entries that matter most.
/// 3. **younger than the threshold** → keep.
/// 4. otherwise → remove.
pub fn plan_gc(
    entries: Vec<Entry>,
    older_than: Duration,
    sources: &[PinSource],
    now: SystemTime,
) -> GcPlan {
    let mut plan = GcPlan::default();
    for entry in entries {
        if entry.state != EntryState::Installed {
            let state = entry.state;
            plan.keep.push((entry, KeepReason::NotInstalled(state)));
            continue;
        }
        let pins = pins_naming(&entry, sources);
        if !pins.is_empty() {
            let files = pins.iter().map(|p| p.path.clone()).collect();
            plan.keep.push((entry, KeepReason::Pinned(files)));
            continue;
        }
        let age = now
            .duration_since(entry.last_used)
            .unwrap_or(Duration::ZERO);
        if age < older_than {
            plan.keep.push((entry, KeepReason::TooNew));
            continue;
        }
        plan.remove.push(entry);
    }
    plan
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// `90d`, `12h`, `30m`, `2w`, `3600s`.
///
/// A bare number is REFUSED rather than read as seconds: `--older-than 90` is
/// overwhelmingly meant as days, and a tool that silently reads it as a minute
/// and a half would delete a store the user believed was protected for a
/// quarter.
pub fn parse_duration(s: &str) -> Result<Duration> {
    let trimmed = s.trim();
    let (digits, suffix) = trimmed.split_at(
        trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len()),
    );
    if digits.is_empty() {
        bail!("`{s}` has no number — write a count and a unit, e.g. `90d`");
    }
    let n: u64 = digits
        .parse()
        .wrap_err_with(|| format!("`{s}`: `{digits}` is not a count"))?;
    let unit = match suffix {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        "w" => 7 * 24 * 60 * 60,
        "" => bail!(
            "`{s}` has no unit. Say which — `{s}d` (days), `{s}h`, `{s}m` \
             (minutes), `{s}w`, `{s}s`. A bare number is refused because \
             reading it as seconds would delete far more than the writer meant."
        ),
        other => bail!("`{s}`: unknown unit `{other}` — use s, m, h, d or w"),
    };
    Ok(Duration::from_secs(n * unit))
}

/// Binary units, because that is what `du -h` prints and this is read beside it.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [(&str, u64); 4] = [
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
        ("B", 1),
    ];
    for (unit, scale) in UNITS {
        if bytes >= scale {
            if scale == 1 {
                return format!("{bytes} B");
            }
            return format!("{:.1} {unit}", bytes as f64 / scale as f64);
        }
    }
    "0 B".to_string()
}

/// Coarse age — days once past a day, because that is the resolution `atime`
/// has under `relatime` and printing minutes would overstate what we measured.
pub fn format_age(age: Duration) -> String {
    let secs = age.as_secs();
    if secs >= 24 * 60 * 60 {
        format!("{}d", secs / (24 * 60 * 60))
    } else if secs >= 60 * 60 {
        format!("{}h", secs / (60 * 60))
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_number_is_refused_and_units_are_exact() {
        assert_eq!(parse_duration("90d").unwrap().as_secs(), 90 * 86_400);
        assert_eq!(parse_duration("2w").unwrap().as_secs(), 14 * 86_400);
        assert_eq!(parse_duration("30m").unwrap().as_secs(), 1_800);
        assert!(
            parse_duration("90").is_err(),
            "a bare number must be refused"
        );
        assert!(parse_duration("d").is_err());
        assert!(parse_duration("7y").is_err());
    }

    #[test]
    fn sizes_read_the_way_du_prints_them() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1 << 20), "1.0 MiB");
        assert_eq!(format_size(3 << 30), "3.0 GiB");
    }

    #[test]
    fn a_generic_pin_file_yields_every_string_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nros-toolchain.toml");
        std::fs::write(
            &path,
            "[toolchain]\nversion = \"0.6.2\"\nchannel = \"stable\"\n",
        )
        .unwrap();
        let src = load_pin_file(&path).unwrap();
        assert!(src.rules.contains(&PinRule::AnyVersion("0.6.2".into())));
        assert!(src.rules.contains(&PinRule::AnyVersion("stable".into())));
    }
}
