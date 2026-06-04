//! Phase 172 WP-A — root `nros.toml` workspace config (the deployment SSOT).
//!
//! One config file at the workspace root holds everything for the revised
//! deployment model: `[workspace]` defaults, the `[system]` /
//! `[systems.<name>]` composition (launch + components + the RMW/domain SSOT +
//! overlays + scheduling + in-binary `[[domain]]`/`[[bridge]]`), and the
//! `[deploy.<name>]` targets the `nros deploy` command-runner drives. This
//! *replaces* the per-package "system `nros.toml` with `target.{triple,board}`"
//! — triple/board move into `[deploy.<name>]`. Component `nros.toml` stays a
//! different scope (reusable intrinsics) and must not carry `rmw`/`domain`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};
use serde::{Deserialize, Serialize};

use super::{
    config::SchedContextConfig,
    schema::{ParameterTable, RemapRule},
};

/// The whole root `nros.toml`. Marked by its `[workspace]` table, distinct
/// from a per-package `nros.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub workspace: WorkspaceSection,
    /// Single-system shorthand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemSection>,
    /// Multi-system: `[systems.<name>]`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub systems: BTreeMap<String, SystemSection>,
    /// Deployment targets: `[deploy.<name>]`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deploy: BTreeMap<String, DeployTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSection {
    /// Deploy target used by a bare `nros build` / `nros deploy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// `[system]` / `[systems.<name>]` — topology + composition + the RMW/domain
/// SSOT defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemSection {
    /// Launch file (topology), relative to the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,
    /// Component package names composing the system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    /// SSOT default RMW.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,
    /// SSOT default ROS domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<u32>,
    /// Per-instance overlays, keyed by instance name (`[overlays.<inst>]`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overlays: BTreeMap<String, OverlaySection>,
    /// Scheduling tiers (`[[scheduling.contexts]]`).
    #[serde(default, skip_serializing_if = "SchedulingSection::is_empty")]
    pub scheduling: SchedulingSection,
    /// Non-default domain groups (in-binary multi-domain).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domain: Vec<DomainGroup>,
    /// In-workspace bridges (build-time, baked into `open_multi`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridge: Vec<BridgeSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OverlaySection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "ParameterTable::is_empty")]
    pub parameters: ParameterTable,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaps: Vec<RemapRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SchedulingSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<SchedContextConfig>,
}

impl SchedulingSection {
    fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainGroup {
    pub id: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeSpec {
    pub name: String,
    /// The sessions the bridge connects; ≥2.
    #[serde(default)]
    pub connect: Vec<BridgeEndpoint>,
    /// Topics forwarded (`"*"` = all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeEndpoint {
    pub rmw: String,
    #[serde(default)]
    pub domain: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

/// `[deploy.<name>]` — one build-ownership target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeployTarget {
    #[serde(default)]
    pub kind: DeployKind,
    /// Cargo target triple (compiled-form self / vendor-lib).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// Per-target RMW override of `[system].rmw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,
    /// Zenoh/DDS locator baked into the generated transport (where the agent /
    /// peer is). Embedded deploys must bake it — there's no runtime env on the
    /// target; hosted deploys usually read `ZENOH_LOCATOR` from the env instead.
    /// Emitted as a synthetic `[[transport]]` in the build overlay (W.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// Which `[systems.<name>]` this deploys (multi-system workspaces).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Ejected code dir (`{self}`), relative to the workspace root.
    #[serde(default, rename = "self", skip_serializing_if = "Option::is_none")]
    pub self_dir: Option<String>,
    /// Shippable form; defaults per kind when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit: Option<EmitForm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<VendorSpec>,
    /// Vendor build command-runner steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<String>,
    /// Post-build packaging steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package: Vec<String>,
    /// RtosOwned config-fragment hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<DeployConfigHook>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DeployKind {
    /// nano-ros owns the build; the whole binary.
    #[default]
    #[serde(rename = "self")]
    Self_,
    /// nano-ros owns the build, linking a vendor static lib.
    VendorLib,
    /// The vendor owns the build; nano-ros is a guest module.
    VendorModule,
}

impl DeployKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeployKind::Self_ => "self",
            DeployKind::VendorLib => "vendor-lib",
            DeployKind::VendorModule => "vendor-module",
        }
    }

    /// Default shippable form: vendor-module compiles in the vendor toolchain
    /// (source form); the others link a nano-ros-compiled archive.
    pub fn default_emit(self) -> EmitForm {
        match self {
            DeployKind::VendorModule => EmitForm::Source,
            DeployKind::Self_ | DeployKind::VendorLib => EmitForm::Compiled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmitForm {
    /// `lib<sys>.a` + cbindgen header (nano-ros owns the toolchain).
    Compiled,
    /// Generated crate + vendor-includable CMake fragment (vendor compiles).
    Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorSpec {
    pub dir: VendorDir,
    /// Expected vendor version; asserted before a build (drift guard).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

/// `vendor.dir` — a plain path, or an env-var reference with a fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VendorDir {
    Path(String),
    Resolved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
}

impl VendorDir {
    /// Resolve to a concrete directory: a plain path as-is; otherwise the env
    /// var if set, else the declared default.
    pub fn resolve(&self) -> Option<PathBuf> {
        match self {
            VendorDir::Path(p) => Some(PathBuf::from(p)),
            VendorDir::Resolved { env, default } => env
                .as_deref()
                .and_then(|name| std::env::var_os(name).map(PathBuf::from))
                .or_else(|| default.as_deref().map(PathBuf::from)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeployConfigHook {
    /// Generated vendor config fragment (Kconfig / defconfig) under `{self}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    /// How the vendor build pulls the fragment in (e.g. `EXTRA_CONF_FILE=...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<String>,
}

impl WorkspaceConfig {
    /// Parse + validate a root `nros.toml`.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("read root nros.toml at {}", path.display()))?;
        let cfg: WorkspaceConfig = toml::from_str(&raw)
            .wrap_err_with(|| format!("parse root nros.toml at {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// The `[system]` a deploy target builds: its explicit `system =`, the
    /// single `[system]`, or the sole `[systems.<name>]` entry.
    pub fn system_for(&self, deploy: &DeployTarget) -> Option<&SystemSection> {
        if let Some(name) = &deploy.system {
            return self.systems.get(name);
        }
        if let Some(s) = &self.system {
            return Some(s);
        }
        if self.systems.len() == 1 {
            return self.systems.values().next();
        }
        None
    }

    /// The deploy target a bare `nros build`/`nros deploy` resolves to.
    pub fn default_deploy(&self) -> Option<(&String, &DeployTarget)> {
        let name = self.workspace.default.as_ref()?;
        self.deploy.get_key_value(name)
    }

    /// Static well-formedness checks (the `nros check` rules for a root file).
    pub fn validate(&self) -> Result<()> {
        match (self.system.is_some(), self.systems.is_empty()) {
            (false, true) => bail!(
                "root nros.toml: declare a [system] (single) or at least one \
                 [systems.<name>] (multi)"
            ),
            (true, false) => {
                bail!("root nros.toml: use either [system] or [systems.<name>], not both")
            }
            _ => {}
        }

        if let Some(def) = &self.workspace.default
            && !self.deploy.contains_key(def)
        {
            bail!(
                "root nros.toml: [workspace].default = \"{def}\" has no matching \
                 [deploy.{def}]"
            );
        }

        let multi_system = self.system.is_none() && self.systems.len() > 1;
        for (name, d) in &self.deploy {
            if let Some(sys) = &d.system {
                if !self.systems.contains_key(sys) {
                    bail!(
                        "root nros.toml: [deploy.{name}].system = \"{sys}\" has no \
                         matching [systems.{sys}]"
                    );
                }
            } else if multi_system {
                bail!(
                    "root nros.toml: [deploy.{name}] needs `system = \"<name>\"` \
                     (the workspace declares multiple [systems.<name>])"
                );
            }

            match d.kind {
                DeployKind::Self_ => {
                    if d.vendor.is_some() {
                        bail!(
                            "root nros.toml: [deploy.{name}] kind=self must not set a \
                             [deploy.{name}.vendor]"
                        );
                    }
                }
                DeployKind::VendorLib | DeployKind::VendorModule => {
                    if d.build.is_empty() {
                        bail!(
                            "root nros.toml: [deploy.{name}] kind={} requires a \
                             `build = [...]` step",
                            d.kind.as_str()
                        );
                    }
                }
            }

            if d.kind == DeployKind::VendorModule && d.self_dir.is_none() {
                bail!(
                    "root nros.toml: [deploy.{name}] kind=vendor-module requires \
                     `self = \"deploy/<name>\"` (the module code dir)"
                );
            }
        }

        for sys in self.system.iter().chain(self.systems.values()) {
            for b in &sys.bridge {
                if b.connect.len() < 2 {
                    bail!(
                        "root nros.toml: [[bridge]] \"{}\" must connect ≥2 sessions \
                         (a bridge spans sessions; a single session is just a node)",
                        b.name
                    );
                }
            }
        }

        Ok(())
    }
}

/// What a single `nros.toml` *is*, decided Cargo-style by the sections present
/// (never by filename). A `[workspace]` table dominates — a file carrying it is
/// the workspace root regardless of also-present `[component]`/`[node]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestKind {
    /// Has `[workspace]` — the deployment SSOT / workspace root.
    Workspace,
    /// Has `[component]` only — a reusable component (member of a workspace).
    Component,
    /// Has `[node]` / `[[transport]]` only — a standalone direct-mode node.
    DirectNode,
    /// None of the discriminating sections present.
    Unknown,
}

/// Permissive probe: which discriminating top-level tables a manifest carries.
/// Ignores every other key (it does NOT validate), so it works on any of the
/// manifest kinds — including a direct-mode `[node]` file that `WorkspaceConfig`
/// would reject.
#[derive(Debug, Default, Deserialize)]
struct ManifestProbe {
    workspace: Option<toml::Value>,
    component: Option<toml::Value>,
    node: Option<toml::Value>,
    #[serde(default)]
    transport: Vec<toml::Value>,
}

/// Classify an `nros.toml` by the sections present.
pub fn probe_manifest_kind(path: &Path) -> Result<ManifestKind> {
    let raw = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("read nros.toml at {}", path.display()))?;
    let probe: ManifestProbe =
        toml::from_str(&raw).wrap_err_with(|| format!("parse nros.toml at {}", path.display()))?;
    Ok(if probe.workspace.is_some() {
        ManifestKind::Workspace
    } else if probe.component.is_some() {
        ManifestKind::Component
    } else if probe.node.is_some() || !probe.transport.is_empty() {
        ManifestKind::DirectNode
    } else {
        ManifestKind::Unknown
    })
}

/// Walk up from `start` (a directory, or a manifest file whose dir is used) to
/// the nearest enclosing **workspace root** — the closest ancestor `nros.toml`
/// carrying a `[workspace]` table. Returns its path, or `None` when there is no
/// enclosing workspace (the start is standalone — exactly Cargo's "package with
/// no workspace").
pub fn resolve_workspace_root(start: &Path) -> Result<Option<PathBuf>> {
    let mut dir: Option<&Path> = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };
    while let Some(d) = dir {
        let candidate = d.join("nros.toml");
        if candidate.is_file() && probe_manifest_kind(&candidate)? == ManifestKind::Workspace {
            return Ok(Some(candidate));
        }
        dir = d.parent();
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[workspace]
default = "native"

[system]
launch     = "launch/sys.launch.xml"
components  = ["talker_node", "listener_node"]
rmw        = "zenoh"
domain_id  = 0

[system.overlays.listener]
parameters = { window = 10 }
remaps = [{ from = "chatter", to = "filtered" }]

[[system.scheduling.contexts]]
id = "rt"
executor = "rt_exec"
class = "real_time"
priority = 80
deadline_policy = "warn"

[deploy.native]
target = "x86_64-unknown-linux-gnu"

[deploy.mcu]
kind = "vendor-module"
target = "zephyr"
board = "nucleo_h753zi"
rmw = "xrce"
self = "deploy/mcu"
build = ["west build -b {board} -d build/mcu {self}"]

[deploy.orin]
kind = "vendor-lib"
target = "armv7r-none-eabihf"
self = "deploy/orin"
vendor.dir = { env = "NV_SPE_FSP_DIR", default = "external/spe-fsp/install" }
vendor.pin = "spe-fsp 36.3"
build = ["arm-none-eabi-gcc {self}/startup.o {entry_lib} -o build/orin/spe.elf"]
package = ["sign build/orin/spe.elf -o build/orin/spe.bin"]
"#;

    fn parse(s: &str) -> WorkspaceConfig {
        toml::from_str(s).expect("parse")
    }

    #[test]
    fn good_config_round_trips_and_validates() {
        let cfg = parse(GOOD);
        cfg.validate().expect("valid");

        // Structure.
        assert_eq!(cfg.workspace.default.as_deref(), Some("native"));
        let sys = cfg.system.as_ref().expect("system");
        assert_eq!(sys.rmw.as_deref(), Some("zenoh"));
        assert_eq!(sys.domain_id, Some(0));
        assert_eq!(sys.components, ["talker_node", "listener_node"]);
        assert_eq!(sys.scheduling.contexts.len(), 1);
        assert!(sys.overlays.contains_key("listener"));

        // Deploy kinds + defaults.
        assert_eq!(cfg.deploy["native"].kind, DeployKind::Self_);
        assert_eq!(cfg.deploy["native"].kind.default_emit(), EmitForm::Compiled);
        assert_eq!(cfg.deploy["mcu"].kind, DeployKind::VendorModule);
        assert_eq!(cfg.deploy["mcu"].kind.default_emit(), EmitForm::Source);
        assert_eq!(cfg.deploy["mcu"].rmw.as_deref(), Some("xrce"));
        assert_eq!(cfg.deploy["orin"].kind, DeployKind::VendorLib);

        // vendor.dir env resolution falls back to the declared default.
        let dir = match &cfg.deploy["orin"].vendor.as_ref().unwrap().dir {
            VendorDir::Resolved { .. } => cfg.deploy["orin"].vendor.as_ref().unwrap().dir.resolve(),
            VendorDir::Path(_) => panic!("expected resolved form"),
        };
        // env unset in test → default.
        assert_eq!(dir, Some(PathBuf::from("external/spe-fsp/install")));

        // default_deploy + system_for resolve.
        let (dname, dtarget) = cfg.default_deploy().expect("default deploy");
        assert_eq!(dname, "native");
        assert!(std::ptr::eq(cfg.system_for(dtarget).unwrap(), sys));

        // Serialize → re-parse → equal (round-trip).
        let reser = toml::to_string(&cfg).expect("serialize");
        assert_eq!(parse(&reser), cfg);
    }

    fn err(s: &str) -> String {
        parse(s).validate().expect_err("should reject").to_string()
    }

    #[test]
    fn rejects_no_system() {
        assert!(err("[workspace]\n").contains("declare a [system]"));
    }

    #[test]
    fn rejects_both_system_forms() {
        let s = "[system]\nrmw=\"zenoh\"\n[systems.a]\nrmw=\"zenoh\"\n";
        assert!(err(s).contains("not both"));
    }

    #[test]
    fn rejects_missing_default_deploy() {
        let s = "[workspace]\ndefault=\"ghost\"\n[system]\nrmw=\"zenoh\"\n";
        assert!(err(s).contains("no matching [deploy.ghost]"));
    }

    #[test]
    fn rejects_self_with_vendor() {
        let s = "[system]\nrmw=\"zenoh\"\n[deploy.x]\nkind=\"self\"\nvendor.dir=\"v\"\n";
        assert!(err(s).contains("must not set"));
    }

    #[test]
    fn rejects_vendor_module_without_build_or_self() {
        let no_build =
            "[system]\nrmw=\"zenoh\"\n[deploy.x]\nkind=\"vendor-module\"\nself=\"deploy/x\"\n";
        assert!(err(no_build).contains("requires a `build"));
        let no_self =
            "[system]\nrmw=\"zenoh\"\n[deploy.x]\nkind=\"vendor-module\"\nbuild=[\"make\"]\n";
        assert!(err(no_self).contains("requires `self"));
    }

    #[test]
    fn rejects_multi_system_deploy_without_system_ref() {
        let s = "[systems.a]\nrmw=\"zenoh\"\n[systems.b]\nrmw=\"xrce\"\n[deploy.x]\n";
        assert!(err(s).contains("needs `system"));
    }

    #[test]
    fn rejects_bridge_with_one_endpoint() {
        let s = "[system]\nrmw=\"zenoh\"\n[[system.bridge]]\nname=\"gw\"\nconnect=[{rmw=\"zenoh\",domain=0}]\n";
        assert!(err(s).contains("≥2 sessions"));
    }

    // --- W.1 slice 3: manifest-kind probe + walk-up resolution ---

    use std::sync::atomic::{AtomicU32, Ordering};

    /// RAII scratch dir under the system temp dir (no `tempfile` dep).
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "nros-rc-test-{}-{}-{}",
                tag,
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn write(&self, rel: &str, body: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, body).unwrap();
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn probe_discriminates_by_section_workspace_dominates() {
        let s = Scratch::new("probe");
        let ws = s.write(
            "ws/nros.toml",
            "[workspace]\ndefault=\"x\"\n[system]\nrmw=\"zenoh\"\n",
        );
        let comp = s.write("comp/nros.toml", "[component]\nversion=1\npackage=\"p\"\ncomponent=\"c\"\nlanguage=\"rust\"\n[component.linkage]\n[component.metadata]\nsource_metadata=\"m.json\"\n");
        let node = s.write(
            "node/nros.toml",
            "[node]\nname=\"n\"\n[[transport]]\nid=\"t\"\nkind=\"udp\"\n",
        );
        // [workspace] + [component] together → still a workspace root.
        let root_comp = s.write("rc/nros.toml", "[workspace]\n[component]\nversion=1\npackage=\"p\"\ncomponent=\"c\"\nlanguage=\"rust\"\n[component.linkage]\n[component.metadata]\nsource_metadata=\"m.json\"\n");
        let empty = s.write("e/nros.toml", "");
        assert_eq!(probe_manifest_kind(&ws).unwrap(), ManifestKind::Workspace);
        assert_eq!(probe_manifest_kind(&comp).unwrap(), ManifestKind::Component);
        assert_eq!(
            probe_manifest_kind(&node).unwrap(),
            ManifestKind::DirectNode
        );
        assert_eq!(
            probe_manifest_kind(&root_comp).unwrap(),
            ManifestKind::Workspace
        );
        assert_eq!(probe_manifest_kind(&empty).unwrap(), ManifestKind::Unknown);
    }

    #[test]
    fn walk_up_finds_nearest_enclosing_workspace() {
        let s = Scratch::new("walkup");
        s.write(
            "nros.toml",
            "[workspace]\ndefault=\"x\"\n[system]\nrmw=\"zenoh\"\n",
        );
        let comp = s.write(
            "src/pkg/nros.toml",
            "[component]\nversion=1\npackage=\"p\"\ncomponent=\"c\"\nlanguage=\"rust\"\n[component.linkage]\n[component.metadata]\nsource_metadata=\"m.json\"\n",
        );
        // From the component file → walks up to the workspace root nros.toml.
        let root = resolve_workspace_root(&comp).unwrap().expect("found");
        assert_eq!(root, s.0.join("nros.toml"));
        // From a deep dir with no nros.toml → same root.
        let deep = s.0.join("src/pkg/sub/dir");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(
            resolve_workspace_root(&deep).unwrap().unwrap(),
            s.0.join("nros.toml")
        );
    }

    #[test]
    fn walk_up_returns_none_for_standalone() {
        let s = Scratch::new("standalone");
        // A direct-mode node with no enclosing [workspace] anywhere above.
        let node = s.write("proj/nros.toml", "[node]\nname=\"n\"\n");
        assert!(resolve_workspace_root(&node).unwrap().is_none());
    }
}
