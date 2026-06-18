//! Project scaffolder — promoted out of `main.rs` so `nros-cli` and any
//! other front-end can share it.
//!
//! v1 emits a colcon-compatible hello-world per `<lang> × <platform>`.
//! Use-case (`talker` / `listener` / `service` / `action`) and RMW-choice
//! diversification arrives once the `templates/` tree lands; until then
//! both fields are accepted for forward-compat but only surfaced in the
//! "Next steps" output.

use crate::rmw_resolver;
use eyre::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct ScaffoldConfig {
    pub name: String,
    pub lang: String,
    pub platform: String,
    pub rmw: String,
    pub use_case: String,
    pub force: bool,
}

pub fn scaffold_package(cfg: &ScaffoldConfig) -> Result<()> {
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

    // Phase 227.4 — validate + lower the declared RMW (RFC-0031). A bad
    // `--rmw` fails here with the known-list, not as a broken downstream build.
    let rmw = rmw_resolver::resolve_rmw(&cfg.rmw).map_err(|e| eyre::eyre!("nros new: {e}"))?;

    let build_type = format!("nros.{}.{}", cfg.lang, cfg.platform);

    fs::create_dir_all(dir.join("src"))?;

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros {platform} package</description>
  <maintainer email="TODO@todo.com">TODO</maintainer>
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>{build_type}</build_type>
  </export>
</package>
"#,
        name = cfg.name,
        platform = cfg.platform,
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    match cfg.lang.as_str() {
        "rust" => scaffold_rust(&cfg.name, &cfg.platform, rmw.cargo_feature, &dir)?,
        "c" => scaffold_c(&cfg.name, &cfg.platform, rmw.cmake_value, &dir)?,
        "cpp" => scaffold_cpp(&cfg.name, &cfg.platform, rmw.cmake_value, &dir)?,
        other => bail!("Unknown language: {other}. Use rust, c, or cpp."),
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
    println!("  cd {}", cfg.name);
    println!("  export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros source checkout");
    println!("  eval \"$(nros ws env)\"   # ROS + interface search path");
    println!("  nros ws sync          # codegen + write the [patch.crates-io] block (RFC-0040)");
    println!("  cargo build           # or: cmake --build build / west build / idf.py build");

    Ok(())
}

#[derive(Debug, Clone)]
pub struct ComponentScaffoldConfig {
    pub name: String,
    /// Node flavor: `talker` / `listener` / `service` / `action`. Template
    /// diversification is TODO — today every flavor emits the publisher+timer
    /// shape, named after the flavor.
    pub use_case: String,
    /// Source language. `rust` lands the planned-mode `nros::Component` shape;
    /// `c` / `cpp` land the typed component shape (RFC-0043):
    /// `nano_ros_workspace_pkg_guard()` + `nano_ros_node_register()` + a
    /// `configure(::nros::Node&)` (C++) / `NROS_C_COMPONENT` (C) seam.
    pub lang: String,
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
    match cfg.lang.as_str() {
        "rust" => scaffold_component_rust(cfg),
        "cpp" => scaffold_component_cpp(cfg),
        "c" => scaffold_component_c(cfg),
        other => bail!(
            "`nros new --component --lang {other}` is not supported. Use \
             `rust`, `c`, or `cpp`."
        ),
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

    let package_xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>{name} — nano-ros reusable component.</description>
  <maintainer email="TODO@todo.com">TODO</maintainer>
  <license>Apache-2.0</license>
</package>
"#,
        name = cfg.name,
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

# A reusable component is a library (rlib); the deployed system links it.
# nano-ros crates are not published to crates.io (RFC-0040) — `version = "*"` is
# only the patched left-hand side. Run `nros ws sync` (with NROS_REPO_DIR set) to
# write the nros-managed [patch.crates-io] block redirecting `nros` to your
# nano-ros checkout, then `cargo build`.
[dependencies]
nros = {{ version = "*", default-features = false }}
"#,
        name = cfg.name,
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = format!(
        r#"#![no_std]

//! `{name}` — a reusable nano-ros component (planned mode).
//!
//! `nros plan` and Entry codegen link this crate into a system and call
//! `{module}::Component::register`. `nros metadata --build` records its
//! declarations into `metadata/{module}.json`. Platform + RMW are chosen at
//! Entry-package build time, not here.

pub mod {module} {{
    use nros::{{
        CallbackId, CdrReader, CdrWriter, ComponentContext, ComponentResult, DeserError,
        Deserialize, EntityId, NodeId, NodeOptions, RosMessage, SerError, Serialize, TimerDuration,
    }};

    pub struct Component;

    impl nros::Component for Component {{
        const NAME: &'static str = "{module}";

        fn register(context: &mut ComponentContext<'_>) -> ComponentResult<()> {{
            let mut node =
                context.create_node(NodeId::new("node_{module}"), NodeOptions::new("{module}"))?;
            let _publisher =
                node.create_publisher::<StringMsg>(EntityId::new("pub_chatter"), "chatter")?;
            let _timer = node.create_timer(
                EntityId::new("timer_publish"),
                CallbackId::new("cb_timer"),
                TimerDuration::from_millis(100),
            )?;
            Ok(())
        }}
    }}

    /// Minimal hand-rolled `std_msgs/String` stand-in. Replace with a generated
    /// message type (`nros generate-rust`) for real payloads.
    pub struct StringMsg;
    impl Serialize for StringMsg {{
        fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {{
            Ok(())
        }}
    }}
    impl Deserialize for StringMsg {{
        fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {{
            Ok(Self)
        }}
    }}
    impl RosMessage for StringMsg {{
        const TYPE_NAME: &'static str = "std_msgs::msg::dds_::String_";
        const TYPE_HASH: &'static str = "std_msgs/String";
    }}
}}

// The planner links the component via the Rust type path above. To also expose
// the C / dynamic registration symbol (`__nros_component_register`), add:
//     nros::component!({module}::Component);
"#,
        name = cfg.name,
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
/// §212.L.9 cmake-fn surface (`nano_ros_node_register`) and a
/// `<UserClass>::configure(::nros::Node&)` real-callback body in the
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
    fs::create_dir_all(dir.join("include").join(&cfg.name.replace('-', "_")))?;

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
  <maintainer email="TODO@todo.com">TODO</maintainer>
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>cmake</build_type>
  </export>
</package>
"#,
        name = cfg.name,
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project({pkg_sym} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

{bootstrap}
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Typed component (RFC-0043). No add_executable, no `main()`; the linked
# Entry pkg's typed runtime constructs this class + calls `configure(node)`
# on the real executor. `DEPLOY native` also builds a standalone runnable
# single-node ELF via the typed native carrier.
nano_ros_node_register(
    NAME    {node_name}
    CLASS   {pkg_sym}::{class_name}
    SOURCES src/{class_name}.cpp
    DEPLOY  native)

# `nros_find_interfaces` declares an INTERFACE lib per dep
# (`std_msgs__nano_ros_cpp` etc.) carrying the generated headers' include
# dirs + FFI-glue link. The typed `Publisher<Int32>` needs them.
target_link_libraries({pkg_sym}_{node_name}_component
    PUBLIC std_msgs__nano_ros_cpp)
"#,
        bootstrap = NROS_BOOTSTRAP_BLOCK,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let class_hpp = format!(
        r#"#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace {pkg_sym} {{

/// {class_name} — a typed component (RFC-0043). `configure` creates a
/// publisher on `/chatter` and binds the member `on_tick` (by identity, no
/// name) as a 1 Hz timer callback that publishes a real counter. The typed
/// Entry constructs this object + calls `configure(node)`; the executor
/// dispatches `on_tick` during `spin_once`.
class {class_name} {{
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body; bound via &{class_name}::on_tick (no name)

  public:
    ::nros::Result configure(::nros::Node& node);
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

::nros::Result {class_name}::configure(::nros::Node& node) {{
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return ::nros::bind_timer<{class_name}, &{class_name}::on_tick>(node, timer_, 1000, this);
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
    println!("  cmake -S . -B build -DNANO_ROS_ROOT=<path-to-nano-ros>");
    println!("  cmake --build build");
    println!();
    println!("  # Or add it as a SUBDIR in a workspace root that calls");
    println!("  # nano_ros_workspace(...), then build the workspace.");

    Ok(())
}

/// Scaffold a **C Node pkg** — typed component (RFC-0043). Emits a
/// `NROS_C_COMPONENT(<state>, <configure>)` factory/configure seam: a raw
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
  <maintainer email="TODO@todo.com">TODO</maintainer>
  <license>Apache-2.0</license>
  <export>
    <build_type>cmake</build_type>
  </export>
</package>
"#,
        name = cfg.name,
    );
    fs::write(dir.join("package.xml"), package_xml)?;

    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project({pkg_sym} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

{bootstrap}
# Typed C component (RFC-0043). The raw `/chatter` publisher carries the type
# name as a string, so no generated C bindings are needed. `DEPLOY native`
# also builds a standalone runnable single-node ELF via the typed C carrier.
nano_ros_node_register(
    NAME     {node_name}
    CLASS    {pkg_sym}::{class_name}
    LANGUAGE C
    TYPED
    SOURCES  src/{class_name}.c
    DEPLOY   native)
"#,
        bootstrap = NROS_BOOTSTRAP_BLOCK,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let source = format!(
        r#"// Generated by `nros new {name} --component --lang c`.
//
// Typed C component (RFC-0043). `{configure_fn}` creates a raw `/chatter`
// publisher + a 1 Hz timer publishing a CDR-encoded Int32 counter.
// `NROS_C_COMPONENT` emits the C-ABI factory/configure the typed Entry calls;
// it runs on the real executor — no declarative descriptor, no interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {{
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
}} {state_ty};

static void write_u32_le(uint8_t* p, uint32_t v) {{
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}}

static void on_tick(void* ctx) {{
    {state_ty}* self = ({state_ty}*)ctx;
    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)self->count);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {{
        printf("Published: %d\n", (int)self->count);
    }}
    self->count++;
}}

static nros_ret_t {configure_fn}(const nros_cpp_node_t* node, void* executor, {state_ty}* self) {{
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {{
        return rc;
    }}
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}}

NROS_C_COMPONENT({state_ty}, {configure_fn})
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
    println!("  cmake -S . -B build -DNANO_ROS_ROOT=<path-to-nano-ros>");
    println!("  cmake --build build");
    println!();
    println!("  # Or add it as a SUBDIR in a C++ or Rust Entry workspace.");

    Ok(())
}

/// Map `talker` → `Talker`, `service-server` → `ServiceServer`.
fn use_case_to_pascal(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-')
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

fn scaffold_rust(name: &str, platform: &str, rmw_feature: &str, dir: &Path) -> Result<()> {
    let mut deps = String::new();
    let is_embedded = platform != "native";

    if is_embedded {
        // Phase 248 C5b + C5c-platform (RFC-0031) — BOTH axes lower to the BOARD
        // crate (the board dep below carries `rmw-X` and brings the concrete
        // `nros-platform/platform-X` impl), so the umbrella `nros` dep stays
        // agnostic — no `platform-*`/`rmw-*`, vtable-only.
        deps.push_str(
            "nros = { version = \"*\", default-features = false, features = [\"ros-humble\"] }\n",
        );
        let board_crate = match platform {
            "freertos" => "nros-board-mps2-an385-freertos",
            "baremetal" => "nros-board-mps2-an385",
            "nuttx" => "nros-board-nuttx-qemu-arm",
            _ => "# TODO: add board crate for this platform",
        };
        deps.push_str(&format!(
            "{board_crate} = {{ version = \"*\", features = [\"{rmw_feature}\"] }}\n"
        ));
        deps.push_str("panic-semihosting = \"0.6\"\n");
    } else {
        // Phase 248 C5b + C5c-platform (RFC-0031) — native: both axes lower to
        // `nros-board-native` (its `rmw-X` feature self-links the backend, and it
        // brings `nros-platform/platform-posix`); `nros` stays agnostic.
        deps.push_str(&format!(
            "# nros = {{ version = \"*\", default-features = false, features = [\"std\", \"ros-humble\"] }}\n\
             # nros-board-native = {{ version = \"*\", features = [\"{rmw_feature}\"] }}\n"
        ));
    }

    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[workspace]

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
{deps}
# nano-ros crates are not published to crates.io (RFC-0040) — the `version = "*"`
# requirements above are patched, not resolved from crates.io. Run `nros ws sync`
# (with NROS_REPO_DIR set) to write the nros-managed [patch.crates-io] block here
# (path deps into your nano-ros checkout + any generated msg crates), then build.

# Phase 204.15 inc 3 — named size/speed profiles so the plain-cargo path honours
# the same intent as `[build].optimize` (`cargo build --profile
# size|speed`), no hand-editing. (panic is left to the target/profile — embedded
# triples are already abort; host keeps its default.)
#
# Phase 204.3 — `opt-level = "s"`, NOT `"z"`: on smoltcp/IP examples `-Oz`'s
# weaker DCE keeps a non-inlined socket-buffer accessor that defeats opt-3's
# per-socket dead-buffer elimination (grew `.bss` +24 KB on a measured talker);
# `"s"` shrinks `.text` *more* and preserves the DCE.
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
"#
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    let main_rs = if is_embedded {
        r#"#![no_std]
#![no_main]

use nros::prelude::*;
// TODO: import your board crate
// use nros_board_mps2_an385_freertos::{Config, run, println};
use panic_semihosting as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // TODO: replace with your board crate's run()
    loop {}
}
"#
        .to_string()
    } else {
        format!(
            r#"fn main() {{
    println!("Hello from {name}!");
}}
"#
        )
    };
    fs::write(dir.join("src/main.rs"), main_rs)?;

    if is_embedded {
        write_default_config_toml(dir)?;
        write_cargo_config(dir, platform)?;
    }

    Ok(())
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

/// Standard preamble that bootstraps the nano-ros workspace cmake fns
/// (`nano_ros_workspace_pkg_guard`, `nano_ros_node_register`,
/// `nano_ros_entry`, `nros_find_interfaces`, …) regardless of whether the
/// pkg is built solo or as a workspace member. Lands in every scaffolded
/// C / C++ CMakeLists at the top, right after `project()`.
///
/// Phase 219.I shape — see `cmake/NanoRosWorkspace.cmake`.
const NROS_BOOTSTRAP_BLOCK: &str = r#"# Phase 219.I — bootstrap nano-ros workspace helpers. Workspace builds
# inherit the helpers from the parent root; standalone solo builds
# require `-DNANO_ROS_ROOT=<path-to-nano-ros>` and locate them via the
# include() below.
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
"#;

fn scaffold_c(name: &str, platform: &str, rmw_value: &str, dir: &Path) -> Result<()> {
    let _ = platform;
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
project({name} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 227.4 — declared RMW baked in (override at configure with -DNANO_ROS_RMW=…).
set(NANO_ROS_RMW {rmw_value} CACHE STRING "nano-ros RMW backend")

{bootstrap}
# Phase 210.E.4 — `nros_find_interfaces` reads `package.xml` and
# generates msg/srv/action bindings (+ FFI glue) for every transitive
# interface dep declared via `<depend>` tags.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Phase 212.N.6 — Entry pkg shape (single-Node self-bringup; Phase 219
# adds the `LAUNCH "<bringup>:<file>.launch.xml"` form for multi-Node).
nano_ros_entry(
    NAME {name}
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries({name} PRIVATE std_msgs__nano_ros_c)
nros_platform_link_app({name})
"#,
        bootstrap = NROS_BOOTSTRAP_BLOCK,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_c = format!(
        r#"// Generated by `nros new {name} --lang c`.
//
// Minimal nano-ros C talker — publishes one `std_msgs/Int32` message on
// `/chatter`, then returns. Swap the body for your own logic; see
// `examples/native/c/talker/src/main.c` for a fuller shape (timer,
// executor, signal handler).

#include <stdio.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include "std_msgs.h"

int main(int argc, char** argv) {{
    (void)argc;
    (void)argv;

    nros_support_t support = nros_support_get_zero_initialized();
    if (nros_support_init(&support, NULL, 0) != NROS_RET_OK) {{
        fprintf(stderr, "nros_support_init failed\n");
        return 1;
    }}

    nros_node_t node = nros_node_get_zero_initialized();
    if (nros_node_init(&node, &support, "{name}", "/") != NROS_RET_OK) {{
        fprintf(stderr, "nros_node_init failed\n");
        return 1;
    }}

    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    if (nros_publisher_init(&pub, &node,
                            std_msgs_msg_int32_get_type_support(),
                            "/chatter") != NROS_RET_OK) {{
        fprintf(stderr, "nros_publisher_init failed\n");
        return 1;
    }}

    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    msg.data = 0;
    (void)std_msgs_msg_int32_publish(&pub, &msg);
    printf("{name}: published 0 on /chatter\n");

    nros_publisher_fini(&pub);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}}
"#,
    );
    fs::write(dir.join("src/main.c"), main_c)?;

    if platform != "native" {
        write_default_config_toml(dir)?;
    }
    Ok(())
}

fn scaffold_cpp(name: &str, platform: &str, rmw_value: &str, dir: &Path) -> Result<()> {
    let _ = platform;
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.22)
project({name} VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 227.4 — declared RMW baked in (override at configure with -DNANO_ROS_RMW=…).
set(NANO_ROS_RMW {rmw_value} CACHE STRING "nano-ros RMW backend")

{bootstrap}
# Phase 210.E.4 — `nros_find_interfaces` reads `package.xml` and
# generates msg/srv/action bindings (+ FFI glue) for every transitive
# interface dep declared via `<depend>` tags.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 212.N.6 — Entry pkg shape (single-Node self-bringup; Phase 219
# adds the `LAUNCH "<bringup>:<file>.launch.xml"` form for multi-Node).
nano_ros_entry(
    NAME {name}
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries({name} PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app({name})
"#,
        bootstrap = NROS_BOOTSTRAP_BLOCK,
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_cpp = format!(
        r#"// Generated by `nros new {name} --lang cpp`.
//
// Minimal nano-ros C++ talker — publishes one `std_msgs/Int32` message on
// `/chatter`, then returns. Swap the body for your own logic; see
// `examples/native/cpp/talker/src/main.cpp` for a fuller shape (timer,
// executor, signal handler).

#include <cstdio>

#include <nros/nros.hpp>
#include "std_msgs.hpp"

int main(int argc, char** argv) {{
    (void)argc;
    (void)argv;

    if (auto r = nros::init(); !r.ok()) {{
        std::fprintf(stderr, "nros::init failed: %d\n", r.raw());
        return 1;
    }}

    nros::Node node;
    if (auto r = nros::create_node(node, "{name}"); !r.ok()) {{
        std::fprintf(stderr, "create_node failed: %d\n", r.raw());
        return 1;
    }}

    nros::Publisher<std_msgs::msg::Int32> pub;
    if (auto r = node.create_publisher(pub, "/chatter"); !r.ok()) {{
        std::fprintf(stderr, "create_publisher failed: %d\n", r.raw());
        return 1;
    }}

    std_msgs::msg::Int32 msg;
    msg.data = 0;
    (void)pub.publish(msg);
    std::printf("{name}: published 0 on /chatter\n");

    nros::shutdown();
    return 0;
}}
"#,
    );
    fs::write(dir.join("src/main.cpp"), main_cpp)?;

    if platform != "native" {
        write_default_config_toml(dir)?;
    }
    Ok(())
}

fn write_default_config_toml(dir: &Path) -> Result<()> {
    // Phase 172.K — scaffold the direct-mode nros.toml shape (one node + one
    // ethernet transport), not the retired config.toml.
    let nros_toml = r#"# nano-ros config (direct mode). See
# docs/design/0004-configuration-and-transports.md.

[node]
domain_id = 0

# CONFIGURE ME: these defaults target QEMU slirp (10.0.2.0/24, gateway/router
# at 10.0.2.2). Set ip/gateway/locator to your board's network + zenoh router.
[[transport]]
kind    = "ethernet"
ip      = "10.0.2.20/24"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7447"
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

    #[test]
    fn rust_scaffold_uses_declared_rmw_feature() {
        // Phase 227.4 — `--rmw xrce` must produce an xrce-wired Cargo.toml, not zenoh.
        let d = tmp();
        scaffold_rust("foo", "freertos", "rmw-xrce", d.path()).unwrap();
        let toml = fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
        assert!(toml.contains("rmw-xrce"), "expected rmw-xrce in:\n{toml}");
        assert!(!toml.contains("rmw-zenoh"), "stray rmw-zenoh in:\n{toml}");
    }

    #[test]
    fn rust_scaffold_native_comments_declared_rmw() {
        let d = tmp();
        scaffold_rust("foo", "native", "rmw-cyclonedds", d.path()).unwrap();
        let toml = fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
        assert!(toml.contains("rmw-cyclonedds"), "{toml}");
    }

    #[test]
    fn c_and_cpp_scaffold_bake_declared_rmw() {
        let dc = tmp();
        scaffold_c("bar", "native", "cyclonedds", dc.path()).unwrap();
        let cm = fs::read_to_string(dc.path().join("CMakeLists.txt")).unwrap();
        assert!(
            cm.contains("set(NANO_ROS_RMW cyclonedds"),
            "expected baked rmw in:\n{cm}"
        );

        let dpp = tmp();
        scaffold_cpp("baz", "native", "xrce", dpp.path()).unwrap();
        let cm = fs::read_to_string(dpp.path().join("CMakeLists.txt")).unwrap();
        assert!(cm.contains("set(NANO_ROS_RMW xrce"), "{cm}");
    }

    #[test]
    fn scaffold_package_rejects_unknown_rmw() {
        // `resolve_rmw` fails before any filesystem write, so no package dir is
        // created — the unique name keeps this safe without touching cwd.
        let cfg = ScaffoldConfig {
            name: "pkg_unknown_rmw_227_4_fixture".to_string(),
            lang: "rust".to_string(),
            platform: "native".to_string(),
            rmw: "dust-dds".to_string(),
            use_case: "talker".to_string(),
            force: false,
        };
        let err = scaffold_package(&cfg).expect_err("unknown rmw must fail");
        assert!(err.to_string().contains("dust-dds"), "{err}");
        assert!(
            !PathBuf::from(&cfg.name).exists(),
            "no package dir on rejected rmw"
        );
    }
}
