---
id: 657
title: "The riscv64 lane demands Ubuntu's `riscv64-unknown-elf-*` while `nros setup` provisions xPack's `riscv-none-elf-*` — so a fully provisioned host cannot build the board"
status: open
type: bug
severity: high
area: build/toolchain
related: [issue-0650, issue-0625, issue-0399, issue-0500, issue-0460, phase-366]
---

## Symptom

```
$ nros setup qemu-riscv64-threadx      # completes, installs riscv-none-elf-gcc
$ just threadx_riscv64 build-fixtures
lane threadx-riscv64 INCOMPLETE — 2 step(s) skipped …
  - no riscv64 bare-metal gcc
```

Before issue 0650 this printed "ThreadX-RV64 test fixtures built." and exited 0,
which is how it stayed invisible: the platform had no coverage on any host
provisioned the documented way, and a source divergence in six of its examples
reached main through the resulting green (phase-366 W5.c).

## Cause

`[board.qemu-riscv64-threadx]` in `nros-sdk-index.toml` names
`riscv-none-elf-gcc` — the xPack build, with `dist.<host>` rows for
linux-x86_64, linux-arm64 and macos-arm64. That is what `nros setup` installs,
on every supported host, and it is the portable choice.

Twenty files then spelled the compiler `riscv64-unknown-elf-*`, which is the
Debian/Ubuntu `gcc-riscv64-unknown-elf` package and nothing else. The index and
the build disagreed about the same board's toolchain, and the build won by
finding nothing.

Underneath that were four separate incompatibilities, none of which is about the
prefix — each is a place where one libc's behaviour had been assumed:

1. **`rand` implicitly declared.** NetX calls `NX_RAND` (bare `rand`) from TUs
   that include no libc header; it compiled only because Ubuntu's newlib pulls
   `stdlib.h` in transitively. Every netxduo TU failed
   `-Werror=implicit-function-declaration` on the provisioned toolchain.
2. **No `-lc` at link.** The board located its C library through picolibc's
   sysroot layout; xPack bundles newlib and has no `picolibc.specs`, so nothing
   was linked and the image failed on `strcmp`, `snprintf`, `memchr`, …
3. **`_sbrk` undefined.** newlib's malloc pulls it; picolibc's does not.
4. **picolibc-only stdio in `startup.c`.** `FDEV_SETUP_STREAM` and defining
   `stdout`/`stderr` are picolibc's contract; newlib defines them itself, so the
   file did not compile.

## Fixed

**One resolver, three spellings** (the build systems cannot call each other):
`scripts/build/riscv64-toolchain.sh`, `nros_build_paths::riscv64`, and
`_nros_riscv64_find_prefix()` in `cmake/toolchain/riscv64-threadx.cmake`. Same
candidate order, all honouring `NROS_RISCV64_PREFIX`, SDK store before `PATH`
(the issue-0500 rule: the store accumulates, and the pin must win).

The resolver went into `nros-build-paths` — which has NO dependencies — after a
first attempt in `nros-build-helpers` dragged cbindgen into the `nros` CLI's
graph: 118 lock lines for a function that reads directory names.

Also fixed: the four libc assumptions above; the doctor's libc probe, which
asked "does picolibc exist" rather than "can this toolchain preprocess a TU that
needs libc headers" (it reported `[MISSING] riscv64 C library headers` while
holding a complete libc, and there was a second branch saying exactly that about
xPack, sitting after an `elif` the first branch always won); and
`nros-zpico-build`, whose own candidate list (issue 0399) preferred Ubuntu's —
so a host with both compiled the zenoh C shim with one toolchain and the board
with the other.

**Verified:** `just threadx_riscv64 doctor` fully green, and the Rust half of
the lane builds end to end — `examples/qemu-riscv64-threadx/rust/listener`
produces a real image:

```
ELF 64-bit LSB executable, UCB RISC-V, RVC, double-float ABI, statically linked
```

## Open: the C/C++ half

`libnros_cpp.a` contains `bswapsi2.o` — one of compiler_builtins' **C**
fallbacks — compiled soft-float, while every cmake object is `-mabi=lp64d`, so
lld refuses: *"cannot link object files with different floating-point ABI"*.

What has been established, so the next person does not repeat it:

* `riscv64-threadx.cmake` already carried `set(ENV{RUSTFLAGS} "-Ctarget-feature=+d")`
  for this. It cannot work: issue 0460 — `set(ENV{})` is configure-time, and
  corrosion's cargo runs at build time.
* Exporting `RUSTFLAGS` from the lane DOES fix the float ABI, and is not
  usable: cargo's `RUSTFLAGS` env **replaces** a leaf's `[build] rustflags`, so
  the Rust images then fail on `_bss_start` / `_sysstack_start` — their linker
  script is gone. Measured both ways.
* Per-target `corrosion_set_env_vars` now attaches both `RUSTFLAGS` and
  `CFLAGS_riscv64gc_unknown_none_elf` to `nros_c-static` / `nros_cpp-static`
  (confirmed firing, 8 times, by its STATUS line). The object is STILL
  soft-float, so the compile that produces it is not governed by either — most
  likely a different cargo invocation than the one those targets name.
* It is not staleness: the build dirs were deleted before the last three runs.

The next step is to find which cargo invocation compiles that object — the
`cargo/nano-ros_23c15/` group dir under the example's build tree — and give
THAT one the flags, rather than guessing at target names.
