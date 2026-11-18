---
id: 2710
title: "The stack-floor walk always keeps a fat-LTO cargo run's merged module, so it walks a run whose archive the image never links"
status: open
type: bug
severity: medium
area: [tooling, stack-floor, cmake]
related: [2653, 2688, 2550, 2400]
found: 2026-11-18
---

**Home:** no phase. Found while re-walking the `talker/c/zenoh@mps2-an385-freertos`
`[[drift]]` row, which fell 16.6 % when a C image began running cargo once.

## What happens

`nros_stack_floor_check` (`cmake/NanoRosStackFloor.cmake`) hands the walker
its Rust bitcode as globs, among them
`${CMAKE_BINARY_DIR}/cargo/*/<target>/<profile>/deps`, whose `*` matches every
cargo run in the build directory. That alone is answered: since issue 2521,
`select_linked_bitcode` (`scripts/nros-stack-floor.py`) drops a codegen unit
the link map does not load, and walks a byte-identical duplicate once.

The gap is fat LTO. `expand_inputs` puts a deps dir that holds a
`*.rcgu.lto.input.bc` into `fat_bcs`, and `bcs = fat_bcs + bcs` keeps it
unconditionally ("that dir's module is the image's code and is always
kept"), because the map does not name a fat module by unit. So every fat-LTO
run in the build dir is walked, linked or not.

Until a C image ran cargo once, a C image on the CMake road had two runs,
`cargo/nros-c-image_*` and `cargo/nros-cpp-image_*`, both fat LTO in the
release profile, and it linked only `libnros_cpp.a`.

## Measured (newslab-118, the FreeRTOS C tutorial row)

One build of main, before the one-run change:

| Walk | OPEN depth |
| --- | --- |
| as the build ran it (both runs' bitcode) | 136,456 B |
| same build, `--bitcode-dir` narrowed to `cargo/nros-cpp-image_*` | 113,264 B |
| the one-run tree, host | 113,264 B |
| the one-run tree, merge queue | 113,264 B |

The image's link map names `libnros_cpp.a` 7,253 times and `libnros_c.a`
zero times. The unit filter ran ("402 Rust codegen unit(s) … not in the
link: not walked"), and the 44 unit keys the two deps dirs share are
byte-identical, so a same-key duplicate is not the cause. Each dir holds one
`*.rcgu.lto.input.bc`, so both merged modules were kept. 23,192 B of the old
walk ran through the merged module of an archive the image does not contain. The recorded row was too deep, not too shallow,
so it wasted RAM rather than overflowing, but the walk was describing a
different program.

## What else it can reach

Any image whose build directory holds a FAT-LTO cargo run it does not
link (a thin-LTO or non-LTO run goes through the unit filter), for example:

- a workspace configure that builds several runtimes or Rust staticlibs
  under one `CMAKE_BINARY_DIR/cargo/`, where an entry links only some of
  them;
- a stale run left in the build dir by an earlier configure (a renamed
  crate, a changed feature set keyed to a new hash).

Not checked here: which other `[[observed]]`, `[[drift]]` or
`[[allowance]]` rows were walked with such extra bitcode.

## Possible shape

Keep a fat dir only when the link map loads an archive built in that dir's
target tree, matched by the archive's cargo run directory, never by a name
pattern. Self-test: two fat dirs, only one linked, and only that one is
walked. The stack-floor owner (nros 133) is taking this into the W9 work and
will then re-check every C/C++ `[[observed]]` / `[[drift]]` row walked on a
two-run tree.
