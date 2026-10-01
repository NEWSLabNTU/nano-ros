---
id: 1624
title: "FreeRTOS's non-zenoh heap base (`NROS_FREERTOS_HEAP_KB` 3072 in the shared
  `FreeRTOSConfig.h`) reaches exactly one in-tree image, and that image is on
  hardware with no QEMU model — so it cannot be derived the way zenoh's was"
status: open
type: tech-debt
area: boards, freertos
severity: low
found: 2026-10-01
related: [1557, 1197, 1145]
---

## Background

Split from [issue 1557](archived/1557-threadx-riscv64-backing-and-allocator-bases-unmeasured.md)
item 2, whose ThreadX half is closed by measurement. Its FreeRTOS half asked for
the cyclone/XRCE base to be derived from those images' `nros: heap peak`, as
issue 1197 did for zenoh (`freertos_config::default_heap_bytes`).

## What 1557 found (2026-10-01)

The 3 MiB default in `packages/boards/nros-board-freertos/config/FreeRTOSConfig.h`
is compiled only by an image whose board crate is built WITHOUT `rmw-zenoh`
(`nros-board-freertos/build.rs` sizes every zenoh image from
`default_heap_bytes`). In-tree:

- `nros-board-mps2-an385-freertos` has no `rmw-xrce`/`rmw-cyclonedds` feature;
  its six Rust leaves are zenoh, and its C/C++ fixture rows are zenoh-only.
  Its Rust Cyclone fixtures were retired in phase 220.C (`just/freertos.just`).
- `nros-board-mps3-an536-freertos` states its own 32 MiB.
- `nros-board-s32z270-freertos` `#include`s the shared header first, so it
  takes 3072 — and `workspace-cpp-s32z270-freertos` (Cyclone, C++) is the one
  fixture row that compiles it. s32z270 is real hardware with no QEMU model.

So there is no image here whose `nros: heap peak` line can be read, and the
base cannot be derived without one. The finding is now written beside the
`#define` so the number is no longer one "nobody wrote down why".

## What closing looks like

Either (a) run `workspace-cpp-s32z270-freertos` on hardware, read its
`nros: heap peak` line, and derive or document the base from it; or (b) build
the first QEMU-runnable cyclone/XRCE FreeRTOS image (mps2-an385 would need the
board features) and derive from that. Until one exists, the 3 MiB is a
stated, unmeasured budget for one hardware image.
