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
- [x] New `build-fixture-rust` recipe builds every native rust fixture from
  `fixtures-manifest.py list --platform native --lang rust` (codegen prep +
  manifest-driven cargo loop); `build-fixtures` now depends on it (replaced
  `build-fixture-role-examples`). Verified: builds all 40 entries with their
  exact options; `_check-fixtures-stale` clean afterwards.
- Separator fix: the reader uses `0x1F` (unit separator), not tab — tab is
  IFS-whitespace so bash `read` collapsed the empty `<env>` field and shifted
  columns. Consumers use `IFS=$'\x1f'` + a subshell `export` (not `env`, which
  mis-parses a leading `--` when the env field is empty).
- Transitional: `build-fixture-extras` still has its hard-coded rust builds
  (now cargo no-ops since options match the manifest); 181.6 removes them.
- **Files**: `just/native.just`, `scripts/build/fixtures-manifest.py`,
  `scripts/test/rust-fixture-stale.sh`.

### 181.4 — Rust fixture migration (per platform)

Shared mechanism (done): `scripts/build/fixtures-build.sh <platform> [lang]`
— DRY build loop every platform reuses. Per platform: author manifest entries
→ point the platform's rust `build-fixtures`/`build-examples` at
`fixtures-build.sh <plat> rust` → verify the build → probe stays automatic.

- [x] **181.4.a native** — 40 entries; `build-fixture-rust` → shared script.
  Verified (2 s).
- [x] **181.4.b qemu-arm-baremetal** — 12 plain-cargo entries (cross target via
  each example's `.cargo/config`); verified via the shared script (17 s).
  Recipe still built by the broad `native build-examples` find → wire in 181.6.
- [ ] **181.4.c stm32f4** — plain cargo, thumbv7em; no SDK gate. Currently
  built by `native build-examples`. **Files**: `just/native.just`,
  `examples/fixtures.toml`.
- [ ] **181.4.d freertos** — plain-cargo zenoh (`--no-default-features
  --features rmw-zenoh --target-dir target-zenoh`) + role examples. SDK-gated:
  `FREERTOS_DIR`/`LWIP_DIR` (direnv `FREERTOS_PORT`). Cyclone rust is cmake →
  181.5. **Files**: `just/freertos.just`.
- [ ] **181.4.e nuttx** — plain cargo (`-Z build-std`, pinned nightly +
  `rust-src`). SDK-gated: nuttx kernel + external apps. **Files**:
  `just/nuttx.just`.
- [ ] **181.4.f threadx-linux** — plain-cargo zenoh (`target-zenoh`) +
  `--manifest-path` bins (`logging-smoke-threadx-linux`) + `--release` host
  variants. SDK-gated: threadx/netx linux (NSOS). **Files**:
  `just/threadx-linux.just`.
- [ ] **181.4.g threadx-riscv64** — plain-cargo zenoh. SDK-gated: threadx/netx
  + riscv64 toolchain. Cyclone rust is cmake → 181.5. **Files**:
  `just/threadx-riscv64.just`.
- [ ] **181.4.h esp32 / qemu-esp32-baremetal** — `cargo +nightly`; xtensa/riscv
  ESP toolchain (not installed in the default dev env). **Files**:
  `just/esp32.just`.
- [ ] **181.4.i zephyr** — rust builds via `west build`, NOT cargo, so the
  cargo manifest + `fixtures-build.sh` do not apply. Decide: a west-aware entry
  type, or keep zephyr rust outside the SSOT. **Files**: `just/zephyr.just`.
- [ ] **181.4.j px4** — uORB is C++-only (no rust/C fixture cells per CLAUDE.md).
  N/A; close when 181 lands.

### 181.5 — C/C++ (cmake) fixture migration (per platform)

Add `cmake_defs` to manifest entries for C/C++ cells; the `build-fixtures`
recipes and `nros_cmake_fixture_build` callers read them. Staleness is already
covered by the `.nros-fixture.inputsig` content hash (no rebuild needed), so
this consolidates BUILD options only.

- [ ] **181.5.a native** c/cpp — `-DNROS_RMW={zenoh,xrce,cyclonedds}` cells
  (talker/listener/service/action) + `cpp parameters`. **Files**:
  `just/native.just`.
- [ ] **181.5.b freertos** c/cpp — cyclone talker fixtures. **Files**:
  `just/freertos.just`.
- [ ] **181.5.c nuttx** c/cpp. **Files**: `just/nuttx.just`.
- [ ] **181.5.d threadx-linux** c/cpp. **Files**: `just/threadx-linux.just`.
- [ ] **181.5.e threadx-riscv64** c/cpp + cmake/corrosion cyclone rust cells.
  **Files**: `just/threadx-riscv64.just`.
- [ ] **181.5.f zephyr** c/cpp/rust — west/cmake cells. **Files**:
  `just/zephyr.just`.
- [ ] **181.5.g esp32** c/cpp (if any). **Files**: `just/esp32.just`.
- [ ] **181.5.h px4** cpp uORB register-check. **Files**: `just/px4.just`.

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
