---
id: 1557
title: "What issue 1145 left behind: `threadx-riscv64` states no executor-backing
  size, so its image still reserves the backing twice — and the allocator BASES
  the pairings subtract from (ThreadX's 4 MiB byte pool, FreeRTOS's 3 MiB
  cyclone/XRCE heap) were never derived"
status: open
type: tech-debt
area: boards, threadx, freertos
severity: low
found: 2026-09-29
related: [1145, phase-392, phase-448, 1171, 1355, 1197, 1388]
---

## Background

[Issue 1145](archived/1145-executor-backing-static-unpaired-with-rtos-heap.md)
asked every RTOS port to stop reserving the executor backing twice after
phase-392 W6 moved it into the `.bss` static
`nros_node::executor::backing::EXECUTOR_BACKING`. Every port it covered is
resolved (Zephyr, NuttX, FreeRTOS, ESP32, `threadx-linux`), and 1145 is closed.
This issue is the two items it could not close, split out so a resolved issue
does not carry an open list.

Neither item is a defect in a running image. Both are about bytes reserved on
purpose-built headroom nobody measured.

## 1. `threadx-riscv64` states no backing size

1145's ThreadX section, verbatim:

> The mechanism is board-agnostic and the rv64 descriptor can state its own rung
> the moment someone measures it. It is NOT stated here because the number has to
> come from that board's own images (`nm -S` on a riscv64 build), and building the
> rv64 lane was not affordable in this session. Unstated is the safe state: no
> define, the C `#ifndef` default of 0 applies, and the pool stays at its base —
> the image pays twice, exactly as it does today.

Where it stands at 2026-09-29:

- `packages/boards/nros-board-threadx-linux/nros-board.toml:105-106` states
  `[board.knobs.executor] backing_u64s = 11069` (raised from 1145's `4494` by
  issue 1388, because the claim must cover the largest default `nros-node`
  compiles on that board, including the synthesised `nros_ws_runtime` umbrella).
- `packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml` has **no**
  `[board.knobs.executor]` table. So `threadx_hooks.c:107-109` applies its
  `#ifndef NROS_EXECUTOR_BACKING_U64S` / `#define … 0` default, and the pool is
  the whole `BYTE_POOL_BASE_SIZE` beside a full-size `EXECUTOR_BACKING`.
- `scripts/check-executor-backing-arena-pairing.py`'s `PORTS` ledger records
  `threadx-riscv64` as `{"kind": "rung", "site": "threadx"}`: the mechanism is
  wired and gated, only the statement is missing.

**Cost:** one double reservation of the backing (tens of KiB) on a QEMU-only
board. **Blocker:** [issue 1355](1355-threadx-riscv64-builds-the-linux-threadx-port.md)
— the `threadx_riscv64` lane builds ThreadX's linux/gnu port and its board crate
does not compile, so there is no image to measure yet.

## 2. The allocator bases are undeclared numbers

Every pairing is `base - 8 * backing_u64s`. The subtrahend is stated once and
gated; the base is not derived from anything.

- **ThreadX byte pool, 4 MiB** —
  `packages/boards/nros-board-common/c/threadx_hooks.c:105`:
  `#define BYTE_POOL_BASE_SIZE     (4 * 1024 * 1024)`. The comment above it
  (lines 93-104) already says so: "the 4 MiB it is subtracted FROM was chosen by
  nobody who wrote down why", the number dates from phase 152's "4 MB byte pool",
  and "The RISC-V board is where the base has to be judged, and it has not been."
  The one measurement: a hosted `threadx-linux` talker touches **181,144** bytes
  of the pool (`nros: byte pool peak`, reporter in
  `packages/boards/nros-board-threadx/src/entry.rs:322`) — but that board's NetX
  init is a no-op overlay, so the packet pool and IP/ARP/BSD stacks that are the
  pool's other consumers allocate nothing there.
- **FreeRTOS heap** — 1145 recorded this as "FreeRTOS's 2 MiB". That literal is
  GONE for the zenoh lane: issue 1197 / phase-448 W4 replaced it with
  `default_heap_bytes(app_stack_bytes)`
  (`packages/boards/nros-board-common/src/freertos_config.rs:267`, selected in
  `packages/boards/nros-board-freertos/build.rs:177-178` when `rmw-zenoh` is on),
  every term of which is measured. What remains undeclared is the
  **cyclone/XRCE** lane, which does not enable that feature and keeps
  `packages/boards/nros-board-freertos/config/FreeRTOSConfig.h:63-65`:
  `#define NROS_FREERTOS_HEAP_KB 3072` — justified in the comment above it only as
  "sized for that heavy boot path (within the 4 MiB MPS2-AN385 SRAM budget)".
  `build.rs`'s own comment: "they keep `FreeRTOSConfig.h`'s 3 MiB, which this PR
  did not measure."

(`nros-board-freertos-posix`'s `configTOTAL_HEAP_SIZE` of 4 MiB is NOT in scope:
heap_3 wraps the host `malloc`, so it is not a budget the port enforces — its own
`FreeRTOSConfig.h:53-55` says so.)

## What closing looks like

1. **riscv64 backing** (after 1355): build the `threadx-riscv64` Rust leaves,
   `just mem-report <elf> --json` each, take the largest `EXECUTOR_BACKING` any
   compiled unit needs (issue 1388's rule, not the heaviest role's), and state
   `[board.knobs.executor] backing_u64s` in the rv64 descriptor. Confirm
   `backing + byte_pool_storage == BYTE_POOL_BASE_SIZE` via `nm -S`, that
   `just check executor-backing-arena-pairing` and `just check node-std-tests`
   accept the claim, and that the images **boot and deliver** under QEMU — not
   merely link.
2. **Bases**: derive each base from measured terms, as `default_heap_bytes` did
   for FreeRTOS/zenoh — the rv64 image's `nros: byte pool peak` for ThreadX (the
   board where NetX actually allocates), and the cyclone/XRCE images' `nros: heap
   peak` for FreeRTOS — or, where a derivation is not worth it, DOCUMENT the base
   beside its definition with the measured peak and the margin it carries. Either
   way the number stops being one "nobody wrote down why".
