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

## The C/C++ half (2026-08-17)

Three more, each the same shape as the rest — a tool named rather than resolved,
and each hiding the next.

**1. The soft-float strip had no reader.** `cmake/strip-compiler-builtins.sh`
exists precisely for rust-lang/rust#83229: it removes compiler_builtins'
soft-float objects so they cannot clash with this board's lp64d ones. It decided
what to strip with a hardcoded `riscv64-unknown-elf-readelf` — absent on a
provisioned host, so `flags` came back EMPTY, nothing matched "soft-float", and
it stripped ZERO objects while reporting nothing (the probe's stderr goes to
`/dev/null`, and an empty result is indistinguishable from "none here"). It now
prefers `llvm-readobj` (which ships beside the `llvm-ar` it is already handed)
and falls back to whichever cross readelf exists; with no reader at all it FAILS
instead of silently stripping nothing. On the archive that was failing: 36
objects stripped where it previously stripped none.

**2. `-lgcc` was missing, and only mattered once (1) worked.** The stripped
objects were also the image's only definition of `__bswapsi2`; the hard-float
equivalents live in the TOOLCHAIN's libgcc, on a different multilib path from
libstdc++ (`lib/gcc/<triple>/<ver>/<arch>/<abi>` vs
`<triple>/lib/<arch>/<abi>`), resolved now with `-print-libgcc-file-name`. One
bug had been hiding the other: the soft-float objects stayed, satisfied the
symbol, and failed the link on the ABI instead.

**3. `corrosion_set_env_vars` was landing on the wrong target — repo-wide.**
Corrosion 0.6 creates TWO targets per crate: `<crate>`, an INTERFACE target
whose properties the cargo build command reads through a generator expression,
and `<crate>-static`, an IMPORTED library naming the `.a`. `set_property`
succeeds on either and only the first is ever read. Every call site passed the
`-static` spelling, so `nros_cargo_profile_env`, `nros_board_facts_env` AND the
riscv64 rustflags were setting a property nothing consumes. Measured on a
configured example before the fix: `build.ninja` contained no `CARGO_PROFILE_*`,
no `NROS_BOARD*` and no `RUSTFLAGS` — **phase-351 W5's board rung was reaching
cargo on zero targets under this Corrosion**, and nothing failed loudly because
every consumer has a default (issue 0529's shape). Fixed with one normaliser,
`nros_corrosion_env_target`, in its own module both helpers include.

Correction to this issue's earlier note: `-Ctarget-feature=+d` does NOT fix the
float ABI. It cannot — the objects come from the PRECOMPILED compiler_builtins
rlib and target-feature does not rebuild it. The earlier reading came from a
run where a lane-wide `RUSTFLAGS` export had also discarded the leaf's linker
script, so the link failed earlier and the ABI error simply never printed.

## Still open: `app_main` on the C application path

With the above, the C/C++ riscv64 link gets past the ABI and past `__bswapsi2`,
and now stops at one undefined symbol: `app_main`.

That is not a toolchain question. `-u app_main` comes from the board overlay,
and the symbol is emitted by the SYNTHESISED typed-entry TU that
`NanoRosNodeRegister.cmake` generates for node-shaped targets. These C examples
are application-shaped — `nano_ros_add_executable(c_listener src/main.c)` with
`nros_app_main()` in `main.c` — so no entry TU is synthesised and nothing
defines the boot symbol. Whoever picks this up should decide whether the
application shape is supposed to be supported on this board (then the board
needs the shim other RTOS carriers have) or whether these examples should be
node-shaped like the platforms whose fixtures do build.
