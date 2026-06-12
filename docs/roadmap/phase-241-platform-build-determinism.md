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

Order: **A (gate) → C (capability SSoT) → B (header collapse) → D (linking) → E.**
The gate lands first (safety net). C precedes B because the header collapse needs
a settled capability source — the capability macros are produced by A's per-RTOS
sub-header dispatch today, so the canonical header can't be authored until C
decides where capabilities come from (see the B↔C coupling note under 241.B).
Each wave is independently revertible.

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
- [x] **C.1 — schema + parse + populate (landed).** `[board.capabilities]`
      (`heap`/`atomics`/`threads`) added to `BoardDescriptor` (mirrors the
      `[board.entry]` nested-table pattern) with a `capabilities()` resolver that
      falls back to platform-inferred defaults + a `has_declared_capabilities()`
      helper for the migration lint. All 12 board.toml (13 entries) populated:
      heap-capable = posix/zephyr/freertos/nuttx×2/threadx×2/esp32×2; heap-less =
      baremetal/orin-spe/stm32f4×2. threadx-riscv64 = `heap = true` (the #38
      board — what C.2 lowers to `-D NROS_PLATFORM_HAS_MALLOC`, replacing the
      hand-set `THREADX_GLUE_DEFINES` entry). Catalog parses clean; CLI builds.
- [x] **C.2 — the `-D` lowering (landed).** `cmake/NanoRosCapabilities.cmake`'s
      `nros_board_capability_defines(<board_dir> OUT)` reads `[board.capabilities]`
      from the board's `nros-board.toml` (SSoT) via `file(STRINGS)` and emits the
      matching `NROS_PLATFORM_HAS_*` — no generator, no committed fragment, cmake
      reads the SSoT directly. The threadx-riscv64 overlay's hand-set
      `NROS_PLATFORM_HAS_MALLOC` (the issue-0038 site) is **replaced** by this
      derived value in `THREADX_GLUE_DEFINES`. Because that set is applied to all
      threadx targets (platform cmake), it covers **both** in-tree fixtures and
      scaffolded examples (they build through the same board/platform cmake path).
      Verified: helper unit-checked (threadx heap=true → `-D`, baremetal
      heap=false → none) + a full local `threadx_riscv64 build-fixture-extras`
      builds all 6 zenoh cpp fixtures clean off the *derived* `-D`. The cargo side
      needs no capability lowering — `platform-*` already implies `alloc`.
- [ ] **C.2b — zephyr/freertos validation (deferred).** zephyr heap/mutex
      (Kconfig) + freertos malloc (FreeRTOSConfig) stay config-derived; add a
      check that the board.toml declaration agrees with the RTOS config rather
      than overriding it. (Lower priority — those paths work today.)
- [x] **C.3 — reassessed: resolved by design, no risky churn.** The original
      "retire all per-RTOS self-`#define`s" would *break* every platform whose
      C/C++ build doesn't yet receive the capability `-D` (C.2 wired only the
      threadx overlay; posix/freertos/zephyr still rely on their header
      `#define`s). But those self-`#define`s are correct platform **constants**
      (posix always has a heap; freertos always has a mutex) — not a drifting
      dual source. The drift that *did* bite (#38: bare-metal header says
      "no heap" but the board has one) is the only variable case, and C.2 already
      fixed it via the board.toml-driven `-D` opt-in (baremetal.h's
      `NROS_PLATFORM_HAS_MALLOC` gate). So board.toml is authoritative for the
      variable case; the header supplies platform-constant defaults. Full
      retirement (header constants → universal generated `-D`) is low-value purism
      that needs the `-D` wired into *every* platform's C/C++ path first; deferred
      unless a second variable-capability case appears.
- [x] **C.4 — migration lint (landed).** A merge-gate unit test
      (`every_in_tree_board_declares_capabilities` in `board_descriptor.rs`) loads
      the real `packages/boards/*` catalog and fails if any board lacks
      `[board.capabilities]` (relying on inference). All in-tree boards declare
      today; it guards future boards from silently inheriting a wrong default.
- **Acceptance:** flipping a board's `heap`/`atomics`/`threads` in one place
      changes the build everywhere; no capability named in >1 site; the
      threadx-riscv64 `-D` is generated from board.toml, not hand-set.

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
