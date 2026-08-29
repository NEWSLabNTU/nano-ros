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

## And it is NOT `flag_if_supported` — the first diagnosis here was wrong

This issue originally named `flag_if_supported`'s probe as the likely suspect,
on the reasoning that it fails silently. **Measured, and it does not fail.** A
standalone crate calling `cc::Build::is_flag_supported` with the identical flag
string reports `true` for both the host and an `armv7a-none-eabi` cross build,
and the object it then produces carries `/nros-out` and no real path — the flag
is applied and works, through cc-rs, on a cross target.

Reproducing the real build's probe by hand agrees: `flag_check.c` compiled with
the full nros-c flag set plus the candidate flag exits 0. And cc-rs's probe
artifacts (`flag_check`, `flag_check.c`) are present and freshly timestamped in
nros-c's `OUT_DIR`, so the probe ran.

So: the probe runs, the probe passes, the flag works when applied — and the
emitted `nros_variant_symbol.o` still has zero `/nros-out` hits and two hits of
the real target-dir path. Reproduced on a forced rebuild (fingerprints removed,
build script re-run, nros-c recompiled), not a stale artifact. Whatever the
mechanism is, it is not the one this issue first named, and the next person
should not re-run that experiment.

Note for reproducing: `CC_ENABLE_DEBUG_OUTPUT=1` buys nothing here — build-script
stdout is captured by cargo unless the script fails, so cc-rs's debug lines never
surface. Getting the resolved argv needs the build script to emit it itself.

## The fix that does not depend on the answer

The path is only in the object because the TU is GENERATED into `OUT_DIR`, which
differs per target dir. Compile a TRACKED source instead and the whole class
goes away — no prefix-map, no probe, nothing silent:

* keep `packages/api/nros-c/src/variant_symbol.c` in the tree, with the symbol
  name as a macro:
  `__attribute__((weak)) const unsigned char NROS_VARIANT_SYMBOL = 0;`
* pass the suffix as `-DNROS_VARIANT_SYMBOL=nros_config_variant_sz_<hash>`.

The source path is then identical in every target dir, so the object is
deterministic by construction rather than by a flag that has to keep working.
`-ffile-prefix-map` can stay as belt-and-braces, but nothing would depend on it.
Verify the same way this was measured: build the leaf into two target dirs and
diff `md5sum` of the target-side objects.

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
