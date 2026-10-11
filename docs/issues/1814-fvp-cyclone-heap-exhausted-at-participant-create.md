---
id: 1814
title: "The FVP Cyclone image aborts at `dds_create_participant`: Cyclone's 1 MiB receive buffer does not fit the 1 MiB nros heap"
status: open
type: bug
area: [rmw, zephyr, memory]
severity: medium
found: 2026-10-11
related: [1288, 1633]
---

# What happens

`just zephyr verify-fvp-runtime` (`fvp_runtime_ws::fvp_ws_entry_two_tier_publishes`)
fails on `main` at `43538b31e`. The image boots, starts Cyclone, and dies in
`session_create` before any tick:

```text
*** Booting Zephyr OS build v3.7.0 ***
<inf> cyclonedds: session_create: calling dds_create_participant
nros: HEAP EXHAUSTED (TOO SMALL): request 1048624 bytes, arena 1049088 bytes,
      free 1013728 bytes, largest free block 1013680 bytes, caller 0x463e8
abort()
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 1
```

`0x463e8` is `ddsrt_malloc` (`third-party/dds/cyclonedds/src/ddsrt/src/heap/nros/heap.c:66`).

Measured on BOTH the hand-written `realtime-cpp/src/fvp_entry` image (built on
this tree before issue 1288's migration) and the generated one after it, whose
merged `.config`, devicetree and section sizes are identical. So it is not the
migration; it predates it.

# Why (read, not yet confirmed by a fix)

- Cyclone's default `Sizing/ReceiveBufferSize` is 1 MiB
  (`src/core/ddsi/defconfig.c: cfg->rbuf_size = 1048576`), and the receive
  buffer pool allocates one buffer of that size plus a header — the
  1,048,624-byte request.
- `nros-rmw-cyclonedds`'s baked config (`cyclone_config.hpp`) lowers it to
  64 KiB, but that baseline is compiled only for FreeRTOS, ThreadX and
  `CONFIG_BOARD_NATIVE_SIM`. On any OTHER Zephyr board (the FVP here) Cyclone
  runs on its own defaults.
- Since issue 1633 (2026-10-05) ddsrt's heap is the nros platform funnel,
  `CONFIG_NROS_ZEPHYR_HEAP_SIZE`, whose Cyclone default is exactly 1 MiB. Before
  it, ddsrt allocated from libc `malloc` (the image's 1 MiB newlib minimum plus
  `HEAP_MEM_POOL_SIZE`). phase-477 W1 records the lane PASSING on 2026-10-05,
  the day 1633 landed; whether that run predates it was not checked.

# Fix directions

- Give real-hardware Zephyr Cyclone images the same `Sizing` baseline the other
  embedded arms get (the receive buffer is the board's RAM budget, not a
  desktop default), or
- size `NROS_ZEPHYR_HEAP_SIZE` from what Cyclone will ask for on such a board.

Acceptance: `just zephyr verify-fvp-runtime` passes.
