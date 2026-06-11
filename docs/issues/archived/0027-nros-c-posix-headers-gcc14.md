---
id: 27
title: nros-c posix platform headers fail to compile under gcc 14 (riscv NuttX C build)
status: resolved
type: bug
area: c-api
related: [phase-194]
resolved_in: 89ccb6eee+812234321
---

> **RESOLVED 2026-06-11 (both symptoms).** #2 fixed by `89ccb6eee` (cbindgen
> `[export] exclude` for the four platform-provided fns → no more
> static-vs-extern collision). #1 fixed by `812234321` (NuttX sysroot
> `${NUTTX_DIR}/include` added SYSTEM to the NuttX NanoRos umbrella → `posix.h`'s
> `<time.h>` resolves to NuttX's unconditional decls, not bare newlib's
> `__rtems__`-gated ones). Verified serially with provisioned exports: both the
> riscv (rv32imac) and arm (cortex-a7) NuttX C talkers compile their generated
> std_msgs message libs + component archive clean. The premature `acab1f81b` only
> matched qualifiers — see the reopen note for why that was insufficient.

> **REOPENED 2026-06-11.** The first pass (`9fcac7d79`) reconciled the
> `volatile`/`const` qualifiers but did not fully resolve either symptom:
>
> - **#2 (atomics) — real defect was cbindgen emission; fixed here.** Matching
>   the qualifiers turned "conflicting types" into "**static declaration follows
>   non-static**": `nros_generated.h` (cbindgen) still emitted `extern` decls for
>   the four platform-provided `nros_platform_{time_ns,sleep_ns,atomic_store_bool,
>   atomic_load_bool}`, colliding with the `static inline` defs in
>   `platform/{posix,…}.h` under gcc 14. Root cause: `[parse] clean` does not
>   strip the **edition-2024 `unsafe extern "C"`** import block in `platform.rs`,
>   and the `// cbindgen:ignore` sits above the `#[cfg]` so it no-ops. Fixed by
>   adding the four names to `cbindgen.toml [export] exclude` (`platform.h` is
>   their canonical, gated declaration site). The message lib now compiles past
>   the atomics.
>
> - **#1 (clock_gettime / nanosleep / CLOCK_MONOTONIC) — still open, root-caused.**
>   Not a feature-macro problem: `riscv-none-elf-gcc 14`'s newlib gates the entire
>   POSIX-options block (`_POSIX_TIMERS`, `_POSIX_MONOTONIC_CLOCK`, …) behind
>   `#ifdef __rtems__` (`…/riscv-none-elf/include/sys/features.h:349`). NuttX is
>   not RTEMS, so bare newlib never declares these regardless of
>   `_POSIX_C_SOURCE`/`_GNU_SOURCE` (`-D_POSIX_C_SOURCE=200809L` tried → no effect,
>   reverted). The decls exist unconditionally in NuttX's own headers
>   (`third-party/nuttx/.../include/time.h`). **Fix:** the NuttX C message-lib
>   compile (`nros_generate_interfaces`) must use the NuttX sysroot includes
>   (`-isystem $NUTTX_DIR/include`) so `posix.h`'s `<time.h>` resolves to NuttX's,
>   not the cross toolchain's bare newlib. The FFI app compile (cc-rs in
>   `run_nuttx`) already adds NuttX includes; the cmake-built message libs do not.
>   The arm path only escaped because its older system newlib lacks the
>   `__rtems__` gate. (Was prematurely `resolved_in: acab1f81b`.)
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
