---
id: 1793
title: "Two ThreadX boards' default locator is two different routers: the Rust
  `Config::default()` and the C `NROS_APP_CONFIG` disagree (threadx-linux,
  threadx-qemu-riscv64)"
status: open
type: bug
area: [boards, threadx, config]
severity: low
found: 2026-10-11
related: [1767]
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
