---
id: 42
title: platform/std-header architecture is fragile — recurring libc/std compile clashes (#27, #36, #38)
status: open
type: tech-debt
area: c-api
related: [issue-0027, issue-0036, issue-0038, issue-0034, phase-240, rfc-0042, phase-241]
---

> **DESIGN 2026-06-12.** The architectural fix is designed in
> [RFC-0042](../design/0042-platform-build-determinism.md) and broken down in
> [phase-241](../roadmap/phase-241-platform-build-determinism.md): one canonical
> `<nros/platform.h>`, capability-driven config SSoT (`nros-board.toml`),
> deterministic linking (generated manifest, one register path), and a
> merge-time platform×lang gate. This issue stays open as the motivating
> tracker; it resolves when phase-241's D1–D4 acceptances pass.

Three recently-fixed bugs are the **same class** — a C/C++ compile clash between
the platform's libc/std headers and nano-ros's platform shim:

- **#27** — NuttX C: newlib `time.h` (`__rtems__`-gated) vs NuttX's; cbindgen
  `extern` vs `static inline` atomics collision.
- **#36** — NuttX C++: libstdc++ `<cstdlib>` `#include_next <stdlib.h>` reaches
  newlib's `div_t` after NuttX's → conflicting typedef.
- **#38** — ThreadX RV64 C++: `baremetal.h` defines `NROS_NO_DYNAMIC_MEMORY` →
  `nros_platform_malloc`/`free` omitted, but the board has a heap.

Each was fixed point-wise. They keep recurring whenever a developer adds a board,
adds a cpp example, edits a platform header, or changes include wiring — because
the architecture has four structural weaknesses that make the *default* outcome a
silent mismatch caught only by a full e2e build.

## Why it's fragile (root causes)

### 1. Two `<nros/platform.h>` headers with divergent surfaces, disambiguated by include order

`packages/core/nros-c/include/nros/platform.h` (canonical: `nros_platform_malloc`
/`free`, ns clock) and `packages/core/nros-platform-cffi/include/nros/platform.h`
(`nros_platform_alloc`/`dealloc`/`realloc`, ms/us clock) **share the same include
name**. On a CFFI build both dirs are on the search path; which one a `#include
<nros/platform.h>` resolves to is decided by `-I`-before-`-isystem` ordering set
in the build files — not by design. A consumer that assumes one surface breaks
silently when the other resolves (#38). There is **no single source of truth** for
the C platform ABI.

### 2. The canonical malloc/free shim is copy-pasted across 5 headers, not funnelled once

`nros_platform_malloc → nros_platform_alloc` (and `free → dealloc`) is
re-implemented independently in `platform/{posix,zephyr,freertos,baremetal}.h`
**and** `nros-platform-cffi/platform.h`. Any platform whose header forgets the
shim (or gates it off) drops the canonical surface. #38 was exactly this: the one
header path that didn't carry the shim won the include race.

### 3. Capability macros are default-deny + opt-in per board, wired in ~10 scattered places

`NROS_PLATFORM_HAS_MALLOC` / `NROS_NO_DYNAMIC_MEMORY` / `NROS_PLATFORM_HAS_ATOMICS`
/ `NROS_PLATFORM_HAS_MUTEX` gate which prototypes `nros-c/platform.h` emits.
`baremetal.h` **defaults to `NROS_NO_DYNAMIC_MEMORY`** (no heap) — so a heap-capable
bare-metal board (ThreadX RV64) must *remember* to add `-DNROS_PLATFORM_HAS_MALLOC`
in its cmake overlay. Those `-D`s + `-isystem` precedence live in ~10 different
sites (7 `cmake/platform/*.cmake`, the board overlays' `EXTRA_DEFINES`/`DEFINES`,
`nuttx_ffi_build.rs`). A new board/example easily lands with the wrong default and
no error until a cpp consumer needs the dropped symbol. Default should be
**deny-only-when-known-absent**, declared **once per board**.

### 4. Two libc header sets reachable per TU, with precedence re-wired at every compile entrypoint

The cross toolchain (arm-none-eabi / riscv newlib/picolibc) ships its own libc
headers; the RTOS (NuttX) ships its own. Both land on the include path. Whether
the RTOS sysroot wins is set **independently** at each compile entrypoint — the
cc-rs FFI build (`nuttx_ffi_build.rs`), the cmake message-lib build, the cmake
entry build — so fixing precedence in one (#27 C path) leaves the next (#36 cpp
path) broken. There is no shared "RTOS-libc-wins" precedence helper.

### 5. No merge-time compile gate for platform × language

The cpp heap containers (`HeapString`/`HeapSequence`, pulled in by every generated
message type) compile **only as a side-effect of the full on-demand e2e build**.
Bare-metal+C++, Zephyr+C++, FreeRTOS+C++, ESP+C++ have **no isolated compile
test**. So a broken combo is invisible on PR CI and surfaces days later in a
`run_e2e` dispatch (which is exactly how #36/#38 were found, during phase-240).
That latency is why it reads as "recurs whenever someone edits": the edit lands
green, the breakage is off the merge path.

## Fix directions (structural, in leverage order)

- **D. Cheap per-platform×lang compile gate on PR CI (highest leverage, lowest
  cost).** Cross-`check`/compile one heap-using cpp TU (e.g. a `HeapString` +
  generated-message stub) per platform target, on every PR. Turns "honest-red e2e
  weeks later" into "red PR now". Catches the whole class regardless of the
  underlying wiring. Mirror the existing `core-libs` cross-`check` lane.
- **A. One canonical platform ABI surface.** Either (a) make
  `nros-platform-cffi/platform.h` `#include` the nros-c canonical header (or be
  generated from one declaration list), or (b) rename the CFFI extras off
  `<nros/platform.h>` so the same include name can't resolve two ways. Add a
  `static_assert`/parity test that both expose identical malloc/free/alloc
  signatures.
- **A2. Funnel malloc/free once.** Define the `malloc→alloc`/`free→dealloc` shim
  in a single shared header included by all platform variants, instead of copy 5×.
- **B. Declare board capabilities once.** Add a `capabilities`/`has_heap` field to
  `nros-board.toml` (it already carries `platform`, `link_kind`, `net_stack`,
  `entry_kind`) and derive every `-DNROS_PLATFORM_HAS_*` from it, replacing the
  scattered `EXTRA_DEFINES`. Default RTOS boards heap-capable.
- **C. One libc-precedence helper.** A single cmake function + build.rs helper
  that, given the platform, applies the RTOS-sysroot-wins include order for *all*
  compile entrypoints (C msg-lib, cpp entry, cc-rs FFI) — so precedence is set in
  one place, not re-derived per entrypoint.

D is the immediate win and unblocks confidence; A/A2/B/C remove the root defaults
that make the wrong outcome the easy one. Owner: nros-c / nros-cpp / CI.
