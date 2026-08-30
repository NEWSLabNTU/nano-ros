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
    /// Zephyr board target, e.g. `native_sim/native/64`.
    pub board: String,
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
        render_cargo_toml(&name, &cfg.board, &cfg.rmw, &components),
    )?;
    put("build.rs", render_build_rs().to_string())?;
    put("src/lib.rs", render_lib_rs(&name, &bringup))?;
    put("CMakeLists.txt", render_cmakelists(&name))?;
    put("prj.conf", render_prj_conf().to_string())?;
    put(&format!("prj-{}.conf", cfg.rmw), render_rmw_conf(&cfg.rmw))?;

    // Board-specific config, through Zephyr's OWN `boards/<board>.conf`
    // discovery rather than another `conf` entry on the image. Two reasons:
    // it is what a Zephyr application already looks like, and the image's
    // `conf` list is per-IMAGE while this is per-BOARD — an image built for a
    // second board would carry the wrong one.
    //
    // native_sim needs it to be useful at all: without NSOS the image takes
    // the `zeth` TAP driver, which needs a host interface set up as root, and
    // a scaffold whose output cannot run unprivileged is not a starting point.
    if let Some(board_conf) = render_board_conf(&cfg.board) {
        put(
            &format!("boards/{}.conf", sanitize_board(&cfg.board)),
            board_conf,
        )?;
    }

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
    block["conf"] = value(conf);
    block.decor_mut().set_prefix(format!(
        "\n# Scaffolded by `nros new entry {entry_name}`.\n\
         # `conf` names the RMW overlay this entry's CMakeLists requires;\n\
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

fn render_cargo_toml(name: &str, board: &str, rmw: &str, components: &[String]) -> String {
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
# left-hand side of the patch `nros sync` writes. Run `nros sync` with
# NROS_REPO_DIR set before building.

[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
# Must be `rustapp`: `rust_cargo_application()` links `librustapp.a` by name.
name = "rustapp"
crate-type = ["staticlib", "rlib"]

# Routes `nros::main!` onto its Zephyr emit branch, and is how `[image.*]`
# finds this package (RFC-0085 D4).
[package.metadata.nros.entry]
deploy = "zephyr"

[package.metadata.nros.deploy.zephyr]
board = "{board}"
rmw = "{rmw}"

[features]
default = ["rmw-{rmw}"]
# On a Rust Zephyr app nothing else links or registers the backend, so the
# entry carries the dep directly (issue 0129).
rmw-{rmw} = ["dep:nros-rmw-{rmw}"]

[dependencies]
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
    # missing `conf` on the image produces. Naming the fix here is the whole
    # value of the branch.
    message(FATAL_ERROR
        "{name} requires an RMW overlay. Declare it on the image:\n"
        "    [image.{name}]\n"
        "    conf = [\"prj-zenoh.conf\"]\n"
        "or pass it yourself: west build -- -DEXTRA_CONF_FILE=prj-zenoh.conf")
endif()

rust_cargo_application()
"#
    )
}

/// Zephyr's own spelling for a board directory: `/` becomes `_`.
fn sanitize_board(board: &str) -> String {
    board.replace(['/', '@'], "_")
}

/// Per-board Kconfig, or `None` for a board that needs none.
fn render_board_conf(board: &str) -> Option<String> {
    if !board.starts_with("native_sim") {
        return None;
    }
    Some(
        r#"# native_sim networking — generated by `nros new entry`.
#
# Native Sim Offloaded Sockets: the image uses the HOST's BSD sockets directly.
# The alternative is Zephyr's `zeth` TAP driver, which needs a host interface
# created as root — so this is what makes a fresh scaffold runnable by an
# ordinary user.
CONFIG_ETH_NATIVE_POSIX=n
CONFIG_NET_SOCKETS_OFFLOAD=y
CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y

# Wall-clock rather than as-fast-as-possible, so timers and lease intervals
# mean what they say when talking to a real router.
CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME=y
"#
        .to_string(),
    )
}

fn render_prj_conf() -> &'static str {
    r#"# Base Kconfig for this application. RMW-independent settings only —
# anything that differs per RMW belongs in prj-<rmw>.conf, which the image's
# `conf` list selects.
CONFIG_NROS=y

# Rust application support (zephyr-lang-rust).
CONFIG_RUST=y
CONFIG_RUST_ALLOC=y

# The Rust allocator on Zephyr is picolibc malloc, sized by this — NOT
# CONFIG_HEAP_MEM_POOL_SIZE. The executor's backing alone needs ~75 KB, and the
# 16 KB default fails at runtime rather than at link (issue 0163).
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=131072

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

# The router this image connects to. 7447 is what `rmw_zenohd` listens on and
# what a `rmw_zenoh_cpp` node connects to, so both halves of a ROS system agree
# by default.
CONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:7447"

# zenoh-pico needs ~8 pthread mutexes; Zephyr's default pool of 5 fails with
# -80 at session open (issue 0129).
CONFIG_MAX_PTHREAD_MUTEX_COUNT=16
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
CONFIG_MAX_PTHREAD_MUTEX_COUNT=64
"#
        .to_string(),
        other => format!(
            "# {other} backend.\nCONFIG_NROS_RMW_{}=y\n",
            other.to_uppercase()
        ),
    }
}
