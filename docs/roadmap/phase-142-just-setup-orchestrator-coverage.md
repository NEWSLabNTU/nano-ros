# Phase 142 — `just setup` Orchestrator Coverage for All Supported Targets

**Goal.** Close the gap between what `just setup` installs by default
and what every supported target actually needs to build + run. Today
ESP-IDF, PX4, and (until Phase 142.1) PlatformIO are NOT pulled by the
top-level orchestrator — users must know to run per-module recipes
out-of-band, and Phase 139's `integration_{esp_idf,platformio,px4}`
smoke tests `[SKIPPED]` cleanly on default boxes. This phase lands an
explicit policy + a tiered orchestrator so a fresh checkout has one
documented incantation per coverage level.

**Status.** 142.1 landed (platformio recipe + orchestrator entry).
Remainder not started.

**Priority.** P2 — quality-of-life cleanup. None of the gaps block
correctness; all surface as honest `[SKIPPED]` panics today.

**Depends on.** None blocking. Coordinates with Phase 139 (per-RTOS
integration shells whose smoke tests this phase makes runnable).

**Related.** Phase 135.4 (cold-start `just ci` verification — the
same "first-run from-scratch CI" use case Phase 142 fixes for SDK
prerequisites), Phase 140 (`install-local` removal — orthogonal
consumption-shape change).

---

## Overview

`just setup` today orchestrates 14 modules. The set was assembled
incrementally per phase, not by any declared policy. Result:

| Module | In orchestrator? | Why |
|--------|-----------------|-----|
| workspace, verification, qemu, freertos, nuttx, threadx_{linux,riscv64}, esp32, zephyr, xrce, zenohd, rmw_zenoh, orin_spe, cyclonedds | ✓ | Pulled in as each phase landed |
| **esp_idf** | ✗ | Heavy (~2 GB clone + Python deps); esp32 bare-metal default |
| **px4** | ✗ | Heavy (PX4-Autopilot clone + Python deps + GCS sim) |
| **platformio** | ✓ (142.1) | Just-added for Phase 139 integration tests |

The unspoken rule was "in orchestrator if a Phase 139-style smoke
test needs it AND the install is cheap". That rule isn't written
down; without it, every new SDK becomes an ad-hoc decision.

Phase 142 writes the rule down + tiers the orchestrator so a user
can pick coverage explicitly:

```
just setup                  # Default tier: every cheap dep needed by `just ci`
just setup --tier=extended  # Adds esp_idf + px4 (heavy, opt-in)
just setup --tier=minimal   # Only workspace + zenohd (Rust-side CI)
```

---

## Architecture

### A. Tiers

| Tier | Modules | Use case |
|------|---------|----------|
| **minimal** | workspace, verification, zenohd | Rust-only contributor, no embedded |
| **default** | minimal + qemu, freertos, nuttx, threadx_{linux,riscv64}, esp32, zephyr, xrce, rmw_zenoh, orin_spe, cyclonedds, platformio | Full `just ci` coverage |
| **extended** | default + esp_idf, px4 | Every Phase 139 integration smoke test runnable |

Tier selection via `just setup --tier=<tier>` or env `NROS_SETUP_TIER`.
Default tier preserves today's behaviour exactly (modulo platformio
addition from 142.1, which is cheap).

### B. Policy: when does a module join the default tier?

A module joins **default** when ALL of:

1. Install is ≤500 MB on disk + ≤5 min wall-clock
2. Required by at least one `nros_tests::skip!` call in
   `packages/testing/nros-tests/tests/`
3. Idempotent on re-run (no destructive ops, no sudo)

A module joins **extended** when (1) or (2) fails but the module is
still needed by some Phase 139 integration shell or supported
platform.

A module stays opt-in (NEITHER tier) when it pulls in private SDKs,
requires sudo, or carries license-restricted bits (e.g. ARM FVP).

### C. Doctor parity

`just doctor` mirrors the tier selection: `--tier=extended` runs the
doctor across every tier's modules; default tier matches today.

### D. `just ci` integration

`just ci` doesn't change — it still runs `just check test-all
cyclonedds-ci`. But `just test-all`'s `[SKIPPED]` count varies by
tier:

- minimal: many skips (no embedded SDKs available)
- default: only esp_idf + px4 + platformio-SDK-driven skips
- extended: theoretically zero precondition skips (modulo
  network-gated tests like ROS 2 interop that need a live router)

CI matrix selects the tier per runner.

---

## Work Items

- [x] **142.1 — Land `just platformio setup` + orchestrator entry.**
      Add `just/platformio.just` with `setup` (pipx → pip fallback)
      + `doctor`. Add `mod platformio` to `justfile`. Add
      `run platformio` to `_orchestrate`. **Landed.**
      **Files.** `just/platformio.just`, `justfile`.

- [ ] **142.2 — Tier flag plumbing.**
      Add `setup tier="default"` arg + `_orchestrate` tier-aware
      module list. Env `NROS_SETUP_TIER` overrides default.
      **Files.** `justfile`.

- [ ] **142.3 — Move esp_idf + px4 into extended tier.**
      Today these are opt-in entirely. Phase 142 keeps default
      behaviour same (they stay opt-in for `--tier=default`) but
      adds them to the `--tier=extended` orchestrator path.
      **Files.** `justfile`.

- [ ] **142.4 — Policy doc.**
      Add `docs/contributing/sdk-tiers.md` explaining the three
      tiers + the criteria from §B. Link from `CLAUDE.md`'s
      "## Build" section + from each new module's `Cargo.toml`-adjacent
      README.
      **Files.** `docs/contributing/sdk-tiers.md` (new),
      `CLAUDE.md`.

- [ ] **142.5 — Update CLAUDE.md "## Build" section.**
      Today says `just setup` runs "workspace, verification, qemu,
      freertos, nuttx, threadx_linux, threadx_riscv64, esp32,
      zephyr, xrce, zenohd". Replace with the tier table from §A.
      **Files.** `CLAUDE.md`.

- [ ] **142.6 — qemu PPA upgrade prompt.**
      `just qemu doctor` already WARNs on qemu < 7.2 with apt
      commands. Lift the warning into `just doctor` summary at end
      (currently only individual `qemu` block shows it). Repeat
      the sudo apt commands so users see them without scrolling.
      **Files.** `justfile`, `just/qemu-baremetal.just`.

- [ ] **142.7 — Document opt-in registries from Phase 139.8.**
      `docs/release/registry-publishing.md` already exists. Add a
      cross-link from `docs/contributing/sdk-tiers.md` noting that
      consuming nano-ros from the ESP / PIO registries (once
      published) means the user never runs `just esp_idf setup` —
      they pull a packaged release. Tier policy applies to local
      development, not to downstream consumers.
      **Files.** `docs/contributing/sdk-tiers.md`,
      `docs/release/registry-publishing.md`.

- [ ] **142.8 — Update Phase 139 doc.**
      Phase 139 status currently says "Smoke tests `[SKIPPED]` on
      dev boxes without RTOS SDKs". After 142.3 lands, that becomes
      "Smoke tests run on `--tier=extended` boxes; default tier
      skips esp_idf/px4 cleanly". One-line forward reference.
      **Files.** `docs/roadmap/phase-139-rtos-integration-shells.md`.

---

## Acceptance

- [ ] `just setup` with no args matches pre-142 behaviour exactly
      (no surprise heavy installs).
- [ ] `just setup --tier=extended` adds esp_idf + px4; user sees
      `~2 GB / ~10 min` warning before proceeding (with `[y/N]`
      prompt OR explicit `--yes` to skip the prompt for CI).
- [ ] `just doctor` mirrors tier; `--tier=extended` shows esp_idf +
      px4 status.
- [ ] `docs/contributing/sdk-tiers.md` exists; documents the
      criteria in §B.
- [ ] `CLAUDE.md` "## Build" section reflects the tier table.
- [ ] Phase 139 doc forward-references Phase 142.
- [ ] `just ci` passes (no regression).

---

## Notes

- **Why not just always install everything.** `just setup` is run
  on contributor laptops, not just CI boxes. A two-hour first-run
  install for someone only fixing a `nros-core` typo is hostile.
  Tiered selection respects the contributor's time.
- **Why not Cargo features.** Tiers describe what gets INSTALLED,
  not what gets COMPILED. The two axes are orthogonal: a contributor
  might install `--tier=extended` (every SDK) but build with
  `--no-default-features` (minimum nros features). Don't conflate.
- **Tier `extended` ≠ "all".** ARM FVP, NVIDIA SDK Manager, and
  similar license-gated installs stay opt-in entirely. The criteria
  in §B make this explicit.
- **Phase 135.4 verification.** Cold-start `rm -rf build/install &&
  just ci` validation pending from Phase 135 — Phase 142's policy
  doc should note that the verification implicitly assumes
  `--tier=default` (the doc that promised it).
