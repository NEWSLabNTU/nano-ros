---
id: 1793
title: "Two ThreadX boards' default locator is two different routers: the Rust
  `Config::default()` and the C `NROS_APP_CONFIG` disagree (threadx-linux,
  threadx-qemu-riscv64)"
status: resolved
type: bug
area: [boards, threadx, config]
severity: low
found: 2026-10-11
resolved: 2026-10-16
related: [1767, phase-484, phase-497]
---

## What this is

One board, one fact, two answers. `nros-board-threadx-linux`'s fallback
network config is stated three times, and the LOCATOR disagrees:

| Where | Locator |
|---|---|
| `packages/boards/nros-board-threadx-linux/src/config.rs` `Config::default()` | `tcp/192.0.3.1:7447` |
| `packages/boards/nros-board-threadx-linux/build.rs` `emit_app_config_def` (the cargo road's C `NROS_APP_CONFIG`) | `tcp/127.0.0.1:7555` |
| `cmake/board/nano-ros-board-threadx-linux.cmake` (the cmake road's C `NROS_APP_CONFIG`) | `tcp/127.0.0.1:7555` |

The identity fields (ip `192.0.3.10`, mac `02:00:00:00:00:00`, gateway
`192.0.3.1`, netmask `/24`) agree in all three.

`nros-board-threadx-qemu-riscv64` has the same split: its Rust
`Config::default()` dials `tcp/10.0.2.2:7447`, while both C emitters
(`nros-board-common/src/threadx_qemu_riscv64_build.rs` and
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`) dial `tcp/10.0.2.2:7553`.
Its identity fields (10.0.2.40, 52:54:00:12:34:56, gateway 10.0.2.2) agree.

## Why it is not fixed with the identity fields

phase-484 W4b moves a board's network defaults into its descriptor's
`[board.net]` (RFC-0103 D1: one owner per fact). The identity fields moved.
The locator did not, because choosing either value changes what an image dials
when nothing else states a locator, and the two values look deliberate for
their roads: the C/C++ examples are run against a localhost router on 7555,
the Rust role examples against the bridge router. Which one a test harness
actually relies on has to be MEASURED (run the threadx-linux cells on both
roads with each value) before one of them is deleted.

A locator is also a SELECTION (RFC-0103 D1 — `system.toml` states it); a
board default is only a fallback. The likely end state is that the board
states one, and every in-tree image states its own in `system.toml`.

## Next

Run the threadx-linux C, C++ and Rust cells with the descriptor's locator set
to each value; keep the one that passes everywhere (or make every in-tree
image state its locator and give the board one fallback), then move the
locator into `[board.net]` and delete the two C copies.

Not measurable on newslab-server-241 (2026-10-11): the threadx-linux cells need
the user-owned TAP/veth interfaces the `setup-network` recipe creates, which takes
root once, and that host grants none. Run it on a host that has them.

## Resolution (2026-10-16)

**The measurement this asked for needed no TAP interfaces.** threadx-linux's
`[board.net]` is `stack = "host"`: NetX goes through the host's sockets
(nsos-netx), so the `veth-tx0` the boot banner prints is a label and a router
on loopback is reachable without `setup-network`. The question "which value
does a harness rely on" was answerable by reading what each image STATES:

- every threadx-linux and rv-virt-threadx Rust example and workspace row
  states `locator` in its `system.toml`;
- every C/C++ zenoh fixture row passes `NROS_ENTRY_LOCATOR`;
- the one rv-virt-threadx row that passes neither, `errno-isolation`, opens no
  session.

So neither fallback was load-bearing in-tree, and the choice is for a user's
image that states nothing. On threadx-linux only loopback is reachable without
root, and 7447 is the port a stock `rmw_zenohd` listens on; on rv-virt-threadx
the host is `10.0.2.2` under slirp. Both boards now fall back to one router,
stated once in `nros-board-common::threadx_config`:

| board | constant | value | was (Rust / C) |
|---|---|---|---|
| threadx-linux | `THREADX_LINUX_DEFAULT_LOCATOR` | `tcp/127.0.0.1:7447` | `tcp/192.0.3.1:7447` / `tcp/127.0.0.1:7555` |
| threadx-qemu-riscv64 | `THREADX_QEMU_RISCV64_DEFAULT_LOCATOR` | `tcp/10.0.2.2:7447` | `tcp/10.0.2.2:7447` / `tcp/10.0.2.2:7553` |

`Config::default()` and the cargo road's C emitter read the constant. The two
cmake board modules cannot, so they keep a literal, and
`threadx_config::tests::cmake_threadx_fallback_locators_match_the_rust_ones`
holds each equal to its constant (mutation: setting the rv-virt module back
to `:7553` fails it). The locator did not move into `[board.net]`: that table
describes the board's network, and a router is the image's selection
(RFC-0103 D1). Not changed: the node-register carrier default
`NROS_LOCATOR_NODEREG_THREADX` (`tcp/10.0.2.2:7553`, `NanoRosEntryLocator.cmake`),
a different symbol that its own comment records as deliberate.

**The same mismatch at run time, found by tier 2.** run-matrix 38818350050
(the first tier-2 run in days to reach its cells) had one real failure,
`rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`.
Reproduced on fresh fixtures: the two Rust threadx-linux workspace rows
(`workspace-rust-threadx-linux`, `-realtime`) failed while the C/C++ rows on
the same board passed. The test (issue 1819) starts its router on a free port
and names it in `NROS_LOCATOR`, and bounds the run with `NROS_ENTRY_SPIN_MS`.
On this board the C and C++ entries honour both at run time; the Rust entry
read neither, so it dialled its baked `127.0.0.1:9030` (strace) and, given
the right router, spun forever. Fixed on the board, not in the test:

- `nros-board-threadx-linux` applies a non-empty `$NROS_LOCATOR` over the
  baked locator (libc `getenv`; with nothing set the baked value still
  decides, so every test that starts its router on the baked port is
  unaffected);
- `ThreadxConfig::entry_spin_ms()` (default `None`) bounds the spin in
  `nros-board-threadx`'s single-executor and boot-tier loops on the
  platform's monotonic clock, printing `nros: application complete` and
  exiting 0 like the linux board; threadx-linux reads it from
  `$NROS_ENTRY_SPIN_MS`.

Measured after: `rmw_coordinate_truth` registers every entry (86 of 86 at the
first run). All 19 threadx-linux tests pass (`rtos_e2e` ×9,
`linux_api` ×7, `logging_smoke`, `condvar_contract`, `entry_build`), as do
the threadx-linux rows of `realtime_tiers` and `sched_dims_applied`. On
threadx-qemu-riscv64 all 13 cells pass (`rtos_e2e` ×6, `threadx_riscv64_qemu`
×4, `logging_smoke`, `typed_carrier`, `param_store_carve`). `just ci gate` green.
