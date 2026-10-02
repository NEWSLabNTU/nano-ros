---
id: 1646
title: "sccache served a staticlib that still held the OLD build-script C object after the C source changed — a museum binary with a FRESH cargo verdict"
status: open
type: bug
area: build
severity: medium
found: 2026-10-02
related: [1636, 0475, 0820]
---

## What

While measuring issue 1636 on the Zephyr native_sim lane (`just zephyr
build-one`, which exports `RUSTC_WRAPPER=sccache`), I edited
`packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c` (compiled by
`zpico-sys`'s build script into `libzpico_platform_aliases.a`). The rebuild:

- re-ran the build script. `out/libzpico_platform_aliases.a` held the NEW
  object: 14 global symbols.
- the `zpico-sys` rlib bundled the new object (14).
- `libnros_cpp.a`, relinked in the same run ("Compiling nros-cpp", the whole
  graph in 1.5 s), still held the OLD object: 47 global symbols, with debug
  line numbers from the pre-edit file. The image link then reported 33
  duplicate symbols that the source no longer defined.

Running the same `cargo build -p nros-cpp` by hand without the wrapper
produced the correct 14-symbol staticlib. The next `just`-driven build put the
47-symbol one back. Only touching `nros-cpp/src/lib.rs` with
`SCCACHE_RECACHE=1` produced a correct image.

So a cached staticlib (or the rlib it was assembled from) is keyed on
something that does not include the bundled native object of a dependency.
`cmake/NanoRosCorrosion.cmake:531` says sccache does not cache
`--crate-type=staticlib`. On this lane, that is not what was observed.

## Reproduce

1. `just zephyr build-one c/talker zenoh native_sim/native/64` (green).
2. Change a symbol set in `platform_aliases.c` (e.g. `#if 0` a function).
3. Rebuild the same way, then `nm` the `platform_aliases.o` member of
   `build/corrosion-cargo/zephyr/<key>/…/libnros_cpp.a`, and compare it with
   `build/zpico-sys-*/out/libzpico_platform_aliases.a`.

## What closing needs

Find which unit is served stale (the `nros-cpp` staticlib, or the `zpico-sys`
rlib as rustc reads it), and either key it correctly or exclude it from the
wrapper. Add a freshness assertion that compares the bundled member against
the build-script output.
