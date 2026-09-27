// Copyright (c) 2026, NEWSLab NTU.
// SPDX-License-Identifier: Apache-2.0

//! Resolve `NROS_ZEPHYR_HEAP_SIZE` for `src/zephyr_heap.rs`.
//!
//! The arena size used to be read straight from the environment with
//! `option_env!`. That has two defects this build script exists to fix, and
//! `check-kconfig-knob-forwarding` fails the build without it.
//!
//! 1. A bare environment read does not reach the Zephyr Rust lane. Kconfig
//!    knobs live in `$DOTCONFIG`, and `nros_zephyr_build::Knob` is the one
//!    spelling that resolves BOTH (issue 0460) -- environment first, then
//!    `CONFIG_*`, then the platform/board rung, then the default.
//!
//! 2. `option_env!` is baked at compile time and, with no build script, cargo
//!    never invalidates on a change to it: an already-built rlib is reused
//!    with the previous size compiled in, and the image starves exactly as if
//!    the knob had never been set. Reading the variable here emits the
//!    `rerun-if-env-changed` that makes a change take effect.
//!
//! The default stays 64 KiB, matching the zenoh images' previous
//! `CONFIG_HEAP_MEM_POOL_SIZE=65536`, so a converted image is RAM-neutral
//! before tuning.

const DEFAULT_HEAP_SIZE: usize = 64 * 1024;

fn main() {
    // phase-400 W6 — this knob does NOT get the `[knobs.memory]` platform and
    // board rungs, and the reason is structural rather than a decision.
    //
    // `nros-board-common` owns the ladder and depends on THIS crate, so a
    // build-dependency the other way is a cycle — cargo rejects it outright.
    // That is the difference from `nros-node`, which board-common does not
    // depend on and which therefore could take it.
    //
    // Nothing is lost in practice: on Zephyr the rung those tables would
    // provide is already served by Kconfig, which `Knob` reads from
    // `$DOTCONFIG` (issue 0460). The platform states the value where a Zephyr
    // image already looks for it.
    //
    // phase-400 W6 — and it NOW HAS the toml rungs. This comment used to end
    // "giving it the toml rungs would need the ladder types in a crate BELOW
    // both — a real refactor, not a dependency line", and that refactor
    // happened: `nros-platform-config` is exactly that crate, extracted so the
    // reader could reach crates `nros-board-common` cannot.
    //
    // The `memory` tenant already mapped this knob — `("zephyr", "heap_bytes")
    // => "NROS_ZEPHYR_HEAP_SIZE"` — so the ladder modelled it while the only
    // reader could not consult it.
    //
    // issue 1504 / phase-468 W4 — this used to read
    // `from_build_env().map(memory_value).unwrap_or_else(|| knob_usize(..))`,
    // and the comment above it claimed `memory_value` composed "env, then
    // Kconfig via `$DOTCONFIG`, then the platform/board rung". It does not:
    // `memory()` has NO Kconfig rung and never had one. So on every build that
    // exports `NROS_PLATFORM_NAME` — which is every build the nros road drives
    // — the `knob_usize` arm was unreachable and `CONFIG_NROS_ZEPHYR_HEAP_SIZE`
    // reached the C lane and not this one. Issue 0460, in the knob that sizes
    // the Zephyr heap.
    //
    // The two are not alternatives, they are RUNGS, so they compose: `Knob` is
    // the ladder (env, then `$DOTCONFIG`), `memory_rungs()` supplies the
    // platform/board rung below it, and `DEFAULT_HEAP_SIZE` is the builtin.
    let rung = nros_platform_config::platform_config::BuildRungs::from_build_env()
        .and_then(|r| r.memory_rungs().heap_bytes);
    let size = nros_zephyr_build::knob("NROS_ZEPHYR_HEAP_SIZE")
        .rung(rung)
        .resolve(DEFAULT_HEAP_SIZE);

    // `zephyr_heap.rs` keeps its `option_env!` read; this simply makes the
    // value it sees the RESOLVED one rather than whatever happened to be in
    // the ambient environment.
    println!("cargo:rustc-env=NROS_ZEPHYR_HEAP_SIZE={size}");
    println!("cargo:rerun-if-env-changed=NROS_ZEPHYR_HEAP_SIZE");
}
