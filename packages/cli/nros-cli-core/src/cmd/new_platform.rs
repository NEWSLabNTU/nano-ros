//! phase-290 (RFC-0049) W4.b — `nros new platform <name>` and
//! `nros new board <name> --for-platform <p>` scaffolders.
//!
//! The porter contract: bringing nano-ros to a new RTOS is **2 crates +
//! 2 tomls, zero central-file edits**. These scaffolds emit the crate
//! skeletons with the full config schema as comments, so the porter never
//! starts from a blank page. See the book's "Porting nano-ros to a new
//! RTOS" checklist.

use eyre::{Result, bail};
use std::path::Path;

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.starts_with('-')
    {
        bail!(
            "invalid name `{name}` — use lowercase ascii, digits and hyphens \
             (it becomes a directory + crate name)"
        );
    }
    Ok(())
}

fn write_new(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        bail!("{} already exists — refusing to overwrite", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    println!("  created {}", path.display());
    Ok(())
}

/// `nros new platform <name>` — a self-contained platform package:
/// `nros-platform-<name>/` crate skeleton + `nros-platform.toml` with the
/// RFC-0049 schema as comments.
pub fn scaffold_platform(name: &str, into: &Path) -> Result<()> {
    validate_name(name)?;
    let dir = into.join(format!("nros-platform-{name}"));
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }

    write_new(
        &dir.join("Cargo.toml"),
        &format!(
            r#"[package]
name = "nros-platform-{name}"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
description = "nano-ros platform layer for {name}: clock, sleep, net shims"

[dependencies]
# Typical platform-layer deps (uncomment as the port grows):
# nros-baremetal-common = {{ version = "*" }}   # busy-wait sleep + idle hooks
"#
        ),
    )?;

    write_new(
        &dir.join("src/lib.rs"),
        &format!(
            r#"//! nano-ros platform layer for {name}.
//!
//! A platform crate owns the software-stack primitives every nano-ros
//! component above it consumes:
//!
//! - `clock`  — a monotonic `clock_us()/clock_ms()` source
//! - `sleep`  — `sleep_ms()` (+ optional idle-yield hook installation)
//! - net shims — the `nros_platform_*` socket ABI, or a zenoh-pico
//!   system-layer under `[build.zenoh]` in `nros-platform.toml`
//!
//! Board crates (`nros-board-*`) layer hardware bring-up on top and pick
//! this platform via their `nros-board.toml`.

#![no_std]

pub mod clock {{
    /// Monotonic microseconds since boot. TODO: wire to the {name} timer.
    pub fn clock_us() -> u64 {{
        todo!("port: monotonic clock for {name}")
    }}

    /// Monotonic milliseconds since boot.
    pub fn clock_ms() -> u64 {{
        clock_us() / 1_000
    }}
}}

pub mod sleep {{
    /// Sleep for `ms` milliseconds. TODO: wire to the {name} scheduler
    /// (or a busy-wait against `clock::clock_ms` on a bare-metal port).
    pub fn sleep_ms(ms: u32) {{
        let _ = ms;
        todo!("port: sleep for {name}")
    }}
}}
"#
        ),
    )?;

    write_new(
        &dir.join("nros-platform.toml"),
        &format!(
            r#"# RFC-0049 — platform package configuration for `{name}`.
#
# Resolution ladder: builtin < platform (this file, via optional
# `inherits`) < board (`nros-board.toml` deltas) < env / Kconfig / -D.
# Debug with: `nros config explain --platform {name}`.
# Schema + loader: nros-board-common/src/platform_config.rs.

# Inherit a sibling platform family's config (e.g. "bare-metal"):
# inherits = "bare-metal"

[capabilities]
# Software-stack FACTS (open vocabulary; policy below is checked against
# them — e.g. split_lock without threads is downgraded at build time).
# threads = true
# per_fd_tx_ceiling = false

# [knobs.zenoh.tx]
# Typed policy defaults (deny_unknown_fields). Flip these only with a
# measurement on THIS platform (the phase-282 rule: a lever that does not
# move the number is reverted).
# batch = false        # one socket send per flush instead of per publish
# split_lock = false   # append while a send is in flight (needs threads)
# flush_ms = 50        # flush cadence / added publish latency bound

# [build.zenoh]
# zenoh-pico system-layer build block (keys documented in
# nros-board-common/src/manifest.rs — defines, defines_kv, include,
# extra_sources, arch, compile, ...). Start from the closest in-tree
# platform's file under packages/platforms/.
# defines = ["ZENOH_GENERIC"]
# include = ["system/common"]
"#
        ),
    )?;

    println!(
        "platform package `{name}` scaffolded — next: fill in clock/sleep, \
         then a board package: `nros new board <board> --for-platform {name}`"
    );
    Ok(())
}

/// `nros new board <name> --for-platform <p>` — a board package skeleton:
/// `nros-board-<name>/` crate + `nros-board.toml` (RFC-0042 descriptor +
/// RFC-0049 `[capabilities]`/`[knobs]` tables as comments).
pub fn scaffold_board(name: &str, platform: &str, into: &Path) -> Result<()> {
    validate_name(name)?;
    validate_name(platform)?;
    let dir = into.join(format!("nros-board-{name}"));
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }

    write_new(
        &dir.join("Cargo.toml"),
        &format!(
            r#"[package]
name = "nros-board-{name}"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
description = "nano-ros board package for {name} (platform: {platform})"

[dependencies]
nros-platform-{platform} = {{ version = "*" }}
# nros-platform = {{ version = "*", default-features = false }}  # BoardEntry traits
"#
        ),
    )?;

    write_new(
        &dir.join("src/lib.rs"),
        &format!(
            r#"//! nano-ros board package for {name} (platform: {platform}).
//!
//! A board crate owns hardware bring-up (clocks, pins, the network
//! device) and implements the `nros_platform::BoardEntry` lifecycle so
//! `nros::main!()` can boot on it. Mirror the closest in-tree board
//! under packages/boards/ for the full trait surface.

#![no_std]

/// Board hardware bring-up. Called once before the executor opens.
pub fn init_hardware() {{
    todo!("port: clocks / pins / network device for {name}")
}}
"#
        ),
    )?;

    write_new(
        &dir.join("nros-board.toml"),
        &format!(
            r#"# RFC-0042 board descriptor + RFC-0049 knob deltas for `{name}`.
#
# Duty rule: the PLATFORM file carries software-stack facts + defaults;
# THIS file carries hardware facts + per-board overrides. Debug the
# resolved ladder with:
#   nros config explain --platform {platform} --board-toml <this file>

[[board]]
names = ["{name}"]
platform = "{platform}"
board_crate = "nros-board-{name}"
# toolchain / link_kind / entry_kind / net_stack — see an in-tree
# nros-board.toml (e.g. packages/boards/nros-board-esp32s3/) for the
# full descriptor key set.

[board.capabilities]
# Hardware FACTS (RFC-0042): what this board can actually do.
# heap = true
# atomics = true
# threads = true

# [knobs.zenoh.tx]
# Per-board OVERRIDES of the platform defaults (rarely needed — prefer
# fixing the platform default if every board wants the same value).
# batch = false
"#
        ),
    )?;

    println!(
        "board package `{name}` scaffolded for platform `{platform}` — next: \
         implement init_hardware + the BoardEntry lifecycle"
    );
    Ok(())
}
