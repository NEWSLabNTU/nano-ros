---
id: 2579
title: "A serial-only bare-metal XRCE port meets three gaps that fail quietly: UDP symbols it never uses, a one-shot executor backing, and a NOLOAD heap"
status: resolved
type: limitation
area: [rmw, xrce, boards, book]
severity: medium
found: 2026-09-16
related: [phase-564, rfc-0012, rfc-0002, 2576, 2577, 2578]
resolved: 2026-11-15
resolved_in: "phase-564 W3"
---

**Home: [phase-564](../../roadmap/phase-564-xrce-device-session-robustness.md) W3.**

GitHub: NEWSLabNTU/nano-ros#1049 (reported by wicanr2)

**Standalone.** Same bring-up as the three XRCE issues 2576, 2577 and 2578. No
active phase owns out-of-tree bare-metal porting; RFC-0012 is the board
integration design.

## Context

nano-ros (`fb89182`) on a bare-metal STM32F427 with **serial-only XRCE**, no
Ethernet, no in-tree board crate, using the library layer directly (not
`nros::main!()`), because the application owns its interrupt priorities (1 kHz
safety loop at RTIC priority 8, CAN at 4, the protocol UART at 3, the executor
spinning in `idle`). It works: an XRCE session against `MicroXRCEAgent` v2.4.3,
interop with ROS 2 Jazzy, 200 samples at 20 Hz with no loss. Flash 116.9 KB,
RAM 99.9 KB, CCM 49.7 KB.

The porting guide the reporter used (`book/src/porting/stm32f4-out-of-tree.md`)
is no longer in the tree; the closest page now is
`book-internals/src/platform/porting-pitfalls.md`.

## 1. A custom-transport-only build still needs `nros_platform_udp_*`

`packages/rmw/xrce/xrce-sources.txt` lists `transport_nros_udp.c` under
`backend_core` (still so on 2026-11-15), so it is compiled when the image
only uses a custom (serial) transport. The link then fails on nine undefined
symbols (`nros_platform_udp_create_endpoint`, `_open`, `_read`, `_read_exact`,
`_send`, `_close`, `_free_endpoint`, `_listen`, `_set_recv_timeout`), and a
board with no IP stack has to stub all nine. Gate the TU on the same condition
the profile defines use (`posix_ip`-like), or document it.

## 2. `Executor::open` can claim the static backing only once

`Executor::open` takes `EXECUTOR_BACKING` through `backing::take()`, guarded by
the one-shot `TAKEN` latch (`packages/core/nros-node/src/executor/backing.rs`,
still a latch on 2026-11-15, by design: "an executor that drops does not give
it back"). A second call falls back to `Box::leak(Box::new_uninit_slice(..))`.

That matters when the Agent is not up yet: retrying `Executor::open` once a
second (normal for a vehicle that powers on before its host) succeeds only the
first time. The retry allocates ~87 KB from the platform heap, fails, and the
image panics in `alloc::alloc`, rebooting in a loop with an allocator panic
rather than "no agent yet".

Workaround: own the storage (`static BACKING: [MaybeUninit<u64>; N]`,
`Executor::open_in(&cfg, backing, ExecutorSizing::DEFAULT)`) with
`NROS_EXECUTOR_BACKING_U64S=0`. Direction: document it, or release the latch
when the executor is dropped on the open's error path (where nothing has been
lent out yet).

## 3. A heap in a NOLOAD section must be zeroed by the board

`zpico_alloc::FreeListHeap` lazy-initialises from an all-zero state. A heap in
CCM (`.ccm_heap`, `NOLOAD`) is not cleared by cortex-m-rt startup (only
`.bss` is), so the first allocation walks a garbage free list. Zeroing the
region from linker symbols first fixes it. Worth one line next to the
`NROS_HEAP_SIZE` knob.

## Also reported

- `nros::main!()`'s RTIC path resolves its board through a hardcoded table
  (`rtic_board_spec_for`, which knew only `rtic-mps2-an385`) and emits tasks at
  priority 1, so an application with its own priority assignment cannot use
  it. The library layer (install transport -> `register()` ->
  `Executor::open_in` -> register nodes -> `spin_once`) works; if that is the
  intended path, name it in the docs. (Phase-562's framework field may change
  this.)
- `nros dev codegen rust` (formerly `nros generate-rust`) finds workspace-local interface packages through
  `NROS_INTERFACE_SEARCH_PATH`, which no porting doc mentions.

The reporter offered a PR against the porting chapter for whichever parts are
wanted.

## Progress (2026-11-15)

Gaps 1 and 2 are fixed and gap 3 is documented; phase-564 W3 has the
measurements. Still open: the porting page (the library path and
`NROS_INTERFACE_SEARCH_PATH`), which is with the book owner. This issue
closes when it lands.

## Resolution (2026-11-15)

All three gaps are fixed or documented on main: the code in 03eb529eff (the
`ip`/`no_ip` UDP split, the failed-open give-back, the NOLOAD note), and the
porting page in c90242ce57, the book's "A bare-metal app that owns its main
loop" how-to under Platforms. It names the library path, the `NROS_LINK_IP=0`
build, `NROS_INTERFACE_SEARCH_PATH` and the NOLOAD heap zeroing.
