//! Project scaffolder — promoted out of `main.rs` so `nros-cli` and any
//! other front-end can share it.
//!
//! v1 emits a colcon-compatible hello-world per `<lang> × <platform>`.
//! Use-case (`talker` / `listener` / `service` / `action`) and RMW-choice
//! diversification arrives once the `templates/` tree lands; until then
//! both fields are accepted for forward-compat but only surfaced in the
//! "Next steps" output.

use nros_lang::Language;

use crate::rmw_resolver;
use eyre::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

// Issue 1659 — the CLI's one spelling of a `git` spawn (shared with
// `nros-cli-core`, which this crate does not depend on, by `include!`).
include!("../../../tooling/nros-build-paths/src/git_env.rs");

/// Issue 0249 — maintainer line for generated `package.xml`. Sourced from
/// `git config user.name`/`user.email` so scaffolded packages carry the real
/// author; falls back to an instructional placeholder (never `TODO@todo.com`,
/// which shipped verbatim through colcon/bloom surfaces).
fn maintainer_xml() -> String {
    fn git_config(key: &str) -> Option<String> {
        let out = nros_git_command("git")
            .args(["config", key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let v = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!v.is_empty()).then_some(v)
    }
    let name = git_config("user.name");
    let email = git_config("user.email");
    match (name, email) {
        (Some(n), Some(e)) => format!(r#"<maintainer email="{e}">{n}</maintainer>"#),
        (Some(n), None) => format!(r#"<maintainer email="you@example.com">{n}</maintainer>"#),
        _ => r#"<maintainer email="you@example.com">Your Name</maintainer>"#.to_string(),
    }
}

/// Issue 0333 — how a `--platform` scaffolds a *runnable* Rust project. The
/// blessed shapes are genuinely different per platform (a hosted std binary vs
/// a `#![no_std]` `nros::main!()` self-bringup vs a west/Kconfig build), so this
/// is not one template. Each variant is modeled on a tracked, compiling example.
enum PlatformKind {
    /// Hosted std binary: explicit `register_linked_rmw()` +
    /// `nros::init()` + `Executor::open` + `spin`.
    /// Models `examples/linux/rust/talker`. (linux)
    Hosted,
    /// `#![no_std]` `nros::main!()` Form-1 self-bringup — `main.rs` is the
    /// one-line entry, `lib.rs` is the node (`nros::node!`), and
    /// `system.toml`'s `[image.*] board` names the board. The runtime dep
    /// profile differs: `CortexM` carries the cortex-m trio + a direct
    /// `nros-rmw-zenoh`; `Esp32` carries `esp-hal`/`esp-backtrace` and lets the
    /// board crate own the RMW. Models the baremetal / esp32 single-package
    /// talkers.
    SelfBringup { runtime: SelfBringupRuntime },
    /// Not yet scaffoldable as a single package — `nros new` bails with a
    /// pointer rather than emit a project that cannot run. The #333 follow-up.
    Deferred {
        /// Why, and where the current runnable example lives.
        reason: &'static str,
    },
}

#[derive(PartialEq)]
enum SelfBringupRuntime {
    /// cortex-m target: `cortex-m` + `cortex-m-rt` + `panic-semihosting`, plus a
    /// direct `nros-rmw-zenoh = { features = ["platform-bare-metal", …] }`.
    CortexM,
    /// esp32c3: `esp-hal` + `esp-backtrace` (panic handler); the board crate's
    /// default features own the RMW.
    Esp32,
}

/// The platform → board-crate + entry-shape SSoT (issue 0333). Every
/// `--platform` value the CLI advertises (`nros-cli-core/src/cmd/new.rs`) MUST
/// resolve here, or scaffolding `bail!`s loudly instead of silently emitting a
/// board dep that is really a TOML comment. `board_crate` / `deploy_token` are
/// the values the tracked entry examples declare; `deploy_token` is what bare
/// `nros::main!()` reads (`nros-orchestration-ir::board_path_for`).
struct PlatformSpec {
    board_crate: &'static str,
    /// Deploy token. For `Hosted` it is the `[package.metadata.nros.application]`
    /// deploy; for `SelfBringup` it is the `system.toml` `[image.*] board` the
    /// macro resolves to a board crate.
    deploy_token: &'static str,
    kind: PlatformKind,
}

/// Resolve a `--platform` token, or `bail!` with the full supported set. Called
/// once up front in [`scaffold_package`] so an unsupported platform fails before
/// any file is written, and reused by [`scaffold_rust`].
fn platform_spec(platform: &str) -> Result<PlatformSpec> {
    let spec = match platform {
        // The host board (`nros_board_linux::LinuxBoard`), selected by its one
        // name, `linux` (phase-500 W1: the board builds on Linux only).
        // `native` and `posix` were deprecated aliases until phase-500 W5;
        // `nros new` refuses them naming `linux`, and here they fall to the
        // unsupported arm like any other word.
        "linux" => PlatformSpec {
            board_crate: "nros-board-linux",
            deploy_token: "linux",
            kind: PlatformKind::Hosted,
        },
        "baremetal" => PlatformSpec {
            board_crate: "nros-board-mps2-an385",
            deploy_token: "mps2-an385",
            kind: PlatformKind::SelfBringup {
                runtime: SelfBringupRuntime::CortexM,
            },
        },
        // ESP32 is DORMANT (issue 1525): `nros new --platform` no longer offers
        // it, but the spec and its self-bringup template are kept for future
        // use (and still unit-tested below), so reviving it is one line.
        "esp32" => PlatformSpec {
            board_crate: "nros-board-esp32-qemu",
            deploy_token: "esp32-qemu",
            kind: PlatformKind::SelfBringup {
                runtime: SelfBringupRuntime::Esp32,
            },
        },
        "freertos" => PlatformSpec {
            board_crate: "nros-board-mps2-an385-freertos",
            deploy_token: "freertos",
            kind: PlatformKind::Deferred {
                reason: "the tracked shape is a split node-lib + `*-entry` bin pair; \
                         see examples/mps2-an385-freertos/rust/ (issue 0333 follow-up)",
            },
        },
        "nuttx" => PlatformSpec {
            board_crate: "nros-board-nuttx-qemu",
            deploy_token: "nuttx",
            kind: PlatformKind::Deferred {
                reason: "the tracked shape is a split node-lib + `*-entry` bin pair; \
                         see examples/qemu-armv7a-nuttx/rust/ (issue 0333 follow-up)",
            },
        },
        "threadx" => PlatformSpec {
            board_crate: "nros-board-threadx-linux",
            deploy_token: "threadx-linux",
            kind: PlatformKind::Deferred {
                reason: "the tracked shape is a split node-lib + `*-entry` bin pair; \
                         see examples/threadx-linux/rust/ (issue 0333 follow-up)",
            },
        },
        "zephyr" => PlatformSpec {
            board_crate: "nros-board-zephyr",
            deploy_token: "zephyr",
            kind: PlatformKind::Deferred {
                reason: "Zephyr builds through west/Kconfig with \
                         `nros::zephyr_component_main!` in a lib-only crate, not a plain \
                         cargo binary; see examples/zephyr/rust/ (issue 0333 follow-up)",
            },
        },
        other => bail!(
            "nros new: unsupported --platform '{other}'. Supported: \
             linux, freertos, baremetal, nuttx, threadx, zephyr."
        ),
    };
    Ok(spec)
}

#[derive(Debug, Clone)]
pub struct ScaffoldConfig {
    pub name: String,
    /// phase-469 — the enum, not the string. `nros new --lang` is already
    /// restricted to `rust`/`c`/`cpp` by clap's `value_parser`, so the
    /// `other => bail!("Unknown language")` arm this field used to require was
    /// UNREACHABLE: a refusal that could never fire, standing exactly where
    /// the compiler now stands.
    pub lang: Language,
    pub platform: String,
    pub rmw: String,
    /// ROS edition (`humble`|`iron`|`jazzy`|`rolling`) → the `ros-<edition>`
    /// cargo feature on the scaffolded package (phase-304 W2b, RFC-0056).
    pub ros_edition: String,
    pub use_case: String,
    pub force: bool,
    /// The `[image.<platform>] board` to write, already resolved by the caller
    /// through the board catalog (phase-517 W2 — `nros new` derives the
    /// platform's default board there, or takes `--board`). `None` keeps the
    /// platform spec's own token, which is right for the Rust shapes: their
    /// board key is the Rust pack's (`BOARD_PATHS`), not a cmake one.
    pub board: Option<String>,
}

pub fn scaffold_package(cfg: &ScaffoldConfig) -> Result<()> {
    scaffold_package_at(&PathBuf::from(&cfg.name), cfg)
}

/// [`scaffold_package`] into `dir` (the package is still named `cfg.name`).
pub fn scaffold_package_at(dir: &Path, cfg: &ScaffoldConfig) -> Result<()> {
    let dir = dir.to_path_buf();
    if dir.exists() {
        if !cfg.force {
            bail!(
                "Directory '{}' already exists (use --force to overwrite)",
                cfg.name
            );
        }
        fs::remove_dir_all(&dir)?;
    }

    // Phase 227.4 — validate + lower the declared RMW (RFC-0031). A bad
    // `--rmw` fails here with the known-list, not as a broken downstream build.
    let rmw = rmw_resolver::resolve_rmw(&cfg.rmw).map_err(|e| eyre::eyre!("nros new: {e}"))?;

    // phase-304 W2b (RFC-0056) — validate the declared ROS edition (a bad
    // `--ros-edition` fails loud here). Issue 2079: it is DECLARED, in the
    // `system.toml` below, and the toolchain lowers it — through the selection
    // package for a Rust leaf, through the leaf reader for C/C++. The cargo
    // feature is kept only for the one unmigrated template (esp32, dormant).
    let edition = rosidl_codegen::RosEdition::parse(&cfg.ros_edition).ok_or_else(|| {
        eyre::eyre!(
            "nros new: unknown ROS edition '{}' (humble | iron | jazzy | rolling)",
            cfg.ros_edition
        )
    })?;
    let edition_feature = edition.cargo_feature();

    // Issue 0333 — validate the platform up front (like `--rmw`/`--ros-edition`
    // above) so an unsupported value fails loud before any file is written,
    // rather than silently degrading into a commented-out board dep or a
    // `deploy=` string nothing recognizes.
    let spec = platform_spec(&cfg.platform)?;

    // Issue 0333 defect 2 — the Rust runnable-template rewrite covers the
    // single-package platforms (linux hosted, baremetal/esp32
    // `nros::main!()` self-bringup). The split-package (freertos/nuttx/threadx)
    // and west/Kconfig (zephyr) shapes are a follow-up; bail before writing any
    // file rather than emit a project that cannot run. C/C++ are unaffected —
    // their platform delta rides `package.xml`, no per-platform cargo shape.
    if cfg.lang == Language::Rust
        && let PlatformKind::Deferred { reason } = spec.kind
    {
        bail!(
            "nros new: single-package Rust scaffolding for --platform {} is not available \
             yet — {reason}. Use linux or baremetal for a runnable Rust \
             starter today.",
            cfg.platform
        );
    }

    // RFC-0048 (phase-287) + RFC-0087 D2/D3 (phase-420 W4) — a scaffolded
    // package is nano-ros-owned, so `<build_type>` says so: `nros_cmake` for
    // the C/C++ shape, `nros_cargo` for Rust. Both replace claims that were
    // false in opposite directions — `ament_cmake` said a stock `colcon build`
    // could handle the package, and `nros.<lang>.<platform>` was an improvised
    // spelling whose only registered readers were 30 colcon entry points that
    // W4 deleted.
    //
    // phase-445 W3b (RFC-0098 D3/D5) — the deployment (board, RMW, domain,
    // node) is the `system.toml` beside the manifest, for EVERY language. It
    // replaces the `<nano_ros deploy= rmw=/>` package.xml tuple (retired and
    // refused by both package.xml readers) and, for the Rust self-bringup, the
    // `[package.metadata.nros.{entry,node}]` tables: `find_package(nano_ros)`,
    // `nros::main!()`, `nros sync` and the colcon task all read this one file.
    let is_cxx = matches!(cfg.lang, Language::C | Language::Cpp);
    let build_type = if is_cxx { "nros_cmake" } else { "nros_cargo" };

    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("system.toml"),
        scaffold_system_toml(
            &cfg.name,
            cfg.lang,
            &cfg.platform,
            &spec,
            rmw.cmake_value,
            &cfg.ros_edition,
            cfg.board.as_deref(),
        ),
    )?;

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros {platform} package</description>
  {maintainer}
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>{build_type}</build_type>
  </export>
</package>
"#,
        name = cfg.name,
        platform = cfg.platform,
        maintainer = maintainer_xml(),
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    // phase-469 — exhaustive over the enumeration. A new `Language` variant is
    // a compile error HERE, where somebody has to write its template, instead
    // of an "Unknown language" at run time.
    match cfg.lang {
        Language::Rust => scaffold_rust(&cfg.name, &cfg.platform, edition_feature, &dir)?,
        Language::C => scaffold_c(&cfg.name, &cfg.platform, rmw.cmake_value, &dir)?,
        Language::Cpp => scaffold_cpp(&cfg.name, &cfg.platform, rmw.cmake_value, &dir)?,
    }

    println!("✓ Created nano-ros package '{}'", cfg.name);
    println!("  Language : {}", cfg.lang);
    println!("  Platform : {}", cfg.platform);
    println!("  RMW      : {}", cfg.rmw);
    println!(
        "  Use case : {} (template diversification: TODO)",
        cfg.use_case
    );
    println!("  Build    : {build_type}");
    println!();
    println!("Next steps:");
    for line in next_steps(&cfg.name, cfg.lang, &cfg.platform) {
        println!("  {line}");
    }

    Ok(())
}

/// The "Next steps" a scaffolded package prints — the build flow the book
/// documents for that language and platform, never another road's:
///
/// * Rust (`first-node-rust.md`, `bare-metal.md`): `nros sync`, then
///   `nros build` — a Rust leaf's message crates and cargo settings must exist
///   before cargo parses anything.
/// * C/C++ (`first-node-c.md`, `freertos.md`, …): `nros build` alone — the
///   message bindings are generated inside CMake at configure time, so there
///   is no sync step.
/// * C/C++ on Zephyr (`integration-zephyr.md`): the app is a stock Zephyr
///   application, so the primary flow is the user's own `west build` with the
///   module named on the line; `nros build` is the convenience that hands off
///   to it once it knows the west workspace.
#[must_use]
pub fn next_steps(name: &str, lang: Language, platform: &str) -> Vec<String> {
    let mut v = Vec::new();
    if !(lang != Language::Rust && platform == "zephyr") {
        v.push(format!("cd {name}"));
    }
    match (lang, platform) {
        (Language::Rust, _) => {
            v.push("nros sync    # message crates + build/<image>/nros-cargo.toml".into());
            v.push("nros build   # builds the image system.toml declares".into());
        }
        (Language::C | Language::Cpp, "zephyr") => {
            v.push(
                "# a stock Zephyr app: with ZEPHYR_BASE set (or from inside your west workspace)"
                    .into(),
            );
            v.push(format!(
                "west build -b native_sim/native/64 {name} -- \
                 -DZEPHYR_EXTRA_MODULES=\"$(nros sdk-root)\""
            ));
            v.push(format!(
                "# or let nros hand off to west: cd {name} && \
                     NROS_ZEPHYR_WORKSPACE=<dir with zephyr/> nros build"
            ));
        }
        (Language::C | Language::Cpp, _) => {
            v.push(
                "nros build   # configures + builds the image system.toml declares (no sync: \
                 C/C++ bindings are generated by CMake)"
                    .into(),
            );
        }
    }
    v
}

/// The `system.toml` a scaffolded package states its deployment in (RFC-0098
/// D3/D5, phase-445 W3b) — the same schema a workspace bringup and every
/// in-tree single-package example use.
///
/// `board` is the caller's resolved board when it passes one (`nros new`
/// always does for C/C++: the platform's default board from the catalog,
/// phase-517 W2); else the self-bringup's `nros::main!()` board key
/// (`board_path_for`), the host board, or the platform token. The last was
/// the C/C++ default until phase-517 W2, and `freertos`/`nuttx`/`baremetal`
/// then reached the cmake road as their own deploy token and configured with
/// no board module; `threadx` named two boards. A Rust starter declares its
/// node: its `main.rs` is `nros::main!()`, which boots the `[[component]]`
/// this file names. The C/C++ starters are plain executables with their own
/// `main`.
///
/// `ros_edition` is stated for every language (issue 2079): the declaration
/// is the one carrier, and both roads lower it — the selection package for a
/// Rust leaf (issue 1305), the leaf reader for C/C++ (issue 2087).
fn scaffold_system_toml(
    name: &str,
    lang: Language,
    platform: &str,
    spec: &PlatformSpec,
    rmw_value: &str,
    ros_edition: &str,
    resolved_board: Option<&str>,
) -> String {
    let self_bringup =
        lang == Language::Rust && matches!(spec.kind, PlatformKind::SelfBringup { .. });
    let rust_node = lang == Language::Rust
        && matches!(
            spec.kind,
            PlatformKind::SelfBringup { .. } | PlatformKind::Hosted
        );
    // Issue 1109 / phase-500 — a hosted image names the host board by its
    // board key (`linux`), never by the `--platform` word (`posix` is not a
    // board, `native` no longer one).
    let board = if let Some(b) = resolved_board {
        b
    } else if self_bringup || matches!(spec.kind, PlatformKind::Hosted) {
        spec.deploy_token
    } else {
        platform
    };
    // A Zephyr C/C++ app is a TYPED component the module's carrier boots by
    // identity (integration-zephyr.md), so it names it like the self-bringup.
    let zephyr_component = lang != Language::Rust && platform == "zephyr";
    let component = if zephyr_component {
        format!(
            "\n# The component the Zephyr typed carrier boots — src/Talker.*'s\n\
             # `{}::Talker`.\n\
             [[component]]\npkg = \"{name}\"\nclass = \"{}::Talker\"\nname = \"talker\"\n",
            name.replace('-', "_"),
            name.replace('-', "_")
        )
    } else if rust_node {
        let dispatch = if self_bringup {
            "# `dispatch = \"deferred\"`: callbacks hand off to a ring (phase-216 A.5).\n"
        } else {
            ""
        };
        let dispatch_key = if self_bringup {
            "dispatch = \"deferred\"\n"
        } else {
            ""
        };
        format!(
            "\n# The node `nros::main!()` boots — this crate's own `nros::node!(Talker)`.\n\
             {dispatch}[[component]]\npkg = \"{name}\"\nclass = \"{}::Talker\"\nname = \"{name}\"\n\
             {dispatch_key}",
            name.replace('-', "_")
        )
    } else {
        String::new()
    };
    format!(
        "# What this package deploys to (RFC-0098 D3/D5) — the same schema a workspace\n\
         # bringup uses. Switching board, RMW or ROS edition is editing this file\n\
         # and re-running `nros sync`.\n\
         \n[system]\nname = \"{name}\"\nrmw = \"{rmw_value}\"\nros_edition = \"{ros_edition}\"\n\
         domain_id = 0\n\
         {component}\n[image.{platform}]\nboard = \"{board}\"\n"
    )
}

#[derive(Debug, Clone)]
pub struct ComponentScaffoldConfig {
    pub name: String,
    /// Node flavor: `talker` / `listener` / `service` / `action`. Template
    /// diversification is TODO — today every flavor emits the publisher+timer
    /// shape, named after the flavor.
    pub use_case: String,
    /// Source language. `rust` lands the planned-mode `nros::Component` shape;
    /// `c` / `cpp` land the typed component shape (RFC-0043) in the RFC-0048
    /// ament spelling: `find_package(nano_ros REQUIRED)` +
    /// `nano_ros_add_node(...)` + a `configure(::rclcpp::Node&)` (C++) /
    /// `RCLC_C_COMPONENT` (C) seam.
    ///
    /// phase-469 — the enum, for the same reason [`ScaffoldConfig::lang`] is.
    pub lang: Language,
    pub force: bool,
}

/// Scaffold a **planned-mode component** — a reusable nano-ros node compiled as
/// a *library* and linked into a system plan. Unlike
/// the direct-mode hello-world binary `scaffold_package` emits (a `[node]`
/// manifest), this produces an `nros::Component` impl plus a *folded*
/// `[component]` table in `nros.toml`. The platform + RMW are chosen later, at
/// Entry-package build time — not baked here.
///
/// The manifest is intentionally minimal: `[linkage]` is omitted (derived —
/// executable ← component short name, `exported_symbol` ← `nros_component_<n>`,
/// `crate_name` ← package) and `[overrides]` defaults to empty (Phase 172 W.3).
pub fn scaffold_component(cfg: &ComponentScaffoldConfig) -> Result<()> {
    match cfg.lang {
        Language::Rust => scaffold_component_rust(cfg),
        Language::Cpp => scaffold_component_cpp(cfg),
        Language::C => scaffold_component_c(cfg),
    }
}

fn scaffold_component_rust(cfg: &ComponentScaffoldConfig) -> Result<()> {
    let dir = PathBuf::from(&cfg.name);
    if dir.exists() {
        if !cfg.force {
            bail!(
                "Directory '{}' already exists (use --force to overwrite)",
                cfg.name
            );
        }
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(dir.join("src"))?;

    let crate_name = cfg.name.replace('-', "_");
    let module = &cfg.use_case; // constrained by the CLI to a valid Rust ident
    // The node TYPE. `nros::node!()` and `[package.metadata.nros.node] class`
    // both name it, and every in-tree Node pkg names it after the node. Not
    // `Component`: that is the trait it implements (phase-483 W2).
    let class_name = {
        let mut c = module.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::from("Node"),
        }
    };

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros reusable component.</description>
  {maintainer}
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>nros_cargo</build_type>
  </export>
</package>
"#,
        name = cfg.name,
        maintainer = maintainer_xml(),
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

# Standalone-buildable: an empty [workspace] makes this its own Cargo root even
# when dropped under a colcon workspace's src/.
[workspace]

# Node pkg shape — a library, with no binary target. `nros::node!()` exports the
# register / init / dispatch trampolines; an Entry pkg brings up the executor and
# spin loop via `nros::main!()`.
[lib]
path = "src/lib.rs"

# Platform/RMW-AGNOSTIC. The concrete backend is linked by the board crate the
# sibling Entry pkg deps, never named here; `rmw-cffi` is the vtable seam only.
[package.metadata.nros.node]
class = "{crate_name}::{class_name}"
name = "{module}"

# A reusable component is a library (rlib); the deployed system links it.
# nano-ros crates are not published to crates.io (RFC-0040) — `version = "*"` is
# only the patched left-hand side. Run `nros sync` to write the nros-managed
# [patch.crates-io] block redirecting `nros` to your nano-ros SDK, then
# `cargo build`. `nros` finds the SDK itself; `--nano-ros-path` names one.
[dependencies]
nros = {{ version = "*", default-features = false, features = [
    "alloc",
    "rmw-cffi",
    "macros",
] }}
# Message crates come from package.xml: each <depend> on a message package
# becomes a `# nros-managed` path row below when you run `nros sync` (or
# `nros build`). Edit package.xml, not this table (RFC-0098, phase-517 W8).
# A node logs through the `log` facade; the board registers the sink at boot.
log = {{ version = "0.4", default-features = false }}
"#,
        name = cfg.name,
        crate_name = crate_name,
        class_name = class_name,
        module = module,
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    // phase-452 W2 / issue 1412 — written against the LIVE API and matched to
    // what the C++ component template demonstrates.
    //
    // The previous body targeted the `Component*` trait family that phase
    // 212.N.12 retired on 2026-06-03 (`Component` -> `Node`,
    // ~~`ComponentContext`~~ -> `NodeContext`, ~~`ComponentResult`~~ -> `NodeResult`,
    // `nros::component!` deleted), so `nros new --component --lang rust`
    // emitted a package that compiled by neither documented route for three
    // and a half months. Nothing noticed because the scaffold tests assert
    // substrings and never build the result — issue 1058, whose C++ half was
    // fixed and whose Rust half was never examined.
    //
    // A rename alone would NOT have been enough, which is the same trap
    // phase-417 fell into one language over: `create_node` also lost its
    // `NodeId` first argument (that form is `create_node_with_id` now), and a
    // node declaring a callback must impl `ExecutableNode` or the generated
    // runtime has no body to dispatch to.
    //
    // Feature parity with the C++ component template, deliberately: the same
    // `std_msgs/Int32` on `/chatter`, the same 1 Hz period, the same counter
    // state, and a log line on success where C++ prints one.
    let lib_rs = format!(
        r#"#![no_std]

//! `{name}` — a reusable nano-ros Node pkg (planned mode).
//!
//! `nros plan` and Entry codegen link this crate into a system and call the
//! `register` trampoline `nros::node!()` exports. Platform + RMW are chosen at
//! Entry-package build time, not here — this crate names neither.
//!
//! `register()` declares the node, a publisher and a 1 Hz timer;
//! `ExecutableNode::on_callback` publishes a counter on every tick.

use nros::{{
    Callback, CallbackCtx, ExecutableNode, Component, NodeContext, DeclarativeNode, NodeOptions, NodeResult,
    TimerDuration,
}};
use std_msgs::msg::Int32;

/// {class_name} — counter state plus a chatter publish on every tick.
pub struct {class_name};

impl Component for {class_name} {{
    const NAME: &'static str = "{module}";

    /// Exact bounds: one publisher (a timer needs no registry slot).
    /// Declaring them stops this node paying for capacity it never fills.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {{
        let mut node = ctx.create_node(NodeOptions::new("{module}"))?;
        let publisher = node.create_publisher_for_topic::<Int32>("/chatter")?;
        node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&publisher)?;
        Ok(())
    }}
}}

impl ExecutableNode for {class_name} {{
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {{
        0
    }}

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {{
        if callback.as_str() == "on_tick" {{
            let msg = Int32 {{ data: *state }};
            let _ = ctx.publish_to_topic::<Int32, 8>("/chatter", &msg);
            log::info!("{module} publishing chatter seq={{}}", *state);
            *state = state.wrapping_add(1);
        }}
    }}
}}

// Exports the register / init / dispatch trampolines the Entry pkg links.
// The type lives at the CRATE ROOT with no module segment — the shape
// `nros::node!(Class)` and the manifest's `class` key both assume.
nros::node!({class_name});
"#,
        name = cfg.name,
        module = module,
        class_name = class_name,
    );
    fs::write(dir.join("src/lib.rs"), lib_rs)?;

    // Folded `[component]` manifest (Phase 172 W.1). Minimal — no `[linkage]`
    // (derived) and no `[overrides]` (defaults to empty). The `crate::module`
    // component id is required by `nros metadata --build`.
    let nros_toml = format!(
        r#"# nano-ros component manifest (planned mode). A reusable node linked into a
# system by `nros plan` and Entry codegen. See
# docs/design/0004-configuration-and-transports.md.

[component]
version = 1
package = "{name}"
component = "{crate_name}::{module}"
# phase-307 W1 / issue 1412 — the registered type's fully qualified path.
# Without it the metadata harness falls back to guessing
# `<crate>::<module>::Component`, which the shipping `nros::node!(Class)` shape
# (`impl Component for Class` at the crate root, no `Component`, no module segment)
# never matches — and the harness then fails to compile.
class = "{crate_name}::{class_name}"
language = "rust"

[component.metadata]
source_metadata = "metadata/{module}.json"
"#,
        name = cfg.name,
    );
    fs::write(dir.join("nros.toml"), nros_toml)?;

    println!("✓ Created nano-ros component '{}'", cfg.name);
    println!("  Component : {crate_name}::{module}");
    println!("  Kind      : planned-mode (library, linked by an Entry pkg)");
    println!();
    println!("Next steps:");
    println!("  cd {}", cfg.name);
    println!("  # add this package to a workspace's [system].components, then:");
    println!("  nros metadata --build   # record its source metadata");

    Ok(())
}

/// Scaffold a **C++ Node pkg** — typed component (RFC-0043). Emits the
/// RFC-0048 ament surface (`find_package(nano_ros)` + `nano_ros_add_node`) and a
/// `<UserClass>::configure(::rclcpp::Node&)` real-callback body in the
/// `<pkg>::` namespace per §212.L.4 (class prefix must equal `PROJECT_NAME`).
/// `configure` creates a `Publisher` + binds a member timer callback by
/// identity (no string descriptor, no interpreter) — the typed Entry
/// constructs the object + calls `configure(node)` on the real executor.
fn scaffold_component_cpp(cfg: &ComponentScaffoldConfig) -> Result<()> {
    let dir = PathBuf::from(&cfg.name);
    if dir.exists() {
        if !cfg.force {
            bail!(
                "Directory '{}' already exists (use --force to overwrite)",
                cfg.name
            );
        }
        fs::remove_dir_all(&dir)?;
    }
    // Header lives at `include/<pkg>/<Class>.hpp` so the typed Entry can
    // `#include "<pkg>/<Class>.hpp"` (nano_ros_node_register adds `include/`).
    fs::create_dir_all(dir.join("src"))?;
    fs::create_dir_all(dir.join("include").join(cfg.name.replace('-', "_")))?;

    // Pkg name → namespace + class. `my-talker` → ns `my_talker`, class
    // `Talker` (PascalCase of use_case). §212.L.4 class prefix must equal
    // PROJECT_NAME (sanitised), so the namespace = the sanitised pkg name.
    let pkg_sym = cfg.name.replace('-', "_");
    let class_name = use_case_to_pascal(&cfg.use_case);
    let node_name = &cfg.use_case;

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros C++ Node pkg.</description>
  {maintainer}
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>nros_cmake</build_type>
  </export>
</package>
"#,
        name = cfg.name,
        maintainer = maintainer_xml(),
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project({pkg_sym} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Typed component (RFC-0043). No add_executable, no `main()`; the linked
# Entry pkg's typed runtime constructs this class + calls `configure(node)`
# on the real executor. `DEPLOY linux` also builds a standalone runnable
# single-node ELF via the typed native carrier.
nano_ros_add_node({node_name} CLASS {pkg_sym}::{class_name}
    SOURCES src/{class_name}.cpp
    DEPLOY  linux)

# The verb generated the package.xml `<depend>` closure as INTERFACE libs
# (`std_msgs__nano_ros_cpp` etc.) carrying the generated headers' include
# dirs + FFI-glue link. The typed `Publisher<Int32>` needs them.
target_link_libraries({pkg_sym}_{node_name}_component
    PUBLIC std_msgs__nano_ros_cpp)
"#,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let class_hpp = format!(
        // phase-483 W1 -- the template writes `rclcpp::` only. phase-417 W-B3 had
        // kept it on `::nros::` because three of the four names did not exist under
        // `rclcpp::` then; they are defined there now, and `nros::` is gone.
        r#"#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace {pkg_sym} {{

/// {class_name} — a typed component (RFC-0043). `configure` creates a
/// publisher on `/chatter` and binds the member `on_tick` (by identity, no
/// name) as a 1 Hz timer callback that publishes a real counter. The typed
/// Entry constructs this object + calls `configure(node)`; the executor
/// dispatches `on_tick` during `spin_once`.
class {class_name} {{
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body; bound via &{class_name}::on_tick (no name)

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
}};

}} // namespace {pkg_sym}
"#,
    );
    fs::write(
        dir.join("include")
            .join(&pkg_sym)
            .join(format!("{class_name}.hpp")),
        class_hpp,
    )?;

    let class_cpp = format!(
        r#"// Generated by `nros new {name} --component --lang cpp`.
//
// Typed component (RFC-0043). `configure` creates a `Publisher<Int32>` on
// `/chatter` + binds the member `on_tick` by identity as a 1 Hz timer; the
// callback publishes a real counter. No string descriptor, no interpreter.

#include "{pkg_sym}/{class_name}.hpp"

#include <cstdio>

namespace {pkg_sym} {{

void {class_name}::on_tick() {{
    std_msgs::msg::Int32 m;
    m.data = count_++;
    if (pub_.publish(m).ok()) {{
        std::printf("Published: %d\n", m.data);
    }}
}}

::rclcpp::Result {class_name}::configure(::rclcpp::Node& node) {{
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return node.create_wall_timer<{class_name}, &{class_name}::on_tick>(timer_, 1000, this);
}}

}} // namespace {pkg_sym}
"#,
        name = cfg.name,
    );
    fs::write(dir.join(format!("src/{class_name}.cpp")), class_cpp)?;

    println!("✓ Created nano-ros C++ Node pkg '{}'", cfg.name);
    println!("  Class     : {pkg_sym}::{class_name}");
    println!("  Node      : {node_name}");
    println!("  Kind      : typed component (RFC-0043)");
    println!();
    println!("Next steps:");
    println!("  cd {}", cfg.name);
    println!("  # Solo build:");
    println!("  cmake -S . -B build");
    println!("  #   (the `nros` on PATH finds its own SDK; to name one explicitly");
    println!("  #    add -DNROS_REPO_DIR=<path-to-nano-ros>)");
    println!("  cmake --build build");
    println!();
    println!("  # Or add it as a SUBDIR in a workspace root that calls");
    println!("  # nano_ros_workspace(...), then build the workspace.");

    Ok(())
}

/// Scaffold a **C Node pkg** — typed component (RFC-0043). Emits a
/// `RCLC_C_COMPONENT(<state>, <configure>)` factory/configure seam: a raw
/// `/chatter` publisher + 1 Hz timer that publishes a CDR-encoded Int32. The
/// typed Entry's `__nros_c_component_<pkg>_{create,configure}` calls run it on
/// the real executor — no declarative descriptor, no interpreter.
fn scaffold_component_c(cfg: &ComponentScaffoldConfig) -> Result<()> {
    let dir = PathBuf::from(&cfg.name);
    if dir.exists() {
        if !cfg.force {
            bail!(
                "Directory '{}' already exists (use --force to overwrite)",
                cfg.name
            );
        }
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(dir.join("src"))?;

    let pkg_sym = cfg.name.replace('-', "_");
    let class_name = use_case_to_pascal(&cfg.use_case);
    let node_name = &cfg.use_case;
    let state_ty = format!("{pkg_sym}_t");
    let configure_fn = format!("{node_name}_configure");

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros C Node pkg.</description>
  {maintainer}
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>nros_cmake</build_type>
  </export>
</package>
"#,
        name = cfg.name,
        maintainer = maintainer_xml(),
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project({pkg_sym} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Typed C component (RFC-0043), written against the C API with the generated
# `std_msgs` bindings (declared in package.xml). `DEPLOY linux` also builds a
# standalone runnable single-node ELF via the typed C carrier.
nano_ros_add_node({node_name} CLASS {pkg_sym}::{class_name} TYPED
    SOURCES src/{class_name}.c
    DEPLOY  linux)
"#,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let source = format!(
        r#"// Generated by `nros new {name} --component --lang c`.
//
// Typed C component (RFC-0043). `{configure_fn}` creates a `/chatter`
// publisher + a 1 Hz timer publishing an Int32 counter, with the C API.
// `RCLC_C_COMPONENT` emits the C-ABI factory/configure the typed Entry calls;
// it runs on the real executor — no declarative descriptor, no interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rcl/rcl.h>
#include <rclc/executor.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

typedef struct {{
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
}} {state_ty};

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {{
    {state_ty}* self = RCLC_CONTAINER_OF(timer, {state_ty}, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {{.data = self->count}};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {{
        printf("Published: %d\n", (int)self->count);
    }}
    self->count++;
}}

static rcl_ret_t {configure_fn}(rcl_node_t* node, rclc_executor_t* executor, {state_ty}* self) {{
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {{
        return rc;
    }}
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc != RCL_RET_OK) {{
        return rc;
    }}
    return rclc_executor_add_timer(executor, &self->timer);
}}

RCLC_C_COMPONENT({state_ty}, {configure_fn})
"#,
        name = cfg.name,
    );
    fs::write(dir.join(format!("src/{class_name}.c")), source)?;

    println!("✓ Created nano-ros C Node pkg '{}'", cfg.name);
    println!("  Class     : {pkg_sym}::{class_name}");
    println!("  Node      : {node_name}");
    println!("  Kind      : typed component (RFC-0043)");
    println!();
    println!("Next steps:");
    println!("  cd {}", cfg.name);
    println!("  # Solo build:");
    println!("  cmake -S . -B build");
    println!("  #   (the `nros` on PATH finds its own SDK; to name one explicitly");
    println!("  #    add -DNROS_REPO_DIR=<path-to-nano-ros>)");
    println!("  cmake --build build");
    println!();
    println!("  # Or add it as a SUBDIR in a C++ or Rust Entry workspace.");

    Ok(())
}

/// Map `talker` → `Talker`, `service-server` → `ServiceServer`.
fn use_case_to_pascal(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Issue 0333 defect 2 — emit a *runnable* Rust starter in the current blessed
/// idiom, dispatched by platform kind. Deferred platforms are rejected earlier
/// in [`scaffold_package`], so only [`PlatformKind::Hosted`] and
/// [`PlatformKind::SelfBringup`] reach here.
fn scaffold_rust(name: &str, platform: &str, edition_feature: &str, dir: &Path) -> Result<()> {
    let spec = platform_spec(platform)?;
    match &spec.kind {
        PlatformKind::Hosted => scaffold_rust_hosted(name, dir),
        PlatformKind::SelfBringup { runtime } => {
            scaffold_rust_self_bringup(name, &spec, runtime, edition_feature, platform, dir)
        }
        // Rejected before any file is written (see `scaffold_package`).
        PlatformKind::Deferred { .. } => {
            unreachable!("Deferred platforms bail in scaffold_package")
        }
    }
}

/// The generated selection package a migrated leaf depends on (RFC-0098 D6,
/// issue 1305): `nros sync` writes it at `build/nros/selection`, naming the
/// board crate, the RMW backend and the ROS edition `system.toml` declares,
/// under the leaf's crate name. Issue 2079 — the scaffold names none of the
/// three itself, so the declaration is the only answer to "which".
fn selection_dep(name: &str) -> String {
    format!(
        "{}_nros_selection = {{ path = \"build/nros/selection\" }}",
        name.replace('-', "_")
    )
}

/// Shared size/speed profiles + the `nros sync` note appended to every
/// generated `Cargo.toml`.
const CARGO_PROFILES: &str = r#"
# nano-ros crates are not published to crates.io (RFC-0040) — the `version = "*"`
# requirements above are patched, not resolved from crates.io. Run `nros sync`
# to write the nros-managed [patch.crates-io] block here (path deps into your
# nano-ros SDK + any generated msg crates), then build. `nros` finds the SDK
# itself; `--nano-ros-path` names one explicitly.

# Named size/speed profiles (Phase 204.15): `cargo build --profile size|speed`.
[profile.size]
inherits = "release"
opt-level = "s"
lto = "fat"
codegen-units = 1
strip = true

[profile.speed]
inherits = "release"
opt-level = 3
lto = "fat"
codegen-units = 1
"#;

/// Hosted (linux) — the shape of `examples/linux/rust/talker` (issue 2079):
/// `lib.rs` is the node, `main.rs` is `nros::main!(spin = "forever")`, and the
/// board, RMW and ROS edition reach the build through the selection package,
/// so the manifest names none of them.
fn scaffold_rust_hosted(name: &str, dir: &Path) -> Result<()> {
    let selection = selection_dep(name);
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "{name}"
path = "src/main.rs"

# `lib.rs` IS the node (`nros::node!(Talker)` emits `register`); `main.rs` is
# the one-line `nros::main!()` entry that boots it.
[lib]
path = "src/lib.rs"
crate-type = ["rlib"]

# The board, RMW, ROS edition and node live in `system.toml` beside this
# manifest (RFC-0098 D3/D5) — nothing here names a board, an RMW or an edition.
# They reach the build through the generated selection package below, which
# `nros sync` writes (RFC-0098 D6, issue 1305).

[dependencies]
nros = {{ version = "*", default-features = false, features = ["alloc", "env", "rmw-cffi", "macros"] }}
nros-log = {{ version = "*", default-features = false }}
{selection}
# Message crates come from package.xml: each <depend> on a message package
# becomes a `# nros-managed` path row below when you run `nros sync` (or
# `nros build`). Edit package.xml, not this table (RFC-0098, phase-517 W8).
{CARGO_PROFILES}"#
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    let main_rs = format!(
        r#"//! Generated by `nros new {name}` — the entry for `lib.rs`'s node.
//!
//! `nros::main!()` and the board own `nros::init`, the executor, RMW
//! registration and the spin loop. `spin = "forever"`: without it the hosted
//! main runs a bounded spin and exits at once (issue 0274).

nros::main!(spin = "forever");
"#
    );
    fs::write(dir.join("src/main.rs"), main_rs)?;
    fs::write(dir.join("src/lib.rs"), talker_node_lib(name))?;
    Ok(())
}

/// Self-bringup (baremetal/esp32) — `main.rs` is the one-line `nros::main!()`
/// Form-1 entry, `lib.rs` is the node. The macro reads the board from the
/// `system.toml` `scaffold_package` writes beside it. Models the
/// single-package `examples/mps2-an385-baremetal/rust/talker` / esp32 talker.
fn scaffold_rust_self_bringup(
    name: &str,
    spec: &PlatformSpec,
    runtime: &SelfBringupRuntime,
    edition_feature: &str,
    platform: &str,
    dir: &Path,
) -> Result<()> {
    let board_crate = spec.board_crate;
    let deploy = spec.deploy_token;

    // Per-runtime dependency + entry deltas.
    let (runtime_deps, main_rs) = match runtime {
        // Issue 2079 — the shape of `examples/mps2-an385-baremetal/rust/talker`:
        // the selection package brings the board (with its entry, runtime and
        // panic provider), the RMW backend and the ROS edition `system.toml`
        // declares, so nothing here names any of them.
        SelfBringupRuntime::CortexM => (
            format!("{}\n", selection_dep(name)),
            // The macro emits `#[cortex_m_rt::entry]` for this deploy.
            // `panic = "own"`: the handler is the board's crate-root provider
            // (`panic-semihosting`), which the selection package links; the
            // macro's default (`panic_to_platform!`) would define a second
            // `panic_impl` (E0152, issue 1265).
            "#![no_std]\n#![no_main]\nnros::main!(panic = \"own\");\n".to_string(),
        ),
        SelfBringupRuntime::Esp32 => (
            // The esp32 board crate's default features own the RMW backend.
            format!(
                r#"{board_crate} = {{ version = "*" }}
esp-hal = {{ version = "~1.0.0", features = ["esp32c3", "unstable"] }}
esp-backtrace = {{ version = "~0.18.0", features = ["esp32c3", "panic-handler", "println"] }}
"#
            ),
            // esp-backtrace is the panic handler (so `panic = "own"`, as above);
            // the macro emits `#[esp_hal::main]`.
            //
            // The app descriptor comes from the BOARD's `esp_bootloader_esp_idf`
            // re-export, as in `examples/esp32-c3-baremetal/rust/talker`:
            // `esp_hal` has no `esp_app_desc!`, so the template's old
            // `esp_hal::esp_app_desc!()` never compiled (E0433, found when issue
            // 1265 first cross-built the scaffold).
            format!(
                "#![no_std]\n#![no_main]\nuse esp_backtrace as _;\n{}::esp_bootloader_esp_idf::esp_app_desc!();\nnros::main!(panic = \"own\");\n",
                board_crate.replace('-', "_")
            ),
        ),
    };

    // Only the esp32 template is unmigrated (it names its board crate by hand
    // and has no selection package), so its manifest pin is the edition's one
    // carrier; a migrated leaf's pin would fight the selection's (issue 2045).
    let edition_pin = match runtime {
        SelfBringupRuntime::CortexM => String::new(),
        SelfBringupRuntime::Esp32 => format!(", \"{edition_feature}\""),
    };
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "{name}"
path = "src/main.rs"
test = false
bench = false

# Self-bringup shape (issue 0100): `lib.rs` IS the node (`nros::node!(Talker)`
# emits `register`); `main.rs` is the one-line `nros::main!()` Form-1 entry that
# dispatches to this same crate's `register`.
[lib]
path = "src/lib.rs"
crate-type = ["rlib"]

# `nros::main!()` resolves the board from `system.toml` beside this manifest
# (`[image.*] board = "{deploy}"`, RFC-0098 D3).

[dependencies]
# `macros` because `lib.rs` calls `nros::node!` itself: inheriting the feature
# from the board crate works only where the board is built, and the host probe
# never builds it.
nros = {{ version = "*", default-features = false, features = ["alloc", "rmw-cffi", "macros"{edition_pin}] }}
nros-log = {{ version = "*", default-features = false }}
# Message crates come from package.xml: each <depend> on a message package
# becomes a `# nros-managed` path row below when you run `nros sync` (or
# `nros build`). Edit package.xml, not this table (RFC-0098, phase-517 W8).

# What the board needs to link + boot, and nothing the node in `lib.rs` uses.
# TARGET-scoped so that `lib.rs` also builds for the host: `nros sync` probes
# what the node creates by compiling it there, and these crates do not build
# for the host (issue 1265). `main.rs` is never built for the host.
[target.'cfg(target_os = "none")'.dependencies]
{runtime_deps}{CARGO_PROFILES}"#
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(dir.join("src/main.rs"), main_rs)?;
    fs::write(dir.join("src/lib.rs"), talker_node_lib(name))?;

    write_default_config_toml(dir)?;
    write_cargo_config(dir, platform)?;
    Ok(())
}

/// The shared `lib.rs` node for every Rust starter — a `std_msgs/String`
/// talker (`Node` + `ExecutableNode` + `nros::node!`), copied from the tracked
/// `examples/mps2-an385-baremetal/rust/talker/src/lib.rs`. RMW/platform-agnostic:
/// the same node compiles under every board, hosted included (issue 2079).
fn talker_node_lib(name: &str) -> String {
    format!(
        r#"//! Declarative talker node — RMW/platform-agnostic application logic.
//!
//! `nros::node!(Talker)` emits the `register` fn that `nros::main!()` (in
//! `main.rs`) dispatches to. The boot scaffold (reset -> BoardEntry -> executor
//! -> spin) is owned by `nros::main!()` + the board crate; none of it is here.
//!
//! Publishes `std_msgs/String` ("Hello World: N") on `/chatter` once a second.

#![no_std]

use core::fmt::Write as _;

use nros::{{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Component, NodeContext, DeclarativeNode, NodeResult,
    TickCtx, TimerDuration,
}};
use nros_log::{{Logger, log_error, log_info}};
use std_msgs::msg::String as StringMsg;

static LOGGER: Logger = Logger::new("{name}");

pub struct Talker;

impl Component for Talker {{
    const NAME: &'static str = "{name}";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {{
        nros_log::register_logger(&LOGGER);
        let mut node = ctx.create_node(nros::NodeOptions::new("{name}"))?;
        node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        Ok(())
    }}
}}

impl ExecutableNode for Talker {{
    type State = i32;

    fn init() -> Self::State {{
        0
    }}

    fn on_callback(state: &mut i32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {{
        if callback.as_str() == "on_tick" {{
            *state = state.wrapping_add(1);
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {{}}", *state);
            match ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg) {{
                Ok(()) => log_info!(&LOGGER, "Publishing: '{{}}'", msg.data),
                Err(e) => log_error!(&LOGGER, "Publish failed: {{:?}}", e),
            }}
        }}
    }}

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {{}}
}}

nros::node!(Talker);
"#
    )
}

/// Scaffold `.cargo/config.toml` for the cortex-m cargo-built platforms
/// (bare-metal / FreeRTOS on the QEMU mps2-an385). Carries the Phase 204
/// size knobs by default: `--gc-sections` at link (204.8) plus a documented,
/// commented block for the serial-only IP-stack opt-out (204.7) and the
/// per-backend static-heap size (204.5). Other embedded platforms (Zephyr,
/// NuttX, ESP-IDF, ThreadX) build through their own toolchains and don't use
/// a cargo target triple here, so they get no `.cargo/config.toml`.
fn write_cargo_config(dir: &Path, platform: &str) -> Result<()> {
    let triple = match platform {
        "baremetal" | "freertos" => "thumbv7m-none-eabi",
        // Non-cargo-target build flows — leave the build config to the
        // platform's own toolchain integration.
        _ => return Ok(()),
    };

    let config = format!(
        r#"[build]
target = "{triple}"

[target.{triple}]
# QEMU mps2-an385 (cortex-m3) + semihosting. Adjust `-machine`/`-cpu` for your board.
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -semihosting-config enable=on,target=native -kernel"
rustflags = [
    # Phase 204.8 — drop unreferenced fns/data at link. `rust-lld` is invoked
    # directly (no gcc driver), so the bare `--gc-sections`, NOT `-Wl,...`.
    # `cortex-m-rt`'s `link.x` KEEPs the vector table, so gc is safe.
    "-C", "link-arg=--gc-sections",
    "-C", "link-arg=-Tlink.x",
]

[env]
# Phase 204.7 — serial-only node: uncomment to drop the IP link layer
# (zenoh-pico TCP/UDP link C via `Z_FEATURE_LINK_TCP/UDP=0`; `--gc-sections`
# above then strips the smoltcp residue). Also switch the board to its
# `serial` feature and use a serial `locator` in `nros.toml`.
# NROS_LINK_IP = "0"
# ZPICO_NO_SMOLTCP = "1"
#
# Phase 204.5 — size the static heap to the backend's working set
# (zenoh-pico ~24 KB, XRCE ~8 KB); default is the per-board value (64 KB on
# mps2-an385). Decimal bytes.
# NROS_HEAP_SIZE = "24576"
#
# Phase 204.2 — a brokered (zenoh/XRCE) client multiplexes over one session;
# drop the spare smoltcp socket buffers (sized for DDS RTPS by default).
# NROS_SMOLTCP_MAX_SOCKETS = "1"
# NROS_SMOLTCP_MAX_UDP_SOCKETS = "1"
"#
    );

    let cargo_dir = dir.join(".cargo");
    fs::create_dir_all(&cargo_dir)?;
    fs::write(cargo_dir.join("config.toml"), config)?;
    Ok(())
}

fn scaffold_c(name: &str, platform: &str, rmw_value: &str, dir: &Path) -> Result<()> {
    let _ = rmw_value;
    if platform == "zephyr" {
        return scaffold_zephyr(name, Language::C, dir);
    }
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
project({name} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# RFC-0048 — ament shape. Board + RMW live in the system.toml beside this
# file (RFC-0098 D3); find_package(nano_ros) reads it.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable({name} src/main.c)
ament_target_dependencies({name} std_msgs)

install(TARGETS {name} DESTINATION lib/${{PROJECT_NAME}})
ament_package()
"#,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_c = format!(
        r#"// Generated by `nros new {name} --lang c`.
//
// Minimal nano-ros C talker — publishes one `std_msgs/Int32` message on
// `/chatter`, then returns. Swap the body for your own logic; see
// `examples/linux/c/talker/src/main.c` for a fuller shape (timer,
// executor, signal handler).

#include <stdio.h>

#include <rcl/rcl.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

int main(int argc, char** argv) {{
    rcl_allocator_t allocator = rcl_get_default_allocator();
    rclc_support_t support = rclc_support_get_zero_initialized();
    if (rclc_support_init(&support, argc, (const char* const*)argv, &allocator) != RCL_RET_OK) {{
        fprintf(stderr, "rclc_support_init failed\n");
        return 1;
    }}

    rcl_node_t node = rcl_get_zero_initialized_node();
    if (rclc_node_init_default(&node, "{name}", "/", &support) != RCL_RET_OK) {{
        fprintf(stderr, "rclc_node_init_default failed\n");
        return 1;
    }}

    rcl_publisher_t pub = rcl_get_zero_initialized_publisher();
    if (rclc_publisher_init_default(&pub, &node,
                                    ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
                                    "/chatter") != RCL_RET_OK) {{
        fprintf(stderr, "rclc_publisher_init_default failed\n");
        return 1;
    }}

    std_msgs__msg__Int32 msg = {{0}};
    msg.data = 0;
    (void)rcl_publish(&pub, &msg, NULL);
    printf("{name}: published 0 on /chatter\n");

    rcl_publisher_fini(&pub, &node);
    rcl_node_fini(&node);
    rclc_support_fini(&support);
    return 0;
}}
"#,
    );
    fs::write(dir.join("src/main.c"), main_c)?;

    if needs_scaffolded_nros_toml(platform) {
        write_default_config_toml(dir)?;
    }
    Ok(())
}

fn scaffold_cpp(name: &str, platform: &str, rmw_value: &str, dir: &Path) -> Result<()> {
    let _ = rmw_value;
    if platform == "zephyr" {
        return scaffold_zephyr(name, Language::Cpp, dir);
    }
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
project({name} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# RFC-0048 — ament shape. Board + RMW live in the system.toml beside this
# file (RFC-0098 D3); find_package(nano_ros) reads it.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable({name} src/main.cpp)
ament_target_dependencies({name} std_msgs)

install(TARGETS {name} DESTINATION lib/${{PROJECT_NAME}})
ament_package()
"#,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_cpp = format!(
        r#"// Generated by `nros new {name} --lang cpp`.
//
// Minimal nano-ros C++ talker — publishes one `std_msgs/Int32` message on
// `/chatter`, then returns. Swap the body for your own logic; see
// `examples/linux/cpp/talker/src/main.cpp` for a fuller shape (timer,
// executor, signal handler).

#include <cstdio>

#include <rclcpp/rclcpp.hpp>
#include "std_msgs.hpp"

int main(int argc, char** argv) {{
    (void)argc;
    (void)argv;

    if (auto r = rclcpp::init_in(); !r.ok()) {{
        std::fprintf(stderr, "rclcpp::init failed: %d\n", r.raw());
        return 1;
    }}

    rclcpp::Node node;
    if (auto r = rclcpp::create_node(node, "{name}"); !r.ok()) {{
        std::fprintf(stderr, "create_node failed: %d\n", r.raw());
        return 1;
    }}

    rclcpp::Publisher<std_msgs::msg::Int32> pub;
    if (auto r = node.create_publisher(pub, "/chatter"); !r.ok()) {{
        std::fprintf(stderr, "create_publisher failed: %d\n", r.raw());
        return 1;
    }}

    std_msgs::msg::Int32 msg;
    msg.data = 0;
    (void)pub.publish(msg);
    std::printf("{name}: published 0 on /chatter\n");

    rclcpp::shutdown();
    return 0;
}}
"#,
    );
    fs::write(dir.join("src/main.cpp"), main_cpp)?;

    if needs_scaffolded_nros_toml(platform) {
        write_default_config_toml(dir)?;
    }
    Ok(())
}

/// phase-517 — a Zephyr C/C++ project is a stock Zephyr application
/// (`book/src/getting-started/integration-zephyr.md` § Project layout), never
/// the host-shaped `main()` the other platforms get: `find_package(Zephyr)`
/// FIRST (it loads the nano-ros module, which defines the `nano_ros` package
/// `find_package(nano_ros)` then reads), a TYPED component the module's
/// carrier boots by identity, and a `prj.conf` holding Zephyr's own settings
/// plus `CONFIG_NROS=y`. The host shape configured as a plain CMake project
/// and refused with `nros_feature_set: unknown PLATFORM 'zephyr'`.
///
/// The board, RMW and component are the `system.toml` beside it; the network
/// is Kconfig, so no `nros.toml` is written.
fn scaffold_zephyr(name: &str, lang: Language, dir: &Path) -> Result<()> {
    let ident = name.replace('-', "_");
    let (src, register) = match lang {
        Language::Cpp => (
            "src/Talker.cpp",
            format!(
                "nros_components_register_node(talker_lib\n    HEADER Talker.hpp\n    \
                 PLUGIN {ident}::Talker\n    EXECUTABLE talker\n    SHAPE configure\n    TYPED)\n"
            ),
        ),
        _ => (
            "src/Talker.c",
            format!(
                "nros_components_register_node(talker_lib\n    PLUGIN {ident}::Talker\n    \
                 EXECUTABLE talker\n    SHAPE configure\n    TYPED)\n"
            ),
        ),
    };
    let cmake = format!(
        r#"# A stock Zephyr application (book: getting-started/integration-zephyr.md).
# `find_package(Zephyr)` loads the nano-ros module (named on the `west build`
# line: -DZEPHYR_EXTRA_MODULES="$(nros sdk-root)"); CONFIG_NROS=y in prj.conf
# switches it on. Board, RMW and the component live in system.toml.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})
project({name})

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# A TYPED component: the module's Zephyr carrier generates the entry and calls
# the component's `configure` — no hand-written main().
nano_ros_auto_add_library(talker_lib STATIC {src})

{register}"#
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let prj = r#"# Zephyr's own settings. nano-ros's half (RMW, endpoint, domain, knobs) is
# system.toml's: the module renders it into a fragment merged AFTER this file,
# so a CONFIG_NROS_* line here would be dead.

CONFIG_LOG=y
CONFIG_PRINTK=y

# Networking (any networked RMW on QEMU / native_sim)
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_UDP=y
CONFIG_NET_TCP=y
CONFIG_NET_SOCKETS=y
CONFIG_NET_L2_ETHERNET=y
CONFIG_NET_MAX_CONTEXTS=16
CONFIG_DNS_RESOLVER=n
CONFIG_SYS_CLOCK_TICKS_PER_SEC=1000

# nano-ros: enable the module.
CONFIG_NROS=y
# The typed carrier entry is C++17, for a C component too.
CONFIG_STD_CPP17=y

# zenoh-pico needs POSIX threads and a roomy heap/stack.
CONFIG_POSIX_API=y
CONFIG_MAX_PTHREAD_MUTEX_COUNT=64
CONFIG_MAX_PTHREAD_COND_COUNT=16
CONFIG_POSIX_THREAD_THREADS_MAX=16
CONFIG_MAIN_STACK_SIZE=16384
CONFIG_HEAP_MEM_POOL_SIZE=65536
CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=4096
CONFIG_NET_PKT_RX_COUNT=32
CONFIG_NET_PKT_TX_COUNT=32
CONFIG_NET_BUF_RX_COUNT=64
CONFIG_NET_BUF_TX_COUNT=64
"#;
    fs::write(dir.join("prj.conf"), prj)?;

    match lang {
        Language::Cpp => {
            let hpp = format!(
                r#"// Generated by `nros new {name} --platform zephyr --lang cpp`.
// A typed component: the Zephyr carrier constructs it and calls configure().
#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace {ident} {{

class Talker {{
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
}};

}} // namespace {ident}
"#
            );
            fs::write(dir.join("src/Talker.hpp"), hpp)?;
            let cpp = format!(
                r#"// Generated by `nros new {name} --platform zephyr --lang cpp`.
// Publishes an Int32 counter on /chatter every 500 ms.
#include "Talker.hpp"

#include <cstdio>

namespace {ident} {{

void Talker::on_tick() {{
    std_msgs::msg::Int32 m;
    m.data = count_++;
    if (pub_.publish(m).ok()) {{
        std::printf("Published: %d\n", (int)m.data);
    }}
}}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {{
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 500, this);
}}

}} // namespace {ident}
"#
            );
            fs::write(dir.join("src/Talker.cpp"), cpp)?;
        }
        _ => {
            let c = format!(
                r#"// Generated by `nros new {name} --platform zephyr --lang c`.
// A typed C component: `talker_configure` creates a publisher on /chatter and
// a timer that publishes an Int32 counter each tick. `RCLC_C_COMPONENT` emits
// the C-ABI factory/configure the Zephyr typed carrier calls.
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

typedef struct {{
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
}} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {{
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {{.data = self->count}};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {{
        printf("Published: %d\n", (int)self->count);
    }}
    self->count++;
}}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {{
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {{
        return rc;
    }}
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 500000000ULL, on_tick);
    if (rc != RCL_RET_OK) {{
        return rc;
    }}
    return rclc_executor_add_timer(executor, &self->timer);
}}

RCLC_C_COMPONENT(talker_t, talker_configure)
"#
            );
            fs::write(dir.join("src/Talker.c"), c)?;
        }
    }
    Ok(())
}

/// Does this platform need a scaffolded `nros.toml`?
///
/// Only a NON-hosted target does: the file it writes is a static QEMU-slirp
/// network config (`ip`, `mac`, `gateway`, `locator`), which a host build has
/// no use for — the host kernel owns the stack.
///
/// Issue 0916: this was `platform != "native"`, a literal comparison against
/// ONE of the host spellings of the day, so `--platform posix` got an embedded
/// network config and another spelling did not — two spellings of one
/// platform producing different trees.
///
/// Asking the KIND is the fix, not comparing names: the question being asked
/// was never about the name (phase-500 W5 left one host spelling, `linux`).
fn needs_scaffolded_nros_toml(platform: &str) -> bool {
    !matches!(
        platform_spec(platform).map(|s| s.kind),
        Ok(PlatformKind::Hosted)
    )
}

fn write_default_config_toml(dir: &Path) -> Result<()> {
    // Issue 2047 — the direct-mode board network file (RFC-0109 D4): the node,
    // and one interface on a network. RFC-0004's `[[transport]]` is retired.
    let nros_toml = r#"# nano-ros config (direct mode): the board package's network file.
# See docs/design/0109-network-config-layers.md (D4).

# CONFIGURE ME: these defaults target QEMU user-mode networking (the built-in
# `qemu` network: 10.0.2.0/24, gateway and router at 10.0.2.2). For your own
# network add `[network.<name>]` with `subnet` and `gateway`, and name it.
[node]
domain_id = 0
locator = "tcp/10.0.2.2:7447"

[eth0]
network = "qemu"
host = 20
mac = "02:00:00:00:00:00"
"#;
    fs::write(dir.join("nros.toml"), nros_toml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("src")).unwrap();
        d
    }

    /// Issue 2079 — the hosted starter is the migrated shape: the selection
    /// package and no board crate, RMW backend or edition feature, with the
    /// node in `lib.rs` and a one-line `nros::main!()` entry.
    #[test]
    fn rust_scaffold_hosted_names_no_board_rmw_or_edition() {
        let d = tmp();
        scaffold_rust("my-app", "linux", "ros-jazzy", d.path()).unwrap();
        let toml = fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
        let doc: toml::Value = toml.parse().expect("scaffolded manifest parses");
        let deps = doc["dependencies"].as_table().unwrap();
        assert_eq!(
            deps["my_app_nros_selection"]["path"].as_str(),
            Some("build/nros/selection"),
            "{toml}"
        );
        for name in deps.keys() {
            assert!(
                !name.starts_with("nros-board-") && !name.starts_with("nros-rmw-"),
                "the selection package owns `{name}`:\n{toml}"
            );
        }
        assert!(
            !toml.contains("ros-jazzy") && !toml.contains("ros-humble"),
            "{toml}"
        );
        assert!(
            doc.get("features").is_none(),
            "no RMW feature table:\n{toml}"
        );
        let main = fs::read_to_string(d.path().join("src/main.rs")).unwrap();
        assert!(main.contains("nros::main!(spin = \"forever\");"), "{main}");
        let lib = fs::read_to_string(d.path().join("src/lib.rs")).unwrap();
        assert!(lib.contains("nros::node!(Talker)"), "{lib}");
    }

    #[test]
    fn c_and_cpp_scaffold_emit_ament_shape() {
        // RFC-0048 — the CMakeLists is the ament shape; the RMW lives in the
        // system.toml `scaffold_package` writes (RFC-0098 D3), so it does not
        // appear in the CMakeLists.
        let dc = tmp();
        scaffold_c("bar", "linux", "cyclonedds", dc.path()).unwrap();
        let cm = fs::read_to_string(dc.path().join("CMakeLists.txt")).unwrap();
        assert!(
            cm.contains("find_package(nano_ros REQUIRED)"),
            "expected ament shape in:\n{cm}"
        );
        assert!(
            cm.contains("nano_ros_add_executable(bar src/main.c)"),
            "{cm}"
        );
        assert!(
            !cm.contains("nano_ros_entry"),
            "stray old-shape verb:\n{cm}"
        );
        assert!(
            !cm.contains("set(NROS_RMW"),
            "RMW should be in package.xml:\n{cm}"
        );

        let dpp = tmp();
        scaffold_cpp("baz", "linux", "xrce", dpp.path()).unwrap();
        let cm = fs::read_to_string(dpp.path().join("CMakeLists.txt")).unwrap();
        assert!(
            cm.contains("nano_ros_add_executable(baz src/main.cpp)"),
            "{cm}"
        );
        // Both starters include the API at upstream's paths.
        let c_src = fs::read_to_string(dc.path().join("src/main.c")).unwrap();
        assert!(c_src.contains("#include <rcl/rcl.h>") && c_src.contains("#include <rclc/rclc.h>"));
        let cpp_src = fs::read_to_string(dpp.path().join("src/main.cpp")).unwrap();
        assert!(
            cpp_src.contains("#include <rclcpp/rclcpp.hpp>"),
            "{cpp_src}"
        );
        assert!(!cpp_src.contains("#include <nros/"), "{cpp_src}");
    }

    #[test]
    fn scaffold_package_rejects_unknown_rmw() {
        // `resolve_rmw` fails before any filesystem write, so no package dir is
        // created — the unique name keeps this safe without touching cwd.
        let cfg = ScaffoldConfig {
            name: "pkg_unknown_rmw_227_4_fixture".to_string(),
            lang: Language::Rust,
            platform: "linux".to_string(),
            rmw: "dust-dds".to_string(),
            ros_edition: "humble".to_string(),
            use_case: "talker".to_string(),
            force: false,
            board: None,
        };
        let err = scaffold_package(&cfg).expect_err("unknown rmw must fail");
        assert!(err.to_string().contains("dust-dds"), "{err}");
        assert!(
            !PathBuf::from(&cfg.name).exists(),
            "no package dir on rejected rmw"
        );
    }

    /// phase-445 W3b — a scaffolded package states its deployment in
    /// system.toml: the self-bringup names the board `nros::main!()` resolves
    /// and the node it boots; C/C++ name the platform token the retired
    /// `deploy=` carried; every one parses as the typed schema requires
    /// (`[system] rmw` and `domain_id` are mandatory).
    #[test]
    fn scaffold_system_toml_names_board_rmw_and_node() {
        let bm = scaffold_system_toml(
            "my-app",
            Language::Rust,
            "baremetal",
            &platform_spec("baremetal").unwrap(),
            "zenoh",
            "jazzy",
            None,
        );
        let doc: toml::Table = bm.parse().expect("parses");
        assert_eq!(doc["system"]["rmw"].as_str(), Some("zenoh"));
        // Issue 2079 — the edition is declared, for the toolchain to lower.
        assert_eq!(doc["system"]["ros_edition"].as_str(), Some("jazzy"));
        assert_eq!(doc["system"]["domain_id"].as_integer(), Some(0));
        assert_eq!(
            doc["image"]["baremetal"]["board"].as_str(),
            Some("mps2-an385")
        );
        let c = &doc["component"].as_array().unwrap()[0];
        assert_eq!(c["class"].as_str(), Some("my_app::Talker"));
        assert_eq!(c["dispatch"].as_str(), Some("deferred"));

        let cf = scaffold_system_toml(
            "c_app",
            Language::C,
            "freertos",
            &platform_spec("freertos").unwrap(),
            "xrce",
            "humble",
            Some("mps2-an385-freertos"),
        );
        let doc: toml::Table = cf.parse().expect("parses");
        assert_eq!(doc["system"]["ros_edition"].as_str(), Some("humble"));
        // phase-517 W2 — the caller's resolved board, under the platform's id.
        assert_eq!(
            doc["image"]["freertos"]["board"].as_str(),
            Some("mps2-an385-freertos")
        );
        assert_eq!(doc["system"]["rmw"].as_str(), Some("xrce"));
        assert!(
            doc.get("component").is_none(),
            "a plain executable declares no node"
        );

        // The hosted Rust starter boots a node too, with no deferred dispatch.
        let h = scaffold_system_toml(
            "h-app",
            Language::Rust,
            "linux",
            &platform_spec("linux").unwrap(),
            "zenoh",
            "humble",
            None,
        );
        let doc: toml::Table = h.parse().expect("parses");
        let c = &doc["component"].as_array().unwrap()[0];
        assert_eq!(c["class"].as_str(), Some("h_app::Talker"));
        assert!(c.get("dispatch").is_none(), "{h}");
    }

    // Issue 0333 — every platform the CLI advertises must resolve; an unknown
    // one bails.
    #[test]
    fn platform_spec_covers_every_advertised_platform() {
        for p in [
            "linux",
            "freertos",
            "baremetal",
            "nuttx",
            "threadx",
            "zephyr",
            // `esp32` is dormant (issue 1525) and no longer advertised; its spec
            // still resolves, which `scaffold_rust_self_bringup_*` exercises.
            "esp32",
        ] {
            platform_spec(p).unwrap_or_else(|e| panic!("{p} must resolve: {e}"));
        }
        // linux is the one hosted spec, against nros-board-linux, selecting
        // the host board as `linux` (phase-500 W1). Its retired aliases
        // `native`/`posix` (W5) resolve to no spec at all.
        let spec = platform_spec("linux").unwrap();
        assert!(matches!(spec.kind, PlatformKind::Hosted));
        assert_eq!(spec.board_crate, "nros-board-linux");
        assert_eq!(spec.deploy_token, "linux");
        for p in ["native", "posix"] {
            assert!(platform_spec(p).is_err(), "{p} must not scaffold");
        }
        assert!(matches!(
            platform_spec("baremetal").unwrap().kind,
            PlatformKind::SelfBringup { .. }
        ));
        assert!(matches!(
            platform_spec("threadx").unwrap().kind,
            PlatformKind::Deferred { .. }
        ));
        assert!(platform_spec("bogus").is_err());
    }

    // Issue 0333 defect 2 — the self-bringup shape writes a real board dep, a
    // `[lib]`, a `nros::node!` lib.rs, and a one-line `nros::main!()` entry —
    // never the retired commented-out dep or `no_mangle` stub.
    #[test]
    fn scaffold_rust_self_bringup_emits_node_lib_and_entry() {
        // (platform, the target-scoped image dep, whether it pins the edition)
        for (platform, image_dep, pins) in [
            // Issue 2079 — migrated: the selection package, no edition pin.
            ("baremetal", "app_nros_selection", false),
            // Dormant and unmigrated (issue 1525): its pin is its one carrier.
            ("esp32", "nros-board-esp32-qemu", true),
        ] {
            let d = tmp();
            scaffold_rust("app", platform, "ros-jazzy", d.path()).unwrap();
            let toml = fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
            assert!(
                !toml.contains("# TODO: add board crate"),
                "{platform}:\n{toml}"
            );
            assert!(toml.contains("[lib]"), "{platform}: missing [lib]:\n{toml}");
            // Issue 1265 -- the board (and every other image-only crate) is
            // TARGET-scoped, so the node lib host-builds and `nros sync` can
            // probe it. A board in `[dependencies]` is what made the scaffold's
            // probe die on `bkpt` fed to an x86 assembler.
            let doc: toml::Value = toml.parse().expect("scaffolded manifest parses");
            assert!(
                doc["dependencies"].get(image_dep).is_none(),
                "{platform}: `{image_dep}` must not be a host-visible dep:\n{toml}"
            );
            assert!(
                doc["target"]["cfg(target_os = \"none\")"]["dependencies"]
                    .get(image_dep)
                    .is_some(),
                "{platform}: `{image_dep}` must be target-scoped:\n{toml}"
            );
            assert_eq!(toml.contains("\"ros-jazzy\""), pins, "{platform}:\n{toml}");
            for host_dep in ["nros", "nros-log"] {
                assert!(
                    doc["dependencies"].get(host_dep).is_some(),
                    "{platform}: `{host_dep}` is what lib.rs uses and must stay host-visible:\n{toml}"
                );
            }
            // phase-517 W8 — the message crate is NOT written by the scaffold:
            // `<depend>std_msgs</depend>` is the one statement, and `nros sync`
            // turns it into a host-visible `# nros-managed` row.
            assert!(
                doc["dependencies"].get("std_msgs").is_none(),
                "{platform}: the message dep is package.xml's alone:\n{toml}"
            );
            // phase-445 W3b — the board lives in system.toml, never the manifest.
            assert!(
                !toml.contains("[package.metadata.nros.entry]")
                    && !toml.contains("[package.metadata.nros.node]"),
                "{platform}: retired deployment table in the manifest:\n{toml}"
            );
            let main = fs::read_to_string(d.path().join("src/main.rs")).unwrap();
            assert!(
                main.contains("nros::main!(panic = \"own\");"),
                "{platform}:\n{main}"
            );
            assert!(
                !main.contains("no_mangle"),
                "{platform}: retired stub:\n{main}"
            );
            let lib = fs::read_to_string(d.path().join("src/lib.rs")).unwrap();
            assert!(
                lib.contains("nros::node!(Talker)"),
                "{platform}: node lib missing:\n{lib}"
            );
        }
    }

    // Issue 0333 defect 2 — split-package (freertos/nuttx/threadx) and west
    // (zephyr) platforms have no single-package template yet; `nros new` must
    // bail before writing any file, not emit a project that cannot run.
    #[test]
    fn scaffold_package_rust_defers_split_and_zephyr_platforms() {
        for (i, platform) in ["freertos", "nuttx", "threadx", "zephyr"]
            .into_iter()
            .enumerate()
        {
            let cfg = ScaffoldConfig {
                name: format!("pkg_deferred_0333_fixture_{i}"),
                lang: Language::Rust,
                platform: platform.to_string(),
                rmw: "zenoh".to_string(),
                ros_edition: "humble".to_string(),
                use_case: "talker".to_string(),
                force: false,
                board: None,
            };
            let err = scaffold_package(&cfg).expect_err("deferred platform must bail");
            assert!(
                err.to_string().contains("not available yet"),
                "{platform}: {err}"
            );
            assert!(
                !PathBuf::from(&cfg.name).exists(),
                "{platform}: no package dir on deferred platform"
            );
        }
    }
}

#[cfg(test)]
mod hosted_alias_tests {
    use super::*;

    /// The host spelling scaffolds no `nros.toml`.
    ///
    /// Issue 0916: the C and C++ scaffolds asked `platform != "native"`, a
    /// literal comparison against one of the host aliases of the day, so
    /// `--platform posix` got an `nros.toml` full of static QEMU-slirp network
    /// settings that a host build has no use for. phase-500 W5 retired those
    /// aliases; `linux` is the one host spelling.
    #[test]
    fn the_hosted_spelling_gets_no_nros_toml() {
        assert!(!needs_scaffolded_nros_toml("linux"));
    }

    /// …and a cross target still gets one, which is what the file is for.
    #[test]
    fn a_cross_target_still_gets_nros_toml() {
        assert!(needs_scaffolded_nros_toml("baremetal"));
        assert!(needs_scaffolded_nros_toml("freertos"));
    }

    /// An unknown platform is not silently treated as hosted. `platform_spec`
    /// errors for it, and erring toward writing the file means the scaffold
    /// output is inspectable rather than mysteriously missing a config.
    #[test]
    fn an_unknown_platform_is_not_assumed_hosted() {
        assert!(needs_scaffolded_nros_toml("no-such-platform"));
    }
}

/// phase-517 — what `nros new` writes and prints, against the book's flow.
#[cfg(test)]
mod scaffold_flow_tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    fn zephyr_cfg(name: &str, lang: Language) -> ScaffoldConfig {
        ScaffoldConfig {
            name: name.to_string(),
            lang,
            platform: "zephyr".to_string(),
            rmw: "zenoh".to_string(),
            ros_edition: "humble".to_string(),
            use_case: "talker".to_string(),
            force: false,
            board: Some("zephyr".to_string()),
        }
    }

    /// phase-517 — `nros new --platform zephyr --lang c|cpp` writes the stock
    /// Zephyr application `integration-zephyr.md` § Project layout shows (and
    /// builds with a plain `west build … -DZEPHYR_EXTRA_MODULES=…`), never the
    /// host `main()` shape, whose plain-CMake configure refused with
    /// `nros_feature_set: unknown PLATFORM 'zephyr'`.
    #[test]
    fn zephyr_c_and_cpp_scaffold_a_stock_zephyr_app() {
        let book =
            fs::read_to_string(repo_root().join("book/src/getting-started/integration-zephyr.md"))
                .unwrap();
        for (lang, src) in [
            (Language::C, "src/Talker.c"),
            (Language::Cpp, "src/Talker.cpp"),
        ] {
            let d = tempfile::tempdir().unwrap();
            let dir = d.path().join("my-zapp");
            scaffold_package_at(&dir, &zephyr_cfg("my-zapp", lang)).unwrap();
            let cm = fs::read_to_string(dir.join("CMakeLists.txt")).unwrap();
            // The book's lines, verbatim, and Zephyr FIRST: it loads the
            // module that `find_package(nano_ros)` then reads.
            for line in [
                "find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})",
                "find_package(nano_ros REQUIRED)",
                "find_package(std_msgs REQUIRED)",
            ] {
                assert!(cm.contains(line), "{lang}: `{line}` missing:\n{cm}");
                assert!(book.contains(line), "the book no longer shows `{line}`");
            }
            assert!(
                cm.find("find_package(Zephyr").unwrap() < cm.find("find_package(nano_ros").unwrap()
            );
            assert!(cm.contains(&format!(
                "nano_ros_auto_add_library(talker_lib STATIC {src})"
            )));
            assert!(
                cm.contains("PLUGIN my_zapp::Talker") && cm.contains("TYPED"),
                "{cm}"
            );
            assert!(!cm.contains("nano_ros_add_executable"), "host shape:\n{cm}");
            assert!(dir.join(src).is_file(), "{lang}: no {src}");
            assert!(!dir.join("src/main.c").exists() && !dir.join("src/main.cpp").exists());
            // Network config is Kconfig on Zephyr; nano-ros's half is system.toml.
            assert!(!dir.join("nros.toml").exists(), "{lang}: stray nros.toml");
            let prj = fs::read_to_string(dir.join("prj.conf")).unwrap();
            assert!(prj.lines().any(|l| l == "CONFIG_NROS=y"), "{prj}");
            assert!(
                !prj.lines().any(|l| l.starts_with("CONFIG_NROS_")),
                "a CONFIG_NROS_* knob in prj.conf is dead (phase-481):\n{prj}"
            );
            // system.toml names the component the CMake PLUGIN registers.
            let st: toml::Table = fs::read_to_string(dir.join("system.toml"))
                .unwrap()
                .parse()
                .unwrap();
            let comp = &st["component"].as_array().unwrap()[0];
            assert_eq!(comp["pkg"].as_str(), Some("my-zapp"));
            assert_eq!(comp["class"].as_str(), Some("my_zapp::Talker"));
            assert_eq!(st["image"]["zephyr"]["board"].as_str(), Some("zephyr"));
            if lang == Language::Cpp {
                let hpp = fs::read_to_string(dir.join("src/Talker.hpp")).unwrap();
                assert!(hpp.contains("namespace my_zapp"), "{hpp}");
                assert!(cm.contains("HEADER Talker.hpp"), "{cm}");
            } else {
                let c = fs::read_to_string(dir.join(src)).unwrap();
                assert!(
                    c.contains("RCLC_C_COMPONENT(talker_t, talker_configure)"),
                    "{c}"
                );
            }
        }
    }

    /// phase-517 — "Next steps" is the build flow the book documents for the
    /// language and platform: never `cargo build` for a C package, `nros sync`
    /// only where a Rust leaf needs it, and the module-naming `west build` for
    /// Zephyr C/C++ (the same `-DZEPHYR_EXTRA_MODULES` the book shows).
    #[test]
    fn next_steps_follow_the_books_flow_per_language_and_platform() {
        let book =
            fs::read_to_string(repo_root().join("book/src/getting-started/integration-zephyr.md"))
                .unwrap();
        let platforms = [
            "linux",
            "baremetal",
            "freertos",
            "nuttx",
            "threadx",
            "zephyr",
        ];
        for lang in [Language::Rust, Language::C, Language::Cpp] {
            for p in platforms {
                let steps = next_steps("pkg", lang, p);
                let all = steps.join("\n");
                assert!(!all.contains("cargo build"), "{lang}/{p}:\n{all}");
                assert!(!all.contains("cmake --build"), "{lang}/{p}:\n{all}");
                match (lang, p) {
                    (Language::Rust, _) => {
                        assert_eq!(steps[0], "cd pkg");
                        assert!(steps[1].starts_with("nros sync"), "{all}");
                        assert!(steps[2].starts_with("nros build"), "{all}");
                    }
                    (_, "zephyr") => {
                        let west = steps
                            .iter()
                            .find(|l| l.starts_with("west build"))
                            .unwrap_or_else(|| panic!("{lang}/zephyr: no west build:\n{all}"));
                        let module = "-DZEPHYR_EXTRA_MODULES=\"$(nros sdk-root)\"";
                        assert!(west.contains(module) && west.ends_with(module), "{west}");
                        assert!(
                            book.contains(module),
                            "the book no longer names the module so"
                        );
                        assert!(west.contains(" pkg "), "{west}");
                        assert!(!all.contains("nros sync"), "{all}");
                    }
                    _ => {
                        assert_eq!(steps[0], "cd pkg");
                        assert!(steps[1].starts_with("nros build"), "{all}");
                        assert!(!all.contains("nros sync"), "C/C++ has no sync step:\n{all}");
                    }
                }
            }
        }
    }
}
