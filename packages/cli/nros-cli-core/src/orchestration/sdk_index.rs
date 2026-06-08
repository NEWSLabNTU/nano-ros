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
}

/// A package compiled with the user's app for their chosen target.
///
/// Phase 195.B — `[source.*]` provisioning is data-driven: `nros setup`
/// fetches the source into [`dest`](Self::dest) from index data, never a
/// hardcoded `third-party/` path. `git`/`ref` record the canonical pin (the
/// SSOT — so `.gitmodules` and the index can't drift); `submodule` is an
/// optional *mode hint*:
/// - **clone mode** (`git` + `ref` + `dest`, no `submodule`): fresh
///   `git clone`@`ref`.
/// - **submodule mode** (`submodule` + `dest`, `git`/`ref` document the pin):
///   `git submodule update --init <submodule>` — used when the canonical
///   source is a committed submodule (the contributor checkout keeps it).
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

    #[test]
    fn host_key_is_os_dash_arch() {
        let k = host_key();
        assert!(k.contains('-'), "host key looks like <os>-<arch>: {k}");
        assert!(!k.contains("aarch64"), "arch normalized to arm64: {k}");
    }
}
