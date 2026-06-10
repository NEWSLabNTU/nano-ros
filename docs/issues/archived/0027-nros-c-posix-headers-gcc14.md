---
id: 27
title: nros-c posix platform headers fail to compile under gcc 14 (riscv NuttX C build)
status: resolved
type: bug
area: c-api
related: [phase-194]
resolved_in: acab1f81b
---

gcc 14 (riscv NuttX, `riscv-none-elf-gcc 14.2.0`) rejected the nros-c C build that
gcc 10 (arm NuttX) tolerated. Two reported symptoms; the verified header defect is #2.

**Symptom #2 — conflicting types for `nros_platform_atomic_{store,load}_bool`
(FIXED).** The cbindgen single-source-of-truth — the Rust `extern "C"` decl in
`packages/core/nros-c/src/platform.rs` → `nros_generated.h` — declares
`store(bool*, bool)` / `load(const bool*)`, and the board `startup.c` impls already
match. But the **hand-written** decls/defs used a stale `volatile bool*` on both:
`platform.h` forward-decls (gated `#ifndef NROS_PLATFORM_HAS_ATOMICS`, emitted on the
NuttX path) and the `static inline` defs in `platform/{posix,freertos,baremetal,
zephyr}.h`. gcc 14 errors on the `volatile`/`const` qualifier mismatch; gcc 10 warned.

Fix: reconcile all five hand-written headers to the cbindgen signature
(`store(bool*, bool)`, `load(const bool*)`). `volatile` was never needed — the
`__atomic_*` builtins carry the ordering. Verified: the two non-static decls now
co-compile under `gcc -std=c11 -Werror` (old `volatile` reproduced the exact
"conflicting types" error); `nros-c` builds clean.

**Symptom #1 — implicit `nanosleep` (NOT an nros-c header bug in-tree).** The pinned
NuttX `<time.h>` (`third-party/nuttx/.../include/time.h`, incl. the export sysroot)
declares `nanosleep` **unconditionally** — no feature-test gate — so there is nothing
for nros-c to fix against the in-tree submodule. A speculative
`#define _POSIX_C_SOURCE` in `posix.h` was tried and **rejected**: it latches too late
(`platform.h` pulls `<stdint.h>` first, freezing glibc `features.h`) *and* switches
glibc out of `_DEFAULT_SOURCE`, which is what declares `nanosleep`/`clock_gettime` on
Linux — a net regression. If a specific NuttX/newlib *config* hides `nanosleep`, the
correct lever is the build passing the feature macro (before the first system header)
or the board's NuttX config, not a global define in a shared platform header.

**Deferred:** add a gcc-14 / riscv-NuttX C compile to CI so the regression class is
caught (needs a provisioned `qemu-riscv-nuttx` rv32imac export, not available in the
default tree).

Discovered 2026-06-11 building `examples/qemu-riscv-nuttx/c/talker`.
