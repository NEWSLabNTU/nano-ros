---
id: 1580
title: "Board crates' own cc-rs compiles watch their sources by hand, blind to headers outside those paths"
status: resolved
type: tech-debt
area: [build, boards]
severity: low
found: 2026-09-29
resolved: 2026-10-01
related: [issue-1570, issue-0475, issue-0491, issue-1586, issue-1599]
---

## What is left

Issue 1570 gave the NuttX image lane a compiler-measured rebuild edge.
`nros_cc_flags::header_deps::{track_header_deps, emit_header_deps}` passes
`-MMD` to the compiler, then replays each `.d` file as
`cargo:rerun-if-changed`, so cargo watches exactly the files the compiler
read. `check-cc-header-deps` requires that helper pair on every compile that
consumes an env `*SOURCES*` list.

The board crates' OWN cc-rs builds are outside that gate. Each one watches its
inputs by hand, per file or per directory:

- FreeRTOS kernel, lwIP and glue;
- ThreadX kernel, NetX and glue;
- threadx-linux;
- mps2 lan9118.

A header those builds include from outside the watched paths can therefore
change without recompiling the object that includes it. That is 1570's class,
at lower stakes: these sources are vendored or in-crate, and they rarely
change underneath a build.

## Direction

Apply the same helper pair to each board cc-rs build. The hand-written
`rerun-if-changed` lines it makes redundant can then go, after checking
whether any of them also watches a non-compiled input. Then widen
`check-cc-header-deps` to every `cc::Build::compile` in a build script, or
record why a site is exempt.

## Acceptance

For each board crate: `touch` a header its C code includes from outside the
watched directories, and the board crate rebuilds. A no-op rebuild stays a
no-op.

## Resolution

Every board crate's cc-rs compile now goes through the 1570 pair —
`track_header_deps` on each `cc::Build`, `emit_header_deps(OUT_DIR)` after the
script's compiles — and the hand-written lines it made redundant are gone.

| Site | Compiles |
| --- | --- |
| `nros-board-freertos/build.rs` | freertos, lwip, nros_platform_freertos, freertos_glue |
| `nros_board_common::freertos_build::run_overlay` | startup (board glue) |
| `nros-board-mps2-an385-freertos/build.rs` | lan9118_lwip, tband (extras) |
| `nros-board-threadx/build.rs` (+ `threadx_sources::add_nros_platform_threadx_build`) | threadx_kernel, nros_platform_threadx |
| `nros-board-threadx-linux/build.rs` | nsos_netx, glue |
| `nros_board_common::threadx_qemu_riscv64_build::run` | port_asm, netxduo, virtio_net_netx, glue, nros_app_config_def |

The hand lists were wrong, not just short: `nros-board-freertos` never
watched `c/freertos_task_glue.c`; the riscv64 overlay never watched
`c/hwtimer.c`, `c/tx_initialize_low_level.S`, the virtio-net driver or NetX
Duo; `nros-board-threadx` watched no kernel source at all.

**Removed** (now declared by the depfiles): per-file lines on compiled `.c`
files and config headers, and directory watches on the FreeRTOS kernel, lwIP,
both platform ports, the platform-cffi include dir, the lan9118 and nsos-netx
trees and the ThreadX config / extra-include dirs. **Kept**, because no
compiler reads them: `build.rs`, the linker scripts, the riscv64 `c/entry.s`
(a lowercase `.s` is not preprocessed, so `-MMD` writes no depfile for it),
the `include_str!` source of `threadx_hooks.c`, and every
`rerun-if-env-changed` on a VALUE.

Two defects in the helper itself surfaced on the way, both fixed in
`nros-cc-flags`:

- **sccache ate the depfile.** cc-rs prefixes `RUSTC_WRAPPER=sccache` (the
  justfile exports it) to the C compiler, and sccache 0.15.0 restores the
  object but not the implicitly named `<obj>.d` on a cache HIT — measured
  directly, and through cargo as an `emit_header_deps` panic on a second fresh
  target dir. `track_header_deps` now pins the resolved compiler, which
  bypasses the wrapper. This also covered the NuttX lane 1570 introduced.
- **Generated TUs made the no-op rebuild dirty.** Two board scripts compile a
  TU they write on every run into `OUT_DIR`; cargo stamps a run with its START
  time, so declaring it would never be fresh. `emit_header_deps` skips paths
  under `OUT_DIR` (they are outputs).

Measured per family — `touch` a header outside the old watched paths, then a
no-op rebuild, reading the board crate's `cargo -v` state:

| Family | Crate | Header touched | Before | After | No-op |
| --- | --- | --- | --- | --- | --- |
| FreeRTOS (thumbv7m) | nros-board-freertos | family `config/FreeRTOSConfig.h` | Fresh | Dirty | Fresh |
| FreeRTOS mps2 overlay | nros-board-mps2-an385-freertos | `nros-c/include/nros/app_config.h` | — | Dirty | Fresh |
| ThreadX linux (x86_64) | nros-board-threadx | kernel `common/inc/tx_api.h` | — | Dirty | Fresh |
| ThreadX linux | nros-board-threadx-linux | `nros/app_config.h` | — | Dirty | Fresh |
| ThreadX riscv64 | nros-board-threadx-qemu-riscv64 | `virtio-net-netx/include/virtio_net_nx.h` | — | Dirty | Fresh |
| ThreadX riscv64 | nros-board-threadx | netxduo `common/inc/nx_api.h` | — | Dirty | Fresh |

`check-cc-header-deps` now reaches every tracked Rust source with a
`.compile(` / `.try_compile(` (16 files): `emit_header_deps` present and one
`track_header_deps` per compile site, with three reasoned tables
(`TRACKED_BY_CONFIGURATOR`, `TRACK_HELPERS`, `EXEMPT`) that the gate also
checks for staleness. The non-board sites adopted the pair additively to make
that true (nros-platform-cffi, nros-rmw-cffi, nros-rmw-xrce-cffi,
nros-rmw-cyclonedds-sys, nros-build-helpers, the mps2-an385-baremetal C
talker). One file is exempt: `nros-zpico-build/src/runner.rs`, because
adopting the pair forfeits sccache for zenoh-pico — issue 1599.
