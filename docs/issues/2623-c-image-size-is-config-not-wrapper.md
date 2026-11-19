---
id: 2623
title: "A C or C++ image is larger than a Rust one because of three build defaults, not because the C/C++ API wraps the Rust API"
status: open
type: performance
severity: medium
area: [build, memory, api, freertos]
found: 2026-11-16
related: [0425, 1146, 1197, 1624, 1657, 2624]
---

**Owner:** unassigned (the fix options below are a design discussion with the
maintainer). **Home:** no phase yet.

## The question

C and C++ images are usually larger than Rust ones. The C and C++ APIs wrap
the Rust API, so the suspicion was wrapper overhead. This measures where the
bytes actually go before anything is changed.

## Method

A matched trio built fresh from main (5b9e8844a4 + #3982) on one board,
`mps2-an385-freertos`, RMW zenoh: `nros new <ws> --board mps2-an385-freertos
--template talker --lang {rust,c,cpp}`, then `nros build`. The three templates
use the same pools and knobs. Sizes come from `arm-none-eabi-size -A`, and the
per-crate and per-symbol split from each image's linker map (every input
section attributed to its Rust crate, from the mangled name, or to its C
archive). The experiments in "Causes" re-link copies of the built archives in
a scratch directory; no source was changed.

The Rust template builds the `dev` profile (unoptimized) by default and the
C and C++ templates build `Release`, so the defaults are not comparable. The
numbers below set `profile = "release"` on all three images.

| image (`release`) | flash (`.text`) | RAM (`.data` + `.bss`) |
| --- | --- | --- |
| Rust | 515,536 | 910,752 |
| C | 631,440 | 3,337,776 |
| C++ | 621,456 | 3,336,848 |
| C minus Rust | +115,904 | +2,427,024 |
| C++ minus C | −9,984 | −928 |

## Causes

| cause | flash (B) | RAM (B) | evidence |
| --- | --- | --- | --- |
| 1. `#[used]` surface anchors keep the whole C and C++ API in every image | **−207,592** C, **−202,096** C++ | −472 (C) | below |
| 2. The cmake road's zenoh FreeRTOS heap is the old 3,072 KiB literal | 0 | **−2,424,832** | below |
| 3. The Rust image's `release` profile has no LTO; the C/C++ staticlib has fat LTO | −15,508 (Rust) | 0 | below |
| 4. The Rust template builds `dev` by default | −399,344 (Rust, dev → release) | −1,824 | `Finished dev profile [unoptimized + debuginfo]` |

**1. Surface anchors (hypothesis b, through `#[used]`, not whole-archive).**
The C and C++ images link no archive whole (`link.txt` has `--gc-sections`
and no `--whole-archive`). They keep the unused API anyway: a C talker that
publishes one topic carries `nros_cpp_action_server_init_polling` (4,862 B),
`nros_cpp_action_client_init_polling` (4,742 B), the service and
subscription entry points and the rest of both surfaces. Nothing calls them.
The one reference to `nros_cpp_action_server_init_polling` in the image is a
pointer inside `nros_cpp::cpp_surface_anchor::CPP_SURFACE_ANCHOR` (824 B).
That table, `nros_c::c_surface_anchor::C_SURFACE_ANCHOR` and the statics
naming them (`_KEEP_C_SURFACE`, the runtime crate's `_KEEP_API`) are
`#[used]`, so rustc gives their sections `SHF_GNU_RETAIN` and GNU ld keeps
them through `--gc-sections`, with every function they name.

The anchors exist for a real reason (nros-cpp/src/lib.rs, Phase 241.D3-rev):
rustc drops a `#[no_mangle]` export of a bundled rlib from the staticlib when
nothing in the staticlib calls it, and a C++ binary may call it through the C
ABI. They solve that at the wrong stage. The anchor keeps the export in the
ARCHIVE, which is needed, and also in every LINKED IMAGE, which is not:
there `--gc-sections` would drop exactly what the image does not call.

Measured by clearing `SHF_GNU_RETAIN` on those four sections in a copy of the
image's `libnros_cpp.a` and re-linking with the build's own `link.txt`:
C 631,440 → 423,848 B, C++ 621,456 → 419,360 B. Both re-linked images boot
in QEMU against `rmw_zenohd` and publish (37 and 27 messages in 40 and 30 s),
with the same stack and heap peaks as the stock image, so nothing they run
was removed.

**2. The heap default (a storage default, hypothesis d's neighbour).** All of
the RAM difference is `ucHeap`: 3,145,728 B in the C and C++ images and
720,896 B in the Rust one. On the cargo road the board's build.rs derives it
(`nros_board_common::freertos_config::default_heap_bytes`). The cmake road
never runs that build.rs, so a zenoh image takes `FreeRTOSConfig.h`'s
fallback, `3072 - NROS_FREERTOS_SCHED_TASK_STACKS_IN_BSS_KB`. That is the
literal issue 1197 replaced on the cargo road. `cmake/platform/nano-ros-freertos.cmake`
says why it was not re-derived for zenoh: issue 1657, where no zenoh cmake
image got past session open. 1657 is resolved, and these images run. The C
talker's own boot line reads `heap peak 91888 of 3145728 bytes`, and the
tutorial's six-node project reads 251,496.

**3. LTO (hypothesis a, against the Rust image).** The C/C++ runtime
staticlib (`nros-c-image` / `nros-cpp-image` Cargo.toml) builds `release`
with `lto = "fat"`, `codegen-units = 1`. The Rust leaf's generated
Cargo.toml sets only `panic = "abort"`, so its `release` is cargo's default:
no LTO, 16 codegen units. Rebuilding the Rust image with
`CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`: 515,536 →
500,028 B.

**4. The Rust default profile.** The Rust template's image has no
`profile` and builds `dev`: 914,880 B of flash against 515,536 B at
`release`. The C and C++ templates default to `Release` (`cargo profile
release (CMAKE_BUILD_TYPE=Release)`), so out of the box they are compared with
an unoptimized Rust image.

## What is NOT the cause

With causes 1 and 3 removed (C and C++ anchor-free, Rust with fat LTO), the
Rust image is the largest:

| | Rust code | C code | flash |
| --- | --- | --- | --- |
| Rust (fat LTO) | 288,082 | 211,854 | 500,028 |
| C (no anchors) | 227,242 | 216,516 | 423,848 |
| C++ (no anchors) | 223,557 | 215,712 | 419,360 |

- **Wrapper overhead (the question).** The C API layer that remains (`nros_cpp` +
  `nros_c` crates, 56,988 B) is larger than the Rust API crate `nros`
  (42,238 B) by 14,750 B. Against that, the Rust image spends +25,414 B in its
  generated entry crate, +10,928 B in `nros_board_freertos` (the C road's entry
  is 1,263 B of C), +9,810 B more `nros_node` and +10,238 B more
  `nros_rmw_cffi`, which is generic code monomorphised per user type.
- **core::fmt at the FFI boundary (e).** `core` is 20,004 B in the C image and
  40,168 B in the Rust one, so the C boundary pulls in less formatting, not
  more.
- **A second serializer / C message structs (d).** `std_msgs__nano_ros_c` is
  370 B.
- **C++ carrying nros-c (f).** Both C and C++ images link the same
  `libnros_cpp.a` umbrella (issue 0425 chose that umbrella for a C/C++ binary).
  So the C image carries nros-cpp, not the other way round, and once the
  anchors are gone the two differ by 4,488 B.
- **Type erasure (c)** was not separated out. The C road's `nros_rmw_cffi`
  is SMALLER than the Rust road's after the anchors go (62,414 against
  72,652 B), so whatever erasure costs, it is below what monomorphisation
  costs here.

## Fix options

1. **Anchor the archive, not the image.** Keep the exports in `libnros_cpp.a`
   without retaining them in the final link. One way: keep the anchor
   `#[used]` for the staticlib, but emit it into a section the image's linker
   script or link line discards, or link with `-Wl,--undefined` only for the
   symbols a header actually declares. The other way: stop relying on rustc's
   staticlib DCE by building the API crates with their exports as the
   staticlib's own roots (a `[lib] crate-type = ["staticlib"]` per surface,
   or `-C export-executable-symbols`-style handling). **Saves about 200 KB of
   flash per C/C++ image (33 %).** Needs a link check that a C++ binary can
   still call a C-API function the C++ FFI never references (the case the
   anchors were added for).
2. **Give the cmake road the derived heap.** Have
   `cmake/platform/nano-ros-freertos.cmake` select the cargo road's
   `default_heap_bytes` for zenoh, as it already selects
   `NROS_FREERTOS_DDS_HEAP_KB` for Cyclone and XRCE. The measured peaks
   (91,888 B for the template, 251,496 B for the tutorial's six nodes) both
   fit the 704 KiB the cargo road derives. **Saves 2,424,832 B (2.3 MiB) of RAM per C/C++
   FreeRTOS zenoh image** (3,145,728 → 720,896 B).
3. **Make the defaults agree.** Build the Rust template's image at `release`
   by default, as the C and C++ ones are, and give the cargo leaf's `release`
   the same `lto = "fat"`, `codegen-units = 1` the staticlib uses. **Saves
   about 15 KB more on a Rust image, and makes a default Rust image about 45 %
   smaller than it is today.**

Options 1 and 2 together take a C talker from 631,440 B flash and 3.34 MB RAM
to about 424 KB and 0.91 MB, below the Rust image on flash and level with it
on RAM.

## Progress

**Option 2 landed (2026-11-16, maintainer-approved).** `nano-ros-freertos.cmake`
selects `NROS_FREERTOS_HEAP_DEFAULT_ZENOH` for a zenoh image, and
`FreeRTOSConfig.h` sizes it as `NROS_FREERTOS_ZENOH_HEAP_KB` = 512. That is the
cargo road's `default_heap_bytes` at the C/C++ carrier's 64 KiB app stack, not
at the Rust road's 128 KiB, so it is 512 KiB rather than the 704 KiB estimated
above: −2,621,440 B of RAM per image (3,145,728 → 524,288).
`freertos_config::tests::the_header_states_the_derived_zenoh_default` holds the
literal to the derivation and to 1.4x the tutorial's measured 251,496 B.
Measured on QEMU mps2-an385 against `rmw_zenohd`: the `--lang c` talker
template boots with `ucHeap` at 524,288 B, prints
`heap peak 91888 of 524288 bytes`, and publishes 80 messages in 84 s.
Two deeper in-tree images on the same default: `examples/workspaces/c`'s
`demo_bringup:freertos` (talker + listener in one image) peaks at 95,744 B and
delivers to itself, and `examples/workspaces/realtime-cpp`'s
`demo_bringup:freertos` (three sched tasks, the deepest C++ FreeRTOS image)
peaks at 100,728 B with every task ticking. The tutorial's chapter 1-7 C
project (six nodes: talker, listener, service server and client, action server
and client), replayed from the book's `probe=` blocks with
`scripts/probe/extract-book-steps.py --track installed --variant qemu --lang c`
and booted the same way, prints `heap peak 251552 of 524288 bytes (272736
free)`: 2.08x, 52 % free. In that run the listener heard every reading, the
client got its sum and the action client got its full Fibonacci result.

Swept for the same pattern on the other platforms' cmake roads: none. ThreadX
sizes its byte pool with one literal (`BYTE_POOL_BASE_SIZE`, 4 MiB) on BOTH
roads. That is not a road split, but it is the same "fixed literal, not
derived" shape, so it is filed as [issue 2624](archived/2624-threadx-byte-pool-base-is-a-literal.md)
and not changed here. Zephyr and NuttX take their
heaps from Kconfig on every road, and freertos-posix uses heap_3 over the host
`malloc`.

Options 1 and 3 are still open.
