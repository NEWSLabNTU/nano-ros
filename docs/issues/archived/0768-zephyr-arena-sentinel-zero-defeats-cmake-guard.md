---
id: 768
title: "Every Zephyr image built with the default got a ZERO-BYTE executor arena — the cmake guard against forwarding the `0` sentinel became inert when issue 0460 taught build.rs to read `.config` itself"
status: resolved
type: bug
area: zephyr, core, build
related: [issue-0460, issue-0752, rfc-0079]
---

## Symptom

`realtime_tiers`' `zephyr/rust` row, on a host with a Zephyr workspace:

```
[00:00:02.009] <inf> rust: ctrl_pkg: Control::register on a tier admitting group `ctrl`
[00:00:02.009] <inf> rust: telem_pkg: Telem::register on a tier admitting group `telem`
[00:00:02.045] <err> nros: node declaration failed — NodeError::BufferTooSmall
[00:00:05.337] <err> rust: rustapp: nros: zephyr entry FAILED: NodeRegister("telem_pkg")
panic: nros: zephyr entry failed: NodeRegister("telem_pkg")
[00:00:05.337] <err> os: >>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

The FIRST node registers; the second fails; the image panics.

The test reported this as **"the low tier was not scheduled"**, which sent the
first look at it into the scheduler. The tier was never scheduled because the
image was dead before any tier ran — the same wrong-layer wording issue 0736
spent three sessions correcting one test over.

## Cause

`pub const ARENA_SIZE: usize = 0;` in the built `nros_node_config.rs`. The
executor arena was zero bytes, so `arena_alloc` fails as soon as the first
node's allocations are done.

`0` is the documented Kconfig SENTINEL for "derive it":

```
config NROS_EXECUTOR_ARENA_SIZE
    int "Executor arena size in bytes (0 = derive)"
    default 0
```

and `nros_cargo_build.cmake` knows that, and deliberately does not forward it:

```cmake
# The arena is tri-state. nros-node build.rs DERIVES a size when the knob
# is absent, so forwarding a literal 0 would hand it a zero-byte arena
# rather than the derivation. Resolve it only when someone actually chose
# a value -- Kconfig non-zero, or an explicit environment override.
```

**That guard is inert.** It works by NOT EXPORTING the variable — which stopped
meaning anything when issue 0460 made `nros_zephyr_build::knob_usize` read
`$DOTCONFIG` directly, precisely so Kconfig knobs could reach the Rust lane at
all. `nros-node/build.rs` now finds `CONFIG_NROS_EXECUTOR_ARENA_SIZE=0` in
`.config` whether cmake exported it or not, and takes it literally:

```rust
let arena_size = env_usize("NROS_EXECUTOR_ARENA_SIZE", derived_arena);
//                 └─ knob_usize(env, "CONFIG_NROS_EXECUTOR_ARENA_SIZE", default)
//                    env unset (cmake declined) -> reads .config -> Some(0) -> 0
```

So 0460's fix silently disabled a guard whose whole mechanism was absence. The
knob is not wrong, the guard is not wrong, and the combination is.

Kconfig's own help predicted the failure shape exactly: *"too small fails at
runtime, not at link."*

## Fix

Honour the sentinel where the value is CONSUMED, not where it is forwarded:

```rust
let arena_size = match env_usize("NROS_EXECUTOR_ARENA_SIZE", derived_arena) {
    0 => derived_arena,
    n => n,
};
```

A guard that depends on a variable being absent cannot survive a reader that
goes and finds it. Placing the sentinel at the consumer is the only position
that both paths pass through.

## Measured

`ws-rs-realtime-entry-zenoh`, rebuilt:

```
before:  pub const ARENA_SIZE: usize = 0;
after:   pub const ARENA_SIZE: usize = 74240;
```

74240 is the derivation for `MAX_CBS=4` — the same figure issue 0752 quotes.

`realtime_tiers`' `zephyr/rust` row now RUNS and PASSES, across four
consecutive suite runs (17 rows ran, 6 skipped: `zephyr/c` and `zephyr/cpp` are
not built on this host for disk reasons, `nuttx-riscv/*` want
`qemu-system-riscv32`, `native/cpp-rclcpp` wants ROS). The one remaining failure
is issue 0736's `nuttx-arm/rust`, which is unrelated and predates this.

## Scope — who else got a zero-byte arena

Everyone building a Zephyr image with the default, which is every Zephyr image
in the tree: nothing in `examples/**/prj*.conf` sets
`CONFIG_NROS_EXECUTOR_ARENA_SIZE`. It only became VISIBLE on an image that
registers two nodes; a single-node image fits inside whatever the first
allocations happened to need and boots fine, which is why the whole Zephyr lane
did not go red.

`NROS_EXECUTOR_ARENA_SIZE` is the only `0 = derive` sentinel in `zephyr/Kconfig`
(checked), so this is a one-off rather than a class — but the SHAPE is a class,
and it is worth stating: **a cmake guard that works by declining to export is
no longer a guard.** Any other knob whose cmake side reasons about "absent"
should be re-read against `knob_usize`.
