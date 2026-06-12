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
- [ ] Make `<nros/platform.h>` (nros-c) the sole header resolvable under that
      include name; `nros-platform-cffi` `#include`s it for the shared surface +
      moves its extras off the shared name (Q1 lean: include, generate nothing).
- [ ] Define the `malloc→alloc` / `free→dealloc` shim **once** (shared inline
      header); delete the 5 copies in `platform/{posix,zephyr,freertos,baremetal}.h`
      + `nros-platform-cffi/platform.h`.
- [ ] Implement the normative include-precedence rule once (RTOS sysroot wins;
      RTOS C++ wrapper dir precedes libstdc++) — see 241.D's shared helper.
- [ ] Add the canonical-surface parity `static_assert`/CI check.
- **Acceptance:** `#include <nros/platform.h>` resolves identically regardless of
      `-I`/`-isystem` order; #27/#36/#38 reproductions stay green with the
      per-board `-D` workarounds (#38's `NROS_PLATFORM_HAS_MALLOC`) removed.

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
