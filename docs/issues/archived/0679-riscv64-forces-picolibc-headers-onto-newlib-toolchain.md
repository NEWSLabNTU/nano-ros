---
id: 679
title: "Duplicate of #0678 (threadx-riscv64 Cyclone `__emutls_v.errno`) — retired, with two refuted attempts and the measurement error that produced them"
status: wontfix
type: bug
area: build, boards
related: [issue-0678, issue-0674, issue-0664, issue-0657]
---

## Retired as a duplicate

Filed independently for the same defect as
[issue 0678](0678-threadx-rv64-cpp-cyclone-emutls-errno-undefined.md): every
CycloneDDS row on `threadx-riscv64` fails to link with
`undefined symbol: __emutls_v.errno`.

**#0678 is the canonical record** and carries the correct analysis: the
provisioned xPack compiler has EMULATED TLS as its only model (it does not even
accept `-fno-emulated-tls`), while the Debian picolibc it links was built by a
compiler with NATIVE TLS. `errno` is `__thread`, so the two cannot agree, and
#0678 states the decision it needs — pick the toolchain's own newlib, or pick
the compiler that built the picolibc being linked. That is a board-ownership
call, not a flag.

This file exists only to carry what was learned HERE and is not in #0678.

## Refuted attempt A — make the picolibc `-isystem` conditional

Probe whether the resolved compiler ships its own libc headers; only give
picolibc's to one that does not. **Made it strictly worse**: it also removed
`NROS_LIBC_PICOLIBC`, so `startup.c` stopped defining `stdout`/`stderr` (#0674's
fix), taking the link from one undefined symbol to three. Reverted.

## Refuted attempt B — define the TLS errno in `startup.c`

Add `__thread int errno;` inside the guard that already supplies
`stdout`/`stderr`, on the reasoning that picolibc DECLARES these and leaves
defining them to the image.

**It links.** From wiped caches all four Cyclone rows built — `c_talker`,
`c_listener`, `cpp_talker`, `cpp_listener`, all RISC-V ELF, zero undefined
symbols, `D __emutls_v.errno` present in the image.

**And it is unsound, so it must not be used.** picolibc's `libc.a` defines
`errno` in NATIVE TLS and references it the same way:

```
$ nm --format=sysv <picolibc>/lib/rv64imafdc/lp64d/libc.a | grep -w errno
errno | | U | TLS | | |*UND*
errno |0000000000000000| B | TLS |0000000000000004| |.tbss
$ nm <picolibc>/…/libc.a | grep -c emutls
0
```

A `__thread` definition compiled by the emulated-TLS-only compiler emits
`__emutls_v.errno` — a DIFFERENT storage from the `.tbss` slot picolibc's own
code reads and writes. The link is satisfied and the semantics are split: a
failing libc call sets picolibc's `errno`, the application reads its own and
sees a stale value, with no diagnostic anywhere. Add it to #0678's
"do not retry" list.

**This is why the fix was measured as working and still discarded.** Linking is
not the acceptance criterion for a symbol whose whole purpose is to carry a
value between two pieces of code.

## The measurement error worth keeping

Attempt B was first tried, observed to change nothing, and wrongly declared
refuted — with an elaborate mechanism invented on top: that
`nros_threadx_setup_picolibc`'s `PARENT_SCOPE` write evaporates because
`cmake/platform/nano-ros-baremetal.cmake` `include()`s the board file inside a
FUNCTION, leaving app targets on newlib headers while the Cyclone subproject
got picolibc's. **That explanation is retracted.** The toolchain was never at
fault; configuring the leaf by hand yields
`CMAKE_C_FLAGS = … -isystem <picolibc>/include -DNROS_LIBC_PICOLIBC=1`.

The real cause was that the build directories were never deleted:

```sh
rm -rf .../c/*/build-cyclonedds .../cpp/*/build-cyclonedds
```

Under zsh an UNMATCHED glob is fatal. `cpp/*/build-cyclonedds` did not exist
yet, so zsh aborted the whole line and removed NOTHING. Every "clean rebuild"
afterwards reused caches predating #0674 — which is exactly why they lacked
`-DNROS_LIBC_PICOLIBC` and why the guard did not fire. The `threadx_kernel`
(had the flags) vs `c_talker` (did not) asymmetry was old cache versus newer
cache.

#0678 reached the same class of correction independently — "this issue's
control was stale", its C-rows-link-fine control having come from incremental
trees.

`activate.sh` documents this exact zsh hazard and bans bare globs in sourced
files. **The ban belongs in throwaway cleanup commands too**: a glob that
silently deletes nothing produces a confident measurement of the WRONG TREE,
which is worse than an error.

## Verification protocol for this platform

* `fixtures-build.sh threadx-riscv64` builds only the RUST rows and reports
  `RV=0` while compiling none of the failing code.
* The build that exercises these rows is `just threadx_riscv64
  build-fixture-extras` — the module name has an UNDERSCORE.
* Judge by ARTIFACTS on disk. Every false conclusion in this history came from
  an exit code, or from a `grep -c` over a log that contained only a usage
  error — and each of those greps honestly returned 0.
