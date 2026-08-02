---
id: 391
title: ThreadX-RV64 C/C++ fixture build only works incrementally — bare
  fixtures-build.sh uses host cc, and a stale zpico.o survives the freshness gate
status: resolved
type: tech-debt
area: testing
related: [rfc-0061, 0196, 0387]
resolved_in: 8ef697c95
---

## Problem

Two related gaps in the ThreadX-RV64 (qemu-riscv64-threadx) C/C++ fixture build
path, both surfaced while confirming the issue-0387 fix on ThreadxRiscv64 C.

### 1. The build only succeeds incrementally; a clean rebuild uses host `cc`

The riscv-threadx C/C++ zenoh fixtures are built via
`scripts/build/fixtures-build.sh threadx-riscv64 {c,cpp} zenoh`, invoked from
`just threadx_riscv64 build-fixture-extras`. The toolchain reaches cmake only
through env the RECIPE sets:

- `THREADX_CONFIG_DIR`/`NETX_CONFIG_DIR` → the riscv64 board config
  (`packages/boards/nros-board-threadx-qemu-riscv64/config`), NOT the
  threadx-**linux** config that `activate.sh` exports by default.
- `NROS_CMAKE_EXTRA_DEFS` → `-DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/riscv64-threadx.cmake`
  (which sets `CMAKE_C_COMPILER=riscv64-unknown-elf-gcc`).

Run `fixtures-build.sh threadx-riscv64 c zenoh` on its own (no recipe env) and
cmake configures with host `/usr/bin/cc`, so the ThreadX kernel + NetX sources
compile with the x86 assembler:

```
tx_block_allocate.c: Assembler messages:
  Error: no such instruction: `csrrci %rax,mstatus,0x08'
```

Worse, `nros_cmake_configure_if_needed` CACHES that broken configure. Once any
invocation has written a host-cc `CMakeCache.txt` into a fixture's `build-zenoh`,
every later (correct) invocation is skipped — the only recovery is
`rm -rf build-zenoh` on each affected example first. That the fixtures ever
build is an artifact of them being configured once, correctly, and never
cleaned; a from-clean CI build of this lane fails.

Fix direction: make the riscv64 toolchain + board config intrinsic to the
`threadx-riscv64` target in `fixtures-build.sh` / the manifest, not dependent on
recipe-exported env; or have `configure_if_needed` detect a compiler/toolchain
mismatch in the cached `CMakeCache.txt` and force-reconfigure.

### 2. A stale `zpico.o` survives the test-side freshness gate (issue-0196 class)

The ThreadX-RV64 C fixtures are PREBUILT-only: `build_cmake_example`
(`nros_tests::fixtures::binaries::threadx_riscv64`) just calls
`require_prebuilt_binary_fresh` on the final ELF — it does not compile. When the
committed `zpico.c` changed (the 0387 fix, `4b8c63b36`), the fixture's
cargo-nested `…/build/zpico-sys-*/out/*-zpico.o` was NOT rebuilt, yet the ELF
passed the freshness check, so `rtos_e2e` ran a museum binary that still showed
the OLD bug ("0 messages"). The freshness gate watches the fixture ELF, not the
C sources compiled into it through cargo/corrosion — exactly the issue-0196 rule
("build-side stale probes must watch the same inputs as test-side gates"), one
level deeper (a Rust dep's vendored C file).

Fix direction: fold the zpico-sys C sources (or the zpico-sys fingerprint) into
whatever the fixture freshness probe hashes, so a `zpico.c` edit marks every
consuming prebuilt fixture stale.

## Impact

Not a runtime bug — the 0387 fix is correct and verified once the fixtures are
rebuilt properly (ThreadxRiscv64 C pubsub 14 msgs / service 1 response green).
The cost is developer time: a stale fixture re-shows a fixed bug, and a from-
clean rebuild of this lane fails confusingly with x86-assembler errors on RISC-V
kernel sources.

## Repro

```
rm -rf examples/qemu-riscv64-threadx/c/*/build-zenoh
bash scripts/build/fixtures-build.sh threadx-riscv64 c zenoh   # host cc → csrrci errors
# vs the working path:
just threadx_riscv64 build-fixture-extras                      # riscv cross, green
```

## Resolution

Both gaps fixed; neither needed the bigger "toolchain intrinsic to the target"
refactor — the recipe already supplies the toolchain, so the fixes make the
mechanism robust rather than move where the toolchain is declared.

**Gap 1 (cache poisoning) — root was subtler than "no toolchain passed".** The
configure helpers (`nros_cmake_configure_if_needed`,
`nros_cmake_fixture_build`) DO arg-stamp and reconfigure when the
`-DCMAKE_TOOLCHAIN_FILE` arg changes. But CMake pins `CMAKE_C/CXX_COMPILER` at
the FIRST configure and a re-configure of an existing cache CANNOT swap the
compiler — it reads a toolchain file only against a fresh cache. So once any
invocation (e.g. a bare `fixtures-build.sh`) wrote a host-cc cache, every later
correct invocation kept host cc. Fix: both helpers now compare the requested
`CMAKE_TOOLCHAIN_FILE` against the cached one and `rm -rf` the build dir on a
mismatch (mirrors the existing generator-switch wipe). The lane self-heals — a
poisoned cache is corrected on the next correct build, no manual
`rm -rf build-zenoh`. No-op on the happy path (want == cached, verified against a
real fixture cache) and for native fixtures (empty both sides).

**Gap 2 (stale cargo-nested C) — fixed in the test-side gate.**
`require_prebuilt_binary_fresh_cmake` → `cmake_dep_info_newer_source` now also
walks `packages/rmw/zenoh/zpico-sys/c/**` and compares each `.c`/`.h` mtime to
the fixture binary, gated on the `build-zenoh` marker (cyclone fixtures don't
link zpico → no false stale). Verified: `touch zpico.c` now trips the gate
("STALE — newer: …/zpico.c") in 0.13 s where it previously passed a museum
binary.

Not addressed (deliberately, low value): the bare `fixtures-build.sh
<cross-platform> c` footgun still defaults to host cc from a truly clean tree —
but it fails LOUDLY (csrrci assembler errors), the recipe is the sanctioned
entry point, and gap-1's self-heal means a bare-call poisoning no longer
persists. Making the per-platform toolchain intrinsic to `fixtures-build.sh`
would need the manifest to carry it; out of scope here.
