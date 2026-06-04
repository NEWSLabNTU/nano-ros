//! Project scaffolder — promoted out of `main.rs` so `nros-cli` and any
//! other front-end can share it.
//!
//! v1 emits a colcon-compatible hello-world per `<lang> × <platform>`.
//! Use-case (`talker` / `listener` / `service` / `action`) and RMW-choice
//! diversification arrives once the `templates/` tree lands; until then
//! both fields are accepted for forward-compat but only surfaced in the
//! "Next steps" output.

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
        "rust" => scaffold_rust(&cfg.name, &cfg.platform, &dir)?,
        "c" => scaffold_c(&cfg.name, &cfg.platform, &dir)?,
        "cpp" => scaffold_cpp(&cfg.name, &cfg.platform, &dir)?,
        other => bail!("Unknown language: {other}. Use rust, c, or cpp."),
    }

    println!("✓ Created nano-ros package '{}'", cfg.name);
    println!("  Language : {}", cfg.lang);
    println!("  Platform : {}", cfg.platform);
    println!("  RMW      : {} (template diversification: TODO)", cfg.rmw);
    println!(
        "  Use case : {} (template diversification: TODO)",
        cfg.use_case
    );
    println!("  Build    : {build_type}");
    println!();
    println!("Next steps:");
    println!("  cd {}", cfg.name);
    println!(
        "  nros build           # or: colcon build --packages-select {}",
        cfg.name
    );

    Ok(())
}

#[derive(Debug, Clone)]
pub struct ComponentScaffoldConfig {
    pub name: String,
    /// Node flavor: `talker` / `listener` / `service` / `action`. Template
    /// diversification is TODO — today every flavor emits the publisher+timer
    /// shape, named after the flavor.
    pub use_case: String,
    pub force: bool,
}

/// Scaffold a **planned-mode component** — a reusable nano-ros node compiled as
/// a *library* and linked into a system by `nros plan` / `nros deploy`. Unlike
/// the direct-mode hello-world binary `scaffold_package` emits (a `[node]`
/// manifest), this produces an `nros::Component` impl plus a *folded*
/// `[component]` table in `nros.toml`. The platform + RMW are chosen later, at
/// deploy time — not baked here.
///
/// The manifest is intentionally minimal: `[linkage]` is omitted (derived —
/// executable ← component short name, `exported_symbol` ← `nros_component_<n>`,
/// `crate_name` ← package) and `[overrides]` defaults to empty (Phase 172 W.3).
pub fn scaffold_component(cfg: &ComponentScaffoldConfig) -> Result<()> {
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
[dependencies]
nros = {{ version = "0.1", default-features = false }}
"#,
        name = cfg.name,
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = format!(
        r#"#![no_std]

//! `{name}` — a reusable nano-ros component (planned mode).
//!
//! `nros plan` / `nros deploy` link this crate into a system and call
//! `{module}::Component::register`. `nros metadata --build` records its
//! declarations into `metadata/{module}.json`. Platform + RMW are chosen at
//! deploy time, not here.

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
# system by `nros plan` / `nros deploy`. See
# docs/design/configuration-and-transports.md.

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
    println!("  Kind      : planned-mode (library, linked by `nros deploy`)");
    println!();
    println!("Next steps:");
    println!("  cd {}", cfg.name);
    println!("  # add this package to a workspace's [system].components, then:");
    println!("  nros metadata --build   # record its source metadata");

    Ok(())
}

fn scaffold_rust(name: &str, platform: &str, dir: &Path) -> Result<()> {
    let mut deps = String::new();
    let is_embedded = platform != "native";

    if is_embedded {
        deps.push_str(&format!(
            "nros = {{ version = \"0.1\", default-features = false, features = [\"rmw-zenoh\", \"platform-{platform}\", \"ros-humble\"] }}\n"
        ));
        let board_crate = match platform {
            "freertos" => "nros-board-mps2-an385-freertos",
            "baremetal" => "nros-board-mps2-an385",
            "nuttx" => "nros-board-nuttx-qemu-arm",
            _ => "# TODO: add board crate for this platform",
        };
        deps.push_str(&format!("{board_crate} = {{ version = \"0.1\" }}\n"));
        deps.push_str("panic-semihosting = \"0.6\"\n");
    } else {
        deps.push_str(
            "# nros = { version = \"0.1\", features = [\"std\", \"rmw-zenoh\", \"platform-posix\", \"ros-humble\"] }\n",
        );
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

# Phase 204.15 inc 3 — named size/speed profiles so the plain-cargo path honours
# the same intent as `nros build`'s `[build].optimize` (`cargo build --profile
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

fn scaffold_c(name: &str, platform: &str, dir: &Path) -> Result<()> {
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.16)
project({name} VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)

find_package(NanoRos REQUIRED CONFIG)

add_executable({name} src/main.c)
target_link_libraries({name} PRIVATE NanoRos::NanoRos)

install(TARGETS {name} RUNTIME DESTINATION lib/{name})
"#
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_c = format!(
        r#"#include <stdio.h>

int main(void) {{
    printf("Hello from {name}!\n");
    return 0;
}}
"#
    );
    fs::write(dir.join("src/main.c"), main_c)?;

    if platform != "native" {
        write_default_config_toml(dir)?;
    }
    Ok(())
}

fn scaffold_cpp(name: &str, platform: &str, dir: &Path) -> Result<()> {
    let cmake = format!(
        r#"cmake_minimum_required(VERSION 3.16)
project({name} VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)

find_package(NanoRos REQUIRED CONFIG)

add_executable({name} src/main.cpp)
target_link_libraries({name} PRIVATE NanoRos::NanoRosCpp)

install(TARGETS {name} RUNTIME DESTINATION lib/{name})
"#
    );
    fs::write(dir.join("CMakeLists.txt"), cmake)?;

    let main_cpp = format!(
        r#"#include <cstdio>

int main() {{
    printf("Hello from {name}!\n");
    return 0;
}}
"#
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
# docs/design/configuration-and-transports.md.

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
