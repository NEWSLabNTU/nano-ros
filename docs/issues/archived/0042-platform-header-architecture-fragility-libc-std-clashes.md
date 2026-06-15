---
id: 42
title: platform/std-header architecture is fragile — recurring libc/std compile clashes (#27, #36, #38)
status: resolved
resolved_in: phase-241 (+ the 241.A cross gate)
type: tech-debt
area: c-api
related: [issue-0027, issue-0036, issue-0038, issue-0034, phase-240, rfc-0042, phase-241, phase-249, issue-0062]
---

## Resolution (2026-06-15) — the header-clash class is fixed + merge-gated

This issue's class is the **platform/libc-std header clash** (its title bugs
#27/#36/#38), all `resolved`, and the whole class is now caught **on every PR**:
- **#38** (capability/heap default-deny) → `[board.capabilities]` SSoT
  (phase-241.C) + the host gate `platform_header_matrix.rs` (D / phase-241.A).
- **#27/#36** (two-libc `.c`-TU `div_t`/`time.h` clash) → the cross gate
  `cross_libc_precedence_gate.rs` (phase-241.A cross tier): a dropped
  RTOS-sysroot-wins precedence goes red on the PR.

Structural fix-directions: **A/A2** (one canonical `<nros/platform.h>` —
phase-241.B + phase-243), **B** (capabilities declared once — phase-241.C), **D**
(merge gate — phase-241.A host + cross + the zephyr prj.conf gate) all **landed**.
**C** (centralise the RTOS-libc precedence into one shared cmake/build.rs helper)
is **DROPPED as a non-goal** (study 2026-06-15). The two-libc-set clash is
**NuttX-only** — NuttX uniquely ships its own libc alongside the cross
toolchain's newlib. Every other platform avoids it: ThreadX *selects* picolibc as
the sole libc (`--specs=picolibc.specs`, no second set), FreeRTOS/bare-metal use
the toolchain's single libc, ESP-IDF/Zephyr own includes via idf/west, POSIX is
host. The live precedence is therefore **one block** in
`nros-board-common/src/nuttx_ffi_build.rs` (prepend `NUTTX_DIR/include/cxx`); the
NuttX *cmake* precedence was already retired on the cross path (the FFI crate owns
it). So "a shared helper across platforms/entrypoints" would consolidate a single
site — no consolidation, and the regression it guards against already reds on the
PR via `cross_libc_precedence_gate`. Keeping the precedence where it's needed
(one commented NuttX site) + the gate is the correct end state.

**Decoupled from D3/phase-249.** Earlier this note gated #42's close on phase-249;
that conflated two classes. phase-249 / [issue 0062](../0062-d3-completion-one-registration-path-and-link-manifest.md)
is the **linking** class (#20 — `--allow-multiple-definition` / one register
path), NOT this header-clash tracker. So #42 closes now; the linking work stands
on its own under #62.

> **DESIGN 2026-06-12 (original).** The architectural fix is designed in
> [RFC-0042](../../design/0042-platform-build-determinism.md) and broken down in
> [phase-241](../../roadmap/archived/phase-241-platform-build-determinism.md): one canonical
> `<nros/platform.h>`, capability-driven config SSoT (`nros-board.toml`),
> deterministic linking (generated manifest, one register path), and a
> merge-time platform×lang gate.

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

> **Largely addressed (2026-06-13).** The 241.A host gate
> (`platform_header_matrix.rs`) now drives one heap-using cpp TU **per platform
> target** (POSIX, bare-metal, FreeRTOS, Zephyr, ThreadX, NuttX, ESP) on every PR.
> Enabled by the D1 collapse: `<nros/platform.h>` is now the one self-contained
> `nros-platform-api` header (no RTOS sysroot include), so the heap-container
> compile is host-cheap for all platforms — it no longer needs the cross toolchain.
> The remaining off-PR class is only the two-libc-set `.c`-TU clash (#27/#36),
> which still needs the cross sysroot (241.A "cross tier").

The cpp heap containers (`HeapString`/`HeapSequence`, pulled in by every generated
message type) used to compile **only as a side-effect of the full on-demand e2e
build**. Bare-metal+C++, Zephyr+C++, FreeRTOS+C++, ESP+C++ had **no isolated
compile test**. So a broken combo was invisible on PR CI and surfaced days later
in a `run_e2e` dispatch (which is exactly how #36/#38 were found, during
phase-240). That latency is why it read as "recurs whenever someone edits": the
edit lands green, the breakage is off the merge path.

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
