---
rfc: 0042
title: "Platform & build determinism — one canonical interface, capability-driven config, deterministic linking"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-241-platform-build-determinism]
supersedes: []
superseded-by: null
---

# RFC-0042 — Platform & build determinism

## Summary

A recurring class of build failures — libc/std header clashes (#27, #36, #38),
ld single-pass undefined-symbol races (#20), and silent capability mismatches —
all share one root: **determinism in the C/C++/Rust build is enforced by
convention and hand-special-casing, not by structure.** Each new board, cpp
example, or platform-header edit can land green and break a different,
un-gated combination days later (only an on-demand e2e build exercises it).

This RFC makes the platform/build contract *structural* along four pillars:

1. **One canonical platform interface.** A single `<nros/platform.h>` C ABI is
   THE interface (per RFC-0006); the CFFI header stops being a second header of
   the same name; the `malloc`/`free`-over-`alloc`/`dealloc` shim is defined
   once, not copied across five headers.
2. **Capability-driven configuration.** `nros-board.toml` becomes the single
   source of truth for board capabilities (`has_heap`, `has_atomics`,
   `has_threads`, …); one generator lowers them to *every* downstream knob
   (cargo features, cmake `-D`, capability macros, include precedence). Drift
   across the ≥5 sites that today each name the platform becomes impossible.
3. **Deterministic linking.** One registration mechanism for all platforms
   (a generated explicit backend-register table), a generated link manifest that
   fixes archive order + the whole-archive set, and the removal of the
   `--allow-multiple-definition` / `-u <sym>` special-cases that today paper over
   ordering bugs. Linking becomes data, not lore.
4. **Merge-time gate.** A platform × language compile+link matrix runs on every
   PR, so the whole class is caught at merge instead of in a later e2e dispatch.

It **amends** RFC-0006 (canonical C ABI — now enforced, single header),
RFC-0031 (RMW select/lower — now validated against the manifest), and references
RFC-0034 (allocator funnel — kept) and RFC-0035 (vtable slot ABI — kept). It
**depends on** RFC-0012 (board crates) and RFC-0014 (`nros setup` / sdk-index).

## Motivation — determinism by convention fails

Evidence (all real, all recent):

| Symptom | Root | Convention that failed |
| --- | --- | --- |
| #27 NuttX C `time.h`/atomics | newlib (`__rtems__`-gated) vs NuttX libc both on path | RTOS-sysroot-wins include precedence set per-entrypoint |
| #36 NuttX C++ `div_t` | libstdc++ `<cstdlib>` `#include_next` reaches newlib `stdlib.h` after NuttX's | same precedence, re-derived for the cpp entrypoint |
| #38 ThreadX RV64 C++ `malloc/free` | `baremetal.h` defaults `NROS_NO_DYNAMIC_MEMORY`; heap-capable board must remember a `-D` | default-deny capability, opt-in per board |
| #20 ThreadX-linux C++ Cyclone | ld single-pass: whole-archive RMW references a symbol defined in an earlier-scanned archive | hand-injected `-u nros_rmw_cffi_register_named` for that one combo |

Each was fixed point-wise. The architecture has good *pieces* — the unified
allocator funnel (RFC-0034 D6), the frozen 34-slot vtable (RFC-0035), the
distributed-slice registration concept — but the *contracts between them* are
upheld by comments and per-combination workarounds:

- **Two `<nros/platform.h>` headers** (`nros-c/include/...` canonical `malloc`/
  `free`; `nros-platform-cffi/include/...` `alloc`/`dealloc`) resolve by
  `-I`-before-`-isystem` order, not by design. The shim that bridges them is
  copy-pasted in `platform/{posix,zephyr,freertos,baremetal}.h` **and**
  `nros-platform-cffi/platform.h` (5×).
- **Capability macros** (`NROS_PLATFORM_HAS_MALLOC`, `NROS_NO_DYNAMIC_MEMORY`,
  `NROS_PLATFORM_HAS_ATOMICS`, `NROS_PLATFORM_HAS_MUTEX`) are default-deny and
  set in ~10 scattered cmake/board/build.rs sites; platform identity is repeated
  in ≥5 places (board.toml, sdk-index, cargo feature, cmake var + module file,
  platform.h macro) with no cross-check.
- **Two registration paths**: `linkme` distributed-slice (`RMW_INIT_ENTRIES`) on
  hosted targets vs weak `nros_app_register_backends` + a cmake-generated strong
  override on bare-metal (linkme drops `target_os = "none"`). Whole-archive +
  "platform shim must link *after* RMW" (ld single-pass) is hand-maintained;
  `--allow-multiple-definition` hides duplicate-symbol smells.
- **No merge gate**: the cpp heap containers (pulled in by *every* generated
  message type) compile only as a side-effect of the on-demand e2e build. That
  latency is why the class "recurs whenever another developer edits."

## Non-goals

- Re-freezing or re-numbering the vtable slot table — RFC-0035 stands.
- Changing the unified-allocator funnel — RFC-0034 D6 stands; this RFC just makes
  the canonical `malloc`/`free` surface over it single-sourced.
- Changing the RMW *declaration* model (system.toml) — RFC-0031 stands; this RFC
  adds *validation* that lowering matches the declaration.
- Runtime plugin loading — registration stays link-time.

## Design

### D1 — One canonical platform interface

- `<nros/platform.h>` (nros-c) is the **sole** canonical C ABI header. RFC-0006
  already declares this; D1 enforces it.
- `nros-platform-cffi`'s header stops being a second file resolvable as
  `<nros/platform.h>`: it either `#include`s the canonical header for the shared
  surface and adds its extras under a distinct include path/name, or is generated
  from the same declaration list. There is exactly one resolution of
  `#include <nros/platform.h>` regardless of include order.
- The `malloc`→`alloc` / `free`→`dealloc` shim is defined **once** (a single
  shared inline header included by every platform variant), replacing the 5
  copies. A platform that has a heap exposes the canonical `malloc`/`free`
  automatically; one that does not, does not — no per-board `-D` to remember.
- **Include-precedence rule (normative):** when an RTOS ships its own libc, its
  sysroot headers win over the toolchain's bare newlib/picolibc for *all*
  entrypoints. This is implemented once (see D3's helper), not re-derived per
  compile site. For C++, the RTOS C++ wrapper dir (e.g. NuttX `include/cxx`)
  precedes libstdc++ so `<cstdlib>`'s `#include_next` cannot reach the toolchain
  libc (the #36 mechanism).
- A `static_assert`/CI parity check guarantees the canonical surface and any
  generated mirror stay signature-identical.

### D2 — Capability-driven configuration (single source of truth)

- `nros-board.toml` gains a `[board.capabilities]` block — the SSoT:
  ```toml
  [board.capabilities]
  heap     = true     # has a usable allocator (drives malloc/free + NO_DYNAMIC_MEMORY)
  atomics  = true     # NROS_PLATFORM_HAS_ATOMICS
  threads  = true     # NROS_FEATURE_THREADS / HAS_MUTEX
  libc     = "nuttx"  # rtos | newlib | picolibc | host — drives include precedence
  ```
- One generator (in the codegen/cmake glue) lowers capabilities to **every**
  downstream knob: cargo features, cmake `-D NROS_PLATFORM_HAS_*`, the capability
  macros, and the include-precedence selection. The ~10 scattered `EXTRA_DEFINES`
  / per-header defaults are removed; they read the generated values.
- Capability defaults become **deny-only-when-known-absent**: a board states what
  it has; the build never silently drops a symbol a linked consumer needs.
- Platform identity is named **once** (board.toml); cmake/cargo/sdk-index consume
  the descriptor (extends the Phase 195.C board-descriptor mechanism) instead of
  re-declaring it.

### D3 — Deterministic linking

- **One registration path.** Codegen emits an explicit backend-register table for
  the binary (the set of `nros_rmw_<x>_register()` to call), used on *all*
  platforms — hosted included. The `linkme`-vs-weak split is removed; the
  distributed-slice may remain an *implementation detail* of the generator's
  hosted path but is no longer a second contract. Bare-metal and hosted register
  identically.
- **Generated link manifest.** The codegen/cmake glue emits the deterministic
  link line for the binary: which archives are whole-archived, and the archive
  order that satisfies ld single-pass (platform shim after RMW, message libs
  before the FFI glue, …). The ordering rules move from comments into generated
  data.
- **No papering-over.** `--allow-multiple-definition` and the per-combo
  `-u <symbol>` injections (e.g. #20) are removed; the manifest's ordering +
  whole-archive set make extraction deterministic, so duplicate/undefined symbols
  surface as real errors, not silently-resolved ones.
- **Link-closure validator.** The FFI-libs closure (today `APP_FFI_LIBS_FILE`,
  pre-computed by cmake with no transitive check) gains a validation pass: every
  symbol referenced by the Rust/C++ FFI glue must be satisfied by a manifest
  entry, failing the build at generation time rather than at `ld`.
- The unified allocator (RFC-0034) and vtable ABI (RFC-0035) are unchanged; D3
  only makes their *linkage* deterministic.

### D4 — Merge-time compile + link gate

- A platform × language matrix runs on **every PR** (not just on-demand e2e):
  cross-`check`/compile one representative TU per cell that exercises the
  fragile surface — a generated message type (pulls in `HeapString`/
  `HeapSequence`) plus a minimal entry that forces backend registration + the
  final link. Mirrors the existing `core-libs` cross-`check` lane.
- Cells: {posix, zephyr, freertos, nuttx, threadx, bare-metal, esp-idf} ×
  {C, C++, Rust} where the combination is supported (driven by the board
  capability matrix from D2).
- This is the **first** thing to land (it is the safety net for D1–D3's
  migration) and the cheapest; see the phase doc's Wave A.

## Compatibility & migration

- D1/D2/D3 are source-and-build-tooling changes; the runtime vtable ABI
  (RFC-0035) and allocator funnel (RFC-0034) are untouched, so existing binaries'
  behavior is unchanged once they build.
- Migration is staged so the gate (D4) lands first and guards the rest (see
  phase-241). Each subsequent wave is independently revertible.
- `nros-board.toml` capability block is additive; boards without it get
  conservative inferred defaults during migration, with a lint that flags
  boards still relying on inference.

## Open questions

- **Q1 — CFFI header collapse mechanism.** Does `nros-platform-cffi`'s header
  `#include` the canonical nros-c header, or is both generated from one IDL-ish
  declaration list? (Affects whether the Rust extern block and the C header share
  a generator.) *Lean: include the canonical header for the shared surface;
  generate nothing new in wave B.*
- **Q2 — Link-manifest owner.** Does the manifest live in the cmake glue, in
  `nros` codegen (`nros plan`/`ws sync`), or both? *Lean: codegen emits a
  manifest file; cmake + build.rs consume it — one producer, two consumers,
  matching the existing `APP_FFI_LIBS_FILE` shape.*
- **Q3 — Capability inference fallback.** During migration, how are
  capabilities inferred for a board lacking the block — from `platform`? *Lean:
  a platform→default-capabilities table, with a deprecation lint.*
- **Q4 — Hosted registration.** Keep `linkme` as the hosted generator backend, or
  unify on the explicit table everywhere from day one? *Lean: explicit table
  everywhere; retire linkme as a contract (may stay as an internal optimization).*

## Relationship to existing RFCs

- **Amends RFC-0006** (portable RMW/platform interface): the "C ABI is canonical"
  stance becomes structurally enforced — one header, one shim.
- **Amends RFC-0031** (RMW selection/lowering): adds validation that the lowered
  feature/`-D` matches the system.toml declaration, via the link manifest.
- **References RFC-0034** (platform layer split): allocator funnel kept; canonical
  malloc/free single-sourced over it.
- **References RFC-0035** (vtable ABI): slot table kept; its linkage made
  deterministic.
- **Depends on RFC-0012** (board crates) and **RFC-0014** (sdk-index): the
  capability block extends the board descriptor those define.
