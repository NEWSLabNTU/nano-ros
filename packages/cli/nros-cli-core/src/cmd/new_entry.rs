//! `nros new entry <name> --platform zephyr` — scaffold a framework entry.
//!
//! WHY A DIFFERENT NOUN THAN `nros new <name> --platform zephyr`
//!
//! That form makes a STANDALONE project: its own cargo root, buildable on its
//! own, copy-out-able (RFC-0026). A Zephyr entry is none of those things. It is
//! a package inside a workspace, built by west rather than cargo, and it is
//! meaningless without a bringup to name. Different inputs, different output,
//! so a different verb — which is what `nros new --platform zephyr` has been
//! saying all along by refusing:
//!
//! ```text
//! Error: nros new: single-package Rust scaffolding for --platform zephyr is
//! not available yet — Zephyr builds through west/Kconfig with
//! `nros::zephyr_component_main!` in a lib-only crate, not a plain cargo binary
//! ```
//!
//! WHY IT WRITES THE IMAGE TOO
//!
//! An entry and its `[image.*]` are two halves of one declaration, and every
//! failure met while getting a Zephyr image to build was the two halves
//! disagreeing:
//!
//! * no `conf` on the image → the entry's own CMakeLists stops the build with
//!   `FATAL_ERROR "… requires an RMW overlay"`;
//! * no `entry` on the image when several packages claim the board → the
//!   resolver refuses, and before it refused it picked the wrong one.
//!
//! A scaffold that wrote only the package would reproduce the bug it exists to
//! prevent. So it writes both, and the acceptance is that `nros build
//! <bringup>:<name> --dry-run` resolves the moment it returns.
//!
//! WHY THE WORKSPACE ROOT IS EDITED
//!
//! A west-built entry must be EXCLUDED from the cargo workspace: cargo would
//! otherwise try to build a `staticlib` for the host as an ordinary member.
//! That exclusion is invisible until it fails, and it is not something a user
//! can be expected to know, so the scaffold adds it.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};
use toml_edit::{Array, DocumentMut, Item, Table, value};

/// What to scaffold.
pub struct EntryScaffold {
    /// Package directory to create, e.g. `<ws>/src/zephyr_entry`.
    pub entry_dir: PathBuf,
    /// The bringup package this entry boots, e.g. `<ws>/src/demo_bringup`.
    pub bringup_dir: PathBuf,
    /// Cargo workspace root whose `exclude` list gains this package.
    pub workspace_root: PathBuf,
    /// The image's board — a NANO-ROS id (`zephyr`), written into
    /// `[image.<id>] board` (issue 1519).
    pub board: String,
    /// The id `west build -b` receives for it (`native_sim/native/64`),
    /// resolved through the board catalog.
    pub west_board: String,
    /// RMW backend, e.g. `zenoh`.
    pub rmw: String,
}

pub struct EntryScaffoldOut {
    pub entry_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub image_id: String,
}

/// Reject a name that is a path, a traversal, or empty — before touching disk.
///
/// Same guard, same reason as `new_system::validate_bringup_name`: a clean
/// diagnostic beats a half-written tree.
pub fn validate_entry_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("`nros new entry <name>` requires a package name");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        bail!(
            "`{name}` is not a package name. Pass a bare name \
             (e.g. `zephyr_entry`); use --into <dir> to choose where it goes."
        );
    }
    Ok(())
}

/// The single bringup in a workspace, or an error naming the candidates.
///
/// Guessing between two bringups would pick which system the entry boots — a
/// decision that belongs to the user and cannot be inferred from a directory
/// listing.
pub fn sole_bringup(src_dir: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = fs::read_dir(src_dir) {
        for e in entries.flatten() {
            if e.path().join("system.toml").is_file() {
                found.push(e.path());
            }
        }
    }
    found.sort();
    match found.len() {
        0 => bail!(
            "no bringup package under {} (a directory carrying `system.toml`).\n  \
             Create one first: nros new system <name>_bringup --components <pkgs>",
            src_dir.display()
        ),
        1 => Ok(found.remove(0)),
        _ => bail!(
            "{} bringup packages here, so the entry's system cannot be derived:\n{}\n  \
             Name it: --bringup <dir>",
            found.len(),
            found
                .iter()
                .map(|p| format!("  {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

pub fn scaffold_entry(cfg: &EntryScaffold) -> Result<EntryScaffoldOut> {
    let name = cfg
        .entry_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| eyre::eyre!("invalid entry package name"))?
        .to_string();

    if cfg.entry_dir.exists() {
        bail!(
            "{} already exists — refusing to overwrite it",
            cfg.entry_dir.display()
        );
    }
    let bringup = cfg
        .bringup_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| eyre::eyre!("invalid bringup package name"))?
        .to_string();
    if !cfg.bringup_dir.join("system.toml").is_file() {
        bail!(
            "{} carries no `system.toml`, so it is not a bringup package",
            cfg.bringup_dir.display()
        );
    }

    fs::create_dir_all(cfg.entry_dir.join("src"))
        .wrap_err_with(|| format!("create {}", cfg.entry_dir.display()))?;

    let mut files = Vec::new();
    let mut put = |rel: &str, body: String| -> Result<()> {
        let p = cfg.entry_dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, body).wrap_err_with(|| format!("write {}", p.display()))?;
        files.push(p);
        Ok(())
    };

    // The node packages this entry links. `nros::main!` emits a
    // `<pkg>::register(...)` per node in the launch file, so a package that is
    // not a dependency is a compile error in GENERATED code — the worst place
    // for it, because the line the user is sent to does not exist in any file
    // they wrote. Read from the bringup's own component list rather than by
    // parsing launch XML here: the components ARE the declaration, and a second
    // reader of the launch file would be a second thing to keep in step.
    let components = bringup_components(&cfg.bringup_dir.join("system.toml"))?;

    put(
        "package.xml",
        render_package_xml(&name, &bringup, &components),
    )?;
    put(
        "Cargo.toml",
        render_cargo_toml(
            &name,
            &cfg.board,
            &cfg.rmw,
            &components,
            &selection_rel(&cfg.workspace_root, &cfg.entry_dir, &name),
        ),
    )?;
    put("build.rs", render_build_rs().to_string())?;
    put("src/lib.rs", render_lib_rs(&name, &bringup))?;
    put("CMakeLists.txt", render_cmakelists(&name))?;
    put("prj.conf", render_prj_conf().to_string())?;
    put(&format!("prj-{}.conf", cfg.rmw), render_rmw_conf(&cfg.rmw))?;

    // No `boards/<board>.conf` (issue 2157). native_sim's network -- NSOS, so
    // the image runs unprivileged instead of on the root-only `zeth` TAP
    // driver -- is the board package's `nros-nsos` snippet, which `nros build`
    // and the module hook apply on every Zephyr line. A copy written here was
    // tied to one line (its TAP-driver symbol is 3.7's) and to one board.

    // The two edits outside the package — both invisible until they fail.
    let image_id = name.clone();
    add_image_block(&cfg.bringup_dir.join("system.toml"), &image_id, cfg, &name)?;
    exclude_from_cargo_workspace(&cfg.workspace_root, &cfg.entry_dir)?;

    Ok(EntryScaffoldOut {
        entry_dir: cfg.entry_dir.clone(),
        files,
        image_id,
    })
}

/// Append `[image.<id>]` to the bringup's `system.toml`.
///
/// `toml_edit` rather than a serialize-round-trip: a user's `system.toml`
/// carries comments and ordering they authored, and rewriting the whole
/// document to add four lines would silently discard them.
fn add_image_block(
    system_toml: &Path,
    image_id: &str,
    cfg: &EntryScaffold,
    entry_name: &str,
) -> Result<()> {
    let text = fs::read_to_string(system_toml)
        .wrap_err_with(|| format!("read {}", system_toml.display()))?;
    let mut doc: DocumentMut = text
        .parse()
        .wrap_err_with(|| format!("parse {}", system_toml.display()))?;

    let images = doc
        .entry("image")
        .or_insert(Item::Table({
            let mut t = Table::new();
            t.set_implicit(true);
            t
        }))
        .as_table_mut()
        .ok_or_else(|| eyre::eyre!("`image` in {} is not a table", system_toml.display()))?;

    if images.contains_key(image_id) {
        bail!(
            "{} already declares `[image.{image_id}]`",
            system_toml.display()
        );
    }

    let mut block = Table::new();
    block["board"] = value(cfg.board.clone());
    // Named even when unambiguous today: a second entry added later would make
    // it ambiguous, and the failure would appear in a file nobody edited.
    block["entry"] = value(entry_name.to_string());
    let mut conf = Array::new();
    conf.push(format!("prj-{}.conf", cfg.rmw));
    block["os_config"] = value(conf);
    block.decor_mut().set_prefix(format!(
        "\n# Scaffolded by `nros new entry {entry_name}`.\n\
         # `os_config` names the RMW overlay this entry's CMakeLists requires;\n\
         # `entry` names the application west is pointed at.\n"
    ));
    images.insert(image_id, Item::Table(block));

    fs::write(system_toml, doc.to_string())
        .wrap_err_with(|| format!("write {}", system_toml.display()))?;
    Ok(())
}

/// Add the entry to the `exclude` list of EVERY enclosing cargo workspace.
///
/// A west-built entry is a `staticlib` for a cross target; left as a member,
/// cargo builds it for the host on every workspace-wide command and fails in a
/// way that names neither west nor Zephyr.
///
/// Every enclosing one, not just the nearest — cargo resolves a package against
/// the outermost manifest that claims it, so excluding one and not the other
/// leaves the build failing on the one you did not edit:
///
/// ```text
/// error: current package believes it's in a workspace when it's not:
/// current:   …/examples/workspaces/rust/src/demo_entry/Cargo.toml
/// workspace: …/nano-ros/Cargo.toml
/// ```
///
/// Measured, by scaffolding into a workspace that is itself inside a cargo
/// workspace — which is exactly the nested shape this repository's own
/// examples have, and the rule CLAUDE.md already records as needing BOTH.
fn exclude_from_cargo_workspace(workspace_root: &Path, entry_dir: &Path) -> Result<()> {
    let mut done_any = false;
    // The workspace root first (it is the one the user thinks of), then every
    // ancestor above it.
    let mut cur: Option<&Path> = Some(workspace_root);
    while let Some(dir) = cur {
        if exclude_from_one_root(dir, entry_dir)? {
            done_any = true;
        }
        cur = dir.parent();
    }
    let _ = done_any;
    Ok(())
}

/// Returns whether this directory held a cargo workspace that was edited.
fn exclude_from_one_root(workspace_root: &Path, entry_dir: &Path) -> Result<bool> {
    let manifest = workspace_root.join("Cargo.toml");
    if !manifest.is_file() {
        // A workspace with no cargo root has nothing to exclude from. Not an
        // error: a C or C++ workspace is exactly this shape.
        return Ok(false);
    }
    let rel = entry_dir
        .strip_prefix(workspace_root)
        .unwrap_or(entry_dir)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");

    let text = fs::read_to_string(&manifest)?;
    let mut doc: DocumentMut = text
        .parse()
        .wrap_err_with(|| format!("parse {}", manifest.display()))?;
    let Some(ws) = doc.get_mut("workspace").and_then(|w| w.as_table_mut()) else {
        return Ok(false);
    };

    let excludes = ws
        .entry("exclude")
        .or_insert(value(Array::new()))
        .as_array_mut()
        .ok_or_else(|| {
            eyre::eyre!(
                "`workspace.exclude` in {} is not an array",
                manifest.display()
            )
        })?;
    if excludes.iter().any(|v| v.as_str() == Some(rel.as_str())) {
        return Ok(true);
    }
    excludes.push(rel);
    fs::write(&manifest, doc.to_string())?;
    Ok(true)
}

// ── templates ───────────────────────────────────────────────────────────────

/// The `pkg` of every `[[component]]` in a bringup's `system.toml`.
///
/// Deliberately tolerant: a bringup with no components yet is a legitimate
/// intermediate state (`nros new system` writes one before its packages
/// exist), and refusing it would make the scaffolds usable only in one order.
fn bringup_components(system_toml: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(system_toml)
        .wrap_err_with(|| format!("read {}", system_toml.display()))?;
    let doc: toml::Value =
        toml::from_str(&text).wrap_err_with(|| format!("parse {}", system_toml.display()))?;
    let mut out = Vec::new();
    if let Some(list) = doc.get("component").and_then(|c| c.as_array()) {
        for c in list {
            if let Some(pkg) = c.get("pkg").and_then(|p| p.as_str())
                && !out.iter().any(|e: &String| e == pkg)
            {
                out.push(pkg.to_string());
            }
        }
    }
    Ok(out)
}

fn render_package_xml(name: &str, bringup: &str, components: &[String]) -> String {
    format!(
        r#"<?xml version="1.0"?>
<!-- generated by `nros new entry {name}` -->
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>Zephyr entry package — boots the {bringup} topology.</description>
  <maintainer email="you@example.com">you</maintainer>
  <license>Apache-2.0</license>

  <exec_depend>{bringup}</exec_depend>
{component_depends}</package>
"#,
        component_depends = components
            .iter()
            .map(|c| format!("  <exec_depend>{c}</exec_depend>\n"))
            .collect::<String>(),
    )
}

/// Where `nros sync` writes this entry's selection package
/// (`<ws>/generated/nros-selection/<entry>`, `generate_facade_crates`), as a
/// path from the entry directory. `nros::main!` takes the image's backend
/// registration from that package, so the entry must depend on it (RFC-0071
/// D11, issue 2152).
fn selection_rel(workspace_root: &Path, entry_dir: &Path, name: &str) -> String {
    let target = workspace_root
        .join("generated")
        .join("nros-selection")
        .join(name);
    let rel = pathdiff::diff_paths(&target, entry_dir).unwrap_or(target);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn render_cargo_toml(
    name: &str,
    board: &str,
    rmw: &str,
    components: &[String],
    selection: &str,
) -> String {
    let node_deps: String = components
        .iter()
        .map(|c| format!("{c} = {{ path = \"../{c}\", default-features = false }}\n"))
        .collect();
    let node_deps = if node_deps.is_empty() {
        "# (none yet — add a path dep per node package the launch file names)\n".to_string()
    } else {
        node_deps
    };
    format!(
        r#"# Zephyr entry package — generated by `nros new entry {name}`.
#
# A `staticlib` exporting `rust_main`, NOT a `[[bin]]`: on Zephyr the RTOS owns
# boot and the C `main`, and zephyr-lang-rust's `rust_cargo_application()` links
# this archive by the fixed name `librustapp.a`.
#
# Deliberately carries NO `[workspace]` table. `nros::main!` walks UP to the
# real workspace root to resolve the bringup and the sibling node packages; an
# empty `[workspace]` here would stop that walk. The workspace root excludes
# this directory instead — `nros new entry` added that line.
#
# nano-ros crates are not published (RFC-0040); `version = "*"` is only the
# left-hand side of the patch `nros sync` writes. Run `nros sync` before
# building; it finds the SDK itself (`--nano-ros-path` names one explicitly).

[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
# Must be `rustapp`: `rust_cargo_application()` links `librustapp.a` by name.
name = "rustapp"
crate-type = ["staticlib", "rlib"]

# This package is an ENTRY because the bringup's `[image.*]` names it
# (`entry =`); nothing in this manifest marks it (issue 1289). Its board
# ({board}) and RMW ({rmw}) are stated there too (RFC-0098 D5), and
# `nros::main!` reads them from there — which also routes it onto its Zephyr
# emit branch.

[features]
default = ["rmw-{rmw}"]
# On a Rust Zephyr app nothing else links or registers the backend, so the
# entry carries the dep directly (issue 0129).
rmw-{rmw} = ["dep:nros-rmw-{rmw}"]

[dependencies]
# The image's selection package, which `nros sync` writes from the bringup:
# the RMW, edition and capability features, and the backend registration
# `nros::main!` expands. It does not exist until the first `nros sync`.
{name}_nros_selection = {{ path = "{selection}" }}
nros = {{ version = "*", default-features = false, features = ["alloc", "rmw-cffi", "macros"] }}
nros-board-zephyr = {{ version = "*" }}
nros-platform = {{ version = "*", default-features = false, features = ["platform-zephyr"] }}
nros-rmw-{rmw} = {{ version = "*", default-features = false, features = ["platform-zephyr"], optional = true }}
zephyr = "0.1.0"
log = "0.4"

# The node packages the bringup declares. `nros::main!` emits a `register` call
# for each node the launch file names, so these must be reachable from here.
{node_deps}
[build-dependencies]
zephyr-build = "0.1.0"
nros-zephyr-build = "*"

[profile.release]
opt-level = "s"
lto = true
debug = false
"#
    )
}

fn render_build_rs() -> &'static str {
    r#"// Kconfig -> cfg bridge, plus the locator/domain bake every Zephyr entry
// needs. Both canonical implementations live in the shared crates; this file
// exists only to call them.
fn main() {
    zephyr_build::export_kconfig_bool_options();
    nros_zephyr_build::bake_nros_config();
}
"#
}

fn render_lib_rs(name: &str, bringup: &str) -> String {
    format!(
        r#"//! `{name}` — Zephyr entry for the `{bringup}` topology.
//!
//! There is no Rust `fn main`: Zephyr emits the C `main`, and this crate
//! exports `rust_main` for `rust_cargo_application()` to call after kernel and
//! network init. `nros::main!` resolves `{bringup}`, reads its launch file, and
//! emits a `register` call per node — the launch file is the single source of
//! truth for the node set, so adding a node needs no edit here.

#![no_std]

// Zephyr owns the allocator, the panic handler and boot; pulling the crate in
// links the kernel's Rust glue.
extern crate zephyr;

nros::main!(launch = "{bringup}");
"#
    )
}

fn render_cmakelists(name: &str) -> String {
    format!(
        r#"# Zephyr application — generated by `nros new entry {name}`.
#
# A stock Zephyr app. `nros build <bringup>:{name}` points `west build` here and
# supplies this image's overlays; `west build -b <board> <this dir>` works
# exactly as well, which is the point (RFC-0085 D2).
cmake_minimum_required(VERSION 3.20.0)

# Your own out-of-tree Zephyr modules go here, BEFORE find_package(Zephyr):
#   list(APPEND ZEPHYR_EXTRA_MODULES "${{CMAKE_CURRENT_SOURCE_DIR}}/my_module")

find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})
project({name})

# The RMW overlay decides the cargo feature. Kconfig is the source: the image's
# `conf` list selects `prj-<rmw>.conf`, which sets CONFIG_NROS_RMW_<X>.
if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)
else()
    # Not a defensive check — this is reachable, and it is the failure a
    # missing `os_config` on the image produces. Naming the fix here is the whole
    # value of the branch.
    message(FATAL_ERROR
        "{name} requires an RMW overlay. Declare it on the image:\n"
        "    [image.{name}]\n"
        "    os_config = [\"prj-zenoh.conf\"]\n"
        "or pass it yourself: west build -- -DEXTRA_CONF_FILE=prj-zenoh.conf")
endif()

rust_cargo_application()
"#
    )
}

fn render_prj_conf() -> &'static str {
    r#"# Base Kconfig for this application. RMW-independent settings only —
# anything that differs per RMW belongs in prj-<rmw>.conf, which the image's
# `conf` list selects.
CONFIG_NROS=y

# Rust application support (zephyr-lang-rust).
CONFIG_RUST=y
# No CONFIG_RUST_ALLOC: Rust `alloc` goes through nros_platform_alloc, the same
# heap zenoh-pico uses, sized by CONFIG_NROS_ZEPHYR_HEAP_SIZE (issue 1324). The
# nros module refuses CONFIG_RUST_ALLOC=y, which would add a second heap.

CONFIG_MAIN_STACK_SIZE=16384
CONFIG_LOG=y
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_TCP=y
CONFIG_NET_SOCKETS=y
CONFIG_POSIX_API=y
"#
}

fn render_rmw_conf(rmw: &str) -> String {
    match rmw {
        "zenoh" => r#"# zenoh-pico backend.
CONFIG_NROS_RMW_ZENOH=y

# The router this image connects to is not set here. The default,
# "tcp/127.0.0.1:7447", is what `rmw_zenohd` listens on and what a
# `rmw_zenoh_cpp` node connects to; a board whose network is not the host's
# states its own in its package (mps2-an385-zephyr's QEMU user network says
# "tcp/10.0.2.2:7447"), and an image states one as `locator` in system.toml.
# A line here merged AFTER the board's file and overrode it (issue 2177).

# zenoh-pico takes ~22 pthread mutexes plus one per subscriber, from a static
# pool (Zephyr's default of 5 fails with -80 at session open, issue 0129). The
# build derives a floor and refuses below it (`_nros_mutex_floor` in
# `zephyr/cmake/nros_cargo_build.cmake`); 64 is the value every in-tree zenoh
# leaf uses. 16 was below the floor for one subscriber (issue 2177).
CONFIG_MAX_PTHREAD_MUTEX_COUNT=64
CONFIG_MAX_PTHREAD_COND_COUNT=16
"#
        .to_string(),
        // phase-405 W1 — this emitted `CONFIG_NROS_XRCE_AGENT_LOCATOR`, which
        // `zephyr/Kconfig` does not declare. Zephyr discards unknown symbols
        // silently, so every XRCE scaffold shipped an inert line that LOOKED
        // like it set the agent endpoint. The real pair is ADDR + PORT
        // (`zephyr/Kconfig:878`, `:884`), whose defaults happen to be the same
        // endpoint the dead line named — so nothing was broken, and nothing
        // was configurable either.
        "xrce" => r#"# Micro-XRCE-DDS backend.
CONFIG_NROS_RMW_XRCE=y
CONFIG_NROS_XRCE_AGENT_ADDR="127.0.0.1"
CONFIG_NROS_XRCE_AGENT_PORT=2018
CONFIG_MAX_PTHREAD_MUTEX_COUNT=16
"#
        .to_string(),
        // issue 0974 — do NOT emit `CONFIG_NROS_CYCLONE_DOMAIN_ID` here.
        // Its Kconfig default is `NROS_DOMAIN_ID`, which is what keeps the two
        // knobs from splitting; pinning a literal is what phase-180 did, and it
        // silently ran every cyclone image on domain 0 while the generic knob
        // said otherwise (issue 0161). A generated project is the worst place
        // for that: the user sets `CONFIG_NROS_DOMAIN_ID=5`, cyclone stays on 0,
        // and discovery just never matches. Leave it unset and let the default
        // track.
        "cyclonedds" => r#"# Cyclone DDS backend.
CONFIG_NROS_RMW_CYCLONEDDS=y
# No CONFIG_NROS_CYCLONE_DOMAIN_ID here on purpose (phase-405 W3). It defaults
# to NROS_DOMAIN_ID so the two knobs cannot split-brain; pinning it to a literal
# is what silently ran every cyclone image on domain 0 in phase-180, and a
# scaffold that emits the pin reproduces that bug in every new user project.
CONFIG_MAX_PTHREAD_MUTEX_COUNT=64
"#
        .to_string(),
        other => format!(
            "# {other} backend.\nCONFIG_NROS_RMW_{}=y\n",
            other.to_uppercase()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 2152 — the entry depends on the selection package `nros sync`
    /// writes for it, at the place sync writes it; `nros::main!` takes the
    /// backend registration from there.
    #[test]
    fn the_entry_depends_on_its_selection_package() {
        let ws = std::path::Path::new("/ws");
        let rel = selection_rel(ws, &ws.join("src/zephyr_entry"), "zephyr_entry");
        assert_eq!(rel, "../../generated/nros-selection/zephyr_entry");
        let toml = render_cargo_toml("zephyr_entry", "native_sim", "zenoh", &[], &rel);
        let v: toml::Value = toml::from_str(&toml).expect("valid manifest");
        assert_eq!(
            v["dependencies"]["zephyr_entry_nros_selection"]["path"].as_str(),
            Some("../../generated/nros-selection/zephyr_entry")
        );
    }

    /// Issue 2157 -- a scaffolded entry's native_sim network comes from its
    /// BOARD, so the board this scaffold targets by default must name the
    /// NSOS snippet. Before, the scaffold wrote its own copy of the NSOS conf,
    /// which named 3.7's TAP-driver symbol and so could not build on 4.4.
    #[test]
    fn the_native_sim_board_names_the_nsos_snippet() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
            .expect("board catalog");
        let d = crate::cmd::board_facts::resolve_board(&catalog, "zephyr").expect("board zephyr");
        let z = d.zephyr.as_ref().expect("[board.zephyr]");
        assert_eq!(z.west_board, "native_sim/native/64");
        assert!(
            z.snippets.iter().any(|s| s == "nros-nsos"),
            "{:?}",
            z.snippets
        );
        assert!(root.join("zephyr/snippets/nros-nsos/snippet.yml").is_file());
    }

    /// The Zephyr chapter's quick-start manifest names `zephyr-lang-rust` at a
    /// literal revision (a Rust entry builds only with it, phase-517 W17); it
    /// must be the revision nano-ros's own `west.yml` pins, or the book drifts
    /// off the supported (zephyr × zephyr-lang-rust) pair on the next bump.
    #[test]
    fn the_zephyr_chapter_pins_the_rust_module_west_yml_pins() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let rev_after = |text: &str| -> Option<String> {
            let i = text.find("- name: zephyr-lang-rust")?;
            text[i..].lines().take(5).find_map(|l| {
                l.trim()
                    .strip_prefix("revision:")
                    .map(|r| r.trim().to_string())
            })
        };
        let west = std::fs::read_to_string(root.join("west.yml")).expect("west.yml");
        let book =
            std::fs::read_to_string(root.join("book/src/getting-started/integration-zephyr.md"))
                .expect("chapter");
        let pinned = rev_after(&west).expect("west.yml pins zephyr-lang-rust");
        assert_eq!(rev_after(&book).as_deref(), Some(pinned.as_str()));
    }
}
