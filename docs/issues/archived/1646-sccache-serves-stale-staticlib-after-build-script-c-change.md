---
id: 1646
title: "sccache served a staticlib that still held the OLD build-script C object after the C source changed — a museum binary with a FRESH cargo verdict"
status: resolved
resolved_in: 2026-10-03
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

## Resolution (2026-10-03)

**The unit served stale is the `nros-cpp` STATICLIB, from sccache's cache.**
The missing edge is in sccache's key. sccache 0.8 refuses only bin, dylib,
cdylib and proc-macro, and it caches `--crate-type staticlib` (the
`NanoRosCorrosion.cmake` comment was wrong). It keys a rustc call on the call's
sources, its `--extern` rlibs and its `-l static=` libs.

A staticlib bundles its TRANSITIVE closure, and rustc reaches that closure
through `-L dependency=`, which is not hashed. `nros-cpp` has no
`--extern zpico_sys`; it reaches `zpico-sys` through `nros-rmw-zenoh`. After a
build-script C edit, `nros-rmw-zenoh` recompiles to a byte-identical rlib,
because rlib metadata records crate hashes, not native objects. So `nros-cpp`
presents its old key and gets its old archive back.

**Reproduced with an incremental cargo build** (host,
`nros-cpp --features rmw-zenoh-cffi,platform-posix,std`, shared target dir,
`RUSTC_WRAPPER=sccache`). A marker function added to `platform_aliases.c`
lands in the `out/libzpico_platform_aliases.a` that cargo re-ran the build
script for, and in the `zpico-sys` rlib. It is **absent from `libnros_cpp.a`**:
`nm` counts 1, 1, 0. The `nros-cpp` rustc line names seven `--extern`s and no
`zpico_sys`.

**Fix.** `RUSTC_WRAPPER` now points at `scripts/bin/rustc-wrapper/sccache`, a
shim that runs any staticlib-emitting rustc call directly and hands every other
call to the real sccache. Every rlib stays cached; a staticlib is one final
assembly per image. The shim keeps the file name `sccache` so cc-rs's C-compiler
prefix behaves as before (issue 1580). Both producers point at it: the root
`justfile` (with `NROS_SCCACHE_REAL`) and `gate.yml`'s cache-warming job.

**Fix verified the same way.** After a second edit of the marker, built
through the shim, the marker is in both the build-script archive and
`libnros_cpp.a` (1 and 1).

**Gate:** `just check rustc-wrapper-staticlib`. It drives the shim with a fake
`sccache` and a fake `rustc` for every `--crate-type` spelling, a C compile,
and a host with no sccache. It also requires every `RUSTC_WRAPPER` producer
that names sccache to name the shim. Mutation evidence:

- restoring `gate.yml`'s bare `RUSTC_WRAPPER=sccache` makes the gate fail;
- making the shim never match `staticlib` makes the gate fail on two routing
  rows.

NOT done: the Zephyr `build-one` reproduction from the report was not re-run.
The fix is at the wrapper both lanes share, and the cargo-level reproduction is
the same unit.

