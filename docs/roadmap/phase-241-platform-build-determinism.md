# Phase 241 — Platform & build determinism

**Implements.** [RFC-0042](../design/0042-platform-build-determinism.md) — one
canonical platform interface, capability-driven config, deterministic linking,
merge-time gate.

**Goal.** End the recurring class of C/C++/Rust build failures (libc/std header
clashes #27/#36/#38, ld single-pass undefined-symbol races #20, silent capability
mismatches) by making the build contract *structural* instead of
convention-enforced. Cross-refs the systemic review in [issue 0042].

**Status.** Planned (2026-06-12).

**Priority.** P2 — no product capability is blocked, but this class of bug recurs
on nearly every board/example/platform-header edit, and each instance currently
surfaces only in an on-demand e2e build (days late). Continuation of the
[Phase 240](archived/phase-240-ci-disk-and-build-optimization.md) honest-e2e work
that exposed the pattern.

**Depends on.** RFC-0042; the Phase 195.C board-descriptor mechanism (capability
block extends it); the per-platform CI matrix (Phase 196 / 240) the gate plugs
into.

---

## Waves

Ordered so the **gate lands first** (safety net), then the most-recurring
fix (interface), then config, then the largest change (linking). Each wave is
independently revertible.

### 241.A — Merge-time compile gate (RFC-0042 D4) — FIRST
Two tiers, by what a cell needs to compile:

- [x] **Host tier (landed).** `packages/testing/nros-tests/tests/platform_header_matrix.rs`
      + `.github/workflows/platform-header-gate.yml`: drives host `g++`/`cc` over
      the real `<nros/platform.h>` + the nros-cpp heap containers for the
      host-compilable cells (POSIX, bare-metal), asserting positive AND negative
      outcomes — bare-metal **without** `NROS_PLATFORM_HAS_MALLOC` MUST fail to
      compile the heap containers, **with** it MUST succeed (the #38 mechanism,
      both directions). Cheap (no SDK, no cross target, ~seconds); mirrors the
      `core-libs` lane; PR-gated on the platform headers + `nros-board.toml` + the
      test. This is the safety net that guards the D1/D2/D3 migration churn.
- [ ] **Cross tier (later).** The two-libc-set class (#27/#36) needs the cross
      toolchain + RTOS sysroot + `#include_next`, so it can't run host-cheap.
      Options: (a) provision just the cross toolchain + a minimal vendored RTOS
      header stub to reproduce the clash without a full export, or (b) compile the
      RTOS cpp examples on the PR path (heavier; needs the export). Until then the
      cross class stays covered by the e2e `build-fixtures` lane (Phase 240).
- **Acceptance (host tier):** ✅ a PR that reintroduces the #38-class capability
      breakage goes red on the gate, not green-then-red-in-e2e. Verified locally:
      the 5-cell matrix passes, the bare-metal-no-malloc negative cell fails to
      compile as required.

### 241.B — Collapse to one canonical interface (RFC-0042 D1)

Direction (corrected, see RFC-0042 D1): **B (the cffi full ABI) is canonical, A
(nros-c legacy) is retired, `nros-platform-api` owns the one header.** The two
files share name + guard `NROS_PLATFORM_H` today → include-order ABI poison.
Staged so each step is CI-validated before the next; the 241.A gate + the existing
`c_stub_platform.rs` parity test guard the churn.

- [ ] **B.1 — canonical header in `nros-platform-api`.** Move B's surface into
      `packages/core/nros-platform-api/include/nros/platform.h` (the sole file
      named so); fold in A's capability macros (`NROS_PLATFORM_HAS_*`,
      `NROS_NO_DYNAMIC_MEMORY`, `nros_mutex_t`) + the single malloc/free shim
      (gated by the capability, forwarding to `alloc`/`dealloc`). One include
      guard. Keep the `lib.rs` extern mirror + `c_stub_platform.rs` pointed at it.
- [ ] **B.2 — rewire include dirs to the canonical.** Repoint the SSoT chokepoints
      — `nros-build-paths` path helper + the `NROS_PLATFORM_CFFI_INCLUDE` cmake/env
      var (rename → `NROS_PLATFORM_INCLUDE`) — to `nros-platform-api/include`, plus
      the literal-path cmake sites (the ~20 from the wave-B investigation). Delete
      `nros-platform-cffi/include/nros/platform.h`.
- [ ] **B.3 — retire A.** Confirm no live C consumer of A's legacy-only surface
      (ns clock, typed mutex, atomics) via grep + the gate; remove it. `nros-c`'s
      `platform.h` becomes a thin re-include of the canonical (back-compat) or is
      dropped; the per-RTOS sub-headers' divergent shims (incl. posix's libc
      outlier) are deleted in favour of the single shim.
- [ ] **B.4 — single shim + parity.** Posix's direct-libc malloc/free is changed
      to forward to the funnel (RFC-0034 D6). Add the canonical-surface parity
      assert (header ↔ `lib.rs`) if `c_stub_platform.rs` doesn't already cover it.
- [ ] **B.5 — repoint the 241.A gate.** The host matrix test currently resolves
      to A; repoint it at the canonical header; keep the negative #38 cell.
- **Acceptance:** exactly one file named `nros/platform.h`; `#include
      <nros/platform.h>` resolves identically regardless of `-I`/`-isystem` order;
      all per-platform CI cells (incl. xrce/cyclone B-only consumers) green;
      #36/#38 reproductions stay fixed.

> **Note — include precedence (#27/#36, the two-libc class) is NOT in B.** That is
> the cross-toolchain/RTOS-sysroot concern; it lands in 241.D's shared
> precedence helper (and the 241.A cross tier). B is purely the
> one-canonical-header collapse.

> **Coupling found in B.1 (2026-06-12): B depends on C's capability home.** The
> capability macros are NOT centralized in A's `platform.h` — they are *produced*
> by A's compile-time dispatch to the per-RTOS sub-headers (`posix.h` `#define`s
> `NROS_PLATFORM_HAS_MALLOC`/`HAS_ATOMICS`/`HAS_MUTEX`; `baremetal.h` `#define`s
> `NROS_NO_DYNAMIC_MEMORY`; etc.). So "fold the capability macros into the one
> canonical header" can't be answered without deciding where capabilities *come
> from*. Two orders:
> - **(rec) Do 241.C's capability-macro home first** (board.toml `[board.capabilities]`
>   → generated `-D`s), then the canonical header simply *consumes* the generated
>   `NROS_PLATFORM_HAS_*` and the per-RTOS sub-headers + their `#define`s are
>   retired. Clean: the collapse lands on a settled capability source.
> - **(interim) Canonical header keeps A's dispatch-to-sub-headers** purely for the
>   capability `#define`s while B's ABI is the body — a transitional two-mechanism
>   header, removed when C lands. Faster to the single file, but carries the
>   dispatch lore B was meant to kill.
> Recommendation: reorder to **C before B** (or do C's macro-home slice first),
> since B's value (one clean header) is undercut if it must re-host A's dispatch.

### 241.C — Capability-driven config SSoT (RFC-0042 D2)
- [ ] Add `[board.capabilities]` to `nros-board.toml` (`heap`, `atomics`,
      `threads`, `libc`); populate every in-tree board.
- [ ] One generator lowers capabilities → cargo features + cmake `-D
      NROS_PLATFORM_HAS_*` + capability macros + include-precedence selection.
- [ ] Remove the ~10 scattered `EXTRA_DEFINES` / per-header capability defaults;
      they read generated values. Capability defaults become
      deny-only-when-known-absent.
- [ ] Migration lint: flag boards relying on inferred capabilities (Q3).
- **Acceptance:** flipping a board's `heap`/`atomics`/`threads` in one place
      changes the build everywhere; no capability named in >1 site.

### 241.D — Deterministic linking (RFC-0042 D3)
- [ ] One registration path: codegen emits the explicit backend-register table,
      used on all platforms; retire the linkme-vs-weak split as a *contract*
      (Q4 lean: explicit table everywhere).
- [ ] Generated link manifest: whole-archive set + archive order (platform shim
      after RMW, msg libs before FFI glue) emitted as data; cmake + build.rs
      consume it (Q2 lean: codegen produces, two consumers).
- [ ] Remove `--allow-multiple-definition` and the per-combo `-u <symbol>`
      injections (#20); the manifest makes extraction deterministic.
- [ ] Link-closure validator: every symbol the FFI glue references must be
      satisfied by a manifest entry — fail at generation, not at `ld`.
- **Acceptance:** the #20 `-u` special-case is gone and threadx-linux+Cyclone
      still links; removing `--allow-multiple-definition` surfaces no real dup;
      a deliberately-dropped lib fails the validator, not `ld`.

### 241.E — Cleanup + docs
- [ ] Flip RFC-0042 sections to `Stable` as each pillar lands (drift rule:
      update ARCHITECTURE.md in the same commit).
- [ ] Resolve issue 0042 when D1–D4 acceptances pass; cross-link #27/#36/#38/#20
      as the motivating instances.
- [ ] Update the C/C++ integration docs (RFC-0018/0019, c-api-cmake.md) to point
      at the capability block + manifest.

## Risks / notes

- 241.D is the largest change and the one most able to regress linking on a
  platform the gate doesn't fully exercise — land it last, behind 241.A's gate,
  and validate each platform's e2e (the Phase 240 `run_e2e` dispatch) after.
- 241.B's header collapse touches every platform header; the parity check + the
  241.A gate are the guard rails.
- Keep RFC-0034 (allocator funnel) and RFC-0035 (vtable slots) invariant — this
  phase changes *wiring*, not those ABIs.
