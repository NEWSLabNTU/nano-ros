---
id: 1820
title: "The generated cargo config drops a board fact's `force = true`, so an inherited env var builds another board's config"
status: resolved
type: bug
area: [cli, build, boards]
severity: high
found: 2026-10-12
related: [1280, phase-484]
---

## Measured

The ThreadX RISC-V board states its config dirs as forced board facts
(`packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml`):

```toml
[env]
THREADX_CONFIG_DIR = { value = "${workspace}/packages/boards/nros-board-threadx-qemu-riscv64/config", force = true }
```

The cargo-leaf road's generated `<leaf>/build/<image>/nros-cargo.toml` for
`param-store-threadx-riscv64` carried:

```toml
THREADX_CONFIG_DIR = { value = "../../../../../boards/nros-board-threadx-qemu-riscv64/config", relative = true }
THREADX_PORT = "risc-v64/gnu"
```

So `force` was gone. Cargo's `[env]` does not override a variable already set
in the process environment unless the row is forced. A shell still carrying a
pre-W2b `activate.sh` export (`THREADX_CONFIG_DIR=…/nros-board-threadx-linux/config`)
won. The riscv64 image then compiled `nros-platform-threadx/src/net.c` and
`zpico.c` with `-I …/nros-board-threadx-linux/config`, which is another
board's `tx_user.h` / `nx_user.h`, and the C compile failed. Run in a clean
environment, the same fixture builds.

Found while removing the recipe-level `THREADX_CONFIG_DIR` exports from
`just/threadx-riscv64.just` (phase-484 W2). Those exports had been covering
for this bug: they overwrote the inherited value with the right one before
cargo ran.

## Cause

`builder::cargo_config::render` copies the descriptor's `cargo_config`
`[env]` verbatim, and `force` survives that copy (a unit test asserted it).
It then writes the caller's `spec.env` / `spec.path_env` rows into the same
table. The cargo-leaf road (`leaf_settings`) hands the board facts back in
through those maps. Each insert REPLACED the descriptor's row with a plain
`value` / `{ value, relative }` row, so `force` was dropped. The module's own
doc states the opposite contract: "A descriptor that itself says
`force = true` keeps it."

## Fix

`render` checks the existing row before replacing it. When the descriptor
authored `force = true`, the caller's row is written with `force = true` too.
That holds for plain rows and path rows alike. A row the descriptor did not
force is still written unforced, so RFC-0049's lane rung still outranks it.
The test is `a_caller_row_restating_a_forced_descriptor_row_keeps_force`.
