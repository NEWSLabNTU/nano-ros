---
id: 0192
title: FVP AEMv8-R cyclone talker link fails — picolibc SSP pulls undefined `getentropy`
status: open
severity: low
area: zephyr/fvp-aemv8r
found: 2026-07-13
phase: 287-W6
---

# FVP AEMv8-R cyclone talker link fails — picolibc SSP pulls undefined `getentropy`

`just zephyr build-fvp-aemv8r-cyclonedds` fails at final link:

```
stack_protector.c:(.text.startup.__stack_chk_init+0x20): undefined reference to `getentropy'
.../zephyr-sdk-0.16.8/.../picolibc/aarch64-zephyr-elf/lib/libc.a(libc_ssp_stack_protector.c.o):
    (.rodata.cst8+0x0): undefined reference to `getentropy'
collect2: error: ld returned 1 exit status
```

## Not a phase-287 regression

Verified 2026-07-13 by building the leaf **twice from a wiped build dir**:
once with the 287-W6 ament-shape migration applied, once at plain `HEAD`
(pre-migration). Both fail with the **identical** signature, so the red
pre-dates the migration — the lane simply hadn't been rebuilt in a while
(museum-lane effect; the fixture sweep does not cover this FVP target).

## Facts

- Board: `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`, SDK 0.16.8
  (`aarch64-zephyr-elf`, gcc 12.2.0, bundled picolibc).
- The failing object is the **SDK picolibc's** `libc_ssp_stack_protector.c.o`
  (`__stack_chk_init` wants `getentropy` for the canary seed). The build's
  `.config` does **not** set `CONFIG_STACK_PROTECTOR` — the object is dragged
  in by a `__stack_chk_guard`/`__stack_chk_fail` reference from some TU in the
  image (likely one of the cargo-built aarch64 staticlibs or a `-fstack-protector`
  default somewhere in the aarch64 flag set), and Zephyr never provides
  `getentropy` for this target (no entropy driver on the FVP model).

## Repro

```sh
rm -rf zephyr-workspace/build-aemv8r-cyclonedds-talker
just zephyr build-fvp-aemv8r-cyclonedds
```

## Candidate directions

- Find the TU referencing `__stack_chk_*` (`aarch64-zephyr-elf-nm -A` over the
  linked archives) and drop its `-fstack-protector*` flag, or
- provide a trivial `getentropy` shim (Zephyr `sys_rand_get`-backed) for the
  FVP image, or
- link `-fno-stack-protector`-built picolibc variant (specs choice).

## Cross-refs

- Phase 287 W6 Zephyr slice (migration verified unrelated).
