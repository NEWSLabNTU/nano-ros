//! Phase 219.A — Entry-pkg codegen shared module.
//!
//! Lifts the pkg-index walk + launch.xml parse + register-call
//! resolution out of the Rust `nros::main!()` proc-macro (which lives
//! in `packages/core/nros-macros/src/main_macro.rs`) into one place
//! every front-end can call. The three emitters
//! ([`emit_rust`], [`emit_cpp`], [`emit_c`]) consume a single in-memory
//! [`Plan`] IR so the per-language differences stay surface-level.
//!
//! Surface (per phase doc §3.2):
//!
//! ```ignore
//! use nros_cli_core::codegen::entry::{Lang, Plan, plan_from_launch};
//!
//! let plan = plan_from_launch(PlanInput {
//!     workspace: ws.as_path(),
//!     launch_spec: "demo_bringup:system.launch.xml",
//!     board: Some("native".into()),
//!     arg_overrides: vec![],
//! })?;
//! let src = match lang {
//!     Lang::Rust => emit_rust::emit(&plan),               // register-based
//!     Lang::Cpp  => emit_cpp::emit_typed(&plan)?,         // typed (RFC-0043)
//!     Lang::C    => emit_c::emit_typed(&plan)?,           // typed (phase-257)
//! };
//! ```
//!
//! Errors carry enough context that the CLI verb's `eyre::Result`
//! wrapper passes them through verbatim.

use std::path::{Path, PathBuf};

use eyre::{Context, Result, bail};

use crate::{
    launch_parser::{LaunchDescription, NodeSpec, parse_launch_file},
    pkg_index::build_pkg_index,
};

pub mod emit_c;
pub mod emit_cpp;
pub mod emit_rust;
pub mod metadata;

/// Target language for the emitted TU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Cpp,
    C,
}

impl Lang {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "rust" => Ok(Lang::Rust),
            "cpp" | "c++" | "cxx" => Ok(Lang::Cpp),
            "c" => Ok(Lang::C),
            other => bail!("unknown --lang `{other}` (expected one of: rust, cpp, c)"),
        }
    }
}

/// Caller-supplied inputs to [`plan_from_launch`].
#[derive(Debug)]
pub struct PlanInput<'a> {
    /// Workspace root — the directory holding `src/<pkg>/package.xml`
    /// trees. Typically the dir containing the workspace-root
    /// `CMakeLists.txt` or `Cargo.toml`.
    pub workspace: &'a Path,
    /// `"<bringup_pkg>"` or `"<bringup_pkg>:<file>.launch.xml"`.
    pub launch_spec: &'a str,
    /// Board key (`"native"`, `"freertos"`, …) or an explicit C++
    /// path-like (`"nros::board::NativeBoard"`). For the Rust emitter
    /// this is treated as the `[package.metadata.nros.entry] deploy =`
    /// value; the emitter dispatches to the matching board ZST.
    /// `None` falls back to `"native"` — the only Entry-pkg target
    /// currently supported by the C/C++ surface (Phase 212.L.2).
    pub board: Option<String>,
    /// Caller-supplied launch-arg overrides (forwarded to the parser).
    pub arg_overrides: Vec<(String, String)>,
}

/// Resolved plan handed to one of the three emitters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Board key from [`PlanInput::board`]; `"native"` by default.
    pub board: String,
    /// Per-node entries in launch-file order (top-scope first, then
    /// each `<group>`'s children). Duplicates per `(pkg, exec)` are
    /// preserved — multiple instances of the same Node pkg may occur
    /// in a single launch.
    pub nodes: Vec<PlanNode>,
    /// Absolute paths of every file the plan read. Caller emits these
    /// as `cargo:rerun-if-changed=` / `--depfile` entries for build-
    /// system rebuild correctness.
    pub depfile_paths: Vec<PathBuf>,
    /// Bringup-pkg name from the launch spec (`"demo_bringup"` for
    /// `"demo_bringup:system.launch.xml"`). Surfaced so emitters can
    /// thread it into the generated banner.
    pub bringup: String,
    /// Resolved launch-file path; surfaces in the generated header
    /// banner.
    pub launch_file: PathBuf,
}

impl Plan {
    /// Phase 211.F — partition a multi-host launch for a single target host.
    ///
    /// Returns a copy of the plan keeping only the nodes that belong on host
    /// `host`: those whose `<node machine="…">` equals `host`, plus every
    /// **unhosted** node (`host == None`) — an unhosted node is shared / runs
    /// everywhere (matches ROS 2, where a node without `machine=` runs on the
    /// local host). The multi-host deploy bakes one entry per host from these
    /// partitions, each deployed to that host's `[deploy.<id>]` target. A
    /// single-host launch (no `machine=` anywhere) is unaffected — every node
    /// is unhosted, so `for_host` returns all of them for any host.
    #[must_use]
    pub fn for_host(&self, host: &str) -> Plan {
        Plan {
            board: self.board.clone(),
            nodes: self
                .nodes
                .iter()
                .filter(|n| match &n.host {
                    Some(h) => h == host,
                    None => true,
                })
                .cloned()
                .collect(),
            depfile_paths: self.depfile_paths.clone(),
            bringup: self.bringup.clone(),
            launch_file: self.launch_file.clone(),
        }
    }

    /// Phase 211.F — the distinct target hosts named across the plan's nodes
    /// (the `machine=` set), sorted + deduped. Empty for a single-host launch.
    /// The multi-host deploy bakes one entry per entry in this set.
    #[must_use]
    pub fn hosts(&self) -> Vec<String> {
        let mut hs: Vec<String> = self.nodes.iter().filter_map(|n| n.host.clone()).collect();
        hs.sort();
        hs.dedup();
        hs
    }
}

/// Phase 211.H (issue #52) — one per-topic QoS override on a node, decomposed
/// from a `qos_overrides.<topic>.<role>.<policy>` launch param. The typed C++
/// entry emitter (`emit_cpp`) bakes these into a static `nros_cpp_qos_override_t[]`
/// + a `node.set_qos_overrides(...)` call before `configure(node)`, so the
/// node's entities honour the override at create time. Fields are the raw
/// decomposed strings; the emitter maps them to the C-ABI `{role, policy, value}`
/// scalar codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QosOverrideSpec {
    /// Resolved topic (e.g. `"/chatter"`).
    pub topic: String,
    /// `"publisher"` or `"subscription"`.
    pub role: String,
    /// `"reliability"` / `"durability"` / `"history"` / `"depth"`.
    pub policy: String,
    /// Raw launch value (e.g. `"best_effort"`, `"transient_local"`, `"5"`).
    pub value: String,
}

/// Decompose a node's launch params into [`QosOverrideSpec`]s. Mirrors the
/// planner's `schema_qos_overrides`: a param named
/// `qos_overrides.<topic>.<role>.<policy>` splits via `rsplitn(3, '.')` (so a
/// `/`-bearing topic survives), and the result is sorted `(topic, role, policy)`
/// for byte-stable codegen. Non-matching params are ignored.
fn qos_overrides_from_params(params: &[crate::launch_parser::ParamSpec]) -> Vec<QosOverrideSpec> {
    const QOS_OVERRIDE_PREFIX: &str = "qos_overrides.";
    let mut out: Vec<QosOverrideSpec> = params
        .iter()
        .filter_map(|p| {
            let rest = p.name.strip_prefix(QOS_OVERRIDE_PREFIX)?;
            // rsplitn(3, '.') → [policy, role, topic]
            let mut parts = rest.rsplitn(3, '.');
            let policy = parts.next()?;
            let role = parts.next()?;
            let topic = parts.next()?;
            if topic.is_empty() || role.is_empty() || policy.is_empty() {
                return None;
            }
            Some(QosOverrideSpec {
                topic: topic.to_string(),
                role: role.to_string(),
                policy: policy.to_string(),
                value: p.value.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.topic, &a.role, &a.policy).cmp(&(&b.topic, &b.role, &b.policy)));
    out
}

/// One Node-pkg invocation in launch order.
///
/// `pkg` is the cargo-style pkg name (sanitised via [`sanitize_pkg`]
/// for symbol-name use). `exec` and `name` come straight from the
/// launch XML; today only `pkg` drives codegen (the per-pkg mangled
/// register symbol is keyed on it), but the `exec` / `name` fields
/// stay on the IR so future `<param>` / `<remap>` routing has them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanNode {
    pub pkg: String,
    pub exec: String,
    pub name: Option<String>,
    pub namespace: Option<String>,
    /// Phase 240.2 (RFC-0043) — fully-qualified C++ component class
    /// (`"talker_pkg::Talker"`), from `nano_ros_node_register(CLASS …)` via the
    /// cmake metadata. Required by the **typed** entry emitter (`emit_cpp_typed`)
    /// which constructs the class; `None` for the legacy register-symbol path.
    pub class_name: Option<String>,
    /// Phase 240.2 — the component class header to `#include`
    /// (`"talker_pkg/Talker.hpp"`). Paired with `class_name`.
    pub class_header: Option<String>,
    /// Phase 240.4 (RFC-0043) — component implementation language from the
    /// cmake metadata (`"c"` / `"cpp"` / `"rust"`). `None` for the launch-only
    /// legacy path. The **typed** entry emitter branches on it: a `"c"` node is
    /// constructed via its C-ABI factory + `configure(node_handle, self)` seam
    /// (`NROS_C_COMPONENT`), a `"cpp"` node via its C++ class + `configure(node)`.
    pub lang: Option<String>,
    /// Phase 242.4 (RFC-0044) — component *shape* from the cmake metadata:
    /// `"rclcpp"` (IS-A-node, ctor-wired — construct-with-handle) or `"configure"`
    /// (RFC-0043 default-construct + `configure(Node&)`). `None` ⇒ `"configure"`
    /// (back-compat). The **typed** entry emitter branches construct on it: an
    /// `"rclcpp"` C++ node is placement-new'd with the executor handle *after*
    /// `nros::init` (the ctor owns the node); a `"configure"` node keeps the
    /// 240.x static-construct-then-`configure(node)` path.
    pub shape: Option<String>,
    /// Phase 211.F — `<node machine="…">` target host (multi-host launch).
    /// `None` for single-host / unhosted nodes. [`Plan::for_host`] partitions a
    /// multi-host launch into per-host bakes: an entry for host `H` keeps nodes
    /// whose `host == Some(H)` plus all unhosted (`None`) nodes.
    pub host: Option<String>,
    /// Phase 211.H (issue #52) — per-topic QoS overrides decomposed from this
    /// node's `qos_overrides.<topic>.<role>.<policy>` launch params. Empty when
    /// none. The typed C++ entry emitter bakes them into a
    /// `node.set_qos_overrides(...)` call before `configure(node)`.
    pub qos_overrides: Vec<QosOverrideSpec>,
}

impl PlanNode {
    // Phase 258 (Track 2, follow-up) — `register_symbol()` (the dead
    // `__nros_component_<pkg>_register` mangled-symbol string) is gone. The
    // post-257 entries link `__nros_component_<pkg>_install`, not `_register`;
    // nothing consumed this string.

    /// Cmake target name for the static lib the Node pkg's
    /// `nano_ros_node_register()` produces:
    /// `<pkg>_<exec>_component`. The Entry pkg's auto-link
    /// (`nano_ros_entry(... LAUNCH …)`) consumes this string.
    pub fn cmake_link_target(&self) -> String {
        format!("{}_{}_component", sanitize_pkg(&self.pkg), self.exec)
    }
}

/// Sanitise a pkg name into a valid identifier (`-` → `_`).
///
/// Mirrors the rule the Rust `nros::node!()` macro and the cmake fn
/// `nano_ros_node_register()` already apply (see
/// `packages/core/nros-macros/src/main_macro.rs::pkg_to_crate_ident`).
pub fn sanitize_pkg(pkg: &str) -> String {
    let mut out = String::with_capacity(pkg.len());
    for c in pkg.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Walk the workspace pkg-index, locate the bringup pkg, parse the
/// launch XML, and lower the result into a [`Plan`].
///
/// Errors carry enough context that the CLI verb's `eyre::Result`
/// wrapper passes them through verbatim.
pub fn plan_from_launch(input: PlanInput<'_>) -> Result<Plan> {
    let pkg_index = build_pkg_index(input.workspace)
        .with_context(|| format!("build pkg-index from `{}`", input.workspace.display()))?;

    let (bringup_name, file_override) = match input.launch_spec.split_once(':') {
        Some((b, f)) => (b.trim().to_string(), Some(f.trim().to_string())),
        None => (input.launch_spec.trim().to_string(), None),
    };
    if bringup_name.is_empty() {
        bail!(
            "empty bringup pkg name in launch spec `{}`",
            input.launch_spec
        );
    }

    let bringup_dir = pkg_index
        .resolve_pkg(&bringup_name)
        .with_context(|| format!("resolve bringup pkg `{bringup_name}`"))?
        .to_path_buf();

    // Pull every package.xml the index walked into the depfile list.
    let mut depfile_paths: Vec<PathBuf> = Vec::new();
    for (_, pkg_dir) in pkg_index.pkgs() {
        depfile_paths.push(pkg_dir.join("package.xml"));
    }

    // Resolve the launch filename. With no `:file` override, consult
    // bringup's `system.toml [system] default_launch`; fall back to
    // `system.launch.xml` (matches the Rust proc-macro shape).
    let launch_filename = match file_override {
        Some(s) => s,
        None => {
            let system_toml = bringup_dir.join("system.toml");
            if system_toml.exists() {
                depfile_paths.push(system_toml.clone());
                read_default_launch(&system_toml)
                    .with_context(|| format!("parse `{}`", system_toml.display()))?
                    .unwrap_or_else(|| "system.launch.xml".to_string())
            } else {
                "system.launch.xml".to_string()
            }
        }
    };
    let launch_path = bringup_dir.join("launch").join(&launch_filename);
    if !launch_path.exists() {
        bail!("launch file not found: `{}`", launch_path.display());
    }
    depfile_paths.push(launch_path.clone());

    let desc: LaunchDescription = parse_launch_file(&launch_path, &pkg_index, &input.arg_overrides)
        .with_context(|| format!("parse launch file `{}`", launch_path.display()))?;

    // Walk every `<node>` (top-level + inside groups) and lower to a
    // PlanNode. Order matches the source XML (top-scope first, then
    // each group's children).
    let mut nodes: Vec<PlanNode> = Vec::new();
    let push_node = |n: &NodeSpec, out: &mut Vec<PlanNode>| {
        out.push(PlanNode {
            pkg: n.pkg.clone(),
            exec: n.exec.clone(),
            name: n.name.clone(),
            namespace: n.namespace.clone(),
            // Phase 240.2 — the launch path doesn't carry the C++ class/header
            // yet (threaded from cmake metadata in 240.2b); the legacy
            // register-symbol emitters ignore these.
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: n.machine.clone(),
            qos_overrides: qos_overrides_from_params(&n.params),
        });
    };
    for n in &desc.nodes {
        push_node(n, &mut nodes);
    }
    for g in &desc.groups {
        for n in &g.nodes {
            push_node(n, &mut nodes);
        }
    }

    if nodes.is_empty() {
        bail!(
            "launch file `{}` has no `<node>` entries — nothing to register",
            launch_path.display()
        );
    }

    // Sort + dedup the depfile entries — pkg-index revisits + sibling
    // `<include>`s can list a single file twice.
    depfile_paths.sort();
    depfile_paths.dedup();

    let board = input.board.unwrap_or_else(|| "native".to_string());

    Ok(Plan {
        board,
        nodes,
        depfile_paths,
        bringup: bringup_name,
        launch_file: launch_path,
    })
}

/// `[system] default_launch = "<file>"` reader, mirrors the proc-macro
/// helper in `packages/core/nros-macros/src/main_macro.rs`.
fn read_default_launch(system_toml: &Path) -> Result<Option<String>> {
    let raw = std::fs::read_to_string(system_toml)
        .with_context(|| format!("read `{}`", system_toml.display()))?;
    let v: toml::Value = toml::from_str(&raw).context("parse toml")?;
    Ok(v.get("system")
        .and_then(|s| s.get("default_launch"))
        .and_then(|d| d.as_str())
        .map(str::to_string))
}

/// Write the depfile in GNU-make `target: dep1 dep2 …` form so cmake's
/// `CMAKE_CONFIGURE_DEPENDS` / make-style consumers ingest it directly.
///
/// The "target" line is the generated TU path (or whatever the caller
/// supplies); each dep is one absolute path on its own continuation
/// line so newline-quoting + path escaping stays trivial.
pub fn write_depfile(target: &Path, deps: &[PathBuf], depfile: &Path) -> Result<()> {
    let mut out = String::new();
    out.push_str(&escape_make(&target.display().to_string()));
    out.push(':');
    for dep in deps {
        out.push_str(" \\\n    ");
        out.push_str(&escape_make(&dep.display().to_string()));
    }
    out.push('\n');
    if let Some(parent) = depfile.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create depfile parent `{}`", parent.display()))?;
    }
    std::fs::write(depfile, out)
        .with_context(|| format!("write depfile `{}`", depfile.display()))?;
    Ok(())
}

/// Make-format target/dep escape: spaces → `\ `, `#` → `\#`.
fn escape_make(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("\\ "),
            '#' => out.push_str("\\#"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_pkg_dash_to_underscore() {
        assert_eq!(sanitize_pkg("talker-pkg"), "talker_pkg");
        assert_eq!(sanitize_pkg("a.b/c"), "a_b_c");
        assert_eq!(sanitize_pkg("plain_pkg"), "plain_pkg");
    }

    #[test]
    fn cmake_link_target_uses_mangled_pkg() {
        let n = PlanNode {
            pkg: "talker-pkg".into(),
            exec: "talker".into(),
            name: None,
            namespace: None,
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: None,
            qos_overrides: Vec::new(),
        };
        assert_eq!(n.cmake_link_target(), "talker_pkg_talker_component");
    }

    /// Phase 211.F — `Plan::for_host` keeps a host's own nodes + all unhosted
    /// (shared) nodes; `hosts()` returns the distinct `machine=` set.
    #[test]
    fn plan_for_host_partitions_by_machine() {
        let node = |pkg: &str, host: Option<&str>| PlanNode {
            pkg: pkg.into(),
            exec: pkg.into(),
            name: None,
            namespace: None,
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: host.map(str::to_string),
            qos_overrides: Vec::new(),
        };
        let plan = Plan {
            board: "native".into(),
            nodes: vec![
                node("sim", Some("workstation")),
                node("ctrl", Some("jetson")),
                node("shared", None),
            ],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: std::path::PathBuf::from("/tmp/x.launch.xml"),
        };

        assert_eq!(
            plan.hosts(),
            vec!["jetson".to_string(), "workstation".to_string()]
        );

        // workstation entry: its own node + the shared (unhosted) one.
        let ws = plan.for_host("workstation");
        let ws_pkgs: Vec<&str> = ws.nodes.iter().map(|n| n.pkg.as_str()).collect();
        assert_eq!(ws_pkgs, vec!["sim", "shared"]);

        // jetson entry: its own node + shared.
        let jetson = plan.for_host("jetson");
        let jetson_pkgs: Vec<&str> = jetson.nodes.iter().map(|n| n.pkg.as_str()).collect();
        assert_eq!(jetson_pkgs, vec!["ctrl", "shared"]);

        // An unknown host still gets the shared nodes (never empty for a launch
        // with shared nodes).
        assert_eq!(plan.for_host("nope").nodes.len(), 1);
    }

    /// Phase 211.H (issue #52) — `qos_overrides.<topic>.<role>.<policy>` params
    /// decompose into sorted `QosOverrideSpec`s; non-matching params are ignored.
    #[test]
    fn qos_overrides_decompose_from_params() {
        use crate::launch_parser::ParamSpec;
        let params = vec![
            ParamSpec {
                name: "qos_overrides./chatter.publisher.reliability".into(),
                value: "best_effort".into(),
            },
            ParamSpec {
                name: "use_sim_time".into(),
                value: "true".into(),
            },
            ParamSpec {
                name: "qos_overrides./chatter.subscription.durability".into(),
                value: "transient_local".into(),
            },
        ];
        let got = qos_overrides_from_params(&params);
        assert_eq!(got.len(), 2);
        // sorted (topic, role, policy): publisher before subscription.
        assert_eq!(got[0].role, "publisher");
        assert_eq!(got[0].policy, "reliability");
        assert_eq!(got[0].topic, "/chatter");
        assert_eq!(got[0].value, "best_effort");
        assert_eq!(got[1].role, "subscription");
        assert_eq!(got[1].policy, "durability");
    }

    #[test]
    fn lang_parse() {
        assert_eq!(Lang::parse("rust").unwrap(), Lang::Rust);
        assert_eq!(Lang::parse("cpp").unwrap(), Lang::Cpp);
        assert_eq!(Lang::parse("c++").unwrap(), Lang::Cpp);
        assert_eq!(Lang::parse("c").unwrap(), Lang::C);
        assert!(Lang::parse("python").is_err());
    }

    #[test]
    fn write_depfile_emits_make_format() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("gen.cpp");
        let dep_a = tmp.path().join("a.xml");
        let dep_b = tmp.path().join("dir with space/b.xml");
        let depfile = tmp.path().join("gen.d");
        write_depfile(&target, &[dep_a.clone(), dep_b.clone()], &depfile).unwrap();
        let body = std::fs::read_to_string(&depfile).unwrap();
        assert!(body.starts_with(&format!("{}:", target.display())));
        assert!(body.contains(&format!("{}", dep_a.display())));
        // Space in dep_b should be escaped.
        assert!(body.contains("dir\\ with\\ space"));
    }
}
