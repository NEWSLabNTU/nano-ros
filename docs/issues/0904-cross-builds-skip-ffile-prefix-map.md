---
id: 904
title: "`nros_variant_symbol.o` still embeds its absolute `OUT_DIR` on the NuttX
  cross build, so phase-340 W6's byte-identical-artifact fix does not hold there"
status: open
type: bug
area: build
related: [phase-340, phase-400, issue-0616, issue-0360, issue-0369]
---

## Problem

phase-340 W6 made `libnros_c.a` byte-identical across two `--target-dir`s by
adding `-ffile-prefix-map={OUT_DIR}=/nros-out` to the `cc::Build` that compiles
the generated `nros_variant_symbol.c`
(`packages/tooling/nros-build-helpers/src/c.rs`, `emit_variant_symbol`). Its
comment records the measurement: that TU was the ONLY carrier of the embedded
path, "in `nros_variant_symbol.o`, twice".

On the NuttX ARM cross build the path is still there.

Built `packages/boards/nros-board-nuttx-qemu/nros-nuttx-ffi` into two scratch
target dirs. The generated source is byte-identical in both — same variant
suffix, `nros_config_variant_sz_7d5cf103c1b9382c` — and `arm-none-eabi-nm`
reports identical symbol tables. The objects still differ, and `strings` says
the whole difference is the target dir:

```
< /home/aeon/repos/nano-ros/tmp/nx-b/armv7a-nuttx-eabihf/debug/build/nros-c-.../out
< /home/aeon/repos/nano-ros/tmp/nx-b/.../out/nros_variant_symbol.c
---
> /home/aeon/repos/nano-ros/tmp/nx-a/armv7a-nuttx-eabihf/debug/build/nros-c-.../out
> /home/aeon/repos/nano-ros/tmp/nx-a/.../out/nros_variant_symbol.c
```

## It is not the compiler

`arm-none-eabi-gcc` 13.2 accepts the flag and it does what W6 wanted — compiling
the same one-line TU by hand, with and without:

```
without -ffile-prefix-map : real path present (1 hit)
with    -ffile-prefix-map : real path absent (0 hits), `/nros-out` present
```

So the flag would strip it. It is not reaching this compile. The likely
suspect is `flag_if_supported`'s probe under a cross `CC`/target — it fails
SILENTLY by design, which is exactly why a fix that depends on it can stop
working without anything going red. Confirm before fixing: print the resolved
argv, or switch to an unconditional `flag()` for compilers known to take it.

## Why it matters

The artifact is the point of W6. Two target dirs producing byte-different
`libnros_c.a` is what defeats cross-target-dir comparison and any content-addressed
cache (sccache) for the cross lanes — and issue 0616 already records what a
second `--target-dir` identity costs. The host lane W6 measured is fixed; the
cross lanes silently are not.

Severity is low: the difference is debug-info/`__FILE__` only, symbols are
identical, and no image behaves differently. This is a determinism/caching
defect, not a correctness one.

## How it was found

Incidentally, while verifying an unrelated claim in phase-400 W2a — that removing
cbindgen from the host build graph does not change NuttX firmware. It does not:
target-side fingerprints match including `-C metadata` hashes, target-side
artifacts are byte-identical, and this one object was the sole diff, caused by
the two scratch dirs rather than by the change. The check that cleared one claim
surfaced this one.

## Sweep

Every `flag_if_supported` whose effect is an INVARIANT rather than an
optimisation deserves the same question — a silent no-op there is a gate that
stopped gating:

```sh
grep -rn 'flag_if_supported' packages/ --include=*.rs
```
