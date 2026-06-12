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
//!     Lang::Rust => emit_rust::emit(&plan),
//!     Lang::Cpp  => emit_cpp::emit(&plan),
//!     Lang::C    => emit_c::emit(&plan),
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
}

impl PlanNode {
    /// Per-pkg mangled register symbol per Phase 212.M.5.a.1:
    /// `__nros_component_<sanitised_pkg>_register`.
    pub fn register_symbol(&self) -> String {
        format!("__nros_component_{}_register", sanitize_pkg(&self.pkg))
    }

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
    fn register_symbol_uses_mangled_pkg() {
        let n = PlanNode {
            pkg: "talker-pkg".into(),
            exec: "talker".into(),
            name: None,
            namespace: None,
            class_name: None,
            class_header: None,
        };
        assert_eq!(n.register_symbol(), "__nros_component_talker_pkg_register");
        assert_eq!(n.cmake_link_target(), "talker_pkg_talker_component");
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
