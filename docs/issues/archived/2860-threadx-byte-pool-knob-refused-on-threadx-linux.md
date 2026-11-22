---
id: 2860
title: "The ThreadX byte-pool messages name `[image.<id>.knobs.threadx] byte_pool_bytes`, which `nros build` refuses for a threadx-linux image"
status: resolved
type: bug
severity: low
area: [build, memory, threadx, ux]
related: [2732, 2624]
found: 2026-11-23
resolved: 2026-11-23
---

## What

Every message about the ThreadX byte pool tells the user to raise
`[image.<id>.knobs.threadx] byte_pool_bytes`: the boot refusal
(`nros_tx_pool_refuse` in `nros-board-common/c/threadx_hooks.c`), the C
`_Static_assert`, and the build-time floor in
`threadx_sources::define_byte_pool`.

For a threadx-linux image that row is refused before anything builds:

```text
Error: configure: image `demo_bringup:threadx`
Caused by:
    `[image.threadx.knobs.threadx] byte_pool_bytes` is a knob of package
    nros_platform_threadx (owner `threadx`), which image `threadx` does not
    link: it links `zenoh`, `threadx_linux`, `threadx_linux`. State it only for
    an image whose rmw, board or platform is `threadx`.
```

(realtime-rust `[image.threadx]`, `board = "threadx-linux"`, 2026-11-23.) The
image does link the ThreadX kernel and the pool, through
`nros-board-threadx-linux`, so the knob catalog's ownership check
(`knob_catalog.rs`) disagrees with what the image runs. The environment
spelling `NROS_THREADX_BYTE_POOL_BYTES` works.

So the one remedy the refusal names cannot be followed on that board, and the
user is left with the env var or `[knobs.memory] heap_bytes` (which has its
own defect, issue 2859).

## Done when

A threadx-linux image accepts `[image.<id>.knobs.threadx] byte_pool_bytes`,
or every message names a spelling that board accepts. A test states the knob
for one image of each ThreadX board.

## Resolved (2026-11-23)

The ownership check compared owner words to the image's platform word as
the board spells it. A platform package's owner word is its CANONICAL name
(`names[0]`), and `threadx-linux` / `threadx-riscv64` are aliases of
`nros_platform_threadx`, whose owner word is `threadx`. `linked_owners` now
also links the canonical name of the platform descriptor that answers to the
image's platform word, read from the platforms tree (`declared_names`), on
both callers (`nros build` and the leaf road). A test covers both aliases and
that another platform's word stays unlinked.

Measured: realtime-rust `[image.threadx]` (threadx-linux) with
`[image.threadx.knobs.threadx] byte_pool_bytes = 900000` now builds, with a
916,640-byte pool (900,000 plus the port's 16,640-byte raise). It was
refused before.
