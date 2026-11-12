//! phase-290 (RFC-0049) W4.b — `nros new platform <name>` and
//! `nros new board <name> --for-platform <p>` scaffolders. The board one is
//! `nros board new` under another spelling (issue 2292).
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
pub fn scaffold_platform(name: &str, into: &Path, inherits: Option<&str>) -> Result<()> {
    validate_name(name)?;
    // Issue 2310: the family a board naming this platform builds as. Stated,
    // it is a live line; absent, the line stays commented and the catalog
    // names it when a board first asks.
    let inherits_line = match inherits {
        Some(family) => format!("inherits = \"{family}\""),
        None => "# inherits = \"bare-metal\"".to_string(),
    };
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
            // See phase-349 W2b.
            r#"//! nano-ros platform layer for {name}.
//!
//! A platform crate owns the software-stack primitives every nano-ros
//! component above it consumes:
//!
//! - `clock`  — a monotonic `clock_ns()` source + its resolution
//! - `sleep`  — `sleep_ms()` (+ optional idle-yield hook installation)
//! - net shims — the `nros_platform_*` socket ABI, or a zenoh-pico
//!   system layer, described by a `[port.<platform>]` row in the zenoh
//!   backend's `nros-rmw.toml`
//!
//! Board crates (`nros-board-*`) layer hardware bring-up on top and pick
//! this platform via their `nros-board.toml`.

#![no_std]

pub mod clock {{
    /// Monotonic nanoseconds since boot. TODO: wire to the {name} timer.
    ///
    /// Nanoseconds is the ABI's unit. Where the counter
    /// frequency divides 1e9 the conversion is a compile-time multiply —
    /// prefer that over a runtime division.
    pub fn clock_ns() -> u64 {{
        todo!("port: monotonic clock for {name}")
    }}

    /// Smallest non-zero difference [`clock_ns`] can report. Non-zero:
    /// state the tick or cycle step honestly rather than implying the
    /// unit's precision.
    pub fn clock_resolution_ns() -> u64 {{
        todo!("port: clock resolution for {name}")
    }}
}}

pub mod sleep {{
    /// Sleep for `ms` milliseconds. TODO: wire to the {name} scheduler
    /// (or a busy-wait against `clock::clock_ns` on a bare-metal port).
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
            r#"# Platform package configuration for `{name}`.
#
# Resolution ladder: builtin < platform (this file, via optional
# `inherits`) < board (`nros-board.toml` deltas) < env / Kconfig / -D.
# Debug with: `nros config explain --platform {name}`.
# Schema + loader: nros-board-common/src/platform_config.rs.

# The names this platform answers to; names[0] is canonical. The directory
# is only where the file lives.
names = ["{name}"]

# The platform family this one builds as, and whose config it inherits.
# A board naming `{name}` builds as the first built-in platform this chain
# reaches (posix, freertos, bare-metal, nuttx, zephyr, threadx-linux,
# threadx-riscv64, stm32), so a board for a platform that inherits none is
# refused until this line is set.
{inherits_line}

[capabilities]
# Software-stack FACTS (open vocabulary; policy below is checked against
# them — e.g. split_lock without threads is downgraded at build time).
# threads = true
# per_fd_tx_ceiling = false

# [knobs.zenoh.tx]
# Typed policy defaults (deny_unknown_fields). Flip these only with a
# measurement on THIS platform (a lever that does not
# move the number is reverted).
# batch = false        # one socket send per flush instead of per publish
# split_lock = false   # append while a send is in flight (needs threads)
# flush_ms = 50        # flush cadence / added publish latency bound

# What a vendored C build on this platform needs goes at the TOP of this file,
# before any table: `arch_profiles = ["<arch>"]` (which [arch.*] profiles
# apply, first match wins), `compile = {{ opt_level = "s" }}` and
# `required_env = [...]`. How zenoh-pico builds here is NOT this file's: it is
# a `[port.{name}]` row in packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml
# (keys: defines, defines_kv, include, extra_sources, include_paths, ...;
# schema `PortRow` in nros-platform-config). Start from the closest in-tree
# platform's row.
"#
        ),
    )?;

    println!(
        "platform package `{name}` scaffolded — next: fill in clock/sleep, \
         then a board package: `nros new board <board> --for-platform {name}`"
    );
    Ok(())
}

/// `nros new board <name> --for-platform <p>`: the same board package
/// `nros board new <name> --platform <p> --out-of-tree <into>` writes.
///
/// Issue 2292 -- this kept its own template (a crate skeleton and a descriptor
/// with no `package.xml`) from before RFC-0064 R5 D5 made the announcement
/// mandatory, so every catalog reader refused the board it reported
/// scaffolded. There is now one board scaffolder, and this spelling calls it.
pub fn scaffold_board(name: &str, platform: &str, into: &Path) -> Result<()> {
    validate_name(name)?;
    validate_name(platform)?;
    if platform == "zephyr" {
        // `nros new` takes no Zephyr board id, and the descriptor cannot be
        // written without one.
        bail!(
            "a Zephyr board needs its Zephyr board id. Scaffold it with\n  \
             nros board new {name} --platform zephyr --west-board <board>/<soc>/<variant> \
             --out-of-tree {}",
            into.display()
        );
    }
    crate::cmd::board_new::run(crate::cmd::board_new::NewArgs {
        name: name.to_string(),
        platform: platform.to_string(),
        west_board: None,
        out_of_tree: Some(into.to_path_buf()),
        workspace: None,
        dry_run: false,
    })
}
