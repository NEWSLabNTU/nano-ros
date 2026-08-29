//! Phase 187.1 — the SDK package index that `nros setup` reads.
//!
//! `nros-sdk-index.toml` is the versioned manifest of host toolchains/tools.
//! Each `[tool.*]` carries a per-host prebuilt `dist` (GitHub Release asset URL,
//! sha256) **and** a `[tool.*.source]` recipe used when no `dist` matches the
//! host — both install into the same `$NROS_HOME/sdk/<tool>/<version>/` prefix.
//! `[source.*]` packages build with the app (target-compiled, never prebuilt);
//! `[gated.*]` are license-gated (never fetched/built — instruct + env check).
//!
//! This module is the format + loader (the rest of `nros setup` — board
//! resolution, fetch/cache, the CI release gate — is Phase 187.2–187.5). See
//! `docs/design/0014-nros-setup-toolchain-management.md`.

use std::{collections::BTreeMap, path::Path};

use eyre::{Result, WrapErr, bail};
use serde::{Deserialize, Serialize};

/// The whole `nros-sdk-index.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkIndex {
    /// Prebuilt host tools (qemu, cross-gcc, zenohd, …), keyed by tool name.
    #[serde(default)]
    pub tool: BTreeMap<String, ToolPackage>,
    /// Source packages built with the app (kernels, small C libs), by name.
    #[serde(default)]
    pub source: BTreeMap<String, SourcePackage>,
    /// License-gated packages (never hosted/built), by name.
    #[serde(default)]
    pub gated: BTreeMap<String, GatedPackage>,
    /// RMW → host package set (Phase 191.6.a). The RMW axis is orthogonal to the
    /// board/platform axis: a board lists only its platform/toolchain packages,
    /// the chosen RMW contributes its host daemon/tool (`zenohd` / `xrce-agent`
    /// / `cyclonedds`). `nros setup <board> --rmw <name>` resolves
    /// `board.packages ∪ rmw.packages` — no `board×rmw` pair enumeration.
    #[serde(default)]
    pub rmw: BTreeMap<String, RmwEntry>,
    /// Board → required package set (Phase 191.1). The board→toolchain SSOT that
    /// ships with the index — replaces board-name keyword guessing in
    /// `resolve_packages`. Keyed by the canonical board id the user passes to
    /// `nros setup <board>`.
    #[serde(default)]
    pub board: BTreeMap<String, BoardEntry>,
    /// Named source groupings not tied to a single board/rmw (Phase 197.2) —
    /// e.g. `[reference.px4]`. Consumed by `tools/setup.sh --with-reference`,
    /// NOT by `nros setup`.
    #[serde(default)]
    pub reference: BTreeMap<String, ReferenceEntry>,
    /// #0390 — the `[source.*]` the REPO's OWN build stage needs, as a UNION
    /// (distinct from the per-board/per-rmw `build_sources`, which cover building
    /// an APP for one target). `just test` links every RMW's `-sys` crate, and
    /// `build-test-fixtures` resolves graphs that path-dep platform sources
    /// (`nuttx-libc`, `px4-rs`) even for a native component — so the contributor
    /// build needs this whole set regardless of which board/rmw was provisioned.
    /// `nros setup --build-sources` provisions them; `--build-sources --check` is
    /// the preflight `just test` / `build-test-fixtures` run before building.
    #[serde(default)]
    pub build_sources: Vec<String>,
    /// phase-327 W1 (RFC-0062) — OS packages by ABSTRACT key, mapped per
    /// package manager. The class `apt-packages` + every module's ad-hoc
    /// prereq probe moves into. `nros setup --system` composes the detected
    /// manager's install command and PRINTS it (`--sudo` to run);
    /// `--system --check` runs the probes (the doctor surface).
    #[serde(default)]
    pub system: BTreeMap<String, SystemDep>,
    /// `[prereq.<key>]` — the prerequisite namespace (RFC-0062, amended
    /// 2026-08-29). Supersedes `[system.*]`, which parses as an alias for
    /// `provider = "system"`. Read the two through [`SdkIndex::prereqs`],
    /// never directly, so a consumer cannot see only half the table.
    #[serde(default)]
    pub prereq: BTreeMap<String, PrereqDep>,
    /// phase-327 W1 — the Rust layer (pinned toolchains, targets, cargo
    /// tools), previously living in `just workspace` recipe bodies.
    #[serde(default)]
    pub rust: RustSection,
    /// phase-327 W1 — pip-installed tools (west, colcon, …), previously
    /// scattered per module.
    #[serde(default)]
    pub python: BTreeMap<String, PythonDep>,
}

/// phase-327 W1 (RFC-0062) — one OS package, declared by abstract key.
///
/// Per-manager mappings are explicit fields (not a flattened map) so
/// `deny_unknown_fields` keeps catching typos; adding a manager is a schema
/// change, which is the point — mappings are reviewed, not guessed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemDep {
    /// One line of intent — surfaces in the composed plan so the user knows
    /// what they are installing and can prune.
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub apt: Vec<String>,
    #[serde(default)]
    pub dnf: Vec<String>,
    #[serde(default)]
    pub pacman: Vec<String>,
    #[serde(default)]
    pub brew: Vec<String>,
    /// Presence probe. Optional: an entry without one is composed into the
    /// install command but reported `unknown` by `--check`.
    #[serde(default)]
    pub check: Option<CheckProbe>,
}

/// Which mechanism installs a prerequisite.
///
/// All four already existed as separate index classes before this table did —
/// `[system.*]`, `[tool.*].dist`, `[tool.*].source` + `install`, `[source.*]`.
/// What did not exist was one NAME a consumer could write without knowing which
/// of the four answers it. `board.packages` had been doing exactly that
/// informally: its entries resolve across `[tool.*]`, `[source.*]`, `[gated.*]`
/// and `[system.*]` already.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// An OS package, via the detected package manager. The default, so every
    /// `[system.*]` entry is a valid `[prereq.*]` entry unchanged.
    #[default]
    System,
    /// A prebuilt dist from the store — the existing `[tool.<key>].dist`.
    Sdk,
    /// Built from source into the store — `[tool.<key>].source` + `install`.
    Source,
    /// A checkout in the tree — the existing `[source.<key>]`.
    Submodule,
}

/// `[prereq.<key>]` — one prerequisite, one or more providers.
///
/// The OS-package fields are flattened in rather than nested under a
/// `[prereq.x.system]` sub-table, so an existing `[system.*]` entry is
/// byte-identical as a `[prereq.*]` entry. That is the whole migration for 25
/// of 25 current entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrereqDep {
    #[serde(default)]
    pub why: Option<String>,
    /// Ordered providers. Empty means "just `provider`", which defaults to
    /// `system` — so the common single-provider entry writes neither field.
    ///
    /// ORDER is preference, and whether it may become a policy-driven chain
    /// (never build from source in CI; prefer the dist offline) is deliberately
    /// still open in RFC-0062 — it interacts with RFC-0065 D14.
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub apt: Vec<String>,
    #[serde(default)]
    pub dnf: Vec<String>,
    #[serde(default)]
    pub pacman: Vec<String>,
    #[serde(default)]
    pub brew: Vec<String>,
    /// The key in `[tool.*]` / `[source.*]` this resolves through, when the
    /// provider is not `system` and the names differ. Absent ⇒ the prereq key
    /// IS the class key.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub check: Option<CheckProbe>,
}

impl PrereqDep {
    /// The providers to try, in order. Never empty: an entry that declares
    /// neither field means `system`.
    #[must_use]
    pub fn provider_chain(&self) -> Vec<Provider> {
        if self.providers.is_empty() {
            vec![self.provider]
        } else {
            self.providers.clone()
        }
    }

    #[must_use]
    pub fn packages_for(&self, manager: &str) -> &[String] {
        match manager {
            "apt" => &self.apt,
            "dnf" => &self.dnf,
            "pacman" => &self.pacman,
            "brew" => &self.brew,
            _ => &[],
        }
    }
}

impl From<&SystemDep> for PrereqDep {
    /// `[system.*]` lowered to `provider = "system"` — the alias that makes W1
    /// additive. Every field maps; none is dropped.
    fn from(d: &SystemDep) -> Self {
        Self {
            why: d.why.clone(),
            providers: Vec::new(),
            provider: Provider::System,
            apt: d.apt.clone(),
            dnf: d.dnf.clone(),
            pacman: d.pacman.clone(),
            brew: d.brew.clone(),
            source: None,
            check: d.check.clone(),
        }
    }
}

impl SystemDep {
    /// The native package list for `manager` ("apt" | "dnf" | "pacman" |
    /// "brew"), empty when unmapped.
    pub fn packages_for(&self, manager: &str) -> &[String] {
        match manager {
            "apt" => &self.apt,
            "dnf" => &self.dnf,
            "pacman" => &self.pacman,
            "brew" => &self.brew,
            _ => &[],
        }
    }
}

/// A presence probe for a [`SystemDep`] / [`PythonDep`]. At LEAST one field —
/// validated, since serde alone would accept an empty table.
///
/// Issue 0487 — the fields are OR-ed, not exclusive: a package can need two
/// spellings to be found on two distros (libgcrypt ships a `.pc` on Arch and a
/// `libgcrypt-config` script on Ubuntu 22.04, never both), so declaring several
/// is correct usage rather than ambiguity. `336eb1724` relaxed the check and
/// left this sentence saying the opposite.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckProbe {
    /// `command -v <cmd>` succeeds.
    #[serde(default)]
    pub cmd: Option<String>,
    /// A shared library the dynamic linker can find (`ldconfig -p` on Linux;
    /// skipped elsewhere — the entry reports `unknown` there).
    ///
    /// Matches by PREFIX, so this answers "is some build of this library
    /// loadable", NOT "is the `-dev` package installed". Those differ: the
    /// runtime package ships `libfoo.so.N`, which a `sharedlib = "libfoo.so"`
    /// probe matches, while the headers and the unversioned symlink come from
    /// `-dev`. Use this when the CONSUMER needs the library at run time — a
    /// `dlopen` (bindgen/libclang) or a runtime dependency (libslirp). When the
    /// consumer compiles against headers, use [`Self::header`]; issue 0603 was
    /// this probe reporting mbedTLS present on a host with only `libmbedtls14`.
    #[serde(default)]
    pub sharedlib: Option<String>,
    /// `pkg-config --exists <name>` succeeds (dev headers).
    ///
    /// The right probe for a `-dev` package that ships a `.pc` — but many do
    /// not (Ubuntu's `libmbedtls-dev` is why `generate_mbedtls_pc_files` has to
    /// fabricate one), so [`Self::header`] covers the rest.
    #[serde(default)]
    pub pkg_config: Option<String>,
    /// A C header exists on the compiler's default search path — the honest
    /// test for a `-dev` package whose consumer `#include`s it (issue 0603).
    ///
    /// Written as the include spelling, not a path: `mbedtls/entropy.h`, so
    /// the probe and the `#include` that needs it are the same string. Checked
    /// against `/usr/include`, `/usr/local/include` and the Debian multiarch
    /// dir; Linux-only, `unknown` elsewhere, same as `sharedlib`.
    ///
    /// Not right for every `-dev` package: a VERSIONED one (`libclang-14-dev`)
    /// installs under `/usr/lib/llvm-14/include`, so a probe here would be a
    /// false negative on a host that genuinely has it.
    #[serde(default)]
    pub header: Option<String>,
    /// phase-397 W2 — the resolved binary EXECUTES.
    ///
    /// Not `cmd`, which is `command_exists`: a PATH lookup, and a store dist is
    /// not on PATH. This is the probe the motivating failure needed — the QEMU
    /// dist's path existed and the dynamic loader was the first thing to
    /// disagree (`libslirp.so.0` missing).
    ///
    /// The value is a command line; a zero exit is PRESENT. `Unknown` where it
    /// cannot answer — a foreign-platform tool cannot be run here, and "cannot
    /// execute" is not "absent".
    #[serde(default)]
    pub runs: Option<String>,
    /// phase-397 W2 — a file that must exist inside the provider's checkout.
    ///
    /// For `source`/`submodule` providers, which have no PATH entry and no
    /// soname: today their presence test is "the directory exists", which is
    /// true of an uninitialised submodule. Relative to the provider's own
    /// `dest`, so the probe does not restate a location the provider already
    /// declares.
    #[serde(default)]
    pub path: Option<String>,
}

impl CheckProbe {
    fn field_count(&self) -> usize {
        [
            self.cmd.is_some(),
            self.sharedlib.is_some(),
            self.pkg_config.is_some(),
            self.header.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }
}

/// phase-327 W1 — the Rust layer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustSection {
    /// Pinned toolchains, keyed by a stable alias (`nightly-pinned`).
    #[serde(default)]
    pub toolchain: BTreeMap<String, RustToolchain>,
    /// rustup targets, keyed by a short alias.
    #[serde(default)]
    pub target: BTreeMap<String, RustTarget>,
    /// `cargo install`ed tools, keyed by binary-ish alias.
    #[serde(default, rename = "cargo-tool")]
    pub cargo_tool: BTreeMap<String, RustCargoTool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustToolchain {
    /// rustup channel (`nightly-2026-04-11`, `stable`).
    pub channel: String,
    #[serde(default)]
    pub components: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustTarget {
    pub triple: String,
    /// Alias of the `[rust.toolchain.*]` this target installs under; `None`
    /// = the default toolchain.
    #[serde(default)]
    pub toolchain: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustCargoTool {
    /// The crates.io crate name (`cargo-nextest`).
    #[serde(rename = "crate")]
    pub crate_name: String,
    #[serde(default)]
    pub version: Option<String>,
    /// `cargo install --locked` (default true).
    #[serde(default = "default_true")]
    pub locked: bool,
    #[serde(default)]
    pub check: Option<CheckProbe>,
}

/// phase-327 W1 — one pip-installed tool.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PythonDep {
    /// The PyPI distribution name.
    pub pip: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub check: Option<CheckProbe>,
}

/// A named `[reference.*]` source grouping (Phase 197.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceEntry {
    /// `[source.*]` names this reference set pulls.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// An RMW's host package set — the orthogonal RMW axis (Phase 191.6.a).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RmwEntry {
    /// The index package names (`[tool]`/`[source]`/`[gated]`) this RMW's host
    /// side needs — e.g. `["zenohd"]`, `["xrce-agent"]`, `["cyclonedds"]`.
    #[serde(default)]
    pub packages: Vec<String>,
    /// `[source.*]` names built with the app for this RMW (Phase 197.2). Consumed
    /// by `tools/setup.sh` (the local dev provisioner), NOT by `nros setup` —
    /// recorded here so the index is the single source manifest.
    #[serde(default)]
    pub build_sources: Vec<String>,
    /// Opt-in dev `[source.*]` (full upstream repos, for hacking on the RMW).
    #[serde(default)]
    pub dev_sources: Vec<String>,
}

/// A prebuilt host tool: a per-host `dist` map + an optional `source` fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPackage {
    pub version: String,
    /// The exact upstream revision the prebuilt is built/repackaged from (Phase
    /// 191.2) — e.g. ARM `13.2.rel1`, xPack `14.2.0-3`, a fork branch. The SSOT
    /// the build scripts consume (as the `build-tool.yml` `upstream` input)
    /// instead of hardcoding/hand-deriving it. For tools with a `source` recipe
    /// this equals `source.ref`; recorded here too for dist-only tools.
    #[serde(default)]
    pub upstream: Option<String>,
    /// host key (`<os>-<arch>`, e.g. `linux-x86_64`) → prebuilt artifact.
    #[serde(default)]
    pub dist: BTreeMap<String, DistArtifact>,
    /// Build-from-source recipe used when no `dist` matches the host.
    #[serde(default)]
    pub source: Option<ToolSource>,
    /// phase-327 W4 (RFC-0062) — `[system.*]` keys this tool's DIST needs at
    /// RUNTIME (e.g. qemu-nros links `libslirp.so.0` dynamically, which stock
    /// Ubuntu does not ship). `nros setup --tool` checks + names them before
    /// the smoke check can fail with a bare loader error.
    #[serde(default)]
    pub system: Vec<String>,
}

/// A board's required SDK package set — the board→toolchain SSOT (Phase 191.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardEntry {
    /// Target arch family (descriptive: `cortex-m3`, `riscv32`, `x86_64`, …).
    #[serde(default)]
    pub arch: Option<String>,
    /// Platform / RTOS (descriptive: `bare-metal`, `freertos`, `posix`, …).
    #[serde(default)]
    pub platform: Option<String>,
    /// The index package names (`[tool]`/`[source]`/`[gated]`) this board needs.
    /// Explicit — no derivation, no board-name guessing. May be empty (e.g. an
    /// ESP32-C3 board whose riscv32 toolchain is rustup-managed).
    #[serde(default)]
    pub packages: Vec<String>,
    /// `[source.*]` names built with the app for this board (Phase 197.2).
    /// Consumed by `tools/setup.sh`, NOT by `nros setup <board>` (they're
    /// target-compiled with the app, not host tools) — recorded here so the
    /// index is the single source manifest.
    #[serde(default)]
    pub build_sources: Vec<String>,
    /// Opt-in dev `[source.*]` (full upstream repos, for in-tree development).
    #[serde(default)]
    pub dev_sources: Vec<String>,
}

/// A prebuilt artifact for one host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistArtifact {
    pub url: String,
    pub sha256: String,
}

/// The source-build fallback recipe — installs into the same prefix as `dist`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSource {
    pub git: String,
    /// Git ref (tag/sha) — pinned in lockstep with the prebuilt `version`.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// Configure step; `{prefix}` is substituted with the install prefix.
    #[serde(default)]
    pub configure: Option<String>,
    /// Build + install step.
    #[serde(default)]
    pub install: Option<String>,
    /// Issue 0374 direction 4 — honour the CHECKOUT's own `rust-toolchain.toml`
    /// instead of building with the workspace's pinned channel.
    ///
    /// Default `false`: a recipe whose checkout pins a different Rust version
    /// makes rustup download a whole second toolchain during `nros setup`,
    /// unannounced (zenoh 1.7.2 pins 1.85.0 and does exactly that). Building it
    /// with the channel the workspace already has avoids the download.
    ///
    /// Set `true` for a recipe that genuinely needs its own pin — a
    /// nightly-only crate cannot be built by a stable channel, and forcing one
    /// would turn a working recipe into a compile error.
    #[serde(default)]
    pub respect_toolchain: bool,
}

/// A package compiled with the user's app for their chosen target.
///
/// Phase 195.B — `[source.*]` provisioning is data-driven: `nros setup`
/// fetches the source into [`dest`](Self::dest) from index data, never a
/// hardcoded `third-party/` path. `submodule` is an optional *mode hint*:
/// - **clone mode** (`git` + `ref` + `dest`, no `submodule`): fresh
///   `git clone`@`ref`. Here the two fields ARE the pin — nothing else records
///   it — so they are required in practice even though the type allows `None`.
/// - **submodule mode** (`submodule` + `dest`, and NO `git`/`ref`):
///   `git submodule update --init <submodule>`, then the gitlink sha.
///
/// Issue 0602 — submodule mode carried `git`/`ref` too, described here as
/// recording "the canonical pin (the SSOT — so `.gitmodules` and the index
/// can't drift)". That claim was false in both halves. The provisioning path
/// never reads them: it takes `submodule`, runs `submodule update --init`, and
/// resolves the commit with `ls-tree HEAD <path>`. And `.gitmodules` plus the
/// gitlink already hold that fact AUTHORITATIVELY, with git enforcing them —
/// so the index held a third recording that nothing kept true, and it had
/// drifted on 6 of the 14 entries (threadx named upstream at a commit one
/// behind our fork's).
///
/// They are gone from those entries. Provenance for a submodule is
/// `.gitmodules` + the gitlink, which is the copy that cannot silently disagree
/// with what gets checked out.
///
/// A source with no fetch fields at all has no provisioning step (e.g. a
/// host-built package whose tree already lives in the workspace).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePackage {
    pub version: String,
    /// Git URL to clone (clone mode). Mutually exclusive with `submodule`.
    #[serde(default)]
    pub git: Option<String>,
    /// Git ref (tag/branch/sha) to check out — pinned in lockstep with
    /// `version`. Required in clone mode.
    #[serde(default, rename = "ref")]
    pub git_ref: Option<String>,
    /// Workspace-relative destination the source is provisioned into. The
    /// index is the SSOT — never a path baked into the `nros` binary.
    #[serde(default)]
    pub dest: Option<String>,
    /// `.gitmodules` path when the canonical source is a committed submodule;
    /// `nros setup` runs `git submodule update --init <path>` instead of a
    /// fresh clone. `git`/`ref` still record the pin (SSOT) in this mode.
    #[serde(default)]
    pub submodule: Option<String>,
    /// Shallow-fetch the submodule (`--depth 1`). Default `true` — pins lag the
    /// upstream branch tip, and `git submodule update --depth 1` fetches the
    /// pinned SHA directly (fetch-by-SHA), so this is a true depth-1 checkout,
    /// not a deepen-to-reach-pin. Set `shallow = false` for a source whose
    /// upstream rejects reachable-SHA shallow fetches. Submodule mode only.
    #[serde(default = "default_true")]
    pub shallow: bool,
    /// Recurse into the source's own nested submodules (`--recursive`). Default
    /// `true`. Only affects a source that *has* nested submodules; it never
    /// pulls sibling top-level sources (e.g. PX4-Autopilot is a separate
    /// `[source.*]`, not nested in `px4-rs`). Set `recursive = false` to pin a
    /// source to its top tree only. Submodule mode only.
    #[serde(default = "default_true")]
    pub recursive: bool,
}

fn default_true() -> bool {
    true
}

// Hand-rolled so `SourcePackage::default()` matches the serde defaults
// (`shallow`/`recursive` default to `true`); a `#[derive(Default)]` would make
// the bools `false` and silently diverge from a TOML-parsed entry.
impl Default for SourcePackage {
    fn default() -> Self {
        Self {
            version: String::new(),
            git: None,
            git_ref: None,
            dest: None,
            submodule: None,
            shallow: true,
            recursive: true,
        }
    }
}

/// How a [`SourcePackage`] is provisioned (Phase 195.B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProvision {
    /// `git clone <git> @ <ref>` into `dest`.
    Clone,
    /// `git submodule update --init <submodule>` (dest is the submodule path).
    Submodule,
    /// No fetch step — the tree already lives in the workspace.
    None,
}

impl SourcePackage {
    /// Which provisioning mode this entry declares (Phase 195.B).
    pub fn provision(&self) -> SourceProvision {
        if self.submodule.is_some() {
            SourceProvision::Submodule
        } else if self.git.is_some() {
            SourceProvision::Clone
        } else {
            SourceProvision::None
        }
    }
}

/// A license-gated package: never fetched or built; `nros setup` instructs the
/// user and `nros doctor` checks the `env` var points at the installed SDK.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatedPackage {
    pub version: String,
    pub env: String,
    #[serde(default)]
    pub installer: Option<String>,
}

impl SdkIndex {
    /// Where a prereq's `path` probe resolves against, if its provider has a
    /// checkout at all.
    ///
    /// phase-397 W2. Only `submodule`/`source` providers have one: an OS
    /// package has no directory we own, and a `sdk` dist lives in the store
    /// under a version we do not know here. Returning `None` makes the probe
    /// abstain (`Unknown`) rather than test against a guessed root.
    ///
    /// The location comes from the provider's own `dest`, so the prereq entry
    /// never restates a path the `[source.*]` entry already declares — one
    /// fact, one place.
    #[must_use]
    pub fn prereq_checkout_dir(
        &self,
        key: &str,
        dep: &PrereqDep,
        repo_root: &Path,
    ) -> Option<std::path::PathBuf> {
        let wants_checkout = dep
            .provider_chain()
            .iter()
            .any(|p| matches!(p, Provider::Submodule | Provider::Source));
        if !wants_checkout {
            return None;
        }
        let class_key = dep.source.as_deref().unwrap_or(key);
        let dest = self.source.get(class_key)?.dest.as_ref()?;
        Some(repo_root.join(dest))
    }

    /// Every prerequisite, `[prereq.*]` and `[system.*]` together.
    ///
    /// THE accessor: no consumer reads either table directly, because one that
    /// reads only `[system.*]` sees a shrinking half of the SSoT while the
    /// migration runs, and one that reads only `[prereq.*]` sees an empty table
    /// today. Both spellings coexist by design until W4 retires the alias.
    ///
    /// `[prereq.*]` WINS on a duplicate key. A key in both is a migration in
    /// progress — the new entry is the intended one — and W4 adds the gate that
    /// makes the overlap illegal once it should no longer exist.
    #[must_use]
    pub fn prereqs(&self) -> BTreeMap<String, PrereqDep> {
        let mut out: BTreeMap<String, PrereqDep> = self
            .system
            .iter()
            .map(|(k, v)| (k.clone(), PrereqDep::from(v)))
            .collect();
        for (k, v) in &self.prereq {
            out.insert(k.clone(), v.clone());
        }
        out
    }

    /// Read, parse, + validate an `nros-sdk-index.toml`.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("failed to read SDK index {}", path.display()))?;
        let idx =
            Self::parse(&raw).wrap_err_with(|| format!("invalid SDK index {}", path.display()))?;
        idx.validate()
            .wrap_err_with(|| format!("invalid SDK index {}", path.display()))?;
        Ok(idx)
    }

    /// Parse from a string (schema only — no cross-reference validation, so unit
    /// tests can parse partial fixtures). [`load`] additionally [`validate`]s.
    pub fn parse(raw: &str) -> Result<Self> {
        toml::from_str(raw).wrap_err("invalid nros-sdk-index.toml schema")
    }

    /// Phase 191.4 — every `[board.*].packages` name must be a defined
    /// `[tool]`/`[source]`/`[gated]` package. Phase 191.6.a extends this to
    /// `[rmw.*].packages`. Catches typos/renames that would otherwise silently
    /// skip (a board's/RMW's tool would just not install).
    pub fn validate(&self) -> Result<()> {
        let known = |pkg: &str| {
            self.tool.contains_key(pkg)
                || self.source.contains_key(pkg)
                || self.gated.contains_key(pkg)
        };
        for (board, entry) in &self.board {
            for pkg in &entry.packages {
                if !known(pkg) {
                    bail!(
                        "board '{board}' references undefined package '{pkg}' \
                         (not a [tool]/[source]/[gated] entry)"
                    );
                }
            }
        }
        for (rmw, entry) in &self.rmw {
            for pkg in &entry.packages {
                if !known(pkg) {
                    bail!(
                        "rmw '{rmw}' references undefined package '{pkg}' \
                         (not a [tool]/[source]/[gated] entry)"
                    );
                }
            }
        }
        // phase-327 W1 — the new classes' cross-references and probe shapes.
        for (name, tool) in &self.tool {
            for key in &tool.system {
                if !self.system.contains_key(key) {
                    bail!(
                        "tool '{name}' declares runtime system dep '{key}' \
                         with no [system.{key}] entry"
                    );
                }
            }
        }
        for (key, dep) in &self.system {
            if dep.apt.is_empty()
                && dep.dnf.is_empty()
                && dep.pacman.is_empty()
                && dep.brew.is_empty()
            {
                bail!("[system.{key}] maps to no package manager at all");
            }
            // issue 0487 — AT LEAST one, not exactly one. The single-probe rule
            // assumed every dependency has one right existence test, and
            // libgcrypt refuted it: Arch's libgcrypt 1.12 ships `libgcrypt.pc`
            // and NO `libgcrypt-config`, Ubuntu 22.04's 1.9 ships the script and
            // no `.pc`. Either probe alone is a false negative on one of the two
            // hosts, and a false negative here HARD-BLOCKS `nros setup` while
            // telling the user to sudo-install a package they already have.
            // Probes are OR-ed (see `run_probe`), so declaring both answers
            // "is the dev package installed" on both distros.
            if let Some(check) = &dep.check
                && check.field_count() == 0
            {
                bail!(
                    "[system.{key}].check must set at least one of cmd/sharedlib/pkg_config/header"
                );
            }
        }
        for (alias, target) in &self.rust.target {
            if let Some(tc) = &target.toolchain
                && !self.rust.toolchain.contains_key(tc)
            {
                bail!(
                    "[rust.target.{alias}] references undefined toolchain alias '{tc}' \
                     (not a [rust.toolchain.*] entry)"
                );
            }
        }
        for (alias, tool) in &self.rust.cargo_tool {
            if let Some(check) = &tool.check
                && check.field_count() != 1
            {
                bail!("[rust.cargo-tool.{alias}].check must set exactly one probe field");
            }
        }
        for (alias, py) in &self.python {
            if let Some(check) = &py.check
                && check.field_count() != 1
            {
                bail!("[python.{alias}].check must set exactly one probe field");
            }
        }
        // Phase 195.B — a `[source.*]` provisioning recipe must be coherent so
        // `nros setup` can act on it without guessing. `submodule` mode needs a
        // `dest`; clone mode (a `git` with no `submodule`) needs both `ref` and
        // `dest`. `git`/`ref` may accompany `submodule` (they record the pin).
        for (name, src) in &self.source {
            match src.provision() {
                SourceProvision::Clone => {
                    if src.git_ref.is_none() {
                        bail!("source '{name}' has `git` but no `ref` (clone needs a pinned ref)");
                    }
                    if src.dest.is_none() {
                        bail!("source '{name}' has `git` but no `dest` (where to provision it)");
                    }
                }
                SourceProvision::Submodule => {
                    if src.dest.is_none() {
                        bail!("source '{name}' has `submodule` but no `dest`");
                    }
                }
                SourceProvision::None => {}
            }
        }
        Ok(())
    }
}

impl ToolPackage {
    /// The prebuilt artifact for `host` (e.g. `linux-x86_64`), if one exists.
    pub fn dist_for(&self, host: &str) -> Option<&DistArtifact> {
        self.dist.get(host)
    }

    /// Whether this tool can be installed on `host` — a matching prebuilt, or a
    /// source recipe to fall back to. (`false` ⇒ no prebuilt + no source.)
    pub fn installable_on(&self, host: &str) -> bool {
        self.dist.contains_key(host) || self.source.is_some()
    }
}

/// The current host key (`<os>-<arch>`), matching `dist` map keys.
pub fn host_key() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other, // x86_64, riscv64, …
    };
    format!("{}-{arch}", std::env::consts::OS) // linux / macos / windows
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[tool.qemu]
version = "11.0-nros1"
dist.linux-x86_64 = { url = "https://github.com/org/nano-ros-sdk/releases/download/qemu-11.0-nros1/qemu-linux-x86_64.tar.zst", sha256 = "aa" }
dist.macos-arm64  = { url = "https://example/qemu-macos-arm64.tar.zst", sha256 = "bb" }
[tool.qemu.source]
git = "https://github.com/org/qemu"
ref = "v11.0-nros1"
configure = "./configure --prefix={prefix} --target-list=arm-softmmu"
install = "make -j && make install"

[tool.arm-none-eabi-gcc]
version = "13.2"
dist.linux-x86_64 = { url = "https://example/arm-gcc-linux-x86_64.tar.zst", sha256 = "cc" }

[source.freertos-kernel]
version = "10.6.2"

[gated.nv-spe-fsp]
version = "36.3"
env = "NV_SPE_FSP_DIR"
installer = "nvidia-sdk-manager"
"#;

    #[test]
    fn parses_tool_source_and_gated_sections() {
        let idx = SdkIndex::parse(SAMPLE).expect("sample parses");
        assert_eq!(idx.tool.len(), 2);
        assert_eq!(idx.source.len(), 1);
        assert_eq!(idx.gated.len(), 1);

        let qemu = &idx.tool["qemu"];
        assert_eq!(qemu.version, "11.0-nros1");
        assert_eq!(qemu.dist_for("linux-x86_64").unwrap().sha256, "aa");
        assert!(qemu.dist_for("windows-x86_64").is_none());
        let src = qemu.source.as_ref().expect("qemu has a source recipe");
        assert_eq!(src.git_ref, "v11.0-nros1"); // the `ref` key
        assert!(src.configure.as_deref().unwrap().contains("{prefix}"));

        assert_eq!(idx.source["freertos-kernel"].version, "10.6.2");
        assert_eq!(idx.gated["nv-spe-fsp"].env, "NV_SPE_FSP_DIR");
    }

    #[test]
    fn installable_on_uses_dist_or_source_fallback() {
        let idx = SdkIndex::parse(SAMPLE).unwrap();
        // qemu: prebuilt for linux, source fallback covers any host.
        assert!(idx.tool["qemu"].installable_on("linux-x86_64"));
        assert!(idx.tool["qemu"].installable_on("freebsd-riscv64")); // via source
        // arm-gcc: prebuilt only for linux-x86_64, no source → not installable elsewhere.
        assert!(idx.tool["arm-none-eabi-gcc"].installable_on("linux-x86_64"));
        assert!(!idx.tool["arm-none-eabi-gcc"].installable_on("macos-arm64"));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let bad = "[tool.qemu]\nversion = \"1\"\nbogus = true\n";
        assert!(SdkIndex::parse(bad).is_err());
    }

    #[test]
    fn validate_rejects_board_referencing_undefined_package() {
        // qemu defined; board references it (ok) + a typo'd one (rejected).
        let ok = SdkIndex::parse("[tool.qemu]\nversion=\"1\"\n[board.x]\npackages=[\"qemu\"]\n")
            .unwrap();
        assert!(ok.validate().is_ok());

        let bad = SdkIndex::parse("[tool.qemu]\nversion=\"1\"\n[board.x]\npackages=[\"qemoo\"]\n")
            .unwrap();
        let err = bad.validate().unwrap_err().to_string();
        assert!(err.contains("undefined package 'qemoo'"), "{err}");

        // source + gated names are valid package targets too.
        let src_gated = SdkIndex::parse(
            "[source.lwip]\nversion=\"1\"\n[gated.fvp]\nversion=\"1\"\nenv=\"E\"\n\
             [board.b]\npackages=[\"lwip\",\"fvp\"]\n",
        )
        .unwrap();
        assert!(src_gated.validate().is_ok());
    }

    #[test]
    fn source_provision_modes_parse_and_validate() {
        // Clone mode: git + ref + dest.
        let clone = SdkIndex::parse(
            "[source.lwip]\nversion=\"2.2.0\"\ngit=\"https://example/lwip.git\"\n\
             ref=\"STABLE-2_2_0\"\ndest=\"third-party/freertos/lwip\"\n",
        )
        .unwrap();
        let lwip = &clone.source["lwip"];
        assert_eq!(lwip.provision(), SourceProvision::Clone);
        assert_eq!(lwip.git_ref.as_deref(), Some("STABLE-2_2_0")); // the `ref` key
        assert!(clone.validate().is_ok());

        // Submodule mode: submodule + dest.
        let sm = SdkIndex::parse(
            "[source.threadx]\nversion=\"6.4.1\"\nsubmodule=\"third-party/threadx/kernel\"\n\
             dest=\"third-party/threadx/kernel\"\n",
        )
        .unwrap();
        assert_eq!(sm.source["threadx"].provision(), SourceProvision::Submodule);
        assert!(sm.validate().is_ok());

        // No-fetch mode: version only.
        let none = SdkIndex::parse("[source.x]\nversion=\"1\"\n").unwrap();
        assert_eq!(none.source["x"].provision(), SourceProvision::None);
        assert!(none.validate().is_ok());
    }

    #[test]
    fn source_submodule_with_pin_is_valid_and_submodule_mode() {
        // git/ref accompany submodule (record the pin/SSOT) — valid, and the
        // mode is Submodule (submodule update preferred over clone).
        let sm = SdkIndex::parse(
            "[source.x]\nversion=\"1\"\ngit=\"https://e/x.git\"\nref=\"abc123\"\n\
             dest=\"third-party/x\"\nsubmodule=\"third-party/x\"\n",
        )
        .unwrap();
        assert!(sm.validate().is_ok());
        assert_eq!(sm.source["x"].provision(), SourceProvision::Submodule);
    }

    #[test]
    fn source_provision_incoherence_is_rejected() {
        // git without ref.
        let no_ref = SdkIndex::parse("[source.x]\nversion=\"1\"\ngit=\"u\"\ndest=\"d\"\n").unwrap();
        assert!(
            no_ref
                .validate()
                .unwrap_err()
                .to_string()
                .contains("no `ref`")
        );

        // git without dest.
        let no_dest = SdkIndex::parse("[source.x]\nversion=\"1\"\ngit=\"u\"\nref=\"r\"\n").unwrap();
        assert!(
            no_dest
                .validate()
                .unwrap_err()
                .to_string()
                .contains("no `dest`")
        );
    }

    /// phase-327 W1 (RFC-0062) — the new classes round-trip, cross-refs are
    /// validated, and probe shapes must be unambiguous.
    #[test]
    fn system_rust_python_classes_parse_and_validate() {
        let idx = SdkIndex::parse(
            r#"
[system.libslirp]
why = "runtime dep of the qemu dist"
apt = ["libslirp0"]
dnf = ["libslirp"]
check = { sharedlib = "libslirp.so.0" }

[system.gnu-parallel]
apt = ["parallel"]
brew = ["parallel"]
check = { cmd = "parallel" }

[tool.qemu]
version = "11.0-nros2"
system = ["libslirp"]

[rust.toolchain.nightly-pinned]
channel = "nightly-2026-04-11"
components = ["rustfmt", "clippy"]

[rust.target.riscv32imc]
triple = "riscv32imc-unknown-none-elf"
toolchain = "nightly-pinned"

[rust.cargo-tool.nextest]
crate = "cargo-nextest"
check = { cmd = "cargo-nextest" }

[python.west]
pip = "west"
check = { cmd = "west" }
"#,
        )
        .expect("new classes parse");
        assert!(idx.validate().is_ok());
        assert_eq!(idx.system["libslirp"].packages_for("apt"), ["libslirp0"]);
        assert_eq!(idx.system["libslirp"].packages_for("dnf"), ["libslirp"]);
        assert!(idx.system["libslirp"].packages_for("pacman").is_empty());
        assert_eq!(idx.tool["qemu"].system, ["libslirp"]);
        assert_eq!(idx.rust.cargo_tool["nextest"].crate_name, "cargo-nextest");
        assert!(
            idx.rust.cargo_tool["nextest"].locked,
            "locked defaults true"
        );
        assert_eq!(idx.python["west"].pip, "west");

        // A tool naming an undefined system key is a validation error.
        let dangling = SdkIndex::parse("[tool.qemu]\nversion=\"1\"\nsystem=[\"nope\"]\n").unwrap();
        let err = dangling.validate().unwrap_err().to_string();
        assert!(err.contains("no [system.nope] entry"), "{err}");

        // A system entry mapping no manager at all is rejected.
        let unmapped = SdkIndex::parse("[system.x]\nwhy=\"w\"\n").unwrap();
        assert!(
            unmapped
                .validate()
                .unwrap_err()
                .to_string()
                .contains("no package manager"),
        );

        // issue 0487 — a MULTI-FIELD `[system.*]` probe is ACCEPTED, and this
        // assertion inverted with it. The old rule was "exactly one"; 0487 made
        // it "at least one" because probes are OR-ed (see `run_probe`), which is
        // what let libgcrypt read as missing on Arch when only one of its
        // spellings matched. libgcrypt genuinely needs both spellings (Arch
        // ships the `.pc` and no `libgcrypt-config`; Ubuntu 22.04 ships the
        // script and no `.pc`), so either probe ALONE is a false negative on one
        // of the two distros. Declaring both is now the answer, not an error.
        let multi_probe = SdkIndex::parse(
            "[system.x]\napt=[\"x\"]\ncheck = { cmd = \"x\", sharedlib = \"libx.so\" }\n",
        )
        .unwrap();
        assert!(
            multi_probe.validate().is_ok(),
            "issue 0487: [system.*].check ORs its probes, so two fields is valid",
        );

        // A `[system.*]` probe declaring NO field at all is still rejected —
        // the rule relaxed to at-least-one, not to zero.
        let no_probe = SdkIndex::parse("[system.x]\napt=[\"x\"]\ncheck = { }\n").unwrap();
        assert!(
            no_probe
                .validate()
                .unwrap_err()
                .to_string()
                .contains("at least one"),
        );

        // ...but the OTHER kinds still take exactly one, and 0487 did not touch
        // them. Kept as a rejection case so relaxing `[system.*]` cannot quietly
        // relax these too — the coverage this test would otherwise have lost.
        let ambiguous_tool = SdkIndex::parse(
            "[rust.cargo-tool.t]\ncrate = \"cargo-x\"\ncheck = { cmd = \"x\", sharedlib = \"libx.so\" }\n",
        )
        .unwrap();
        assert!(
            ambiguous_tool
                .validate()
                .unwrap_err()
                .to_string()
                .contains("exactly one"),
        );

        // A target referencing an undefined toolchain alias is rejected.
        let bad_tc = SdkIndex::parse(
            "[rust.target.t]\ntriple=\"thumbv7m-none-eabi\"\ntoolchain=\"ghost\"\n",
        )
        .unwrap();
        assert!(
            bad_tc
                .validate()
                .unwrap_err()
                .to_string()
                .contains("undefined toolchain alias 'ghost'"),
        );
    }

    #[test]
    fn host_key_is_os_dash_arch() {
        let k = host_key();
        assert!(k.contains('-'), "host key looks like <os>-<arch>: {k}");
        assert!(!k.contains("aarch64"), "arch normalized to arm64: {k}");
    }
}

#[cfg(test)]
mod prereq_tests {
    use super::*;

    fn idx(body: &str) -> SdkIndex {
        toml::from_str(body).expect("parses")
    }

    /// W1 is additive: every existing `[system.*]` entry must reach consumers
    /// through `prereqs()` unchanged, with no field dropped. This is the wave's
    /// acceptance criterion, asserted rather than eyeballed.
    #[test]
    fn a_system_entry_survives_the_alias_intact() {
        let i = idx(r#"
            [system.libslirp]
            why = "runtime dep of the qemu dist"
            apt = ["libslirp0"]
            dnf = ["libslirp"]
            pacman = ["libslirp"]
            brew = ["libslirp"]
            check = { sharedlib = "libslirp.so.0" }
        "#);
        let p = i.prereqs();
        let d = p.get("libslirp").expect("aliased into the prereq table");
        assert_eq!(d.provider, Provider::System, "the default and the alias");
        assert_eq!(d.provider_chain(), vec![Provider::System]);
        assert_eq!(d.why.as_deref(), Some("runtime dep of the qemu dist"));
        assert_eq!(d.packages_for("apt"), ["libslirp0"]);
        assert_eq!(d.packages_for("dnf"), ["libslirp"]);
        assert_eq!(d.packages_for("pacman"), ["libslirp"]);
        assert_eq!(d.packages_for("brew"), ["libslirp"]);
        assert_eq!(
            d.check.as_ref().and_then(|c| c.sharedlib.as_deref()),
            Some("libslirp.so.0"),
            "the probe is the half that makes a miss diagnosable"
        );
        assert!(d.packages_for("zypper").is_empty(), "unmapped manager");
    }

    /// A key in both tables is a migration in progress, and the NEW entry is
    /// the intended one. W4 adds the gate that makes the overlap illegal once
    /// it should no longer exist.
    #[test]
    fn a_prereq_entry_wins_over_the_system_alias() {
        let i = idx(r#"
            [system.genromfs]
            why = "old"
            apt = ["genromfs"]

            [prereq.genromfs]
            why = "new"
            providers = ["system", "source"]
            apt = ["genromfs"]
            source = "genromfs"
        "#);
        let p = i.prereqs();
        assert_eq!(p.len(), 1, "one key, not two");
        let d = &p["genromfs"];
        assert_eq!(d.why.as_deref(), Some("new"));
        assert_eq!(
            d.provider_chain(),
            vec![Provider::System, Provider::Source],
            "ordered preference: prefer the OS package, build if absent"
        );
        assert_eq!(d.source.as_deref(), Some("genromfs"));
    }

    /// The common entry writes neither `provider` nor `providers`.
    #[test]
    fn the_default_provider_is_system_and_the_chain_is_never_empty() {
        let i = idx("[prereq.parallel]\napt = [\"parallel\"]\n");
        let d = &i.prereqs()["parallel"];
        assert_eq!(d.provider, Provider::System);
        assert_eq!(d.provider_chain(), vec![Provider::System]);
    }

    /// The non-system providers parse. They install nothing yet — W2 gives
    /// them probes — but the schema must already carry them, or `[prereq.*]`
    /// is just `[system.*]` renamed.
    #[test]
    fn every_provider_parses() {
        let i = idx(r#"
            [prereq.qemu]
            provider = "sdk"
            [prereq.freertos-kernel]
            provider = "submodule"
            [prereq.sccache]
            provider = "source"
        "#);
        let p = i.prereqs();
        assert_eq!(p["qemu"].provider, Provider::Sdk);
        assert_eq!(p["freertos-kernel"].provider, Provider::Submodule);
        assert_eq!(p["sccache"].provider, Provider::Source);
    }

    /// `deny_unknown_fields` is load-bearing: a mistyped key must be an error,
    /// not a silently ignored declaration — the same reasoning `[image.*]`
    /// gives, and the failure mode this whole RFC exists to delete.
    #[test]
    fn a_mistyped_field_is_an_error() {
        let e = toml::from_str::<SdkIndex>("[prereq.x]\naptt = [\"y\"]\n");
        assert!(e.is_err(), "a typo must not parse as an empty entry");
    }
}
