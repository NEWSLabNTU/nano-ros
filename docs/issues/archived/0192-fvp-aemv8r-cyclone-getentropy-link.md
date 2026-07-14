---
id: 0192
title: FVP AEMv8-R cyclone talker link fails — picolibc SSP pulls undefined `getentropy`
status: resolved
resolved_in: "2026-07-14 — comma-joined whole-archive token in zephyr nros_generate_interfaces (the #193 CMake<3.24 flag-dedup class)"
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

## Resolution (2026-07-14)

None of our code references `__stack_chk_*` at all (an `nm -A` sweep over
every archive on the link line finds ZERO undefined `__stack_chk`/`*_chk`
refs). The map shows every `libc_ssp_*` member — including
`stack_protector.c.o` — included via `(--whole-archive)`: the link line's
SECOND whole-archive bracket (the generated-interface FFI libs) was never
closed, so picolibc's trailing `-lc` was swallowed whole-archive and every
SSP member was force-included; `__stack_chk_init` then wants `getentropy`,
which this target never provides.

Root cause is the #193 class on the Zephyr generator:
`zephyr/cmake/nros_generate_interfaces.cmake` linked each FFI lib as three
separate items (`-Wl,--whole-archive` `<lib>` `-Wl,--no-whole-archive`),
and CMake < 3.24 de-duplicates repeated identical flag items across the
aggregated link line — with two generated packages (std_msgs +
builtin_interfaces) plus Zephyr's own whole-archive bracket, the surviving
tokens left an unclosed opener (observed: 4 openers / 2 closers in
build.ninja).

Fix: fold each triple into ONE comma-joined item
(`-Wl,--whole-archive,<lib>,--no-whole-archive`) — unique per lib, so the
de-dup cannot split it. Verified: the FVP lane links + smoke passes
(`Phase 117.14 … build smoke OK`; zero `stack_chk`/`getentropy` symbols in
the ELF, ffi symbols present); regression `build-cpp-talker-zenoh` rebuilt
from a wiped dir with balanced brackets and
`test_zephyr_cpp_talker_to_{listener_e2e,native_listener}` pass.

## Cross-refs

- Phase 287 W6 Zephyr slice (migration verified unrelated).
- Issue #193 (same CMake<3.24 whole-archive flag-dedup class, native path).
