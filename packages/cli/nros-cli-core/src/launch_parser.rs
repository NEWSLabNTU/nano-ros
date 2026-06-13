//! Phase 212.N.11 — ROS 2 launch.xml parser (v1 tag set).
//!
//! Copy-paste compatibility with stock nav2 / Autoware / turtlebot3
//! `*.launch.xml` files. Per design-doc §11.5 the v1 tag set is:
//!
//! - `<launch>` — root.
//! - `<arg name=".." default=".." value=".."/>` — launch arg.
//! - `<node pkg=".." exec=".." name=".." namespace=".."/>` — spawn.
//! - `<param name=".." value=".."/>` — child of `<node>`.
//! - `<remap from=".." to=".."/>` — child of `<node>` or `<group>`.
//! - `<group ns=".."/>` — namespace wrapper, may carry `<node>`,
//!   `<include>`, `<remap>`.
//! - `<include file=".."/>` — recursive XML pull; optional child
//!   `<arg>` pass-through.
//!
//! Substitutions:
//!
//! - `$(find <pkg>)` — pkg-index lookup via [`PkgIndex`].
//! - `$(var <arg>)` — current launch-arg scope.
//! - `$(env <name>)` — `std::env::var(name)`.
//!
//! Nested substitutions are NOT supported in v1 (`$(find $(var pkg))`
//! errors). Python `.launch.py` is out of scope.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use eyre::{Context, Result, bail, eyre};
use quick_xml::{
    events::{BytesStart, Event},
    name::QName,
    reader::Reader,
};

use crate::pkg_index::PkgIndex;

/// Maximum `<include>` recursion depth. Cap matches design-doc §11.5
/// guidance — deep stacks of launch files are a smell, and unbounded
/// recursion would hand control to a malicious launch file.
const MAX_INCLUDE_DEPTH: usize = 16;

/// Fully-resolved launch description, post-substitution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchDescription {
    /// `<arg>` declarations seen at the top scope, in order.
    pub args: Vec<LaunchArg>,
    /// `<node>` spawns at the top scope.
    pub nodes: Vec<NodeSpec>,
    /// `<include>` references (the included file's contents are also
    /// merged into `nodes` / `groups`; this list keeps the original
    /// reference for tooling that wants the raw graph).
    pub includes: Vec<IncludeSpec>,
    /// `<group>` wrappers seen at the top scope.
    pub groups: Vec<GroupSpec>,
}

/// `<arg name=".." default=".." value=".."/>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchArg {
    pub name: String,
    pub default: Option<String>,
    pub value: Option<String>,
}

/// `<node pkg=".." exec=".." name=".." namespace=".."/>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSpec {
    pub pkg: String,
    pub exec: String,
    pub name: Option<String>,
    pub namespace: Option<String>,
    /// Phase 211.F — `<node machine="…">` target host (ROS 2 multi-host launch).
    /// `None` for single-host launches. Used to partition a multi-host launch
    /// into per-host entry bakes (`Plan::for_host`).
    pub machine: Option<String>,
    pub params: Vec<ParamSpec>,
    pub remaps: Vec<RemapSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemapSpec {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncludeSpec {
    pub file: String,
    pub args: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupSpec {
    pub namespace: Option<String>,
    pub nodes: Vec<NodeSpec>,
    pub includes: Vec<IncludeSpec>,
    pub remaps: Vec<RemapSpec>,
}

/// Parse a launch file, resolving substitutions + recursive
/// `<include>`s through `pkg_index`. `args_override` is a caller-
/// supplied list of `(arg-name, arg-value)` pairs that beats both
/// `<arg value=...>` and `<arg default=...>` for the matching name.
pub fn parse_launch_file(
    path: &Path,
    pkg_index: &PkgIndex,
    args_override: &[(String, String)],
) -> Result<LaunchDescription> {
    let mut scope = ArgScope::default();
    for (k, v) in args_override {
        scope.set_override(k.clone(), v.clone());
    }
    let mut include_stack: Vec<PathBuf> = Vec::new();
    parse_file(path, pkg_index, &mut scope, &mut include_stack)
}

/// Substitution-resolution + sub-tree iteration scope.
#[derive(Clone, Debug, Default)]
struct ArgScope {
    /// Caller-supplied overrides (highest precedence).
    overrides: BTreeMap<String, String>,
    /// Materialised `<arg>` values in the current launch-file scope.
    values: BTreeMap<String, String>,
}

impl ArgScope {
    fn set_override(&mut self, k: String, v: String) {
        self.overrides.insert(k, v);
    }
    fn lookup(&self, name: &str) -> Option<&str> {
        if let Some(v) = self.overrides.get(name) {
            return Some(v.as_str());
        }
        self.values.get(name).map(String::as_str)
    }
    fn record_arg(&mut self, arg: &LaunchArg) {
        // Precedence: caller override > `value=` > `default=`.
        if self.overrides.contains_key(&arg.name) {
            // Override already in scope; do nothing.
            return;
        }
        if let Some(v) = &arg.value {
            self.values.insert(arg.name.clone(), v.clone());
        } else if let Some(d) = &arg.default {
            self.values
                .entry(arg.name.clone())
                .or_insert_with(|| d.clone());
        }
    }
    /// Snapshot — pushed across `<include>` boundaries; the include
    /// inherits the parent's overrides but starts a fresh value map
    /// per ROS 2 launch semantics.
    fn child_for_include(&self, include_args: &[(String, String)]) -> ArgScope {
        let mut child = ArgScope::default();
        child.overrides = self.overrides.clone();
        // Included file's `<include><arg name="X" value="Y"/></include>`
        // children are treated as caller-supplied overrides on that file.
        for (k, v) in include_args {
            child.overrides.insert(k.clone(), v.clone());
        }
        child
    }
}

fn parse_file(
    path: &Path,
    pkg_index: &PkgIndex,
    scope: &mut ArgScope,
    include_stack: &mut Vec<PathBuf>,
) -> Result<LaunchDescription> {
    if include_stack.len() >= MAX_INCLUDE_DEPTH {
        bail!(
            "<include> depth exceeded {MAX_INCLUDE_DEPTH} at `{}`",
            path.display()
        );
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize launch file `{}`", path.display()))?;
    if include_stack.iter().any(|p| p == &canonical) {
        bail!(
            "<include> cycle detected at `{}` (stack: {:?})",
            canonical.display(),
            include_stack
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        );
    }
    include_stack.push(canonical.clone());

    let raw = fs::read_to_string(&canonical)
        .with_context(|| format!("read launch file `{}`", canonical.display()))?;
    let mut reader = Reader::from_str(&raw);
    reader.config_mut().trim_text(true);

    let mut desc = LaunchDescription::default();
    // Group / node parse stack. Each frame is one of:
    //   Root, Launch, Node(open_node), Group(open_group), Include(open_include).
    let mut stack: Vec<Frame> = vec![Frame::Root];

    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .with_context(|| format!("XML parse `{}`", canonical.display()))?;
        match event {
            Event::Start(e) => {
                handle_start(
                    &e, &mut stack, &mut desc, scope, pkg_index, &canonical,
                    /* self_closing = */ false,
                )?;
            }
            Event::Empty(e) => {
                handle_start(
                    &e, &mut stack, &mut desc, scope, pkg_index, &canonical,
                    /* self_closing = */ true,
                )?;
            }
            Event::End(e) => {
                handle_end(&e, &mut stack, &mut desc, scope, pkg_index, &canonical)?;
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    // Walk includes recursively while THIS file is still on the
    // include stack — pop happens after the recursion (cycle detection
    // works because the stack still contains us while we descend).
    let mut merged = LaunchDescription::default();
    merged.args = std::mem::take(&mut desc.args);
    merged.groups = std::mem::take(&mut desc.groups);
    merged.nodes = std::mem::take(&mut desc.nodes);
    for include in &desc.includes {
        // Already-substituted file path.
        let included_path = PathBuf::from(&include.file);
        let mut child_scope = scope.child_for_include(&include.args);
        let sub = parse_file(&included_path, pkg_index, &mut child_scope, include_stack)
            .with_context(|| {
                format!(
                    "while processing <include file=\"{}\"> from `{}`",
                    include.file,
                    canonical.display()
                )
            })?;
        // Merge sub's top-scope nodes / groups (not args — those are
        // include-local) into the parent. Sub's own includes are
        // already recursively merged.
        merged.nodes.extend(sub.nodes);
        merged.groups.extend(sub.groups);
    }
    merged.includes = desc.includes;
    // Pop this file from the cycle-detection stack.
    include_stack.pop();
    Ok(merged)
}

#[derive(Debug)]
enum Frame {
    Root,
    Launch,
    Node(NodeSpec),
    Group(GroupSpec),
    Include(IncludeSpec),
}

#[allow(clippy::too_many_arguments)]
fn handle_start(
    e: &BytesStart<'_>,
    stack: &mut Vec<Frame>,
    desc: &mut LaunchDescription,
    scope: &mut ArgScope,
    pkg_index: &PkgIndex,
    here: &Path,
    self_closing: bool,
) -> Result<()> {
    let local = e.local_name();
    let tag = std::str::from_utf8(local.as_ref())
        .map_err(|_| eyre!("non-UTF-8 XML tag in `{}`", here.display()))?
        .to_string();

    let attrs = collect_attrs(e, scope, pkg_index, here)?;

    match tag.as_str() {
        "launch" => {
            if self_closing {
                stack.push(Frame::Launch);
                stack.pop();
            } else {
                stack.push(Frame::Launch);
            }
        }
        "arg" => {
            let name = attrs
                .get("name")
                .cloned()
                .ok_or_else(|| eyre!("<arg> missing `name=` in `{}`", here.display()))?;
            let arg = LaunchArg {
                name,
                default: attrs.get("default").cloned(),
                value: attrs.get("value").cloned(),
            };
            scope.record_arg(&arg);
            // <arg> at top of <launch> records on the description; inside
            // an <include>, the arg attaches to the include's pass-through
            // list. Inside a <node> / <group> is rejected.
            let top = stack.last_mut();
            match top {
                Some(Frame::Include(inc)) => {
                    if let Some(v) = arg.value.as_ref().or(arg.default.as_ref()).cloned() {
                        inc.args.push((arg.name, v));
                    } else {
                        bail!(
                            "<arg> child of <include> in `{}` needs `value=` or `default=`",
                            here.display()
                        );
                    }
                }
                _ => desc.args.push(arg),
            }
        }
        "node" => {
            let pkg = attrs
                .get("pkg")
                .cloned()
                .ok_or_else(|| eyre!("<node> missing `pkg=` in `{}`", here.display()))?;
            let exec = attrs
                .get("exec")
                .cloned()
                .ok_or_else(|| eyre!("<node> missing `exec=` in `{}`", here.display()))?;
            let node = NodeSpec {
                pkg,
                exec,
                name: attrs.get("name").cloned(),
                namespace: attrs.get("namespace").cloned(),
                machine: attrs.get("machine").cloned(),
                params: Vec::new(),
                remaps: Vec::new(),
            };
            if self_closing {
                attach_node(node, stack, desc);
            } else {
                stack.push(Frame::Node(node));
            }
        }
        "param" => {
            let name = attrs
                .get("name")
                .cloned()
                .ok_or_else(|| eyre!("<param> missing `name=` in `{}`", here.display()))?;
            let value = attrs
                .get("value")
                .cloned()
                .ok_or_else(|| eyre!("<param> missing `value=` in `{}`", here.display()))?;
            match stack.last_mut() {
                Some(Frame::Node(n)) => n.params.push(ParamSpec { name, value }),
                _ => bail!("<param> must be a child of <node> in `{}`", here.display()),
            }
        }
        "remap" => {
            let from = attrs
                .get("from")
                .cloned()
                .ok_or_else(|| eyre!("<remap> missing `from=` in `{}`", here.display()))?;
            let to = attrs
                .get("to")
                .cloned()
                .ok_or_else(|| eyre!("<remap> missing `to=` in `{}`", here.display()))?;
            match stack.last_mut() {
                Some(Frame::Node(n)) => n.remaps.push(RemapSpec { from, to }),
                Some(Frame::Group(g)) => g.remaps.push(RemapSpec { from, to }),
                _ => bail!(
                    "<remap> must be a child of <node> or <group> in `{}`",
                    here.display()
                ),
            }
        }
        "group" => {
            let mut group = GroupSpec::default();
            group.namespace = attrs.get("ns").cloned();
            if self_closing {
                attach_group(group, stack, desc);
            } else {
                stack.push(Frame::Group(group));
            }
        }
        "include" => {
            let file_attr = attrs
                .get("file")
                .cloned()
                .ok_or_else(|| eyre!("<include> missing `file=` in `{}`", here.display()))?;
            // Resolve relative path against `here` (the launch file's dir).
            let resolved = if Path::new(&file_attr).is_absolute() {
                file_attr.clone()
            } else {
                here.parent()
                    .map(|p| p.join(&file_attr))
                    .unwrap_or_else(|| PathBuf::from(&file_attr))
                    .to_string_lossy()
                    .into_owned()
            };
            let include = IncludeSpec {
                file: resolved,
                args: Vec::new(),
            };
            if self_closing {
                attach_include(include, stack, desc);
            } else {
                stack.push(Frame::Include(include));
            }
        }
        other => {
            bail!(
                "<{other}> is not in the Phase 212.N.11 v1 launch tag set (file: `{}`)",
                here.display()
            )
        }
    }
    Ok(())
}

fn handle_end(
    e: &quick_xml::events::BytesEnd<'_>,
    stack: &mut Vec<Frame>,
    desc: &mut LaunchDescription,
    _scope: &mut ArgScope,
    _pkg_index: &PkgIndex,
    here: &Path,
) -> Result<()> {
    let tag = std::str::from_utf8(e.local_name().as_ref())
        .map_err(|_| eyre!("non-UTF-8 closing tag in `{}`", here.display()))?
        .to_string();
    let frame = stack.pop().ok_or_else(|| {
        eyre!(
            "unbalanced `</{tag}>` at `{}` (no open frame)",
            here.display()
        )
    })?;
    match (tag.as_str(), frame) {
        ("launch", Frame::Launch) | ("arg", _) | ("param", _) | ("remap", _) => Ok(()),
        ("node", Frame::Node(n)) => {
            attach_node(n, stack, desc);
            Ok(())
        }
        ("group", Frame::Group(g)) => {
            attach_group(g, stack, desc);
            Ok(())
        }
        ("include", Frame::Include(i)) => {
            attach_include(i, stack, desc);
            Ok(())
        }
        (other, _) => bail!(
            "</{other}> closed an unexpected frame in `{}`",
            here.display()
        ),
    }
}

fn attach_node(n: NodeSpec, stack: &mut [Frame], desc: &mut LaunchDescription) {
    match stack.last_mut() {
        Some(Frame::Group(g)) => g.nodes.push(n),
        _ => desc.nodes.push(n),
    }
}

fn attach_group(g: GroupSpec, stack: &mut [Frame], desc: &mut LaunchDescription) {
    // Groups nest into the parent group / top-level. Inject ns into
    // children: prepend the group ns to every child node's namespace.
    let mut g = g;
    if let Some(ns) = &g.namespace {
        for node in &mut g.nodes {
            node.namespace = Some(join_namespace(ns, node.namespace.as_deref()));
        }
    }
    match stack.last_mut() {
        Some(Frame::Group(parent)) => parent.nodes.extend(g.nodes.drain(..)),
        _ => desc.groups.push(g),
    }
}

fn attach_include(i: IncludeSpec, stack: &mut [Frame], desc: &mut LaunchDescription) {
    match stack.last_mut() {
        Some(Frame::Group(g)) => g.includes.push(i),
        _ => desc.includes.push(i),
    }
}

fn join_namespace(group_ns: &str, child_ns: Option<&str>) -> String {
    match child_ns {
        None => group_ns.to_string(),
        Some(c) => {
            if c.starts_with('/') {
                c.to_string()
            } else if group_ns.ends_with('/') {
                format!("{group_ns}{c}")
            } else {
                format!("{group_ns}/{c}")
            }
        }
    }
}

fn collect_attrs(
    e: &BytesStart<'_>,
    scope: &ArgScope,
    pkg_index: &PkgIndex,
    here: &Path,
) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for attr_res in e.attributes() {
        let attr = attr_res.with_context(|| {
            format!(
                "read XML attribute on `<{}>` in `{}`",
                to_string(e.name()),
                here.display()
            )
        })?;
        let key = std::str::from_utf8(attr.key.local_name().as_ref())
            .map_err(|_| eyre!("non-UTF-8 attribute name in `{}`", here.display()))?
            .to_string();
        let raw = attr
            .unescape_value()
            .with_context(|| format!("unescape attr `{key}` in `{}`", here.display()))?;
        let resolved = substitute(raw.as_ref(), scope, pkg_index, here)?;
        out.insert(key, resolved);
    }
    Ok(out)
}

fn to_string(q: QName<'_>) -> String {
    std::str::from_utf8(q.as_ref())
        .map(str::to_string)
        .unwrap_or_else(|_| "<?>".to_string())
}

/// Substitution scanner. Single pass; supports `$(find <pkg>)`,
/// `$(var <name>)`, `$(env <name>)`. Nested substitutions are
/// rejected — a `$(` inside an open `$(...)` errors.
fn substitute(s: &str, scope: &ArgScope, pkg_index: &PkgIndex, here: &Path) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'(' {
            // Find the matching `)`. Nested substitutions are NOT
            // supported v1; encountering a `$(` inside is a hard error.
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b')' {
                if j + 1 < bytes.len() && bytes[j] == b'$' && bytes[j + 1] == b'(' {
                    bail!(
                        "nested substitutions are not supported in v1: `{s}` in `{}`",
                        here.display()
                    );
                }
                j += 1;
            }
            if j >= bytes.len() {
                bail!(
                    "unterminated `$(...)` substitution in `{s}` at `{}`",
                    here.display()
                );
            }
            let body = std::str::from_utf8(&bytes[i + 2..j])
                .map_err(|_| eyre!("non-UTF-8 substitution body in `{}`", here.display()))?;
            let resolved = resolve_substitution(body, scope, pkg_index, here)?;
            out.push_str(&resolved);
            i = j + 1;
        } else {
            // Push the UTF-8 char starting at `i`. Walk the multi-byte
            // boundary so we don't split a codepoint.
            let ch_end = next_char_boundary(s, i);
            out.push_str(&s[i..ch_end]);
            i = ch_end;
        }
    }
    Ok(out)
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

fn resolve_substitution(
    body: &str,
    scope: &ArgScope,
    pkg_index: &PkgIndex,
    here: &Path,
) -> Result<String> {
    let body = body.trim();
    let (verb, arg) = match body.find(char::is_whitespace) {
        Some(idx) => (&body[..idx], body[idx + 1..].trim()),
        None => (body, ""),
    };
    match verb {
        "find" => {
            if arg.is_empty() {
                bail!("`$(find …)` missing pkg name in `{}`", here.display());
            }
            // Reuse the pkg-index resolver — it returns just the pkg dir;
            // any trailing path is glued back by the surrounding template.
            let resolved = pkg_index
                .resolve_pkg(arg)
                .with_context(|| format!("$(find {arg}) at `{}`", here.display()))?;
            Ok(resolved.to_string_lossy().into_owned())
        }
        "var" => {
            if arg.is_empty() {
                bail!("`$(var …)` missing arg name in `{}`", here.display());
            }
            scope
                .lookup(arg)
                .map(str::to_string)
                .ok_or_else(|| eyre!("`$(var {arg})` is not declared at `{}`", here.display()))
        }
        "env" => {
            if arg.is_empty() {
                bail!("`$(env …)` missing var name in `{}`", here.display());
            }
            std::env::var(arg).map_err(|_| {
                eyre!(
                    "`$(env {arg})` is unset; v1 has no default-value form (file: `{}`)",
                    here.display()
                )
            })
        }
        other => bail!(
            "unknown substitution verb `{other}` in `{}` (supported: find, var, env)",
            here.display()
        ),
    }
}
