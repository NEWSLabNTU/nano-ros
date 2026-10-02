---
id: 1624
title: "FreeRTOS's non-zenoh heap base (`NROS_FREERTOS_HEAP_KB` 3072 in the shared
  `FreeRTOSConfig.h`) reaches exactly one in-tree image, and that image is on
  hardware with no QEMU model — so it cannot be derived the way zenoh's was"
status: resolved
type: tech-debt
area: boards, freertos
severity: low
found: 2026-10-01
related: [1557, 1197, 1145]
---

## Background

Split from [issue 1557](1557-threadx-riscv64-backing-and-allocator-bases-unmeasured.md)
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

## Resolution (2026-10-02)

**Route (b), and the image already existed.** `workspace-cpp-mps3-an536-freertos`
is a QEMU-runnable FreeRTOS Cyclone image — the SAME C++ `demo_bringup` entry,
Cortex-R52 and `GCC/ARM_CRx_No_GIC` port as the S32Z270 row that compiles the
default. 1557 ruled it out because the an536 board states its own 32 MiB; that
makes its peak UNCONSTRAINED, which is exactly what a derivation wants.

**It had not booted since phase-206 W2** — `cyclonedds.log: cannot open for
writing` → `RMW session open failed`. That is issue 1634, fixed first
(`9318f4e30`). Its pre-fix "peak" of 96,744 bytes was a session that never
opened, which is why the new test asserts delivery BEFORE reading the peak.

**Measured** (qemu `mps3-an536`, `-net socket,mcast`, store QEMU 11.0.0-nros2,
fixture built at this tree, 2026-10-02): in-image talker → listener delivering
(`Received: 0..49`), `nros: app task stack peak 10192 of 65536 bytes`,
**`nros: heap peak 447944 of 33554432 bytes`**.

**Derived** like `default_heap_bytes`, in `nros-board-common`
(`freertos_config.rs`): `default_dds_heap_bytes(app_stack) = app_stack +
DEFAULT_HEAP_DDS_WORKING_SET_BYTES`. One app-task slot, not three: on the C/C++
carrier tier stacks/TCBs are entry statics (1598) and tier executor backing is
`.bss` (1568). Working set measured 447,944 − 65,536 = 382,408; the term is
589,824 (1.54x). At the carrier's 65,536-byte stack: **655,360 B = 640 KiB**,
stated in `FreeRTOSConfig.h` as `NROS_FREERTOS_DDS_HEAP_KB`.

**The rule is PER RMW, chosen where the RMW is known** (revised 2026-10-03
after rebasing onto main's `fba1b024a9`). On the CMAKE road — every C/C++ image,
whose kernel is `freertos_kernel` and never runs the board build.rs — the
header's default is the whole answer, whatever the RMW, so a flat number there
would have reached the zenoh images too. So:

| road / RMW | heap default | where decided |
| --- | --- | --- |
| cargo, `rmw-zenoh` | `default_heap_bytes(app_stack)` (unchanged) | board `build.rs` passes `-DNROS_FREERTOS_HEAP_KB` |
| cmake, `cyclonedds` / `xrce` | `NROS_FREERTOS_DDS_HEAP_KB` = 640 KiB, **no** tier subtraction | `cmake/platform/nano-ros-freertos.cmake` defines `NROS_FREERTOS_HEAP_DEFAULT_DDS=1` PUBLIC on `freertos_kernel` |
| cmake, `zenoh`; cargo with no RMW feature | fallback `3072 − NROS_FREERTOS_TIER_STACKS_IN_BSS_KB` (main's, unchanged) | the header |
| any, explicit `NROS_FREERTOS_HEAP_KB` | that value, never adjusted | the caller |

No tier subtraction on the DDS default, deliberately: the measured image has no
tiers, and on the cmake road spawned-tier stacks (1598) and tier executors
(1568) are `.bss`, so the derivation never contained them — subtracting them
would take them out a second time (a two-tier image with 256 KiB tiers would
lose 512 KiB of a 640 KiB budget). Main's subtraction stays where it belongs, on
the 3072 fallback that DID contain them.

The zenoh cmake default was NOT re-derived: on this host every zenoh C/C++
FreeRTOS image opens its session and never ticks — on pristine `origin/main`
too — so no peak past session open could be read. Filed as
[issue 1657](../1657-freertos-zenoh-cmake-images-open-a-session-and-never-tick.md);
cutting a budget nobody could measure was not done here.

**`ucHeap`, before (origin/main + this branch's header untouched) → after**, by
`arm-none-eabi-nm -S`:

| image (row) | RMW | before | after |
| --- | --- | --- | --- |
| `workspace-cpp-s32z270-freertos` | Cyclone | 0x300000 (3 MiB; no tiers) | **0x0A0000 (640 KiB)** |
| `workspace-cpp-mps3-an536-freertos` | Cyclone | 0x2000000 (board's own 32 MiB) | 0x2000000 (unchanged) |
| `workspace-cpp-freertos-realtime` | zenoh | 0x280000 (+ 2 x 0x40000 tier stacks) | 0x280000 (unchanged) |
| `workspace-c-freertos-realtime` | zenoh | 0x2C0000 (+ 1 x 0x40000) | 0x2C0000 (unchanged) |
| `workspace-cpp-freertos` / `-c-` / `-mixed-` | zenoh | 0x300000 | 0x300000 (unchanged) |

The selector reached exactly the TUs that read `configTOTAL_HEAP_SIZE` on the
S32Z270 build (`freertos_kernel`, `nros_platform_freertos`, the entry and the
component libs — `flags.make`), and no TU of any zenoh build.

**Kept measured.** Unit tests hold the header literal to the derivation and the
carrier template's `.app_stack_bytes` to the constant it uses
(`the_header_states_the_derived_dds_default`), and the measured peak under the
budget. Runtime: `freertos_qemu::an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default`
boots the fixture row, requires ≥3 in-image deliveries, parses the heap-peak
line and fails if it exceeds the header's default (PASS, 37 s). Before: no test
consumed the an536 row.

**NOT measured.**
- A remote participant on the LAN (proxy state per peer) — hence the 1.54x.
- The S32Z270 hardware image itself (no QEMU model); the twin is the evidence.
- XRCE: still no FreeRTOS XRCE image. Its working set is static pools, so a
  Cyclone-derived bound over- rather than under-provisions it.
- newlib's `_sbrk` heap: C++ `new`/libc `malloc` on these images go through
  newlib's `_malloc_r`, NOT heap_4 (`nm`: `malloc` → `_malloc_r`, `_sbrk`), so
  the heap-peak line does not include them. This knob sizes `ucHeap` only;
  shrinking it by 2.4 MiB leaves that much more for the sbrk region.
- The S32Z270 image links at 640 KiB; it cannot be booted here.
- The zenoh cmake road (issue 1657).
