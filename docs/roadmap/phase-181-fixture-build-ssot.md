# Phase 181 - Fixture build SSOT (`examples/fixtures.toml`)

**Goal.** A single source of truth for every test-fixture's build options
(features, `--no-default-features`, `--target-dir`, cross `--target`, build
env, and — for C/C++ — cmake `-D` defs), consumed by BOTH the fixture build
recipes (`just <plat> build-fixtures`) AND the Phase 177.9 test-all staleness
probe. Today those options are duplicated/divergent across `just/*.just`; the
probe cannot know them, so it fell back to default features — which gives a
false staleness signal and triggers feature-thrash rebuilds. One manifest
fixes both.

**Status.** In progress (started on branch `phase-181-fixture-build-ssot`).
Foundation + native-rust authored; rollout ongoing.

**Priority.** P2 — improves `just test-all` correctness/UX (Phase 177.9
follow-up). Does not block `just ci` once landed.

**Depends on.** Phase 177.9 (the fixtures preflight + `_check-fixtures-stale`
+ `.nros-fixture.inputsig` for C/C++).

## Overview

- Manifest: `examples/fixtures.toml` — `[[fixture]]` entries, one per built
  artifact. Schema in the file header.
- Reader: `scripts/build/fixtures-manifest.py list --platform P [--lang L]
  [--rmw R]` → TSV `<dir>\t<env>\t<cargo-args>` per entry (profile added by
  the caller).
- Consumers (SSOT): the per-platform `build-fixtures` recipes (build) and
  `scripts/test/rust-fixture-stale.sh` + `just _check-fixtures-stale` (probe).
- Scope split: per-FIXTURE options live in the manifest; platform ENV
  (toolchain paths, board cache vars, SDK dirs) stays in the recipes.

## Architecture

- Rust cells: probe runs `cargo build <profile> <manifest-args>
  --message-format=json`; cargo's `fresh` flag is the staleness oracle
  (rebuilds stale = self-heal). Build recipe runs the same `cargo build`.
- C/C++ cells: `nros_cmake_fixture_build` already content-hashes inputs
  (`.nros-fixture.inputsig`) for staleness; the manifest consolidates their
  cmake `-D` BUILD options (the recipe still provides platform toolchain/board
  cache vars).

## Work Items

### 181.1 — Manifest + reader (foundation)
- [x] `examples/fixtures.toml` schema + native-rust entries (roles, the
  param-services talker, tls/safety/zero-copy variants, per-RMW target-`<rmw>`
  builds, custom-transport, bench, large-buf env).
- [x] `scripts/build/fixtures-manifest.py` reader (`tomllib`/`tomli`).

### 181.2 — Rust staleness probe reads the manifest (A)
- [x] `scripts/test/rust-fixture-stale.sh` takes a manifest TSV record and
  builds with the exact features/target-dir/env (not default features).
- [x] `just _check-fixtures-stale` drives the rust pass from
  `fixtures-manifest.py list --lang rust`.
- Verified: full run over all 40 native-rust entries with their real options
  finished in ~15 s, flagged only the 2 genuinely-stale custom-transport
  cells (self-healed), no feature-thrash.
- **Files**: `scripts/test/rust-fixture-stale.sh`, `justfile`.

### 181.3 — Native rust build recipe reads the manifest (B, proof)
- [ ] `just native build-fixtures` rust builds loop the manifest instead of
  hard-coded `cargo build ...` lines; verify `just native build-fixtures`.
- **Files**: `just/native.just`.

### 181.4 — Roll out remaining rust platforms
- [ ] Author manifest entries + migrate recipes for: qemu-arm-baremetal,
  freertos, nuttx, threadx-linux, threadx-riscv64, esp32, stm32f4, zephyr,
  px4. Cross `--target` + per-platform env handled by the recipe; per-fixture
  options from the manifest. Verify each platform's `build-fixtures`.
- **Files**: `just/*.just`, `examples/fixtures.toml`.

### 181.5 — C/C++ cells
- [ ] Add `cmake_defs` to manifest entries for C/C++ cells; `build-fixtures`
  recipes + `nros_cmake_fixture_build` callers read them. Probe stays on the
  `.nros-fixture.inputsig` content hash (no rebuild).
- **Files**: `examples/fixtures.toml`, `just/*.just`,
  `scripts/build/fixture-matrix.sh`.

### 181.6 — Remove duplicated options from recipes (true SSOT)
- [ ] After each migration, delete the now-redundant inline build options so
  the manifest is the only source. `examples/README.md` coverage matrix and
  the manifest must agree.

## Acceptance

- [ ] `just <plat> build-fixtures` builds identical artifacts to before, with
  options sourced only from `examples/fixtures.toml`.
- [ ] `just _check-fixtures-stale` probes each rust fixture with its real
  options (no feature-thrash; fresh after a clean build).
- [ ] No fixture build options remain hard-coded in `just/*.just` (grep clean).
- [ ] `just test-all` preflight still fast when fresh.

## Notes

- Reader has no `tomllib` on Python 3.10; falls back to `tomli` (present in
  the dev env). If neither is available the manifest can't be read — keep the
  `tomli` dependency documented.
- Migration is incremental + per-platform because each `build-fixtures`
  verification is slow; land one platform at a time, keeping the manifest and
  recipe consistent so the probe never thrashes mid-rollout.
